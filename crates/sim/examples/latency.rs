//! Measures a full forward and return pass, per service group. `cargo run --release -p sim --example latency`.
//!
//! Task 22: each size runs twice, with bare modems on the added taps and with a full home on every added tap port (ground block,
//! 2-way splitter, house cable, a modem and a set-top), and reports the median of several passes.

use std::fs;
use std::time::Instant;

use plant_graph::PlantGraph;
use schema::common::Timestamp;
use schema::component::ComponentLibrary;
use schema::environment::EnvironmentSample;
use schema::plant::Plant;
use schema::profile::Profile;
use sim::Overlay;
use sim::walk::{Band, Grid, ModemView, Walker};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).expect("fixture")).expect("parse")
}

/// A chain of 4-port taps under the fixture's second amp, with a modem on every port: on its drop, or behind a home.
fn with_taps(base: &Plant, taps: usize, premises: bool) -> Plant {
    use schema::common::{EdgeId, ElementId, PortId};
    use schema::plant::{
        Edge, EdgeKind, Element, ElementKind, Installation, Port, PortRef, PortRole, RecordStatus,
    };
    let mut plant = base.clone();
    let mut from = PortRef {
        element_id: ElementId::new("tap-4"),
        port: PortId::new("tap-3"),
    };
    let drop_ref = schema::common::ComponentId::new("drop-rg6");
    for t in 0..taps {
        let tap_id = ElementId::new(format!("x-tap-{t}"));
        let mut ports = vec![
            Port {
                id: PortId::new("in"),
                role: PortRole::Input,
            },
            Port {
                id: PortId::new("through"),
                role: PortRole::Through,
            },
        ];
        ports.extend((1..=4).map(|p| Port {
            id: PortId::new(format!("tap-{p}")),
            role: PortRole::TapPort,
        }));
        plant.elements.push(Element {
            id: tap_id.clone(),
            kind: ElementKind::Tap,
            component_ref: schema::common::ComponentId::new("tap-generic-23db-4p"),
            geo: None,
            ports,
            record_status: RecordStatus::Designed,
            fitted: None,
            structure: None,
        });
        plant.edges.push(Edge {
            id: EdgeId::new(format!("x-span-{t}")),
            from: from.clone(),
            to: PortRef {
                element_id: tap_id.clone(),
                port: PortId::new("in"),
            },
            kind: EdgeKind::Span {
                cable_ref: schema::common::ComponentId::new("cable-500-p3"),
                len_ft: 100.0,
                installation: Installation::Aerial,
                power_passing: true,
                fittings: Vec::new(),
                path: Vec::new(),
                support: None,
            },
            record_status: None,
        });
        for p in 1..=4 {
            if premises {
                add_home(&mut plant, &tap_id, t, p);
                continue;
            }
            let modem_id = ElementId::new(format!("x-modem-{t}-{p}"));
            plant.elements.push(Element {
                id: modem_id.clone(),
                kind: ElementKind::Modem,
                component_ref: schema::common::ComponentId::new("modem-generic-d31"),
                geo: None,
                ports: vec![Port {
                    id: PortId::new("rf"),
                    role: PortRole::Input,
                }],
                record_status: RecordStatus::Designed,
                fitted: None,
                structure: None,
            });
            plant.edges.push(Edge {
                id: EdgeId::new(format!("x-drop-{t}-{p}")),
                from: PortRef {
                    element_id: tap_id.clone(),
                    port: PortId::new(format!("tap-{p}")),
                },
                to: PortRef {
                    element_id: modem_id,
                    port: PortId::new("rf"),
                },
                kind: EdgeKind::Drop {
                    cable_ref: drop_ref.clone(),
                    len_ft: 100.0,
                    fittings: Vec::new(),
                    installation: None,
                    path: Vec::new(),
                },
                record_status: None,
            });
        }
        from = PortRef {
            element_id: tap_id,
            port: PortId::new("through"),
        };
    }
    plant
}

/// A home on tap port `p`: drop, ground block, 2-way splitter, a modem on one leg and a set-top on the other, F connectors at both
/// ends of every cable.
fn add_home(plant: &mut Plant, tap: &schema::common::ElementId, t: usize, p: usize) {
    use schema::common::{ComponentId, EdgeId, ElementId, PortId};
    use schema::plant::{
        Edge, EdgeKind, Element, ElementKind, Fitted, Fitting, GroundBlockFitted, Port, PortRef,
        PortRole, RecordStatus,
    };
    let id = |what: &str| ElementId::new(format!("x-{what}-{t}-{p}"));
    let port = |id: &str, role| Port {
        id: PortId::new(id),
        role,
    };
    let element = |id: ElementId, kind, part: &str, ports: Vec<Port>, fitted| Element {
        id,
        kind,
        component_ref: ComponentId::new(part),
        geo: None,
        ports,
        record_status: RecordStatus::Designed,
        fitted,
        structure: None,
    };
    let ends = |len_ft: f64| {
        [0.0, len_ft]
            .map(|pos_ft| Fitting {
                pos_ft,
                component_ref: ComponentId::new("conn-generic-f"),
            })
            .to_vec()
    };
    let edge = |name: &str, from: (ElementId, &str), to: (ElementId, &str), kind| Edge {
        id: EdgeId::new(format!("x-{name}-{t}-{p}")),
        from: PortRef {
            element_id: from.0,
            port: PortId::new(from.1),
        },
        to: PortRef {
            element_id: to.0,
            port: PortId::new(to.1),
        },
        kind,
        record_status: None,
    };
    let house = |len_ft: f64| EdgeKind::House {
        cable_ref: ComponentId::new("drop-rg6"),
        len_ft,
        fittings: ends(len_ft),
    };
    plant.elements.push(element(
        id("gb"),
        ElementKind::GroundBlock,
        "gb-generic",
        vec![port("in", PortRole::Input), port("out", PortRole::Through)],
        Some(Fitted::GroundBlock(GroundBlockFitted { coupling_db: 60.0 })),
    ));
    plant.elements.push(element(
        id("hsp"),
        ElementKind::Splitter,
        "splitter-generic-2way-home",
        vec![
            port("in", PortRole::Input),
            port("leg-1", PortRole::Leg),
            port("leg-2", PortRole::Leg),
        ],
        None,
    ));
    plant.elements.push(element(
        id("modem"),
        ElementKind::Modem,
        "modem-generic-d31",
        vec![port("rf", PortRole::Input)],
        None,
    ));
    plant.elements.push(element(
        id("stb"),
        ElementKind::Cpe,
        "cpe-generic-settop",
        vec![port("rf", PortRole::Input)],
        None,
    ));
    plant.edges.push(edge(
        "drop",
        (tap.clone(), &format!("tap-{p}")),
        (id("gb"), "in"),
        EdgeKind::Drop {
            cable_ref: ComponentId::new("drop-rg6"),
            len_ft: 100.0,
            fittings: ends(100.0),
            installation: None,
            path: Vec::new(),
        },
    ));
    plant.edges.push(edge(
        "house-a",
        (id("gb"), "out"),
        (id("hsp"), "in"),
        house(6.0),
    ));
    plant.edges.push(edge(
        "house-b",
        (id("hsp"), "leg-1"),
        (id("modem"), "rf"),
        house(25.0),
    ));
    plant.edges.push(edge(
        "house-c",
        (id("hsp"), "leg-2"),
        (id("stb"), "rf"),
        house(40.0),
    ));
}

/// One full pass, forward and return, and how many modems it read.
fn pass(graph: &PlantGraph<'_>, sample: &EnvironmentSample) -> (usize, std::time::Duration) {
    let overlay = Overlay::default();
    let start = Instant::now();
    let walker = Walker::new(graph, sample, &overlay).expect("walker");
    let down = Grid::uniform(835.0, 0.05, 3800);
    let up = Grid::uniform(108.5, 0.05, 1900);
    let mut count = 0usize;
    walker
        .run(&down, Band::Down, &mut |_: &ModemView<'_>| {
            count += 1;
            Ok(())
        })
        .expect("down");
    walker
        .run(&up, Band::Up, &mut |_: &ModemView<'_>| Ok(()))
        .expect("up");
    (count, start.elapsed())
}

/// Passes per measurement; the median is reported.
const PASSES: usize = 9;

fn median(graph: &PlantGraph<'_>, sample: &EnvironmentSample) -> (usize, std::time::Duration) {
    let mut times: Vec<std::time::Duration> = Vec::with_capacity(PASSES);
    let mut count = 0;
    for _ in 0..PASSES {
        let (n, t) = pass(graph, sample);
        count = n;
        times.push(t);
    }
    times.sort();
    (count, times[PASSES / 2])
}

fn main() {
    let library: ComponentLibrary = read("library/generic.json");
    let profile: Profile = read("profiles/d31_highsplit_1218_rpd.json");
    let base: Plant = read("one_street/plant_highsplit_rpd.json");
    let sample = EnvironmentSample {
        t: Timestamp(0),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    };
    for taps in [0, 25, 125] {
        let plant = with_taps(&base, taps, false);
        let graph = PlantGraph::new(&plant, &library, &profile).expect("valid plant");
        // The first pass, as version 1 measured it, then the median.
        let (count, first) = pass(&graph, &sample);
        println!("{count} modems, 3800 down and 1900 up frequencies: {first:?}");
        let (_, bare) = median(&graph, &sample);
        println!("  bare modems, median of {PASSES}: {bare:?}");
        if taps > 0 {
            let homes = with_taps(&base, taps, true);
            let graph = PlantGraph::new(&homes, &library, &profile).expect("valid plant");
            let (count, full) = median(&graph, &sample);
            println!(
                "  {count} modems behind full homes, median of {PASSES}: {full:?}, {:.2} times bare",
                full.as_secs_f64() / bare.as_secs_f64()
            );
        }
    }
}
