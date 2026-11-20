//! What the optimizer builds and what the sim reads on it healthy, as numbers a field design can be held against: tap values, port
//! fill, slack, amp spacing, supplies, modem levels, RxMER and upstream SNR. `cargo run --release -p design --example realism_survey`
//! uses the generic library, and `-- sourced` the parts read from data sheets. All synthetic. `docs/realism-review.md` quotes this output.
//!
//! Both paths (task 23): version 1's designer, the review's "before", and the version 2 designer under
//! `fixtures/design_rules/generic.json`, "after", with its bars and its cost. `-- v1` or `-- v2` reads one.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use schema::common::ComponentId;

use design::fiber::fiber_route;
use design::geo::dist_ft;
use design::osm::read_overpass;
use design::{DesignParts, RegionDesign, design_region, generate};
use plant_graph::PlantGraph;
use schema::common::Timestamp;
use schema::component::{ComponentLibrary, ComponentSpec};
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::FaultSet;
use schema::plant::{EdgeKind, ElementKind, Installation, Plant, PortRole};
use schema::profile::Profile;
use schema::region::{DesignControls, LayoutFamily, StreetMap};
use sim::Overlay;
use sim::budget::level_budget;
use sim::telemetry::{Detail, Scenario, SimOptions, simulate};

/// A span this much longer than the straight line between its ends is counted as slack. A right-angle corner is 1.41 times, so it
/// stays under.
const SLACK_RATIO: f64 = 1.5;
/// Five real node areas, (homes passed per mile, actives per mile), A to E by density [ULM2016 Table 3, via
/// `docs/reference/design-practice.md` §6]. Field data, a 2016 vendor study of legacy N+2 and N+3 areas.
const REAL_NODES: [(f64, f64); 5] = [
    (37.0, 5.0),
    (57.0, 4.9),
    (112.0, 5.9),
    (187.0, 7.6),
    (274.0, 7.4),
];
/// Nodes read through the sim per design. Enough to see the spread, and quick.
const NODES_SIMULATED: usize = 3;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).expect("fixture")).expect("parse")
}

fn at(temp_c: f64) -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(0),
        temp_c,
        ground_temp_c: temp_c,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

/// The value `share` of the way through the sorted list.
fn pct(values: &mut [f64], share: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * share).round() as usize]
}

#[derive(Default)]
struct Counts {
    tap_values: BTreeMap<i64, u32>,
    taps: u32,
    taps_without_drop: u32,
    tap_ports: u32,
    tap_ports_with_drop: u32,
    splitters: u32,
    home_splitters: u32,
    couplers: u32,
    line_eqs: u32,
    amps_into_splitter: u32,
    terminators: u32,
    supplies: u32,
    modems: u32,
    minimum_spans: u32,
    root_ports: Vec<f64>,
    slack_ft: f64,
    span_ft: Vec<f64>,
    between_actives_ft: Vec<f64>,
    drop_ft: Vec<f64>,
    level_low: Vec<f64>,
    level_high: Vec<f64>,
    tx_low: Vec<f64>,
    tx_high: Vec<f64>,
}

fn count(plants: &[&Plant], library: &ComponentLibrary, profile: &Profile) -> Counts {
    let mut c = Counts::default();
    for plant in plants {
        let plant = *plant;
        let graph = PlantGraph::new(plant, library, profile).expect("graph");
        c.supplies += plant.power_supplies.len() as u32;
        let used: BTreeSet<_> = plant
            .edges
            .iter()
            .map(|e| (&e.from.element_id, &e.from.port))
            .collect();
        for element in &plant.elements {
            match element.kind {
                ElementKind::Tap => {
                    c.taps += 1;
                    if let Some(ComponentSpec::Tap(tap)) =
                        graph.component(&element.component_ref).map(|x| &x.spec)
                    {
                        *c.tap_values.entry(tap.tap_value_db as i64).or_default() += 1;
                    }
                    let ports = element.ports.iter().filter(|p| p.role == PortRole::TapPort);
                    let mut with_drop = 0;
                    for port in ports {
                        c.tap_ports += 1;
                        if used.contains(&(&element.id, &port.id)) {
                            with_drop += 1;
                        }
                    }
                    c.tap_ports_with_drop += with_drop;
                    if with_drop == 0 {
                        c.taps_without_drop += 1;
                    }
                }
                ElementKind::Splitter => {
                    // In the home if a drop lies between it and the root.
                    let mut current = element.id.clone();
                    let mut in_home = false;
                    while let Ok(Some((parent, edge))) = graph.parent(&current) {
                        if matches!(edge.kind, EdgeKind::Drop { .. }) {
                            in_home = true;
                            break;
                        }
                        current = parent.id.clone();
                    }
                    if in_home {
                        c.home_splitters += 1;
                    } else {
                        c.splitters += 1;
                    }
                }
                ElementKind::DirectionalCoupler => c.couplers += 1,
                ElementKind::LineEqualizer => c.line_eqs += 1,
                ElementKind::Terminator => c.terminators += 1,
                ElementKind::Node { .. } | ElementKind::Rpd { .. } => {
                    c.root_ports.push(element.ports.len() as f64);
                }
                ElementKind::Modem => c.modems += 1,
                ElementKind::Amp { .. } => {
                    let children = graph.children(&element.id).expect("children");
                    if children
                        .iter()
                        .any(|k| matches!(k.kind, ElementKind::Splitter))
                    {
                        c.amps_into_splitter += 1;
                    }
                    // Hardline back to the active above.
                    let mut len_ft = 0.0;
                    let mut current = element.id.clone();
                    while let Ok(Some((parent, edge))) = graph.parent(&current) {
                        if let EdgeKind::Span { len_ft: span, .. } = edge.kind {
                            len_ft += span;
                        }
                        if !matches!(
                            parent.kind,
                            ElementKind::Tap
                                | ElementKind::Splitter
                                | ElementKind::DirectionalCoupler
                                | ElementKind::PowerInserter
                                | ElementKind::LineEqualizer
                        ) {
                            break;
                        }
                        current = parent.id.clone();
                    }
                    c.between_actives_ft.push(len_ft);
                }
                _ => {}
            }
        }
        for edge in &plant.edges {
            match edge.kind {
                EdgeKind::Span { len_ft, .. } => {
                    c.span_ft.push(len_ft);
                    if len_ft <= 3.0 {
                        c.minimum_spans += 1;
                    }
                    let ends = (
                        plant.element(&edge.from.element_id).and_then(|e| e.geo),
                        plant.element(&edge.to.element_id).and_then(|e| e.geo),
                    );
                    if let (Some(a), Some(b)) = ends {
                        let straight_ft = dist_ft(a, b);
                        if len_ft > straight_ft * SLACK_RATIO + 20.0 {
                            c.slack_ft += len_ft - straight_ft;
                        }
                    }
                }
                EdgeKind::Drop { len_ft, .. } => c.drop_ft.push(len_ft),
                EdgeKind::Direct | EdgeKind::House { .. } => {}
            }
        }
        for temp_c in [-20.0, 45.0] {
            let budget = level_budget(&graph, &at(temp_c), &Overlay::default()).expect("budget");
            for modem in budget.modems.values() {
                c.level_low.push(modem.demod_min_dbmv);
                c.level_high.push(modem.demod_max_dbmv);
                c.tx_low.push(modem.tx_used_min_dbmv_per_1p6mhz);
                c.tx_high.push(modem.tx_used_max_dbmv_per_1p6mhz);
            }
        }
    }
    c
}

/// Healthy telemetry on a few nodes, with §8's domain randomization on and no scatter.
fn healthy(
    plants: &[&Plant],
    fiber: Option<&schema::fiber::FiberPlant>,
    library: &ComponentLibrary,
    profile: &Profile,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let (mut rxmer_mean, mut rxmer_stddev, mut us_snr) = (Vec::new(), Vec::new(), Vec::new());
    let environment = Environment {
        schema_version: schema::environment::SCHEMA_VERSION.to_string(),
        samples: vec![EnvironmentSample {
            ground_temp_c: 15.0,
            ..at(20.0)
        }],
        ambient_sources: Vec::new(),
    };
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.to_string(),
        seed: 1,
        faults: Vec::new(),
    };
    for plant in plants.iter().take(NODES_SIMULATED) {
        let graph = PlantGraph::new(plant, library, profile).expect("graph");
        let link = fiber.and_then(|f| {
            sim::optics::node_link(f, plant, library, &profile.channel_plan)
                .ok()
                .flatten()
        });
        let steps = simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults: &faults,
            seed: 7,
            options: SimOptions {
                detail: Detail::Summary { stride: 8 },
                randomize: Some(11),
                measurement_noise_db: 0.0,
                rxmer_scatter_db: 0.0,
            },
            link: link.as_ref(),
        })
        .expect("sim");
        for step in &steps {
            for modem in &step.modems {
                for record in modem.rxmer.iter().flatten() {
                    rxmer_mean.extend(record.rxmer_mean_db);
                    rxmer_stddev.extend(record.rxmer_stddev_db);
                }
            }
            us_snr.extend(step.service_group.us_snr.iter().map(|s| s.us_snr_db));
        }
    }
    (rxmer_mean, rxmer_stddev, us_snr)
}

/// What a report reads, from either path.
struct View<'a> {
    plants: Vec<&'a Plant>,
    homes: Vec<u32>,
    issues: usize,
    unserved: usize,
    fiber: Option<&'a schema::fiber::FiberPlant>,
    v2: Option<&'a design::v2::DesignV2>,
}

fn v1_view(design: &RegionDesign) -> View<'_> {
    View {
        plants: design.nodes.iter().map(|n| &n.plant).collect(),
        homes: design.nodes.iter().map(|n| n.homes_passed).collect(),
        issues: design
            .nodes
            .iter()
            .map(|n| n.topology_issues.len() + n.physics_issues.len())
            .sum(),
        unserved: design.unserved.len(),
        fiber: None,
        v2: None,
    }
}

fn v2_view(design: &design::v2::DesignV2) -> View<'_> {
    View {
        plants: design.nodes.iter().map(|n| &n.plant).collect(),
        homes: design.nodes.iter().map(|n| n.homes_passed).collect(),
        issues: design.nodes.iter().map(|n| n.issues.len()).sum(),
        unserved: design.unserved.len(),
        fiber: Some(&design.fiber),
        v2: Some(design),
    }
}

fn report(
    name: &str,
    map: &StreetMap,
    view: &View<'_>,
    library: &ComponentLibrary,
    profile: &Profile,
) {
    let nodes = view.plants.len().max(1) as f64;
    let mut homes: Vec<f64> = view.homes.iter().map(|h| *h as f64).collect();
    let amps: u32 = view
        .plants
        .iter()
        .map(|p| {
            p.elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Amp { .. }))
                .count() as u32
        })
        .sum();
    let hardline_ft: f64 = view
        .plants
        .iter()
        .flat_map(|p| &p.edges)
        .map(|e| match e.kind {
            EdgeKind::Span { len_ft, .. } => len_ft,
            _ => 0.0,
        })
        .sum();
    let miles = hardline_ft / 5280.0;
    let issues = view.issues;
    let cascade = view
        .plants
        .iter()
        .map(|p| {
            let g = PlantGraph::new(p, library, profile).expect("graph");
            g.modems()
                .iter()
                .map(|m| g.cascade(&m.id).map_or(0, |c| c.len()))
                .max()
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0);
    let served: u32 = view.homes.iter().sum();
    println!(
        "\n== {name} on {}: {} homes, {} nodes of {:.0} to {:.0} homes (median {:.0}), N+{cascade}, {issues} issues, {} unserved",
        profile.id,
        map.homes.len(),
        view.plants.len(),
        pct(&mut homes, 0.0),
        pct(&mut homes, 1.0),
        pct(&mut homes, 0.5),
        view.unserved,
    );
    if let Some(d) = view.v2 {
        let over: usize = d.over_ceiling.iter().map(|o| o.homes.len()).sum();
        println!(
            "   version 2: {served} homes built, {over} over the ${:.0} ceiling; ${:.0} per home passed ({}); {} candidates, kept {}",
            rules_ceiling(),
            d.cost.usd_per_home,
            if d.cost.all_generic {
                "every price Generic"
            } else {
                "Generic prices but for sourced construction rates"
            },
            d.candidates.len(),
            d.candidates
                .iter()
                .find(|c| c.kept)
                .map_or("none".into(), |c| format!(
                    "{} at step {}",
                    c.express_cable, c.step
                )),
        );
    }
    println!(
        "   hardline {miles:.1} mi at {:.0} homes per mile; {amps} amps, {:.1} per node, {:.1} per mile",
        f64::from(served.max(1)) / miles.max(1e-9),
        f64::from(amps) / nodes,
        f64::from(amps) / miles.max(1e-9),
    );
    // The only outside check on a design's density: five real node areas, actives per mile against homes passed per
    // mile [ULM2016 Table 3, design-practice.md §6]. A read-out, not a bar.
    let homes_per_mile = f64::from(served.max(1)) / miles.max(1e-9);
    let (lo, hi) = REAL_NODES
        .windows(2)
        .find(|w| homes_per_mile >= w[0].0 && homes_per_mile <= w[1].0)
        .map_or_else(
            || {
                if homes_per_mile < REAL_NODES[0].0 {
                    (REAL_NODES[0], REAL_NODES[0])
                } else {
                    (REAL_NODES[4], REAL_NODES[4])
                }
            },
            |w| (w[0], w[1]),
        );
    println!(
        "   real nodes [ULM2016 Table 3], actives per mile at homes per mile: {}; this design {:.1} at {homes_per_mile:.0}, beside {:.1} at {:.0} and {:.1} at {:.0}",
        REAL_NODES
            .iter()
            .map(|(h, a)| format!("{a:.1} at {h:.0}"))
            .collect::<Vec<_>>()
            .join(", "),
        f64::from(amps) / miles.max(1e-9),
        lo.1,
        lo.0,
        hi.1,
        hi.0,
    );

    let mut c = count(&view.plants, library, profile);
    println!("   {} taps, by value in dB: {:?}", c.taps, c.tap_values);
    println!(
        "   {} tap ports, {:.0} % with a drop; {} taps with none; {} modems",
        c.tap_ports,
        100.0 * f64::from(c.tap_ports_with_drop) / f64::from(c.tap_ports.max(1)),
        c.taps_without_drop,
        c.modems,
    );
    println!(
        "   {} line splitters and {} couplers, {} splitters the first device after an amp; {} line equalizers; {} splitters in homes; {} terminators; root ports in use, median {:.0}",
        c.splitters,
        c.couplers,
        c.amps_into_splitter,
        c.line_eqs,
        c.home_splitters,
        c.terminators,
        pct(&mut c.root_ports, 0.5),
    );
    println!(
        "   {} supplies, {:.1} actives each",
        c.supplies,
        (f64::from(amps) + nodes) / f64::from(c.supplies.max(1)),
    );
    println!(
        "   spans: median {:.0} ft, longest {:.0} ft, {} at the 3 ft minimum; slack about {:.0} ft, {:.0} % of hardline",
        pct(&mut c.span_ft, 0.5),
        pct(&mut c.span_ft, 1.0),
        c.minimum_spans,
        c.slack_ft,
        100.0 * c.slack_ft / hardline_ft.max(1.0),
    );
    println!(
        "   hardline between actives: {:.0} to {:.0} ft, median {:.0}; drops median {:.0} ft, longest {:.0}",
        pct(&mut c.between_actives_ft, 0.1),
        pct(&mut c.between_actives_ft, 0.9),
        pct(&mut c.between_actives_ft, 0.5),
        pct(&mut c.drop_ft, 0.5),
        pct(&mut c.drop_ft, 1.0),
    );
    println!(
        "   modem input over -20 to 45 C, dBmV per 6 MHz: lowest in band {:.1} (5th pct) and {:.1} (median); highest {:.1} (median) and {:.1} (95th)",
        pct(&mut c.level_low, 0.05),
        pct(&mut c.level_low, 0.5),
        pct(&mut c.level_high, 0.5),
        pct(&mut c.level_high, 0.95),
    );
    println!(
        "   modem transmit, dBmV per 1.6 MHz: {:.1} (5th pct of lowest) to {:.1} (95th of highest)",
        pct(&mut c.tx_low, 0.05),
        pct(&mut c.tx_high, 0.95),
    );

    let (mut mean, mut stddev, mut us_snr) = healthy(&view.plants, view.fiber, library, profile);
    println!(
        "   healthy and randomized: RxMER mean {:.1} to {:.1} dB (5th to 95th), median {:.1}; within a channel, stddev median {:.2} dB; upstream SNR {:.1} to {:.1} dB",
        pct(&mut mean, 0.05),
        pct(&mut mean, 0.95),
        pct(&mut mean, 0.5),
        pct(&mut stddev, 0.5),
        pct(&mut us_snr, 0.0),
        pct(&mut us_snr, 1.0),
    );

    match view.fiber {
        Some(fiber) => {
            let sheath_ft: f64 = fiber
                .cables
                .iter()
                .filter(|c| c.id.as_str().starts_with("fiber-"))
                .map(|c| c.len_ft)
                .sum();
            let closures = fiber
                .elements
                .iter()
                .filter(|e| matches!(e.kind, schema::fiber::FiberElementKind::SpliceClosure))
                .count();
            println!(
                "   fiber: {:.1} sheath miles, {} cables, {closures} closures, {} links, every one checked by `sim::optics`",
                sheath_ft / 5280.0,
                fiber.cables.len(),
                fiber.links.len(),
            );
        }
        None => {
            let sites: Vec<_> = view
                .plants
                .iter()
                .filter_map(|p| {
                    let root = p.root()?;
                    Some((p.service_group.clone(), root.geo?))
                })
                .collect();
            let fiber = fiber_route(map, &sites);
            println!(
                "   fiber: {:.1} route miles to {} nodes, {} unreached; no cable, splice, optic or budget behind it",
                fiber.route_ft / 5280.0,
                sites.len(),
                fiber.unreached.len(),
            );
        }
    }
    if let Some(d) = view.v2 {
        let rules: schema::design_rules::DesignRules = read("design_rules/generic.json");
        let kit = design::v2::kit::Kit::new(&rules, library, profile, None).expect("kit");
        let plants: Vec<Plant> = view.plants.iter().map(|p| (*p).clone()).collect();
        let bars = design::v2::bars::all(
            &design::v2::bars::Design {
                kit: &kit,
                region: &d.region,
                plants: &plants,
                fiber: &d.fiber,
            },
            &Default::default(),
        );
        let failed: Vec<String> = bars
            .iter()
            .filter(|b| !b.met)
            .map(|b| format!("{} ({} of {})", b.name, b.failed, b.checked))
            .collect();
        println!(
            "   bars: {} of {} met{}",
            bars.iter().filter(|b| b.met).count(),
            bars.len(),
            if failed.is_empty() {
                String::new()
            } else {
                format!("; failed: {}", failed.join(", "))
            },
        );
    }
}

fn rules_ceiling() -> f64 {
    let rules: schema::design_rules::DesignRules = read("design_rules/generic.json");
    rules
        .costs
        .ceiling_usd_per_home
        .map_or(f64::INFINITY, |c| c.value)
}

/// §4.5: `MER_src` steps at 1002 MHz, inside the high-split profile's first OFDM channel. What a healthy modem on the hand-built
/// street reads either side of it.
fn source_step(library: &ComponentLibrary) {
    let profile: Profile = read("profiles/d31_highsplit_1218_rpd.json");
    let plant: Plant = read("one_street/plant_highsplit_rpd.json");
    let graph = PlantGraph::new(&plant, library, &profile).expect("graph");
    let environment = Environment {
        schema_version: schema::environment::SCHEMA_VERSION.to_string(),
        samples: vec![at(20.0)],
        ambient_sources: Vec::new(),
    };
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.to_string(),
        seed: 1,
        faults: Vec::new(),
    };
    let steps = simulate(&Scenario {
        graph: &graph,
        environment: &environment,
        faults: &faults,
        seed: 7,
        options: SimOptions::default(),
        link: None,
    })
    .expect("sim");
    println!(
        "\n== Healthy `one_street`, high-split, no randomization: RxMER either side of 1002 MHz"
    );
    for modem in steps.iter().flat_map(|step| &step.modems) {
        for record in modem.rxmer.iter().flatten() {
            let axis = &record.axis;
            let spacing_mhz = axis.spacing_khz / 1000.0;
            let first_mhz = axis.zero_freq_mhz + f64::from(axis.first_active_index) * spacing_mhz;
            // A missing value keeps its place, so an index still means a frequency.
            let values: Vec<f64> = record
                .rxmer_db
                .iter()
                .flatten()
                .map(|v| v.unwrap_or(f64::NAN))
                .collect();
            let read_at = |f_mhz: f64| {
                let index = ((f_mhz - first_mhz) / spacing_mhz).round();
                (index >= 0.0)
                    .then(|| values.get(index as usize).copied())
                    .flatten()
            };
            if let (Some(below), Some(above)) = (read_at(1001.9), read_at(1002.1)) {
                println!(
                    "   {} {}: {below:.1} dB at 1001.9 MHz, {above:.1} dB at 1002.1 MHz; channel stddev {:.2} dB",
                    modem.element_id,
                    record.channel_id,
                    record.rxmer_stddev_db.unwrap_or(f64::NAN),
                );
            } else {
                println!(
                    "   {} {}: 1002 MHz isn't in this channel; channel stddev {:.2} dB",
                    modem.element_id,
                    record.channel_id,
                    record.rxmer_stddev_db.unwrap_or(f64::NAN),
                );
            }
        }
    }
}

fn main() {
    let sourced = std::env::args().any(|arg| arg == "sourced");
    let (library, parts): (ComponentLibrary, DesignParts) = if sourced {
        // The same swap `tests/library.rs` makes: the parts with a data sheet behind them.
        let parts = DesignParts {
            cable: ComponentId::new("cable-commscope-p3-625"),
            amp: ComponentId::new("amp-commscope-ble120"),
            supply: ComponentId::new("psu-alpha-xm3-915hp"),
            ..DesignParts::default()
        };
        (read("library/sourced.json"), parts)
    } else {
        (read("library/generic.json"), DesignParts::default())
    };
    println!(
        "{} library. Every plant here is synthetic.",
        if sourced { "Sourced" } else { "Generic" }
    );
    let controls = DesignControls::default();
    let real = fs::read_to_string(format!(
        "{}/../../fixtures/osm/real_broomfield_large_co.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("extract");
    let (real_map, _) = read_overpass("broomfield", &real, Installation::Aerial).expect("osm");
    let which: Vec<String> = std::env::args()
        .filter(|a| a == "v1" || a == "v2")
        .collect();
    let v1 = which.is_empty() || which.iter().any(|a| a == "v1");
    let v2 = which.is_empty() || which.iter().any(|a| a == "v2");
    let rules: schema::design_rules::DesignRules = read("design_rules/generic.json");
    for profile_file in [
        "profiles/d31_lowsplit_1002_iccap.json",
        "profiles/d31_highsplit_1218_rpd.json",
    ] {
        let profile: Profile = schema::profile::migrate(read(profile_file)).expect("profile");
        let mut maps: Vec<(String, StreetMap)> = [
            LayoutFamily::UrbanGrid,
            LayoutFamily::Suburban,
            LayoutFamily::Rural,
        ]
        .into_iter()
        .map(|f| (format!("{f:?}"), generate(f, 1, 600)))
        .collect();
        maps.push((
            "OpenStreetMap extract, real streets and an invented plant".into(),
            real_map.clone(),
        ));
        for (name, map) in &maps {
            if v1 {
                let design =
                    design_region(map, &profile, &library, &controls, &parts).expect("design");
                report(
                    &format!("{name}, version 1"),
                    map,
                    &v1_view(&design),
                    &library,
                    &profile,
                );
            }
            if v2 && !sourced {
                let design =
                    design::v2::design(map, &profile, &library, &rules, &controls).expect("design");
                if design.nodes.is_empty() {
                    let over: usize = design.over_ceiling.iter().map(|o| o.homes.len()).sum();
                    println!(
                        "\n== {name}, version 2 on {}: nothing built; all {over} homes over the ${:.0} ceiling, coax at ${:.0} to ${:.0} a home",
                        profile.id,
                        rules_ceiling(),
                        design
                            .over_ceiling
                            .iter()
                            .map(|o| o.usd_per_home)
                            .fold(f64::INFINITY, f64::min),
                        design
                            .over_ceiling
                            .iter()
                            .map(|o| o.usd_per_home)
                            .fold(0.0, f64::max),
                    );
                    // What the designer would build there, held to the same bars: the nodes over the ceiling built,
                    // and still marked as over it.
                    let over_built = DesignControls {
                        build_over_ceiling: true,
                        ..controls.clone()
                    };
                    let design = design::v2::design(map, &profile, &library, &rules, &over_built)
                        .expect("design");
                    report(
                        &format!("{name}, version 2 built over the ceiling"),
                        map,
                        &v2_view(&design),
                        &library,
                        &profile,
                    );
                } else {
                    report(
                        &format!("{name}, version 2"),
                        map,
                        &v2_view(&design),
                        &library,
                        &profile,
                    );
                }
            }
        }
    }
    if !sourced {
        legacy_then_upgraded(&library, &rules, &controls);
    }
    source_step(&library);
}

/// The second mode on the urban grid: designed at 750 MHz under the legacy rules, then upgraded in place to high split.
fn legacy_then_upgraded(
    library: &ComponentLibrary,
    rules: &schema::design_rules::DesignRules,
    controls: &DesignControls,
) {
    let profile: Profile =
        schema::profile::migrate(read("profiles/d31_highsplit_1218_rpd.json")).expect("profile");
    let legacy_profile: Profile =
        schema::profile::migrate(read("profiles/d30_lowsplit_750_legacy.json")).expect("profile");
    let legacy_rules: schema::design_rules::DesignRules = read("design_rules/legacy-750.json");
    let map = generate(LayoutFamily::UrbanGrid, 1, 600);
    let design = design::v2::design_upgraded(
        &map,
        &profile,
        library,
        rules,
        &legacy_profile,
        &legacy_rules,
        controls,
    )
    .expect("design");
    if let Some(up) = &design.upgrade {
        let mut by_move: BTreeMap<String, usize> = BTreeMap::new();
        for change in up.changes.values().flatten() {
            *by_move
                .entry(change.by.map_or("none".into(), |m| format!("{m:?}")))
                .or_default() += 1;
        }
        let stuck: usize = up.stuck.values().map(Vec::len).sum();
        println!(
            "\n== UrbanGrid, designed under {} on {}, then upgraded to {}: {} changes {by_move:?}; {stuck} parts no allowed move replaces",
            up.legacy_rules_id,
            up.legacy_profile,
            profile.id,
            up.changes.values().map(Vec::len).sum::<usize>(),
        );
    }
    report(
        "UrbanGrid, legacy then upgraded",
        &map,
        &v2_view(&design),
        library,
        &profile,
    );
}
