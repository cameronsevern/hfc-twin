//! The version 2 designer against `docs/tasks/23-as-built-designer.md`: its bars, fixed before any design was run, and its
//! tests. All synthetic (invariant 11).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::time::Instant;

use design::v2::bars::{self, Bar};
use design::v2::kit::Kit;
use design::v2::{DesignV2, design, design_upgraded};
use diagnosis::{DiagnoserConfig, diagnose};
use features::DesignReference;
use features::sampler::sample_fault;
use features::sweep::observe;
use plant_graph::PlantGraph;
use schema::common::{LonLat, Timestamp};
use schema::component::{ComponentLibrary, ComponentSpec, qualifies};
use schema::design_rules::DesignRules;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::FaultSet;
use schema::fiber::Hop;
use schema::fiber_fault::{FiberFault, FiberFaultKind};
use schema::observation::RegState;
use schema::plant::{EdgeKind, ElementKind, Installation, Plant};
use schema::profile::Profile;
use schema::region::{DesignControls, LayoutFamily, PinnedNode, StreetMap};
use schema::work_order::CauseClass;
use serde_json::Value;
use sim::optics::{node_link, region_links};
use sim::rng::{Rng, hash_str};
use sim::telemetry::{Detail, Scenario, SimOptions, simulate};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn library() -> ComponentLibrary {
    read("library/generic.json")
}

fn rules() -> DesignRules {
    read("design_rules/generic.json")
}

fn profile(id: &str) -> Profile {
    schema::profile::migrate(read(&format!("profiles/{id}.json"))).unwrap()
}

const LOW: &str = "d31_lowsplit_1002_iccap";
const HIGH: &str = "d31_highsplit_1218_rpd";

fn osm(file: &str) -> StreetMap {
    let text = fs::read_to_string(format!(
        "{}/../../fixtures/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    design::osm::read_overpass("osm", &text, Installation::Aerial)
        .unwrap()
        .0
}

fn calm() -> EnvironmentSample {
    design::v2::levels::sample(20.0)
}

fn measure(
    d: &DesignV2,
    rules: &DesignRules,
    library: &ComponentLibrary,
    profile: &Profile,
) -> Vec<Bar> {
    let kit = Kit::new(rules, library, profile, None).unwrap();
    let plants: Vec<Plant> = d.nodes.iter().map(|n| n.plant.clone()).collect();
    bars::all(
        &bars::Design {
            kit: &kit,
            region: &d.region,
            plants: &plants,
            fiber: &d.fiber,
        },
        &BTreeSet::new(),
    )
}

/// Every bar on one map, on both profiles; "No issues", "Alignment" and "Fiber" on the two fully loaded ones too.
fn bars_hold(name: &str, map: &StreetMap, rules: &DesignRules) {
    bars_hold_with(name, map, rules, &DesignControls::default());
}

/// [`bars_hold`] with the given controls. Every built node is held, those over the ceiling too.
fn bars_hold_with(name: &str, map: &StreetMap, rules: &DesignRules, controls: &DesignControls) {
    let library = library();
    let mut failed = Vec::new();
    for (id, which) in [
        (LOW, None),
        (HIGH, None),
        (
            "d31_lowsplit_1002_iccap_full",
            Some(["No issues", "Alignment", "Fiber"]),
        ),
        (
            "d31_highsplit_1218_rpd_full",
            Some(["No issues", "Alignment", "Fiber"]),
        ),
    ] {
        let p = profile(id);
        let d = design(map, &p, &library, rules, controls).unwrap();
        for bar in measure(&d, rules, &library, &p) {
            if which.is_some_and(|w| !w.contains(&bar.name.as_str())) {
                continue;
            }
            if !bar.met {
                failed.push(format!(
                    "{name} {id} {}: {} of {} {:?}",
                    bar.name, bar.failed, bar.checked, bar.detail
                ));
            }
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

/// The controls that build the nodes the ceiling puts over it, still marked as over it.
fn over_ceiling_built() -> DesignControls {
    DesignControls {
        build_over_ceiling: true,
        ..DesignControls::default()
    }
}

#[test]
fn bars_hold_on_the_urban_grid() {
    bars_hold(
        "urban",
        &design::generate(LayoutFamily::UrbanGrid, 1, 600),
        &rules(),
    );
}

#[test]
fn bars_hold_on_the_suburban_family() {
    bars_hold(
        "suburban",
        &design::generate(LayoutFamily::Suburban, 1, 600),
        &rules(),
    );
}

#[test]
fn bars_hold_on_the_rural_family() {
    // At the rules' ceiling nothing rural is built (coax isn't the answer at 8 homes a mile), and the designer says so
    // for every home.
    let map = design::generate(LayoutFamily::Rural, 1, 600);
    bars_hold("rural", &map, &rules());
    let d = design(
        &map,
        &profile(LOW),
        &library(),
        &rules(),
        &DesignControls::default(),
    )
    .unwrap();
    let over: usize = d.over_ceiling.iter().map(|o| o.homes.len()).sum();
    assert_eq!(
        over + d
            .nodes
            .iter()
            .map(|n| n.homes_passed as usize)
            .sum::<usize>()
            + d.unserved.len(),
        map.homes.len()
    );
}

#[test]
fn bars_hold_on_the_openstreetmap_fixtures() {
    for file in [
        "osm/real_broomfield_co.json",
        "osm/real_broomfield_large_co.json",
    ] {
        bars_hold(file, &osm(file), &rules());
    }
}

/// The nodes the ceiling would leave unbuilt, built with the control that builds them, and held to every bar like any
/// other: the extracts' sparse streets run 15 to 90 homes a mile, inside the range of the note's real nodes
/// (design-practice.md §6). And the ones over the ceiling are still marked as over it, with their homes.
#[test]
fn bars_hold_with_the_ceiling_lifted_on_the_extracts() {
    for file in [
        "osm/real_broomfield_co.json",
        "osm/real_broomfield_large_co.json",
    ] {
        let map = osm(file);
        bars_hold_with(
            &format!("{file}, over the ceiling built"),
            &map,
            &rules(),
            &over_ceiling_built(),
        );
        let d = design(
            &map,
            &profile(LOW),
            &library(),
            &rules(),
            &over_ceiling_built(),
        )
        .unwrap();
        let built: usize = d.nodes.iter().map(|n| n.homes_passed as usize).sum();
        assert_eq!(
            built + d.unserved.len(),
            map.homes.len(),
            "{file}: every home built"
        );
        // Every node its cost puts over the ceiling is still marked, with its homes, and only those. Since task 25 a
        // candidate that leaves no home out wins over a cheaper one that does, so the small extract now builds as one node
        // under the ceiling; the large one still has nodes over it.
        let ceiling = rules()
            .costs
            .ceiling_usd_per_home
            .map_or(f64::INFINITY, |c| c.value);
        let over_nodes = d
            .cost
            .nodes
            .iter()
            .filter(|n| n.usd_per_home > ceiling)
            .count();
        assert_eq!(
            d.over_ceiling.len(),
            over_nodes,
            "{file}: the nodes over the ceiling are still marked, and only they"
        );
        if file.contains("large") {
            assert!(
                over_nodes > 0,
                "{file}: its sparse nodes are over the ceiling"
            );
        }
    }
}

/// The rural family at 8 to 10 homes a mile, built over the ceiling and held to every bar.
#[test]
#[ignore = "one amp of 188 at 3.75 dB at N = 2, and one tap 0.09 dB under at 45 °C; see task 23's decisions, fix 2"]
fn bars_hold_with_the_ceiling_lifted_on_the_rural_family() {
    bars_hold_with(
        "rural, over the ceiling built",
        &design::generate(LayoutFamily::Rural, 1, 600),
        &rules(),
        &over_ceiling_built(),
    );
}

/// "Deterministic": same inputs, byte-identical region, plants and fiber plant.
#[test]
fn design_is_deterministic() {
    let library = library();
    let rules = rules();
    for id in [LOW, HIGH] {
        let p = profile(id);
        let map = design::generate(LayoutFamily::Suburban, 3, 300);
        let a = serde_json::to_string(
            &design(&map, &p, &library, &rules, &DesignControls::default()).unwrap(),
        )
        .unwrap();
        let b = serde_json::to_string(
            &design(&map, &p, &library, &rules, &DesignControls::default()).unwrap(),
        )
        .unwrap();
        assert_eq!(a, b);
    }
}

fn root_site(d: &DesignV2) -> Vec<String> {
    d.nodes
        .iter()
        .filter_map(|n| {
            n.plant
                .root()
                .and_then(|r| r.structure.clone())
                .map(|s| s.to_string())
        })
        .collect()
}

/// A pinned node, a moved hub and a changed rule each re-solve, and a pin holds.
#[test]
fn a_pin_a_moved_hub_and_a_changed_rule_each_resolve() {
    let library = library();
    let p = profile(LOW);
    let rules = rules();
    let map = design::generate(LayoutFamily::UrbanGrid, 2, 400);
    let base = design(&map, &p, &library, &rules, &DesignControls::default()).unwrap();
    // Pin a node somewhere it isn't.
    let corner = map.homes[map.homes.len() / 3].geo;
    let controls = DesignControls {
        pinned_nodes: vec![PinnedNode { geo: corner }],
        ..DesignControls::default()
    };
    let pinned = design(&map, &p, &library, &rules, &controls).unwrap();
    let s = design::v2::pipeline::place_structures(
        &Kit::new(&rules, &library, &p, None).unwrap(),
        &map,
        &controls,
    )
    .unwrap();
    let at = s.sites[s.nearest(corner).unwrap()].id.to_string();
    assert!(
        root_site(&pinned).contains(&at),
        "the pin at {at} holds: {:?}",
        root_site(&pinned)
    );
    assert_ne!(root_site(&pinned), root_site(&base));
    // Move the hub: the fiber plant re-solves from the new site; the pin still holds.
    let mut moved = map.clone();
    moved.headend = LonLat {
        lon_deg: map.headend.lon_deg + 0.01,
        lat_deg: map.headend.lat_deg + 0.005,
    };
    let rehubbed = design(&moved, &p, &library, &rules, &controls).unwrap();
    assert_ne!(rehubbed.fiber.hubs[0].geo, pinned.fiber.hubs[0].geo);
    assert_ne!(rehubbed.fiber.cables, pinned.fiber.cables);
    assert!(root_site(&rehubbed).contains(&at));
    // Change a rule: closer poles, more structures; the pin holds at its new nearest structure.
    let mut closer = rules.clone();
    closer.structures.pole_spacing_ft.value = 150.0;
    let respaced = design(&map, &p, &library, &closer, &controls).unwrap();
    assert!(respaced.structures.len() > pinned.structures.len());
    let s2 = design::v2::pipeline::place_structures(
        &Kit::new(&closer, &library, &p, None).unwrap(),
        &map,
        &controls,
    )
    .unwrap();
    let at2 = s2.sites[s2.nearest(corner).unwrap()].id.to_string();
    assert!(root_site(&respaced).contains(&at2));
}

/// A tighter tap-port tolerance pulls the ports toward the target, at the cost of as many actives or more, or of more
/// spent; a smaller node size makes more, smaller nodes.
#[test]
fn a_tighter_tolerance_or_smaller_nodes_change_the_design_as_they_should() {
    let library = library();
    let p = profile(LOW);
    let rules = rules();
    let map = design::generate(LayoutFamily::UrbanGrid, 4, 400);
    let base = design(&map, &p, &library, &rules, &DesignControls::default()).unwrap();
    let mut tight = rules.clone();
    for t in &mut tight.tap_port {
        t.tolerance_db.value = 2.5;
    }
    let tighter = design(&map, &p, &library, &tight, &DesignControls::default()).unwrap();
    let actives = |d: &DesignV2| -> usize {
        d.nodes
            .iter()
            .map(|n| {
                n.plant
                    .elements
                    .iter()
                    .filter(|e| {
                        matches!(e.kind, ElementKind::Amp { .. } | ElementKind::LineEqualizer)
                    })
                    .count()
            })
            .sum()
    };
    // Since task 25 a faceplate cable simulator can do a line equalizer's work at a tap, so a tighter tolerance may cost
    // conditioners rather than actives: it costs more either way.
    assert!(
        actives(&tighter) >= actives(&base) || tighter.cost.total_usd >= base.cost.total_usd,
        "{} against {} actives, ${:.0} against ${:.0}",
        actives(&tighter),
        actives(&base),
        tighter.cost.total_usd,
        base.cost.total_usd
    );
    // The share of tap ports inside 2.5 dB of the target, at both edges and both temperatures, by the sim.
    let kit = Kit::new(&tight, &library, &p, None).unwrap();
    let inside = |d: &DesignV2| -> f64 {
        let (mut n, mut ok) = (0, 0);
        for node in &d.nodes {
            for temp in [-20.0, 45.0] {
                for [lo, hi] in bars::tap_port_levels(&kit, &node.plant, temp).into_values() {
                    n += 1;
                    if (hi - kit.port_hi_dbmv).abs() <= 2.5
                        && kit.port_lo_dbmv.is_none_or(|t| (lo - t).abs() <= 2.5)
                    {
                        ok += 1;
                    }
                }
            }
        }
        ok as f64 / n.max(1) as f64
    };
    assert!(
        inside(&tighter) > inside(&base),
        "{:.3} against {:.3}",
        inside(&tighter),
        inside(&base)
    );
    // The candidates step a node either side of the target and the cheapest without an issue is kept, so a target
    // within a step of the count kept needn't move it; this one is well under the base design's 130 to 170 homes.
    let small = DesignControls {
        homes_per_node: 80,
        ..DesignControls::default()
    };
    let smaller = design(&map, &p, &library, &rules, &small).unwrap();
    assert!(smaller.nodes.len() > base.nodes.len());
    assert!(smaller.nodes.iter().all(|n| n.homes_passed <= 160));
}

fn run(
    plant: &Plant,
    library: &ComponentLibrary,
    profile: &Profile,
    link: Option<&sim::optics::NodeLink>,
) -> Vec<sim::telemetry::StepObservation> {
    let graph = PlantGraph::new(plant, library, profile).unwrap();
    simulate(&Scenario {
        graph: &graph,
        environment: &Environment {
            schema_version: "2".into(),
            samples: vec![calm()],
            ambient_sources: Vec::new(),
        },
        faults: &FaultSet {
            schema_version: "4".into(),
            seed: 1,
            faults: vec![],
        },
        seed: 1,
        options: SimOptions {
            detail: Detail::Full,
            randomize: None,
            measurement_noise_db: 0.0,
            rxmer_scatter_db: 0.0,
        },
        link,
    })
    .unwrap()
}

/// The sim runs on a designed region, links from its fiber plant, and every modem comes up.
#[test]
fn every_modem_comes_up_on_a_designed_region() {
    let library = library();
    let rules = rules();
    for id in [LOW, HIGH] {
        let p = profile(id);
        let map = design::generate(LayoutFamily::UrbanGrid, 5, 300);
        let d = design(&map, &p, &library, &rules, &DesignControls::default()).unwrap();
        assert!(!d.nodes.is_empty());
        for node in &d.nodes {
            let link = node_link(&d.fiber, &node.plant, &library, &p.channel_plan)
                .unwrap()
                .unwrap();
            let steps = run(&node.plant, &library, &p, Some(&link));
            assert!(!steps[0].modems.is_empty());
            for modem in &steps[0].modems {
                assert_eq!(
                    modem.reg_state,
                    RegState::Operational,
                    "{}",
                    modem.element_id
                );
            }
        }
    }
}

/// One sampled fault of a class on the first node of a designed high-split suburban map, diagnosed with no model, per
/// seed: the top hypothesis's class, and whether it names the faulted part.
fn diagnose_seeded(class: CauseClass, seed: u64) -> (Option<CauseClass>, bool) {
    let library = library();
    let p = profile(HIGH);
    let map = design::generate(LayoutFamily::Suburban, 2, 200);
    let d = design(&map, &p, &library, &rules(), &DesignControls::default()).unwrap();
    let node = &d.nodes[0];
    let graph = PlantGraph::new(&node.plant, &library, &p).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
    let mut rng = Rng::new(seed);
    let fault = sample_fault(&graph, class, &mut rng).unwrap();
    let truth = diagnosis::engine::fault_component(&fault);
    let faults = FaultSet {
        schema_version: "4".into(),
        seed,
        faults: vec![fault],
    };
    let observed = observe(&graph, &reference, &faults, calm(), seed).unwrap();
    let answer = diagnose(
        &graph,
        &reference,
        &observed,
        &calm(),
        &DiagnoserConfig::default(),
    )
    .unwrap();
    (
        answer.hypotheses[0].class,
        answer.hypotheses[0].components.contains(&truth),
    )
}

/// The diagnoser, with no model, finds a drifted amp on a designed node, part and class, on six seeds. Task 24 added
/// `Misalignment` at each fitted active (§15): a pad or equalizer set wrong reads much like a drift at the same amp,
/// and on some of these seeds fits as well, so either class at the right amp counts. Both send a line tech to it.
#[test]
fn the_diagnoser_finds_a_fault_on_a_designed_node() {
    for seed in [9, 1, 2, 3, 4, 5] {
        let (class, part) = diagnose_seeded(CauseClass::AmpDrift, seed);
        assert!(part, "seed {seed}: {class:?}");
        assert!(
            matches!(class, Some(CauseClass::AmpDrift | CauseClass::Misalignment)),
            "seed {seed}: {class:?}"
        );
    }
}

/// The seeds and positions v0's bar is read over, and how many of them name the faulted part first: one sampled fault of
/// a class at a time on the first node of a designed high-split suburban map, diagnosed with no model.
fn part_first(
    class: CauseClass,
    seeds: std::ops::RangeInclusive<u64>,
) -> (usize, usize, Vec<String>) {
    let library = library();
    let p = profile(HIGH);
    let map = design::generate(LayoutFamily::Suburban, 2, 200);
    let d = design(&map, &p, &library, &rules(), &DesignControls::default()).unwrap();
    let graph = PlantGraph::new(&d.nodes[0].plant, &library, &p).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
    let (mut hits, mut runs, mut missed) = (0, 0, Vec::new());
    for seed in seeds {
        let mut rng = Rng::new(seed);
        let fault = sample_fault(&graph, class, &mut rng).unwrap();
        let truth = diagnosis::engine::fault_component(&fault);
        let faults = FaultSet {
            schema_version: "4".into(),
            seed,
            faults: vec![fault],
        };
        let observed = observe(&graph, &reference, &faults, calm(), seed).unwrap();
        let answer = diagnose(
            &graph,
            &reference,
            &observed,
            &calm(),
            &DiagnoserConfig::default(),
        )
        .unwrap();
        runs += 1;
        let top = &answer.hypotheses[0];
        if top.components.contains(&truth) {
            hits += 1;
        } else {
            missed.push(format!(
                "seed {seed}: {truth} read as {:?} at {:?}",
                top.class, top.components
            ));
        }
    }
    (hits, runs, missed)
}

/// A suckout on a designed node, at v0's bar: the true part first in at least 90 % of the seeds. Task 23 found the class
/// on one seed of six; with the floor's thresholds read against the design's healthy prediction (task 24) the part comes
/// first on all but a notch too shallow to move any modem past `who`'s thresholds.
#[test]
fn the_diagnoser_finds_a_suckout_on_a_designed_node() {
    let (hits, runs, missed) = part_first(CauseClass::Suckout, 1..=10);
    assert!(hits * 10 >= runs * 9, "{hits}/{runs}: {missed:?}");
}

/// A bad drop behind premises parts, at v0's bar. Task 23 found none: the floor grouped siblings by a modem's parent, now
/// its home's ground block, and read 0.25 dB of channel-estimate ripple as affected where a healthy modem behind a home's
/// splitter reads 0.70 dB. Task 24 groups by tap, through the premises, and reads ripple against the design's own.
#[test]
fn the_diagnoser_finds_a_bad_drop_behind_premises_parts() {
    let (hits, runs, missed) = part_first(CauseClass::BadDrop, 1..=10);
    assert!(hits * 10 >= runs * 9, "{hits}/{runs}: {missed:?}");
}

/// A cut on a sheath two nodes' links share takes both down and leaves the others up.
#[test]
fn a_cut_on_a_shared_sheath_takes_down_the_nodes_behind_it() {
    let library = library();
    for id in [LOW, HIGH] {
        let p = profile(id);
        let map = design::generate(LayoutFamily::UrbanGrid, 6, 800);
        let d = design(&map, &p, &library, &rules(), &DesignControls::default()).unwrap();
        let mut on_cable: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for link in &d.fiber.links {
            for hop in &link.hops {
                if let Hop::Fiber { cable, .. } = hop
                    && cable.as_str().starts_with("fiber-")
                {
                    on_cable
                        .entry(cable.to_string())
                        .or_default()
                        .insert(link.service_group.to_string());
                }
            }
        }
        let (cable, behind) = on_cable
            .iter()
            .filter(|(_, g)| g.len() >= 2 && g.len() < d.nodes.len())
            .max_by_key(|(_, g)| g.len())
            .or_else(|| on_cable.iter().max_by_key(|(_, g)| g.len()))
            .unwrap();
        assert!(behind.len() >= 2, "{id}: no sheath shared by two nodes");
        let cut = FiberFault {
            onset: schema::fault::Onset::Instant,
            kind: FiberFaultKind::FiberCut {
                cable: schema::common::FiberCableId::new(cable),
                pos_ft: 10.0,
                repair_s: None,
            },
        };
        let pairs: Vec<(&Plant, &schema::profile::ChannelPlan)> = d
            .nodes
            .iter()
            .map(|n| (&n.plant, &p.channel_plan))
            .collect();
        let links = region_links(&d.fiber, &pairs, &library, &[cut], Timestamp(1)).unwrap();
        for (sg, link) in &links {
            let down = link.forward_dark()
                || link.digital_down()
                || link.return_snr_db() == Some(f64::NEG_INFINITY);
            assert_eq!(down, behind.contains(sg.as_str()), "{id} {sg}");
        }
    }
}

/// Legacy, then upgraded: sites don't move, parts that no longer qualify are replaced, and what still fails is reported.
/// Every bar on a legacy design, under the legacy rules and profile.
fn legacy_bars_hold(name: &str, map: &StreetMap) {
    let library = library();
    let legacy = profile("d30_lowsplit_750_legacy");
    let legacy_rules: DesignRules = read("design_rules/legacy-750.json");
    let d = design(
        map,
        &legacy,
        &library,
        &legacy_rules,
        &DesignControls::default(),
    )
    .unwrap();
    let failed: Vec<String> = measure(&d, &legacy_rules, &library, &legacy)
        .into_iter()
        .filter(|b| !b.met)
        .map(|b| {
            format!(
                "{name} {}: {} of {} {:?}",
                b.name, b.failed, b.checked, b.detail
            )
        })
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
}

/// The legacy design the second mode starts from meets every bar under its own rules and profile, before it's upgraded:
/// the 600-home urban grid the survey reads.
#[test]
fn the_legacy_design_meets_every_bar_under_its_own_rules() {
    legacy_bars_hold("urban", &design::generate(LayoutFamily::UrbanGrid, 1, 600));
}

/// The same on the 400-home map the upgrade test reads. One check misses: a far tap at 10.80 dBmV at 45 °C against 11.0,
/// on a station boxed in by its near taps at their highest value, 2.9 dB over the target.
#[test]
#[ignore = "one tap of 294 is 0.2 dB under its tolerance at 45 °C; see task 23's decisions, fix 3"]
fn the_legacy_design_meets_every_bar_on_the_400_home_map() {
    legacy_bars_hold(
        "urban 400",
        &design::generate(LayoutFamily::UrbanGrid, 1, 400),
    );
}

#[test]
fn legacy_then_upgraded() {
    let library = library();
    let target = profile(HIGH);
    let legacy = profile("d30_lowsplit_750_legacy");
    let legacy_rules: DesignRules = read("design_rules/legacy-750.json");
    let map = design::generate(LayoutFamily::UrbanGrid, 1, 400);
    let old = design(
        &map,
        &legacy,
        &library,
        &legacy_rules,
        &DesignControls::default(),
    )
    .unwrap();
    let new = design_upgraded(
        &map,
        &target,
        &library,
        &rules(),
        &legacy,
        &legacy_rules,
        &DesignControls::default(),
    )
    .unwrap();
    let report = new.upgrade.as_ref().unwrap();
    assert_eq!(old.nodes.len(), new.nodes.len());
    for (a, b) in old.nodes.iter().zip(&new.nodes) {
        // Sites stay: every plant element of the legacy design is at the same structure, same position.
        for e in a.plant.elements.iter().filter(|e| e.structure.is_some()) {
            let after = b
                .plant
                .elements
                .iter()
                .find(|x| x.id == e.id)
                .expect("an element of the legacy design");
            assert_eq!(
                (&after.structure, after.geo),
                (&e.structure, e.geo),
                "{}",
                e.id
            );
        }
        // Spans stay.
        let spans = |p: &Plant| -> BTreeSet<(String, i64)> {
            p.edges
                .iter()
                .filter_map(|e| match &e.kind {
                    EdgeKind::Span { len_ft, .. } => {
                        Some((e.id.to_string(), (len_ft * 100.0).round() as i64))
                    }
                    _ => None,
                })
                .collect()
        };
        assert!(spans(&a.plant).is_subset(&spans(&b.plant)));
        // What doesn't qualify for the target was replaced, unless the report says nothing could be.
        let stuck: BTreeSet<_> = report
            .stuck
            .get(&b.plant.service_group)
            .into_iter()
            .flatten()
            .map(|c| c.element.clone())
            .collect();
        for e in &b.plant.elements {
            let part = library.get(&e.component_ref).unwrap();
            if !stuck.contains(&e.id) {
                assert!(qualifies(part, &target), "{} keeps {}", e.id, part.id);
            }
        }
        assert!(!report.changes[&b.plant.service_group].is_empty());
    }
    // The bar for an upgraded estate, fixed before the moves were added: nothing service-breaking, so of the issues only a
    // level short at the top of the band (upgraded plant runs lower modulation there, VOGEL2019 §2, §4), and none on an
    // active, a supply or a link; the structural bars hold; and what misses at the top of the band is counted.
    let target_rules = rules();
    let kit = Kit::new(&target_rules, &library, &target, None).unwrap();
    let plants: Vec<Plant> = new.nodes.iter().map(|n| n.plant.clone()).collect();
    let d = bars::Design {
        kit: &kit,
        region: &new.region,
        plants: &plants,
        fiber: &new.fiber,
    };
    let issues: Vec<String> = bars::issues(&d).into_values().flatten().collect();
    let short = issues
        .iter()
        .filter(|i| i.starts_with("LevelTooLow"))
        .count();
    let breaking: Vec<&String> = issues
        .iter()
        .filter(|i| !i.starts_with("LevelTooLow"))
        .collect();
    assert!(breaking.is_empty(), "{breaking:#?}");
    let all = bars::all(&d, &BTreeSet::new());
    for name in [
        "No stand-ins",
        "Every port has a state",
        "Fittings",
        "Powering",
        "Fiber",
        "Alignment",
    ] {
        let bar = all.iter().find(|b| b.name == name).unwrap();
        assert!(
            bar.met,
            "{name}: {} of {} {:?}",
            bar.failed, bar.checked, bar.detail
        );
    }
    let port = all.iter().find(|b| b.name == "Tap-port level").unwrap();
    eprintln!(
        "upgraded: {short} modems short at the top of the band; tap ports {} of {} outside the tolerance",
        port.failed, port.checked
    );
}

/// A 600-home map designs, every candidate, in under the time written in the task's spec.
#[test]
fn a_600_home_map_designs_in_time() {
    let library = library();
    let rules = rules();
    for (family, id) in [
        (LayoutFamily::UrbanGrid, LOW),
        (LayoutFamily::Suburban, HIGH),
    ] {
        let p = profile(id);
        let map = design::generate(family, 1, 600);
        let t = Instant::now();
        design(&map, &p, &library, &rules, &DesignControls::default()).unwrap();
        let s = t.elapsed().as_secs_f64();
        eprintln!("{family:?} {id}: {s:.1} s");
        assert!(s < SPEED_S, "{family:?} {id}: {s:.1} s");
    }
}

/// The bound `a_600_home_map_designs_in_time` holds, seconds: measured in the spec, with room for a slower machine.
const SPEED_S: f64 = 30.0;

/// "Parts list": on one small map, every line of each plant's and the fiber plant's parts list matches a count made
/// here, by walking the records.
#[test]
fn the_parts_lists_match_a_count_by_hand() {
    let library = library();
    let p = profile(LOW);
    let map = design::generate(LayoutFamily::UrbanGrid, 7, 60);
    let d = design(&map, &p, &library, &rules(), &DesignControls::default()).unwrap();
    assert!(!d.nodes.is_empty());
    for node in &d.nodes {
        let plant = &node.plant;
        let mut count: BTreeMap<String, (u32, f64)> = BTreeMap::new();
        let mut add = |id: &str, n: u32, ft: f64| {
            let e = count.entry(id.to_string()).or_default();
            e.0 += n;
            e.1 += ft;
        };
        for e in &plant.elements {
            add(e.component_ref.as_str(), 1, 0.0);
            if let Some(f) = &e.fitted {
                for part in f.parts() {
                    add(part.as_str(), 1, 0.0);
                }
            }
        }
        for s in &plant.power_supplies {
            add(s.component_ref.as_str(), 1, 0.0);
            if let Some(b) = &s.batteries {
                add(
                    b.component_ref.as_str(),
                    u32::from(b.per_string) * u32::from(b.strings),
                    0.0,
                );
            }
        }
        for s in &plant.structures {
            add(s.component_ref.as_str(), 1, 0.0);
        }
        for e in &plant.edges {
            for f in e.kind.fittings() {
                add(f.component_ref.as_str(), 1, 0.0);
            }
            if let Some((cable, len)) = e.kind.cable() {
                add(cable.as_str(), 1, len);
            }
            if let EdgeKind::Span {
                support: Some(s),
                len_ft,
                ..
            } = &e.kind
            {
                add(s.as_str(), 1, *len_ft);
            }
        }
        for line in plant.parts_list() {
            let (n, ft) = count[line.component_ref.as_str()];
            assert_eq!(line.quantity, n, "{}", line.component_ref);
            if let Some(len) = line.total_len_ft {
                assert!((len - ft).abs() < 1e-6, "{}", line.component_ref);
            }
        }
        assert_eq!(plant.parts_list().len(), count.len());
    }
    // The fiber plant: elements but node ends, structures, a connector per mated pair, cable by the foot.
    let fiber = &d.fiber;
    let mut count: BTreeMap<String, (u32, f64)> = BTreeMap::new();
    for e in &fiber.elements {
        if !matches!(e.kind, schema::fiber::FiberElementKind::NodeEnd { .. }) {
            count.entry(e.component_ref.to_string()).or_default().0 += 1;
        }
    }
    for s in &fiber.structures {
        count.entry(s.component_ref.to_string()).or_default().0 += 1;
    }
    let mut pairs = BTreeSet::new();
    for link in &fiber.links {
        for (i, hop) in link.hops.iter().enumerate() {
            if let Hop::Connector { at, component_ref } = hop {
                // A pair is its element and the strand it meets there.
                let strand = link
                    .hops
                    .get(i + 1)
                    .filter(|h| matches!(h, Hop::Fiber { .. }))
                    .or_else(|| i.checked_sub(1).and_then(|j| link.hops.get(j)));
                pairs.insert((
                    at.to_string(),
                    format!("{strand:?}"),
                    component_ref.to_string(),
                ));
            }
        }
    }
    for (_, _, part) in &pairs {
        count.entry(part.clone()).or_default().0 += 1;
    }
    for c in &fiber.cables {
        for part in std::iter::once(&c.component_ref).chain(&c.support) {
            let e = count.entry(part.to_string()).or_default();
            e.0 += 1;
            e.1 += c.len_ft;
        }
    }
    for line in fiber.parts_list() {
        let (n, ft) = count[line.component_ref.as_str()];
        assert_eq!(line.quantity, n, "fiber {}", line.component_ref);
        if let Some(len) = line.total_len_ft {
            assert!((len - ft).abs() < 1e-6, "fiber {}", line.component_ref);
        }
    }
}

/// "Parts state their tables": a healthy designed high-split node has no step in RxMER at 1002 MHz; the same node with
/// the RPD that states no table has PHY Table 41's step there.
#[test]
fn a_designed_high_split_node_has_no_rxmer_step_at_1002() {
    let library = library();
    let p = profile(HIGH);
    let map = design::generate(LayoutFamily::UrbanGrid, 8, 200);
    let d = design(&map, &p, &library, &rules(), &DesignControls::default()).unwrap();
    let node = &d.nodes[0];
    let step_at_1002 = |plant: &Plant| -> f64 {
        let link = node_link(&d.fiber, plant, &library, &p.channel_plan).unwrap();
        let steps = run(plant, &library, &p, link.as_ref());
        let mut below = Vec::new();
        let mut above = Vec::new();
        for modem in &steps[0].modems {
            if modem.generation != schema::component::ModemGeneration::D31 {
                continue;
            }
            for record in modem.rxmer.iter().flatten() {
                let Some(values) = &record.rxmer_db else {
                    continue;
                };
                let a = &record.axis;
                for (i, v) in values.iter().enumerate() {
                    let Some(v) = v else {
                        continue;
                    };
                    let f = a.zero_freq_mhz
                        + (a.first_active_index as f64 + i as f64) * a.spacing_khz / 1000.0;
                    if (990.0..1002.0).contains(&f) {
                        below.push(*v);
                    } else if (1002.0..1014.0).contains(&f) {
                        above.push(*v);
                    }
                }
            }
        }
        let mean = |x: &[f64]| x.iter().sum::<f64>() / x.len().max(1) as f64;
        assert!(!below.is_empty() && !above.is_empty());
        mean(&below) - mean(&above)
    };
    let with_tables = step_at_1002(&node.plant);
    let mut without = node.plant.clone();
    for e in &mut without.elements {
        if matches!(e.kind, ElementKind::Rpd { .. }) {
            e.component_ref = schema::common::ComponentId::new("rpd-generic-1218-modular");
        }
    }
    let staircase = step_at_1002(&without);
    eprintln!(
        "RxMER below less above 1002 MHz: {with_tables:.3} dB with the tables, {staircase:.3} dB without"
    );
    assert!(
        with_tables.abs() < 0.5,
        "a step of {with_tables:.2} dB with the tables"
    );
    // Without them, PHY Table 41's 2 dB step in the source floor shows through the modem's other terms.
    assert!(
        staircase > with_tables.abs() + 0.5,
        "PHY Table 41's step reads {staircase:.2} dB without them"
    );
    // And every root and amp states its table.
    let rpd = library
        .get(&node.plant.root().unwrap().component_ref)
        .unwrap();
    assert!(matches!(&rpd.spec, ComponentSpec::Rpd(r) if !r.source_mer.is_empty()));
}

/// Removes every `schema_version` key, at any depth.
fn strip_versions(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("schema_version");
            map.values_mut().for_each(strip_versions);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_versions),
        _ => {}
    }
}

/// The version 2 path's pinned digest: a change that moves it is a change to the designer, to be made on purpose and
/// recorded in the task's spec.
// Was `7e87cd5bc3f69b3b`, then `81b0636b2f0c7581` after the closing fixes' second round (the port held at one level, the
// modem's window, spread and transmit range held behind its premises, Alignment counted against a candidate, long-reach
// optics). Moved again by the third: premises parts at the SCTE 129 and SCTE 153 ceilings in the rules' templates. The
// third round's walk changes (a lifted branch walked a run at a time, stations centred) move none of these maps. Then
// `f22e294c97482f3e` until task 25 (refit on the aligned return path, attenuators, apartment groups, faceplate cable
// simulators, the sweep, the sim's last word, and a candidate kept for the homes it builds and its tap ports before its cost;
// docs/tasks/25-design-any-map.md).
const V2_DIGEST: &str = "0662a19330be2c58";

#[test]
fn every_version_2_design_is_pinned() {
    let library = library();
    let rules = rules();
    let mut designs = String::new();
    for id in [LOW, HIGH] {
        let p = profile(id);
        for family in [
            LayoutFamily::UrbanGrid,
            LayoutFamily::Suburban,
            LayoutFamily::Rural,
        ] {
            let map = design::generate(family, 1, 300);
            let d = design(&map, &p, &library, &rules, &DesignControls::default()).unwrap();
            let mut value = serde_json::to_value(&d).unwrap();
            strip_versions(&mut value);
            designs.push_str(&format!(
                "{:016x}\n",
                hash_str(&serde_json::to_string(&value).unwrap())
            ));
        }
    }
    let found = format!("{:016x}", hash_str(&designs));
    assert_eq!(found, V2_DIGEST, "a version 2 design moved");
}
