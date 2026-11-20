//! Rows on version 2 plant (`docs/tasks/24-as-built-diagnosis-and-selection.md`): designed under the generic rules with
//! every component a field build has, fiber included, half greenfield and half legacy then upgraded. The streets of a map
//! are real and everything on them is invented, so every row is `Synthetic` (invariant 11).
//!
//! `datagen ab-survey <manifest.json> <osm dir> <out manifest.json>`: task 15's rule for using a map, on version 2 designs.
//! `datagen ab-rows <out.jsonl> <designs per family> <rows per node> <floor share> <seed>`: generated families.
//! `datagen ab-maps-rows <out.jsonl> <manifest.json> <osm dir> <split> <rows per node> <floor share> <seed>`.
//! `datagen ab-hybrid <out.jsonl> <model.json> <manifest.json> <osm dir> <split> <rows per node> <share> <seed> <floor|nofloor|proposal>`.
//! `datagen ab-fiber <out.jsonl> <designs> <seed>`: `diagnosis::fiber` on regions of six nodes or more.
//! `datagen ab-spectra <out.jsonl> <manifest.json> <osm dir> <split> <rows per node> <seed>`: task 17's capture rows.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::{Arc, OnceLock};

use design::v2::{DesignV2, design, design_upgraded};
use diagnosis::fiber::{FiberVerdict, LinkReading, diagnose_fiber};
use diagnosis::{DiagnoserConfig, diagnose};
use features::DesignReference;
use features::StepFeatures;
use features::sampler::{
    AS_BUILT_CLASSES, CLASSES, FIBER_CLASSES, RPD_CLASSES, class_in, fault_element,
    sample_fault_as_built, sample_fiber_fault,
};
use features::sweep::{observe_in, random_sample};
use plant_graph::PlantGraph;
use proposal::{ComponentRow, GraphNode, component_rows, graph_nodes};
use schema::common::{Origin, ServiceGroupId, Timestamp};
use schema::component::ComponentLibrary;
use schema::design_rules::DesignRules;
use schema::environment::{AmbientSource, EnvironmentSample};
use schema::fault::{Fault, FaultSet, Schedule};
use schema::fiber::FiberPlant;
use schema::fiber_fault::FiberFault;
use schema::plant::{Installation, Plant};
use schema::profile::{ChannelPlan, NodeKind, Profile};
use schema::region::{DesignControls, DesignMode, LayoutFamily, StreetMap};
use schema::work_order::CauseClass;
use serde::Serialize;
use sim::optics::{NodeLink, region_links};
use sim::rng::{Rng, hash_str};

use crate::maps::{Manifest, load_map};
use crate::rows::{Error, Mode, read, write_sharded};

const FAMILIES: [LayoutFamily; 3] = [
    LayoutFamily::UrbanGrid,
    LayoutFamily::Suburban,
    LayoutFamily::Rural,
];

/// Homes a generated design starts from: version 2 makes three or four nodes of these (task 24, "Fewer, larger nodes").
const GENERATED_HOMES: usize = 600;
const RURAL_HOMES: usize = 300;
const HYBRID_TOP: usize = 8;

/// Jobs per shard of a long run, unless `HFC_SHARD_JOBS` says otherwise: a few minutes of hybrid runs on every core, and a
/// restart loses at most one shard's work.
fn shard_jobs() -> usize {
    std::env::var("HFC_SHARD_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64)
}

/// The rules, the legacy rules and profile a legacy design starts from, and the parts, read once.
pub(crate) struct Inputs {
    pub(crate) library: ComponentLibrary,
    rules: DesignRules,
    legacy_rules: DesignRules,
    legacy_profile: Profile,
    profiles: [Arc<Profile>; 2],
}

impl Inputs {
    /// The rules' cost ceiling, US dollars per home passed; none is no ceiling.
    fn ceiling(&self) -> f64 {
        self.rules
            .costs
            .ceiling_usd_per_home
            .as_ref()
            .map_or(f64::INFINITY, |c| c.value)
    }
}

pub(crate) fn inputs() -> Result<Inputs, Error> {
    inputs_on(false)
}

/// The inputs, on the two shipped profiles or, for the held-out read-out task 24 reports beside its rules, the two fully
/// loaded ones, which also report optics and each port's level.
pub(crate) fn inputs_on(full: bool) -> Result<Inputs, Error> {
    let migrate = |p: Profile| schema::profile::migrate(p).map_err(Error::from);
    let suffix = if full { "_full" } else { "" };
    Ok(Inputs {
        library: read("fixtures/library/generic.json")?,
        rules: read("fixtures/design_rules/generic.json")?,
        legacy_rules: read("fixtures/design_rules/legacy-750.json")?,
        legacy_profile: migrate(read("fixtures/profiles/d30_lowsplit_750_legacy.json")?)?,
        profiles: [
            Arc::new(migrate(read(&format!(
                "fixtures/profiles/d31_lowsplit_1002_iccap{suffix}.json"
            ))?)?),
            Arc::new(migrate(read(&format!(
                "fixtures/profiles/d31_highsplit_1218_rpd{suffix}.json"
            ))?)?),
        ],
    })
}

/// How a design was made, as a row says it.
fn mode_name(upgraded: bool) -> &'static str {
    if upgraded {
        "LegacyThenUpgrade"
    } else {
        "Greenfield"
    }
}

/// A design of one map on one profile, in either mode. Nodes over the rules' cost ceiling are built and marked (task 24).
///
/// With `HFC_DESIGN_CACHE` set to a directory, a design is kept there under a hash of everything it's derived from (the
/// map, the profile, both sets of rules, the legacy profile, the parts, the seed and the mode) and read back by the next
/// command that needs it: a real map takes minutes to design, and a long run reads each one dozens of times. It's a copy
/// of a derived value keyed by its inputs, never an input (invariant 1); delete the directory and nothing changes.
pub(crate) fn design_map(
    inputs: &Inputs,
    map: &StreetMap,
    profile: &Profile,
    seed: u64,
    upgraded: bool,
) -> Result<DesignV2, Error> {
    let Ok(dir) = std::env::var("HFC_DESIGN_CACHE") else {
        return design_fresh(inputs, map, profile, seed, upgraded);
    };
    let key = [
        serde_json::to_string(map)?,
        serde_json::to_string(profile)?,
        serde_json::to_string(&inputs.rules)?,
        serde_json::to_string(&inputs.legacy_rules)?,
        serde_json::to_string(&inputs.legacy_profile)?,
        serde_json::to_string(&inputs.library)?,
        format!("{seed}/{upgraded}"),
    ]
    .iter()
    .fold(0u64, |h, part| h.rotate_left(17) ^ hash_str(part));
    let path = format!("{dir}/{}-{key:016x}.json", map.id);
    if let Ok(text) = fs::read_to_string(&path) {
        return Ok(serde_json::from_str(&text)?);
    }
    let built = design_fresh(inputs, map, profile, seed, upgraded)?;
    fs::create_dir_all(&dir)?;
    // Written whole and then renamed, so a reader never sees half a file.
    let partial = format!("{path}.partial-{}", std::process::id());
    fs::write(&partial, serde_json::to_string(&built)?)?;
    fs::rename(&partial, &path)?;
    Ok(built)
}

fn design_fresh(
    inputs: &Inputs,
    map: &StreetMap,
    profile: &Profile,
    seed: u64,
    upgraded: bool,
) -> Result<DesignV2, Error> {
    let controls = DesignControls {
        seed,
        rules_id: Some(inputs.rules.id.clone()),
        mode: Some(if upgraded {
            DesignMode::LegacyThenUpgrade {
                legacy_rules_id: inputs.legacy_rules.id.clone(),
                legacy_profile_ref: inputs.legacy_profile.id.clone(),
            }
        } else {
            DesignMode::Greenfield
        }),
        build_over_ceiling: true,
        ..DesignControls::default()
    };
    Ok(if upgraded {
        design_upgraded(
            map,
            profile,
            &inputs.library,
            &inputs.rules,
            &inputs.legacy_profile,
            &inputs.legacy_rules,
            &controls,
        )?
    } else {
        design(map, profile, &inputs.library, &inputs.rules, &controls)?
    })
}

/// One designed region, shared by its nodes' rows.
pub(crate) struct Region {
    family: String,
    pub(crate) map: Option<String>,
    design_seed: u64,
    pub(crate) profile: Arc<Profile>,
    pub(crate) rules_id: String,
    pub(crate) mode: &'static str,
    pub(crate) fiber: FiberPlant,
    pub(crate) plants: Vec<Plant>,
    /// Nodes the rules' cost ceiling puts over it.
    pub(crate) over: BTreeSet<ServiceGroupId>,
    pub(crate) healthy: BTreeMap<ServiceGroupId, NodeLink>,
    references: Vec<OnceLock<Result<DesignReference, String>>>,
}

impl Region {
    /// `source` is the family, the map if it's on one, and the design's seed.
    fn new(
        source: (&str, Option<&str>, u64),
        profile: &Arc<Profile>,
        upgraded: bool,
        d: DesignV2,
        inputs: &Inputs,
    ) -> Result<Region, Error> {
        let (family, map, design_seed) = source;
        let (library, ceiling_usd_per_home) = (&inputs.library, inputs.ceiling());
        // Over the rules' cost ceiling: what the design's own cost report says each node costs per home passed.
        let over: BTreeSet<ServiceGroupId> = d
            .cost
            .nodes
            .iter()
            .filter(|n| n.usd_per_home > ceiling_usd_per_home)
            .map(|n| n.service_group.clone())
            .collect();
        let plants: Vec<Plant> = d.nodes.iter().map(|n| n.plant.clone()).collect();
        let pairs: Vec<(&Plant, &ChannelPlan)> =
            plants.iter().map(|p| (p, &profile.channel_plan)).collect();
        let healthy = region_links(&d.fiber, &pairs, library, &[], Timestamp(0))?;
        Ok(Region {
            family: family.to_string(),
            map: map.map(str::to_string),
            design_seed,
            profile: Arc::clone(profile),
            rules_id: d.rules_id.clone(),
            mode: mode_name(upgraded),
            references: plants.iter().map(|_| OnceLock::new()).collect(),
            fiber: d.fiber,
            plants,
            over,
            healthy,
        })
    }

    pub(crate) fn pairs(&self) -> Vec<(&Plant, &ChannelPlan)> {
        self.plants
            .iter()
            .map(|p| (p, &self.profile.channel_plan))
            .collect()
    }

    /// The node's healthy reading, as designed: computed once, by the first row that needs it.
    fn reference(&self, node: usize, graph: &PlantGraph<'_>) -> Result<DesignReference, Error> {
        let sg = &self.plants[node].service_group;
        self.references[node]
            .get_or_init(|| {
                DesignReference::compute_with_link(graph, self.healthy.get(sg))
                    .map_err(|e| e.to_string())
            })
            .clone()
            .map_err(Error::from)
    }
}

pub(crate) struct AbJob {
    region: Arc<Region>,
    node: usize,
    row_seed: u64,
    index: usize,
    run_floor: bool,
}

/// The rows for every node of a region with no issue, `rows_per_node` each. Rows come from clean nodes only (task 15).
fn region_jobs(
    jobs: &mut Vec<AbJob>,
    region: &Arc<Region>,
    issues: &[bool],
    rows_per_node: usize,
    floor_share: f64,
    seed: u64,
) {
    for (node, plant) in region.plants.iter().enumerate() {
        if issues[node] {
            continue;
        }
        for r in 0..rows_per_node {
            let row_seed = Rng::derive(
                seed,
                &[
                    hash_str(region.map.as_deref().unwrap_or(&region.family)),
                    region.design_seed,
                    hash_str(region.mode),
                    hash_str(region.profile.id.as_str()),
                    hash_str(plant.service_group.as_str()),
                    r as u64,
                ],
            )
            .next_u64();
            jobs.push(AbJob {
                region: Arc::clone(region),
                node,
                row_seed,
                index: r,
                run_floor: Rng::new(row_seed ^ 0x5eed).uniform() < floor_share,
            });
        }
    }
}

/// Radio around the plant, varied per row so healthy isn't one line (task 24): none, or up to two of a short-wave band
/// in the return, FM broadcast, and LTE above 600 MHz [PNM-HFC §7.1.4.2] (impairments.md §8). Levels at zero coupling
/// are judgement, not a survey: −40 to −10 dBmV per MHz.
fn ambient(rng: &mut Rng) -> Vec<AmbientSource> {
    let bands = [
        ("ambient-hf", 5.0, 30.0),
        ("ambient-fm", 88.0, 108.0),
        ("ambient-lte", 698.0, 806.0),
    ];
    let n = rng.index(3);
    let mut out = Vec::new();
    let first = rng.index(bands.len());
    for k in 0..n {
        let (id, lo_mhz, hi_mhz) = bands[(first + k) % bands.len()];
        out.push(AmbientSource {
            id: id.to_string(),
            lo_mhz,
            hi_mhz,
            level_dbmv_per_mhz: rng.range(-40.0, -10.0),
            schedule: Schedule {
                hourly_multipliers: [1.0; 24],
            },
        });
    }
    out
}

/// One row's world: what went wrong, and what the node reported.
struct Seen<'a> {
    graph: PlantGraph<'a>,
    reference: DesignReference,
    fault: Option<Fault>,
    fiber_fault: Option<FiberFault>,
    class: Option<CauseClass>,
    component: Option<String>,
    fiber_place: Option<String>,
    observed: StepFeatures,
    at_reading: EnvironmentSample,
    /// What the diagnoser believes around the node: its link as designed, and the radio the row was seen in.
    config: DiagnoserConfig,
}

/// One row's world: `world` is the parts as they really are, where they differ from what the diagnoser believes.
fn seen<'a>(
    job: &'a AbJob,
    library: &'a ComponentLibrary,
    world: Option<&'a ComponentLibrary>,
) -> Result<Seen<'a>, Error> {
    let region = &job.region;
    let plant = &region.plants[job.node];
    let graph = PlantGraph::new(plant, library, &region.profile).map_err(|e| format!("{e:?}"))?;
    let world_graph = match world {
        Some(w) => Some(PlantGraph::new(plant, w, &region.profile).map_err(|e| format!("{e:?}"))?),
        None => None,
    };
    let reference = region.reference(job.node, &graph)?;
    let mut rng = Rng::new(job.row_seed);
    let daa = region.profile.node_kind == NodeKind::Rpd;
    let classes: Vec<CauseClass> = CLASSES
        .iter()
        .copied()
        .chain(RPD_CLASSES.iter().copied().filter(|_| daa))
        .chain(AS_BUILT_CLASSES)
        .chain(FIBER_CLASSES)
        .collect();
    let class = (job.index % 10 != 9).then(|| classes[job.index % classes.len()]);
    let sg = &plant.service_group;
    let (mut fault, mut fiber_fault) = (None, None);
    match class {
        Some(c) if FIBER_CLASSES.contains(&c) => {
            fiber_fault = sample_fiber_fault(&region.fiber, sg, c, &mut rng);
        }
        Some(c) => fault = sample_fault_as_built(&graph, c, &mut rng),
        None => {}
    }
    let link = match &fiber_fault {
        Some(f) => region_links(
            &region.fiber,
            &region.pairs(),
            library,
            std::slice::from_ref(f),
            Timestamp(0),
        )?
        .remove(sg),
        None => region.healthy.get(sg).cloned(),
    };
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.to_string(),
        seed: job.row_seed,
        faults: fault.iter().cloned().collect(),
    };
    let around = ambient(&mut rng);
    let mut sample = random_sample(&mut rng, 0);
    sample.t = Timestamp(features::sampler::reading_t(fault.as_ref(), &mut rng));
    let observed = observe_in(
        world_graph.as_ref().unwrap_or(&graph),
        &reference,
        &faults,
        sample.clone(),
        &around,
        job.row_seed,
        link.as_ref(),
    )?;
    let mut at_reading = sample;
    at_reading.t = Timestamp(at_reading.t.0 + 300);
    // A fault up the fiber is, from the node, at its root: the region's fiber diagnoser names the cable or optic.
    let (label_class, component, fiber_place) = match (&fault, &fiber_fault) {
        (Some(f), _) => (
            Some(class_in(&graph, &f.kind)),
            fault_element(&f.kind),
            None,
        ),
        (None, Some(f)) => (
            Some(CauseClass::from(&f.kind)),
            Some(graph.root().id.to_string()),
            Some(fiber_place(f)),
        ),
        (None, None) => (None, None, None),
    };
    let config = DiagnoserConfig {
        link: region.healthy.get(sg).cloned(),
        ambient: around,
        ..DiagnoserConfig::default()
    };
    Ok(Seen {
        graph,
        reference,
        fault,
        fiber_fault,
        class: label_class,
        component,
        fiber_place,
        observed,
        at_reading,
        config,
    })
}

/// Where on the fiber a fault is, by name.
fn fiber_place(fault: &FiberFault) -> String {
    use schema::fiber_fault::{FiberFaultKind, FiberLocation};
    match &fault.kind {
        FiberFaultKind::FiberCut { cable, .. } => cable.to_string(),
        FiberFaultKind::EventLoss { location, .. } => match location {
            FiberLocation::Cable { cable, .. } => cable.to_string(),
            FiberLocation::Element { element } | FiberLocation::Splice { element, .. } => {
                element.to_string()
            }
        },
        FiberFaultKind::TransmitterDrift { element, .. } => element.to_string(),
    }
}

#[derive(Serialize)]
struct Versions {
    observation: &'static str,
    fault: &'static str,
    fiber_fault: &'static str,
    plant: &'static str,
    environment: &'static str,
    work_order: &'static str,
    region: &'static str,
}

fn versions() -> Versions {
    Versions {
        observation: schema::observation::SCHEMA_VERSION,
        fault: schema::fault::SCHEMA_VERSION,
        fiber_fault: schema::fiber_fault::SCHEMA_VERSION,
        plant: schema::plant::SCHEMA_VERSION,
        environment: schema::environment::SCHEMA_VERSION,
        work_order: schema::work_order::SCHEMA_VERSION,
        region: schema::region::SCHEMA_VERSION,
    }
}

#[derive(Serialize)]
struct FloorAnswer {
    components: Vec<Vec<String>>,
    classes: Vec<Option<CauseClass>>,
    posterior: Vec<f64>,
    simulations: usize,
}

#[derive(Serialize)]
struct AbRow {
    origin: Origin,
    /// Kept beside `versions` so a reader of task 15's rows reads these too.
    observation_schema: &'static str,
    fault_schema: &'static str,
    versions: Versions,
    rules_id: String,
    design_mode: &'static str,
    over_ceiling: bool,
    family: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    map: Option<String>,
    profile: String,
    installation: Installation,
    design_seed: u64,
    node: String,
    seed: u64,
    class: Option<CauseClass>,
    component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fiber_place: Option<String>,
    candidates: Vec<ComponentRow>,
    graph: Vec<GraphNode>,
    floor: Option<FloorAnswer>,
}

fn installation_of(plant: &Plant) -> Installation {
    plant
        .edges
        .iter()
        .find_map(|e| match e.kind {
            schema::plant::EdgeKind::Span { installation, .. } => Some(installation),
            _ => None,
        })
        .unwrap_or(Installation::Aerial)
}

fn one_row(job: &AbJob, library: &ComponentLibrary) -> Result<String, Error> {
    let s = seen(job, library, None)?;
    let region = &job.region;
    let plant = &region.plants[job.node];
    let floor = if job.run_floor {
        let answer = diagnose(
            &s.graph,
            &s.reference,
            &s.observed,
            &s.at_reading,
            &s.config,
        )?;
        Some(FloorAnswer {
            components: answer
                .hypotheses
                .iter()
                .map(|h| h.components.clone())
                .collect(),
            classes: answer.hypotheses.iter().map(|h| h.class).collect(),
            posterior: answer.hypotheses.iter().map(|h| h.posterior).collect(),
            simulations: answer.simulations,
        })
    } else {
        None
    };
    let row = AbRow {
        origin: Origin::Synthetic,
        observation_schema: schema::observation::SCHEMA_VERSION,
        fault_schema: schema::fault::SCHEMA_VERSION,
        versions: versions(),
        rules_id: region.rules_id.clone(),
        design_mode: region.mode,
        over_ceiling: region.over.contains(&plant.service_group),
        family: region.family.clone(),
        map: region.map.clone(),
        profile: region.profile.id.to_string(),
        installation: installation_of(plant),
        design_seed: region.design_seed,
        node: plant.service_group.to_string(),
        seed: job.row_seed,
        class: s.class,
        component: s.component.clone(),
        fiber_place: s.fiber_place.clone(),
        candidates: component_rows(&s.graph, &s.observed),
        graph: graph_nodes(&s.graph, plant, &s.observed),
        floor,
    };
    Ok(serde_json::to_string(&row)?)
}

/// Every generated design: each family, both profiles, `designs` seeds, the even ones greenfield and the odd ones legacy
/// then upgraded.
fn generated_jobs(
    inputs: &Inputs,
    designs: u64,
    rows_per_node: usize,
    floor_share: f64,
    seed: u64,
) -> Result<Vec<AbJob>, Error> {
    let mut work = Vec::new();
    for family in FAMILIES {
        for profile in &inputs.profiles {
            for d in 0..designs {
                work.push((family, Arc::clone(profile), d));
            }
        }
    }
    let regions = in_parallel(&work, |(family, profile, d)| {
        let design_seed = seed * 1000 + d;
        let homes = match family {
            LayoutFamily::Rural => RURAL_HOMES,
            _ => GENERATED_HOMES,
        };
        let map = design::generate(*family, design_seed, homes);
        let upgraded = d % 2 == 1;
        let built = design_map(inputs, &map, profile, design_seed, upgraded)?;
        let issues: Vec<bool> = built.nodes.iter().map(|n| !n.issues.is_empty()).collect();
        let region = Region::new(
            (&format!("{family:?}"), None, design_seed),
            profile,
            upgraded,
            built,
            inputs,
        )?;
        Ok((Arc::new(region), issues))
    })?;
    let mut jobs = Vec::new();
    for (region, issues) in &regions {
        region_jobs(&mut jobs, region, issues, rows_per_node, floor_share, seed);
    }
    Ok(jobs)
}

/// Each area the version 2 survey kept in a split, both profiles, both modes.
fn map_jobs(
    inputs: &Inputs,
    manifest: &str,
    dir: &str,
    split: &str,
    rows_per_node: usize,
    floor_share: f64,
    seed: u64,
) -> Result<Vec<AbJob>, Error> {
    let mut jobs = Vec::new();
    for (region, issues) in &map_regions(inputs, manifest, dir, split)? {
        region_jobs(&mut jobs, region, issues, rows_per_node, floor_share, seed);
    }
    Ok(jobs)
}

/// One designed map, with which of its nodes can't be used.
pub(crate) type MapRegion = (Arc<Region>, Vec<bool>);

/// Every map of a split designed on both profiles, greenfield and legacy then upgraded, with which nodes have an issue or
/// too few homes to use (task 15's rule).
pub(crate) fn map_regions(
    inputs: &Inputs,
    manifest: &str,
    dir: &str,
    split: &str,
) -> Result<Vec<MapRegion>, Error> {
    let manifest: Manifest = read(manifest)?;
    let mut work = Vec::new();
    for entry in manifest
        .areas
        .iter()
        .filter(|e| e.used && e.area.split == split)
    {
        let (map, hash) = load_map(dir, &entry.area)?
            .ok_or_else(|| format!("no extract for {}", entry.area.name))?;
        if format!("{hash:016x}") != entry.extract_hash {
            return Err(format!("{} isn't the extract surveyed", entry.area.name).into());
        }
        let map = Arc::new(map);
        for profile in &inputs.profiles {
            for upgraded in [false, true] {
                work.push((
                    entry.area.name.clone(),
                    Arc::clone(&map),
                    Arc::clone(profile),
                    upgraded,
                ));
            }
        }
    }
    let regions = in_parallel(&work, |(name, map, profile, upgraded)| {
        let built = design_map(inputs, map, profile, 0, *upgraded)?;
        let issues: Vec<bool> = built
            .nodes
            .iter()
            .map(|n| !n.issues.is_empty() || n.homes_passed < crate::maps::MIN_NODE_HOMES)
            .collect();
        let region = Region::new(("Osm", Some(name), 0), profile, *upgraded, built, inputs)?;
        Ok((Arc::new(region), issues))
    })?;
    Ok(regions)
}

/// Runs `f` on every item on every core, in item order.
pub(crate) fn in_parallel<T: Sync, R: Send>(
    items: &[T],
    f: impl Fn(&T) -> Result<R, Error> + Sync,
) -> Result<Vec<R>, Error> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    // `HFC_THREADS` caps it, for a run beside another on the threads that one leaves.
    let workers = std::env::var("HFC_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()));
    let mut out: Vec<(usize, R)> = Vec::new();
    std::thread::scope(|scope| -> Result<(), Error> {
        let handles: Vec<_> = (0..workers.min(items.len().max(1)))
            .map(|_| {
                scope.spawn(|| -> Result<Vec<(usize, R)>, Error> {
                    let mut local = Vec::new();
                    loop {
                        let at = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(item) = items.get(at) else {
                            return Ok(local);
                        };
                        local.push((at, f(item)?));
                    }
                })
            })
            .collect();
        for handle in handles {
            out.extend(handle.join().map_err(|_| "a worker panicked")??);
        }
        Ok(())
    })?;
    out.sort_by_key(|r| r.0);
    Ok(out.into_iter().map(|r| r.1).collect())
}

/// `datagen ab-rows`.
pub fn rows(args: &[String]) -> Result<String, Error> {
    let [out, designs, rows_per_node, floor_share, seed] = args else {
        return Err("usage: datagen ab-rows <out.jsonl> <designs per family> <rows per node> <floor share> <seed>".into());
    };
    let inputs = inputs()?;
    let jobs = generated_jobs(
        &inputs,
        designs.parse()?,
        rows_per_node.parse()?,
        floor_share.parse()?,
        seed.parse()?,
    )?;
    let total = write_sharded(&jobs, out, shard_jobs(), |job| {
        one_row(job, &inputs.library)
    })?;
    Ok(format!("{total} synthetic rows in {out}"))
}

/// `datagen ab-maps-rows`.
pub fn maps_rows(args: &[String]) -> Result<String, Error> {
    let [out, manifest, dir, split, rows_per_node, floor_share, seed] = args else {
        return Err("usage: datagen ab-maps-rows <out.jsonl> <manifest.json> <osm dir> <split> <rows per node> <floor share> <seed>".into());
    };
    let inputs = inputs()?;
    let jobs = map_jobs(
        &inputs,
        manifest,
        dir,
        split,
        rows_per_node.parse()?,
        floor_share.parse()?,
        seed.parse()?,
    )?;
    let total = write_sharded(&jobs, out, shard_jobs(), |job| {
        one_row(job, &inputs.library)
    })?;
    Ok(format!("{total} synthetic rows in {out}"))
}

#[derive(Serialize)]
struct SurveyEntry {
    name: String,
    split: String,
    reserve: bool,
    homes: usize,
    street_miles: f64,
    /// Nodes, and clean nodes of at least 10 homes, per profile and mode.
    nodes: Vec<usize>,
    clean_nodes: Vec<usize>,
    meets_rule: bool,
    /// Why not, where it doesn't.
    #[serde(skip_serializing_if = "Option::is_none")]
    why_not: Option<String>,
    /// Whether the legacy-then-upgraded design also has a clean node of at least 10 homes on both profiles. Part of no rule.
    upgraded_clean_on_both: bool,
}

/// `datagen ab-survey`: task 15's rule (150 homes, 3 homes per street mile, a clean node of at least 10 homes on both
/// profiles), applied as written with version 2's checks deciding clean, on the greenfield design. Writes a manifest in task
/// 15's form with `used` set by the rule here, lists the areas that fall out, and records whether the upgraded design is
/// clean on both profiles too.
pub fn survey(args: &[String]) -> Result<String, Error> {
    let [manifest, dir, out] = args else {
        return Err(
            "usage: datagen ab-survey <manifest.json> <osm dir> <out manifest.json>".into(),
        );
    };
    let inputs = inputs()?;
    let text = fs::read_to_string(manifest)?;
    let mut value: serde_json::Value = serde_json::from_str(&text)?;
    let parsed: Manifest = serde_json::from_str(&text)?;
    let mut work = Vec::new();
    for (i, entry) in parsed.areas.iter().enumerate() {
        if !entry.used {
            continue;
        }
        if let Some((map, _)) = load_map(dir, &entry.area)? {
            let map = Arc::new(map);
            for profile in &inputs.profiles {
                for upgraded in [false, true] {
                    work.push((i, Arc::clone(&map), Arc::clone(profile), upgraded));
                }
            }
        }
    }
    let counts = in_parallel(&work, |(i, map, profile, upgraded)| {
        Ok((
            *i,
            match design_map(&inputs, map, profile, 0, *upgraded) {
                Ok(d) => (
                    d.nodes.len(),
                    d.nodes
                        .iter()
                        .filter(|n| {
                            n.issues.is_empty() && n.homes_passed >= crate::maps::MIN_NODE_HOMES
                        })
                        .count(),
                ),
                Err(_) => (0, 0),
            },
        ))
    })?;
    let mut entries = Vec::new();
    for (i, entry) in parsed.areas.iter().enumerate() {
        if !entry.used {
            continue;
        }
        let mine: Vec<(usize, usize)> = counts.iter().filter(|c| c.0 == i).map(|c| c.1).collect();
        let homes_ok = entry.homes >= crate::maps::MIN_HOMES
            && entry.homes as f64 >= entry.street_miles * crate::maps::MIN_HOMES_PER_STREET_MILE;
        // In `work`'s order: low-split greenfield, low-split upgraded, high-split greenfield, high-split upgraded. The rule
        // names profiles, not modes, and is read on the greenfield design; the upgraded one's result is recorded beside it,
        // and its rows come from whatever clean nodes it has.
        let clean_on =
            |upgraded: usize| mine.len() == 4 && mine[upgraded].1 > 0 && mine[2 + upgraded].1 > 0;
        let why_not = if !homes_ok {
            Some("homes".to_string())
        } else if !clean_on(0) {
            Some(format!(
                "no clean node of {}+ homes on both profiles, greenfield: {:?}",
                crate::maps::MIN_NODE_HOMES,
                mine
            ))
        } else {
            None
        };
        let upgraded_clean_on_both = clean_on(1);
        entries.push(SurveyEntry {
            name: entry.area.name.clone(),
            split: entry.area.split.clone(),
            reserve: entry.area.reserve,
            homes: entry.homes,
            street_miles: entry.street_miles,
            nodes: mine.iter().map(|c| c.0).collect(),
            clean_nodes: mine.iter().map(|c| c.1).collect(),
            meets_rule: why_not.is_none(),
            why_not,
            upgraded_clean_on_both,
        });
    }
    // The manifest, with `used` as the rule here has it.
    let kept: BTreeSet<&str> = entries
        .iter()
        .filter(|e| e.meets_rule)
        .map(|e| e.name.as_str())
        .collect();
    if let Some(areas) = value.get_mut("areas").and_then(|a| a.as_array_mut()) {
        for area in areas {
            let name = area
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let used = area.get("used").and_then(|u| u.as_bool()).unwrap_or(false);
            area["used"] = serde_json::Value::Bool(used && kept.contains(name.as_str()));
        }
    }
    value["as_built_survey"] = serde_json::to_value(&entries)?;
    fs::write(out, serde_json::to_string_pretty(&value)?)?;
    let dropped: Vec<&str> = entries
        .iter()
        .filter(|e| !e.meets_rule)
        .map(|e| e.name.as_str())
        .collect();
    Ok(format!(
        "{} of {} areas kept, in {out}; out: {dropped:?}",
        kept.len(),
        entries.len()
    ))
}

#[derive(Serialize)]
struct HybridLine {
    origin: Origin,
    versions: Versions,
    rules_id: String,
    design_mode: &'static str,
    over_ceiling: bool,
    family: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    map: Option<String>,
    design_seed: u64,
    node: String,
    seed: u64,
    profile: String,
    installation: Installation,
    class: Option<CauseClass>,
    component: Option<String>,
    /// The truth's home, by its parts' and in-home cable's names, where it's in one.
    in_home: Vec<String>,
    floor_top: Vec<Vec<String>>,
    floor_classes: Vec<Option<CauseClass>>,
    floor_posterior: Vec<f64>,
    floor_sims: usize,
    fault: Option<Fault>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fiber_fault: Option<FiberFault>,
    calibration_x: Vec<f64>,
    proposal_top: Vec<String>,
    proposal_probability: Vec<f64>,
    hybrid_top: Vec<Vec<String>>,
    hybrid_classes: Vec<Option<CauseClass>>,
    hybrid_posterior: Vec<f64>,
    hybrid_sims: usize,
    /// Every place that's in a home on this node, so a reader can tell inside from outside.
    home_places: Vec<String>,
}

/// Every place in a home on the node: its parts and in-home cable, and the devices.
fn home_places(graph: &PlantGraph<'_>, plant: &Plant) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for edge in &plant.edges {
        if !matches!(edge.kind, schema::plant::EdgeKind::Drop { .. }) {
            continue;
        }
        if let Some(home) = graph.premises_of(&edge.to.element_id).ok().flatten() {
            if home.elements.iter().all(|e| {
                matches!(
                    e.kind,
                    schema::plant::ElementKind::Modem | schema::plant::ElementKind::Cpe
                )
            }) && home.house_edges.is_empty()
            {
                continue;
            }
            out.extend(home.elements.iter().map(|e| e.id.to_string()));
            out.extend(home.house_edges.iter().map(|e| e.id.to_string()));
        }
    }
    out
}

fn one_hybrid(
    job: &AbJob,
    library: &ComponentLibrary,
    world: Option<&ComponentLibrary>,
    model: &proposal::Proposer,
    mode: Mode,
) -> Result<String, Error> {
    let s = seen(job, library, world)?;
    let region = &job.region;
    let plant = &region.plants[job.node];
    let config = &s.config;
    let floor = if mode == Mode::Floor {
        Some(diagnose(
            &s.graph,
            &s.reference,
            &s.observed,
            &s.at_reading,
            config,
        )?)
    } else {
        None
    };
    let rows = component_rows(&s.graph, &s.observed);
    let proposals = model.propose(&s.graph, plant, &s.observed, &rows)?;
    let prior: Vec<(String, f64)> = proposals
        .iter()
        .take(HYBRID_TOP)
        .map(|p| (p.component.clone(), p.probability))
        .collect();
    let answer = if mode == Mode::ProposalOnly {
        None
    } else {
        Some(diagnosis::engine::diagnose_with_prior(
            &s.graph,
            &s.reference,
            &s.observed,
            &s.at_reading,
            config,
            &prior,
        )?)
    };
    let calibration_x = answer
        .as_ref()
        .and_then(|a| diagnosis::engine::top_answer_summary(a, s.observed.modems.len()))
        .map_or([0.0; proposal::CALIBRATION_FEATURES], |top| {
            proposal::calibration_features(
                top.posterior,
                top.chi2_per_dof,
                proposals
                    .iter()
                    .find(|p| p.component == top.component)
                    .map_or(0.0, |p| p.probability),
                top.component.is_empty(),
                top.affected_share,
            )
        });
    let places = home_places(&s.graph, plant);
    let in_home = match (&s.fault, s.class) {
        (Some(f), Some(CauseClass::InHomeWiring)) => {
            let at = fault_element(&f.kind).unwrap_or_default();
            let edge = s.graph.edge(&schema::common::EdgeId::new(&at)).ok();
            let element = edge
                .map(|e| e.to.element_id.clone())
                .unwrap_or_else(|| schema::common::ElementId::new(&at));
            match s.graph.premises_of(&element).ok().flatten() {
                Some(home) => home
                    .elements
                    .iter()
                    .map(|e| e.id.to_string())
                    .chain(home.house_edges.iter().map(|e| e.id.to_string()))
                    .collect(),
                None => Vec::new(),
            }
        }
        _ => Vec::new(),
    };
    let line = HybridLine {
        origin: Origin::Synthetic,
        versions: versions(),
        rules_id: region.rules_id.clone(),
        design_mode: region.mode,
        over_ceiling: region.over.contains(&plant.service_group),
        family: region.family.clone(),
        map: region.map.clone(),
        design_seed: region.design_seed,
        node: plant.service_group.to_string(),
        seed: job.row_seed,
        profile: region.profile.id.to_string(),
        installation: installation_of(plant),
        class: s.class,
        component: s.component.clone(),
        in_home,
        floor_top: floor
            .iter()
            .flat_map(|f| &f.hypotheses)
            .map(|h| h.components.clone())
            .collect(),
        floor_classes: floor
            .iter()
            .flat_map(|f| &f.hypotheses)
            .map(|h| h.class)
            .collect(),
        floor_posterior: floor
            .iter()
            .flat_map(|f| &f.hypotheses)
            .map(|h| h.posterior)
            .collect(),
        floor_sims: floor.as_ref().map_or(0, |f| f.simulations),
        fault: s.fault.clone(),
        fiber_fault: s.fiber_fault.clone(),
        calibration_x: calibration_x.to_vec(),
        proposal_top: proposals
            .iter()
            .take(HYBRID_TOP)
            .map(|p| p.component.clone())
            .collect(),
        proposal_probability: proposals
            .iter()
            .take(HYBRID_TOP)
            .map(|p| p.probability)
            .collect(),
        hybrid_top: answer
            .iter()
            .flat_map(|a| &a.hypotheses)
            .map(|h| h.components.clone())
            .collect(),
        hybrid_classes: answer
            .iter()
            .flat_map(|a| &a.hypotheses)
            .map(|h| h.class)
            .collect(),
        hybrid_posterior: answer
            .iter()
            .flat_map(|a| &a.hypotheses)
            .map(|h| h.posterior)
            .collect(),
        hybrid_sims: answer.as_ref().map_or(0, |a| a.simulations),
        home_places: places.into_iter().collect(),
    };
    Ok(serde_json::to_string(&line)?)
}

/// `datagen ab-hybrid`: the whole diagnoser on the maps of one split, on a share of rows; with an estimate and an end, in a
/// world where that estimate is at that end and the diagnoser still believes it nominal (task 15's worlds).
pub fn hybrid(args: &[String]) -> Result<String, Error> {
    let [
        out,
        model,
        manifest,
        dir,
        split,
        rows_per_node,
        share,
        seed,
        with_floor,
        moved @ ..,
    ] = args
    else {
        return Err("usage: datagen ab-hybrid <out.jsonl> <model.json> <manifest.json> <osm dir> <split> <rows per node> <share> <seed> <floor|nofloor|proposal> [full] [estimate low|high]".into());
    };
    let model = proposal::Proposer::from_json(&fs::read_to_string(model)?)?;
    let full = moved.first().is_some_and(|m| m == "full");
    let moved = if full { &moved[1..] } else { moved };
    let inputs = inputs_on(full)?;
    let jobs = map_jobs(
        &inputs,
        manifest,
        dir,
        split,
        rows_per_node.parse()?,
        share.parse()?,
        seed.parse()?,
    )?;
    let jobs: Vec<&AbJob> = jobs.iter().filter(|j| j.run_floor).collect();
    let mode = Mode::parse(with_floor)?;
    let world = match moved {
        [] => None,
        [index, end] => Some(
            crate::sensitivity::move_estimate(
                &inputs.library,
                &inputs.profiles[0],
                index.parse()?,
                end,
            )?
            .1,
        ),
        _ => return Err("an estimate takes an index and low or high".into()),
    };
    let total = write_sharded(&jobs, out, shard_jobs(), |job| {
        one_hybrid(job, &inputs.library, world.as_ref(), &model, mode)
    })?;
    Ok(format!("{total} synthetic runs in {out}"))
}

#[derive(Serialize)]
struct FiberLine {
    origin: Origin,
    family: String,
    design_seed: u64,
    profile: String,
    design_mode: &'static str,
    nodes: usize,
    /// `cut_shared`, `loss`, `drift` or `coax`.
    case: &'static str,
    truth: String,
    /// How many nodes the truth's place carries.
    carries: usize,
    verdict: String,
    place: Option<String>,
    /// Elements between the named place and the truth along the affected links, where both are on them.
    hops_off: Option<usize>,
}

/// `datagen ab-fiber`: `diagnosis::fiber` on generated regions of six nodes or more, in both modes. A cut on a cable two
/// or more nodes share, named within one element; and a coax fault under one node, which shouldn't be called fiber.
pub fn fiber(args: &[String]) -> Result<String, Error> {
    let [out, designs, seed] = args else {
        return Err("usage: datagen ab-fiber <out.jsonl> <designs> <seed>".into());
    };
    let inputs = inputs()?;
    let (designs, seed): (u64, u64) = (designs.parse()?, seed.parse()?);
    let mut work = Vec::new();
    for family in [LayoutFamily::UrbanGrid, LayoutFamily::Suburban] {
        for profile in &inputs.profiles {
            for d in 0..designs {
                work.push((family, Arc::clone(profile), d));
            }
        }
    }
    let lines: Vec<Vec<String>> = in_parallel(&work, |(family, profile, d)| {
        let design_seed = seed * 1000 + d;
        let map = design::generate(*family, design_seed, 1600);
        let upgraded = d % 2 == 1;
        let built = design_map(&inputs, &map, profile, design_seed, upgraded)?;
        if built.nodes.len() < 6 {
            return Ok(Vec::new());
        }
        let plants: Vec<Plant> = built.nodes.iter().map(|n| n.plant.clone()).collect();
        let pairs: Vec<(&Plant, &ChannelPlan)> =
            plants.iter().map(|p| (p, &profile.channel_plan)).collect();
        let readings =
            |faults: &[FiberFault]| -> Result<BTreeMap<ServiceGroupId, LinkReading>, Error> {
                Ok(
                    region_links(&built.fiber, &pairs, &inputs.library, faults, Timestamp(0))?
                        .iter()
                        .map(|(sg, l)| (sg.clone(), LinkReading::of(l)))
                        .collect(),
                )
            };
        // Which nodes each outside cable carries.
        let mut carries: BTreeMap<String, BTreeSet<ServiceGroupId>> = BTreeMap::new();
        for link in &built.fiber.links {
            for hop in &link.hops {
                if let schema::fiber::Hop::Fiber { cable, .. } = hop {
                    carries
                        .entry(cable.to_string())
                        .or_default()
                        .insert(link.service_group.clone());
                }
            }
        }
        let mut out = Vec::new();
        let mut rng = Rng::derive(seed, &[design_seed, hash_str(profile.id.as_str())]);
        let line = |case: &'static str,
                    truth: String,
                    n: usize,
                    answer: &diagnosis::fiber::FiberAnswer| {
            let (verdict, place) = match &answer.verdict {
                FiberVerdict::Healthy => ("Healthy".to_string(), None),
                FiberVerdict::NotFiber { .. } => ("NotFiber".to_string(), None),
                FiberVerdict::Fiber { place, .. } => ("Fiber".to_string(), Some(place.clone())),
            };
            let hops_off = place
                .as_ref()
                .and_then(|p| hops_between(&built.fiber, p, &truth));
            FiberLine {
                origin: Origin::Synthetic,
                family: format!("{family:?}"),
                design_seed,
                profile: profile.id.to_string(),
                design_mode: mode_name(upgraded),
                nodes: plants.len(),
                case,
                truth,
                carries: n,
                verdict,
                place,
                hops_off,
            }
        };
        // Cuts on shared cables: each such cable once.
        for (cable, behind) in carries
            .iter()
            .filter(|(c, g)| c.starts_with("fiber-") && g.len() >= 2)
        {
            let len = built
                .fiber
                .cables
                .iter()
                .find(|c| c.id.as_str() == cable)
                .map_or(0.0, |c| c.len_ft);
            let cut = FiberFault {
                onset: schema::fault::Onset::Instant,
                kind: schema::fiber_fault::FiberFaultKind::FiberCut {
                    cable: schema::common::FiberCableId::new(cable),
                    pos_ft: rng.uniform() * len,
                    repair_s: None,
                },
            };
            let answer = diagnose_fiber(&built.fiber, &pairs, &inputs.library, &readings(&[cut])?)?;
            out.push(serde_json::to_string(&line(
                "cut_shared",
                cable.clone(),
                behind.len(),
                &answer,
            ))?);
        }
        // A coax fault under one node leaves every link as designed.
        for plant in plants.iter().take(4) {
            let graph =
                PlantGraph::new(plant, &inputs.library, profile).map_err(|e| format!("{e:?}"))?;
            let classes: Vec<CauseClass> =
                CLASSES.iter().copied().chain(AS_BUILT_CLASSES).collect();
            let class = classes[rng.index(classes.len())];
            let Some(fault) = sample_fault_as_built(&graph, class, &mut rng) else {
                continue;
            };
            // The fiber reads only the links; the coax fault is in the node's plant, so its links are the healthy ones.
            let _ = &fault;
            let answer = diagnose_fiber(&built.fiber, &pairs, &inputs.library, &readings(&[])?)?;
            out.push(serde_json::to_string(&line(
                "coax",
                fault_element(&fault.kind).unwrap_or_default(),
                1,
                &answer,
            ))?);
        }
        Ok(out)
    })?;
    let all: Vec<String> = lines.into_iter().flatten().collect();
    fs::write(out, all.join("\n") + "\n")?;
    Ok(format!("{} synthetic fiber cases in {out}", all.len()))
}

/// Elements between two named places along any one link that passes both, or `None` where no link passes both. A cable
/// and the closure at its end are one apart.
fn hops_between(fiber: &FiberPlant, a: &str, b: &str) -> Option<usize> {
    use schema::fiber::Hop;
    let name = |hop: &Hop| match hop {
        Hop::Fiber { cable, .. } => cable.to_string(),
        Hop::Splice { at, .. } | Hop::Connector { at, .. } => at.to_string(),
        Hop::Through { element, .. } => element.to_string(),
    };
    fiber
        .links
        .iter()
        .filter_map(|link| {
            let names: Vec<String> = link.hops.iter().map(name).collect();
            let i = names.iter().position(|n| n == a)?;
            let j = names.iter().position(|n| n == b)?;
            Some(i.abs_diff(j))
        })
        .min()
}

/// `datagen ab-spectra`: task 17's capture rows on version 2 plant, without its stand-in for low modems.
pub fn spectra(args: &[String]) -> Result<String, Error> {
    let [out, manifest, dir, split, rows_per_node, seed] = args else {
        return Err("usage: datagen ab-spectra <out.jsonl> <manifest.json> <osm dir> <split> <rows per node> <seed>".into());
    };
    let inputs = inputs()?;
    let jobs = map_jobs(
        &inputs,
        manifest,
        dir,
        split,
        rows_per_node.parse()?,
        0.0,
        seed.parse()?,
    )?;
    let total = write_sharded(&jobs, out, shard_jobs(), |job| {
        crate::spectra::capture_rows(
            &job.region.plants[job.node],
            &job.region.profile,
            &inputs.library,
            job.row_seed,
            job.region.map.clone(),
            false,
        )
    })?;
    Ok(format!("{total} synthetic runs of captures in {out}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run stopped partway and started again writes the same bytes as one that ran straight through: whole shards are kept,
    /// the rest are redone, and every row is seeded from its own index (invariant 4).
    #[test]
    fn an_interrupted_and_resumed_run_is_byte_identical() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        std::env::set_current_dir(root).unwrap();
        let inputs = inputs().unwrap();
        let profile = Arc::clone(&inputs.profiles[0]);
        let map = design::generate(LayoutFamily::Suburban, 4, 200);
        let built = design_map(&inputs, &map, &profile, 4, false).unwrap();
        let issues: Vec<bool> = built.nodes.iter().map(|n| !n.issues.is_empty()).collect();
        let region =
            Arc::new(Region::new(("Suburban", None, 4), &profile, false, built, &inputs).unwrap());
        let mut jobs = Vec::new();
        region_jobs(&mut jobs, &region, &issues, 24, 0.0, 9);
        jobs.truncate(24);
        assert_eq!(jobs.len(), 24);
        let dir = std::env::temp_dir().join(format!("hfc-resume-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (straight, resumed) = (dir.join("straight.jsonl"), dir.join("resumed.jsonl"));
        let row = |j: &AbJob| one_row(j, &inputs.library);
        crate::rows::write_sharded_until(&jobs, straight.to_str().unwrap(), 4, row, &|| false)
            .unwrap();
        // Stopped after four jobs have started, so at most one of the six shards is whole.
        let started = AtomicUsize::new(0);
        let stop = || started.fetch_add(1, Ordering::Relaxed) >= 4;
        let first =
            crate::rows::write_sharded_until(&jobs, resumed.to_str().unwrap(), 4, row, &stop);
        assert!(first.is_err(), "it stopped");
        assert!(!resumed.exists());
        crate::rows::write_sharded_until(&jobs, resumed.to_str().unwrap(), 4, row, &|| false)
            .unwrap();
        assert_eq!(
            std::fs::read(&straight).unwrap(),
            std::fs::read(&resumed).unwrap()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Rows are deterministic: the same seed gives the same bytes, row by row and file by file (task 24).
    #[test]
    fn the_same_seed_gives_the_same_rows() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        std::env::set_current_dir(root).unwrap();
        let inputs = inputs().unwrap();
        let profile = Arc::clone(&inputs.profiles[1]);
        let map = design::generate(LayoutFamily::Suburban, 3, 200);
        let built = design_map(&inputs, &map, &profile, 3, false).unwrap();
        let issues: Vec<bool> = built.nodes.iter().map(|n| !n.issues.is_empty()).collect();
        let region =
            Arc::new(Region::new(("Suburban", None, 3), &profile, false, built, &inputs).unwrap());
        let mut jobs = Vec::new();
        region_jobs(&mut jobs, &region, &issues, 20, 0.0, 5);
        assert!(!jobs.is_empty());
        // On a high-split node every class comes round in twenty rows, the fiber's three included.
        let rows: Vec<String> = jobs
            .iter()
            .take(20)
            .map(|j| one_row(j, &inputs.library).unwrap())
            .collect();
        let again: Vec<String> = jobs
            .iter()
            .take(20)
            .map(|j| one_row(j, &inputs.library).unwrap())
            .collect();
        assert_eq!(rows, again);
        let classes: BTreeSet<String> = rows
            .iter()
            .map(|r| serde_json::from_str::<serde_json::Value>(r).unwrap()["class"].to_string())
            .collect();
        for fiber in [
            "\"FiberCut\"",
            "\"FiberEventLoss\"",
            "\"OpticalTransmitterDrift\"",
        ] {
            assert!(classes.contains(fiber), "{classes:?}");
        }
        let first: serde_json::Value = serde_json::from_str(&rows[0]).unwrap();
        assert_eq!(first["origin"], "Synthetic");
        assert_eq!(first["design_mode"], "Greenfield");
        assert_eq!(first["versions"]["plant"], schema::plant::SCHEMA_VERSION);
    }
}
