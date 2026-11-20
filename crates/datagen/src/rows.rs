//! Labeled rows from generated plant, by family and profile. Each row also carries its plant as a graph, for models that read one.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use design::{DesignParts, design_region, generate};
use diagnosis::{DiagnoserConfig, diagnose};
use features::DesignReference;
use features::sampler::{CLASSES, RPD_CLASSES, class_of, fault_element, sample_fault};
use features::sweep::{observe, random_sample};
use plant_graph::PlantGraph;
use proposal::{ComponentRow, GraphNode, component_rows, graph_nodes};
use schema::common::{Origin, Timestamp};
use schema::component::ComponentLibrary;
use schema::fault::{AmpState, Fault, FaultKind, FaultSet};
use schema::plant::{Installation, Plant};
use schema::profile::{NodeKind, Profile};
use schema::region::{DesignControls, LayoutFamily};
use schema::work_order::CauseClass;
use serde::Serialize;
use sim::rng::{Rng, hash_str};

pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;

pub(crate) fn read<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, Error> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

#[derive(Serialize)]
struct FloorAnswer {
    components: Vec<Vec<String>>,
    classes: Vec<Option<CauseClass>>,
    posterior: Vec<f64>,
    simulations: usize,
}

#[derive(Serialize)]
struct Row {
    origin: Origin,
    observation_schema: &'static str,
    fault_schema: &'static str,
    family: String,
    /// The real street map this plant was invented on, if it's on one.
    #[serde(skip_serializing_if = "Option::is_none")]
    map: Option<String>,
    profile: String,
    installation: Installation,
    design_seed: u64,
    node: String,
    seed: u64,
    class: Option<CauseClass>,
    component: Option<String>,
    candidates: Vec<ComponentRow>,
    graph: Vec<GraphNode>,
    floor: Option<FloorAnswer>,
}

const FAMILIES: [LayoutFamily; 3] = [
    LayoutFamily::UrbanGrid,
    LayoutFamily::Suburban,
    LayoutFamily::Rural,
];

/// A family's name as rows write it. Rows on a real map say `Osm`.
fn family_name(family: LayoutFamily) -> String {
    format!("{family:?}")
}

pub(crate) struct Job {
    pub(crate) family: String,
    pub(crate) map: Option<String>,
    pub(crate) profile: Arc<Profile>,
    pub(crate) design_seed: u64,
    pub(crate) plant: Arc<Plant>,
    pub(crate) row_seed: u64,
    pub(crate) index: usize,
    pub(crate) run_floor: bool,
}

/// The rows for one designed node.
pub(crate) fn node_jobs(
    jobs: &mut Vec<Job>,
    source: (&str, Option<&str>, &str, u64),
    profile: &Arc<Profile>,
    node: &design::NodeDesign,
    rows_per_node: usize,
    floor_share: f64,
    seed: u64,
) {
    let (family, map, map_id, design_seed) = source;
    let plant = Arc::new(node.plant.clone());
    for r in 0..rows_per_node {
        let row_seed = Rng::derive(
            seed,
            &[
                hash_str(map_id),
                hash_str(profile.id.as_str()),
                hash_str(node.plant.service_group.as_str()),
                r as u64,
            ],
        )
        .next_u64();
        let run_floor = Rng::new(row_seed ^ 0x5eed).uniform() < floor_share;
        jobs.push(Job {
            family: family.to_string(),
            map: map.map(str::to_string),
            profile: Arc::clone(profile),
            design_seed,
            plant: Arc::clone(&plant),
            row_seed,
            index: r,
            run_floor,
        });
    }
}

fn build_jobs(
    designs: u64,
    rows_per_node: usize,
    floor_share: f64,
    seed: u64,
) -> Result<(Vec<Job>, ComponentLibrary), Error> {
    let library: ComponentLibrary = read("fixtures/library/generic.json")?;
    let profiles = both_profiles()?;

    let mut jobs = Vec::new();
    for family in FAMILIES {
        for profile in &profiles {
            for d in 0..designs {
                let design_seed = seed * 1000 + d;
                let homes = match family {
                    LayoutFamily::Rural => 100,
                    _ => 200,
                };
                let map = generate(family, design_seed, homes);
                let controls = DesignControls {
                    homes_per_node: 120,
                    seed: design_seed,
                    ..DesignControls::default()
                };
                let design =
                    design_region(&map, profile, &library, &controls, &DesignParts::default())?;
                for node in design
                    .nodes
                    .iter()
                    .filter(|n| n.physics_issues.is_empty() && n.topology_issues.is_empty())
                {
                    node_jobs(
                        &mut jobs,
                        (&family_name(family), None, &map.id, design_seed),
                        profile,
                        node,
                        rows_per_node,
                        floor_share,
                        seed,
                    );
                }
            }
        }
    }

    Ok((jobs, library))
}

pub(crate) fn both_profiles() -> Result<[Arc<Profile>; 2], Error> {
    Ok([
        Arc::new(read("fixtures/profiles/d31_lowsplit_1002_iccap.json")?),
        Arc::new(read("fixtures/profiles/d31_highsplit_1218_rpd.json")?),
    ])
}

/// Runs every job on every core and writes the lines in job order, so the same seed gives the same bytes however the threads ran.
pub(crate) fn write_lines<J: Sync>(
    jobs: &[J],
    out: &str,
    one: impl Fn(&J) -> Result<String, Error> + Sync,
) -> Result<usize, Error> {
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut results: Vec<(usize, String)> = Vec::with_capacity(jobs.len());
    std::thread::scope(|scope| -> Result<(), Error> {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| -> Result<Vec<(usize, String)>, Error> {
                    let mut local = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(at) else {
                            return Ok(local);
                        };
                        local.push((at, one(job)?));
                    }
                })
            })
            .collect();
        for handle in handles {
            results.extend(handle.join().map_err(|_| "a worker panicked")??);
        }
        Ok(())
    })?;
    results.sort_by_key(|r| r.0);
    let mut writer = BufWriter::new(File::create(out)?);
    for (_, line) in &results {
        writeln!(writer, "{line}")?;
    }
    Ok(results.len())
}

pub fn run(args: &[String]) -> Result<String, Error> {
    let [out, designs, rows_per_node, floor_share, seed] = args else {
        return Err("usage: datagen rows <out.jsonl> <designs per family> <rows per node> <floor share> <seed>".into());
    };
    let (jobs, library) = build_jobs(
        designs.parse()?,
        rows_per_node.parse()?,
        floor_share.parse()?,
        seed.parse()?,
    )?;
    let total = write_lines(&jobs, out, |job| one_row(job, &library))?;
    Ok(format!("{total} synthetic rows in {out}"))
}

/// One labeled row.
pub(crate) fn one_row(job: &Job, library: &ComponentLibrary) -> Result<String, Error> {
    one_row_in(job, library, library, false)
}

/// Moves a fault past everything `sample_fault` draws, for a stress test training never sees. Physical parameters only, so what it does to
/// the telemetry is still computed (invariant 5). The ranges are judgement, not sourced: "really bad", not a survey of the field.
fn harsher(fault: &mut Fault, seed: u64) {
    let mut rng = Rng::derive(seed, &[0x6861_7264]);
    match &mut fault.kind {
        FaultKind::WaterIngress {
            k_w_db_per_100ft, ..
        } => *k_w_db_per_100ft = rng.range(6.0, 20.0),
        FaultKind::ConnectorReflection { return_loss_db, .. } => {
            *return_loss_db = rng.range(0.5, 4.0);
        }
        FaultKind::Suckout {
            depth_db,
            width_mhz,
            ..
        } => {
            *depth_db = rng.range(30.0, 60.0);
            *width_mhz = rng.range(15.0, 60.0);
        }
        FaultKind::FrequencySelectiveLoss { depth_db, .. } => *depth_db = rng.range(40.0, 70.0),
        FaultKind::AmpDrift {
            state: AmpState::Drift { gain_delta_db, .. },
            ..
        } => *gain_delta_db = rng.range(-25.0, -9.0),
        FaultKind::UpstreamIngress { power_dbmv, .. } => *power_dbmv = rng.range(30.0, 50.0),
        FaultKind::DownstreamIngress {
            power_dbmv, bw_mhz, ..
        } => {
            *power_dbmv = rng.range(35.0, 55.0);
            *bw_mhz = rng.range(20.0, 60.0);
        }
        FaultKind::PowerSupplySag { v_delta, .. } => *v_delta = rng.range(-60.0, -35.0),
        FaultKind::BadDrop { extra_loss_db, .. } => *extra_loss_db = rng.range(20.0, 40.0),
        _ => {}
    }
}

/// A labeled row from a world whose parts differ from what the diagnoser believes. `world` is the parts as they really are, and
/// `library` what the row's drifts are measured against.
pub(crate) fn one_row_in(
    job: &Job,
    library: &ComponentLibrary,
    world: &ComponentLibrary,
    severe: bool,
) -> Result<String, Error> {
    let graph = PlantGraph::new(&job.plant, library, &job.profile).map_err(|e| format!("{e:?}"))?;
    let world_graph =
        PlantGraph::new(&job.plant, world, &job.profile).map_err(|e| format!("{e:?}"))?;
    let reference = DesignReference::compute(&graph)?;
    let mut rng = Rng::new(job.row_seed);
    let daa = job.profile.node_kind == NodeKind::Rpd;
    let classes: Vec<CauseClass> = CLASSES
        .iter()
        .copied()
        .chain(RPD_CLASSES.iter().copied().filter(|_| daa))
        .collect();
    let mut fault = if job.index % 10 == 9 {
        None
    } else {
        sample_fault(&graph, classes[job.index % classes.len()], &mut rng)
    };
    if severe && let Some(fault) = &mut fault {
        harsher(fault, job.row_seed);
    }
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.to_string(),
        seed: job.row_seed,
        faults: fault.iter().cloned().collect(),
    };
    let mut sample = random_sample(&mut rng, 0);
    sample.t = Timestamp(features::sampler::reading_t(fault.as_ref(), &mut rng));
    let observed = observe(
        &world_graph,
        &reference,
        &faults,
        sample.clone(),
        job.row_seed,
    )?;
    let candidates = component_rows(&graph, &observed);
    let floor = if job.run_floor {
        let mut at_reading = sample.clone();
        at_reading.t = Timestamp(sample.t.0 + 300);
        let answer = diagnose(
            &graph,
            &reference,
            &observed,
            &at_reading,
            &DiagnoserConfig::default(),
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
    let installation = job
        .plant
        .edges
        .iter()
        .find_map(|e| match e.kind {
            schema::plant::EdgeKind::Span { installation, .. } => Some(installation),
            _ => None,
        })
        .unwrap_or(Installation::Aerial);
    let row = Row {
        origin: Origin::Synthetic,
        observation_schema: schema::observation::SCHEMA_VERSION,
        fault_schema: schema::fault::SCHEMA_VERSION,
        family: job.family.clone(),
        map: job.map.clone(),
        profile: job.profile.id.to_string(),
        installation,
        design_seed: job.design_seed,
        node: job.plant.service_group.to_string(),
        seed: job.row_seed,
        class: fault.as_ref().map(|f| class_of(&f.kind)),
        component: fault.as_ref().and_then(|f| fault_element(&f.kind)),
        candidates,
        graph: graph_nodes(&graph, &job.plant, &observed),
        floor,
    };
    Ok(serde_json::to_string(&row)?)
}

#[derive(Serialize)]
struct HybridLine {
    origin: Origin,
    family: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    map: Option<String>,
    design_seed: u64,
    node: String,
    seed: u64,
    profile: String,
    class: Option<CauseClass>,
    component: Option<String>,
    floor_top: Vec<Vec<String>>,
    floor_posterior: Vec<f64>,
    floor_sims: usize,
    fault: Option<schema::fault::Fault>,
    calibration_x: Vec<f64>,
    /// What the model alone said, before the sim verified anything.
    proposal_top: Vec<String>,
    proposal_probability: Vec<f64>,
    hybrid_top: Vec<Vec<String>>,
    hybrid_classes: Vec<Option<CauseClass>>,
    hybrid_posterior: Vec<f64>,
    hybrid_sims: usize,
}

/// `datagen hybrid <out.jsonl> <model.json> <family> <designs> <rows per node> <floor share> <seed>`: on the same faults as `rows`,
/// the model's proposals as a prior with the sim verifying, beside the floor, on one held-out family.
pub fn hybrid(args: &[String]) -> Result<String, Error> {
    let [
        out,
        model,
        family,
        designs,
        rows_per_node,
        floor_share,
        seed,
        with_floor,
    ] = args
    else {
        return Err("usage: datagen hybrid <out.jsonl> <model.json> <family> <designs> <rows per node> <floor share> <seed> <floor|nofloor>".into());
    };
    let model = proposal::Proposer::from_json(&fs::read_to_string(model)?)?;
    let (jobs, library) = build_jobs(
        designs.parse()?,
        rows_per_node.parse()?,
        floor_share.parse()?,
        seed.parse()?,
    )?;
    let jobs: Vec<&Job> = jobs
        .iter()
        .filter(|j| j.family == *family && j.run_floor)
        .collect();
    let total = write_lines(&jobs, out, |job| {
        one_hybrid(job, &library, &library, &model, Mode::parse(with_floor)?)
    })?;
    Ok(format!("{total} synthetic runs in {out}"))
}

/// How much of the diagnoser a run does.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    /// The floor and the hybrid.
    Floor,
    /// The hybrid. Fitting the confidence needs no floor, and leaving it out halves the run.
    NoFloor,
    /// The model alone, with no sim verifying it.
    ProposalOnly,
}

impl Mode {
    pub(crate) fn parse(text: &str) -> Result<Mode, Error> {
        match text {
            "floor" => Ok(Mode::Floor),
            "nofloor" => Ok(Mode::NoFloor),
            "proposal" => Ok(Mode::ProposalOnly),
            other => Err(format!("{other} isn't floor, nofloor or proposal").into()),
        }
    }
}

/// One run of the whole diagnoser. `world` is the parts as they really are, and `library` what the diagnoser believes.
pub(crate) fn one_hybrid(
    job: &Job,
    library: &ComponentLibrary,
    world: &ComponentLibrary,
    model: &proposal::Proposer,
    mode: Mode,
) -> Result<String, Error> {
    let graph = PlantGraph::new(&job.plant, library, &job.profile).map_err(|e| format!("{e:?}"))?;
    let world_graph =
        PlantGraph::new(&job.plant, world, &job.profile).map_err(|e| format!("{e:?}"))?;
    let reference = DesignReference::compute(&graph)?;
    let mut rng = Rng::new(job.row_seed);
    let daa = job.profile.node_kind == NodeKind::Rpd;
    let classes: Vec<CauseClass> = CLASSES
        .iter()
        .copied()
        .chain(RPD_CLASSES.iter().copied().filter(|_| daa))
        .collect();
    let fault = if job.index % 10 == 9 {
        None
    } else {
        sample_fault(&graph, classes[job.index % classes.len()], &mut rng)
    };
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.to_string(),
        seed: job.row_seed,
        faults: fault.iter().cloned().collect(),
    };
    let mut sample = random_sample(&mut rng, 0);
    sample.t = Timestamp(features::sampler::reading_t(fault.as_ref(), &mut rng));
    let observed = observe(
        &world_graph,
        &reference,
        &faults,
        sample.clone(),
        job.row_seed,
    )?;
    let mut at_reading = sample.clone();
    at_reading.t = Timestamp(sample.t.0 + 300);
    let config = DiagnoserConfig::default();
    let floor = if mode == Mode::Floor {
        Some(diagnose(
            &graph,
            &reference,
            &observed,
            &at_reading,
            &config,
        )?)
    } else {
        None
    };
    let proposals = model.propose(
        &graph,
        &job.plant,
        &observed,
        &component_rows(&graph, &observed),
    )?;
    let prior: Vec<(String, f64)> = proposals
        .iter()
        .take(HYBRID_TOP)
        .map(|p| (p.component.clone(), p.probability))
        .collect();
    let answer = if mode == Mode::ProposalOnly {
        None
    } else {
        Some(diagnosis::engine::diagnose_with_prior(
            &graph,
            &reference,
            &observed,
            &at_reading,
            &config,
            &prior,
        )?)
    };
    let calibration_x = answer
        .as_ref()
        .and_then(|a| diagnosis::engine::top_answer_summary(a, observed.modems.len()))
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
    let line = HybridLine {
        origin: Origin::Synthetic,
        family: job.family.clone(),
        map: job.map.clone(),
        design_seed: job.design_seed,
        node: job.plant.service_group.to_string(),
        seed: job.row_seed,
        profile: job.profile.id.to_string(),
        class: fault.as_ref().map(|f| class_of(&f.kind)),
        component: fault.as_ref().and_then(|f| fault_element(&f.kind)),
        floor_top: floor
            .iter()
            .flat_map(|f| &f.hypotheses)
            .map(|h| h.components.clone())
            .collect(),
        floor_posterior: floor
            .iter()
            .flat_map(|f| &f.hypotheses)
            .map(|h| h.posterior)
            .collect(),
        floor_sims: floor.as_ref().map_or(0, |f| f.simulations),
        fault: fault.clone(),
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
    };
    Ok(serde_json::to_string(&line)?)
}

/// How many of the model's proposals the sim verifies.
const HYBRID_TOP: usize = 5;

/// [`write_lines`], resumable (task 24): jobs go in shards of `size`, each written whole under `<out>.shards-<size>/` as it
/// completes and skipped by a later run that finds it, and `out` is the shards in order once all are there. Every line
/// depends on its job alone, seeded from its index, so an interrupted and resumed run writes the same bytes as one that
/// ran straight through (invariant 4). With `HFC_STOP_FILE` naming a file that exists, no new job starts: the shards
/// already whole are kept, and the call returns an error saying how many.
pub(crate) fn write_sharded<J: Sync>(
    jobs: &[J],
    out: &str,
    size: usize,
    one: impl Fn(&J) -> Result<String, Error> + Sync,
) -> Result<usize, Error> {
    let stop_file = std::env::var("HFC_STOP_FILE").ok();
    let stop = || {
        stop_file
            .as_deref()
            .is_some_and(|f| std::path::Path::new(f).exists())
    };
    write_sharded_until(jobs, out, size, one, &stop)
}

/// [`write_sharded`], stopping when `stop` says so.
pub(crate) fn write_sharded_until<J: Sync>(
    jobs: &[J],
    out: &str,
    size: usize,
    one: impl Fn(&J) -> Result<String, Error> + Sync,
    stop: &(dyn Fn() -> bool + Sync),
) -> Result<usize, Error> {
    use std::sync::Mutex;
    let size = size.max(1);
    let dir = format!("{out}.shards-{size}");
    fs::create_dir_all(&dir)?;
    let shards = jobs.len().div_ceil(size);
    let path = |s: usize| format!("{dir}/{s:06}.jsonl");
    let done: Vec<bool> = (0..shards)
        .map(|s| std::path::Path::new(&path(s)).exists())
        .collect();
    let todo: Vec<usize> = (0..jobs.len()).filter(|i| !done[i / size]).collect();
    let pending: Mutex<BTreeMap<usize, Vec<Option<String>>>> = Mutex::new(BTreeMap::new());
    let next = AtomicUsize::new(0);
    let written = AtomicUsize::new(done.iter().filter(|d| **d).count());
    let started = std::time::Instant::now();
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| -> Result<(), Error> {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| -> Result<(), Error> {
                    loop {
                        if stop() {
                            return Ok(());
                        }
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&index) = todo.get(at) else {
                            return Ok(());
                        };
                        let line = one(&jobs[index])?;
                        let shard = index / size;
                        let whole = {
                            let mut map = pending.lock().map_err(|_| "a worker panicked")?;
                            let len = size.min(jobs.len() - shard * size);
                            let slot = map.entry(shard).or_insert_with(|| vec![None; len]);
                            slot[index - shard * size] = Some(line);
                            if slot.iter().all(Option::is_some) {
                                map.remove(&shard)
                            } else {
                                None
                            }
                        };
                        if let Some(lines) = whole {
                            let mut text = String::new();
                            for line in lines.into_iter().flatten() {
                                text.push_str(&line);
                                text.push('\n');
                            }
                            let partial = format!("{}.partial", path(shard));
                            fs::write(&partial, text)?;
                            fs::rename(&partial, path(shard))?;
                            let n = written.fetch_add(1, Ordering::Relaxed) + 1;
                            eprintln!(
                                "{out}: shard {shard} written, {n} of {shards} whole, {:.0} s",
                                started.elapsed().as_secs_f64()
                            );
                        }
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().map_err(|_| "a worker panicked")??;
        }
        Ok(())
    })?;
    let whole = written.load(Ordering::Relaxed);
    if whole < shards {
        return Err(
            format!("stopped: {whole} of {shards} shards of {out} whole, kept in {dir}").into(),
        );
    }
    let partial = format!("{out}.partial");
    {
        let mut writer = BufWriter::new(File::create(&partial)?);
        for s in 0..shards {
            writer.write_all(&fs::read(path(s))?)?;
        }
        writer.flush()?;
    }
    fs::rename(&partial, out)?;
    Ok(jobs.len())
}
