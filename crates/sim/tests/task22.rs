//! Task 22's cross-cutting tests: each new mechanism alone reaches only where it can, and everything new at once is still
//! deterministic. All synthetic.

use std::collections::BTreeMap;
use std::fs;

use plant_graph::PlantGraph;
use schema::common::{ComponentId, EdgeId, ElementId, FiberCableId, PortId, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::{AmbientSource, Environment, EnvironmentSample};
use schema::fault::{ConnectorCouplings, Fault, FaultKind, FaultSet, Location, Onset, Schedule};
use schema::plant::{
    Edge, EdgeKind, Element, ElementKind, Fitted, Fitting, GroundBlockFitted, Plant, Port, PortRef,
    PortRole, RecordStatus,
};
use schema::profile::Profile;
use sim::telemetry::{Detail, Scenario, SimOptions, StepObservation, simulate};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn sample(t: i64) -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(t),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        wind_m_per_s: None,
        solar_w_per_m2: None,
    }
}

fn no_faults() -> FaultSet {
    FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.into(),
        seed: 1,
        faults: Vec::new(),
    }
}

fn run(plant: &Plant, library: &ComponentLibrary, profile: &Profile) -> StepObservation {
    let graph = PlantGraph::new(plant, library, profile).unwrap();
    let environment = Environment {
        schema_version: schema::environment::SCHEMA_VERSION.into(),
        samples: vec![sample(0)],
        ambient_sources: Vec::new(),
    };
    simulate(&Scenario {
        graph: &graph,
        environment: &environment,
        faults: &no_faults(),
        seed: 5,
        options: SimOptions {
            detail: Detail::Full,
            ..SimOptions::default()
        },
        link: None,
    })
    .unwrap()
    .remove(0)
}

/// Each modem's observation as JSON, by id.
fn by_modem(step: &StepObservation) -> BTreeMap<String, String> {
    step.modems
        .iter()
        .map(|m| (m.element_id.to_string(), serde_json::to_string(m).unwrap()))
        .collect()
}

fn base() -> (Plant, ComponentLibrary, Profile) {
    (
        read("one_street/plant_lowsplit_iccap.json"),
        read("library/generic.json"),
        read("profiles/d31_lowsplit_1002_iccap.json"),
    )
}

/// What changed, by modem.
fn changed(a: &StepObservation, b: &StepObservation) -> Vec<String> {
    let (a, b) = (by_modem(a), by_modem(b));
    a.iter()
        .filter(|(id, json)| b.get(*id) != Some(*json))
        .map(|(id, _)| id.clone())
        .collect()
}

#[test]
fn a_ground_block_reaches_only_its_tap() {
    let (plant, library, profile) = base();
    let before = run(&plant, &library, &profile);
    // `modem-8`'s drop now ends at a ground block, with 6 ft of house cable on to the modem.
    let mut with = plant.clone();
    for edge in &mut with.edges {
        if edge.id.as_str() == "drop-8" {
            edge.to = PortRef {
                element_id: ElementId::new("gb-x"),
                port: PortId::new("in"),
            };
        }
    }
    with.elements.push(Element {
        id: ElementId::new("gb-x"),
        kind: ElementKind::GroundBlock,
        component_ref: ComponentId::new("gb-generic"),
        geo: None,
        ports: vec![
            Port {
                id: PortId::new("in"),
                role: PortRole::Input,
            },
            Port {
                id: PortId::new("out"),
                role: PortRole::Through,
            },
        ],
        record_status: RecordStatus::Designed,
        fitted: Some(Fitted::GroundBlock(GroundBlockFitted { coupling_db: 60.0 })),
        structure: None,
    });
    with.edges.push(Edge {
        id: EdgeId::new("house-x"),
        from: PortRef {
            element_id: ElementId::new("gb-x"),
            port: PortId::new("out"),
        },
        to: PortRef {
            element_id: ElementId::new("modem-8"),
            port: PortId::new("rf"),
        },
        kind: EdgeKind::House {
            cable_ref: ComponentId::new("drop-rg6"),
            len_ft: 6.0,
            fittings: Vec::new(),
        },
        record_status: None,
    });
    let after = run(&with, &library, &profile);
    let moved = changed(&before, &after);
    assert!(moved.contains(&"modem-8".to_string()));
    assert!(
        moved.iter().all(|m| m == "modem-7" || m == "modem-8"),
        "{moved:?}"
    );
    assert_eq!(
        serde_json::to_string(&before.service_group).unwrap(),
        serde_json::to_string(&after.service_group).unwrap()
    );
}

#[test]
fn a_fitting_on_a_span_reaches_only_what_is_below_it() {
    let (plant, library, profile) = base();
    let before = run(&plant, &library, &profile);
    let mut with = plant.clone();
    for edge in &mut with.edges {
        if edge.id.as_str() == "span-6"
            && let EdgeKind::Span { fittings, .. } = &mut edge.kind
        {
            fittings.push(Fitting {
                pos_ft: 50.0,
                component_ref: ComponentId::new("conn-generic-pin-500"),
            });
        }
    }
    let after = run(&with, &library, &profile);
    let moved = changed(&before, &after);
    assert!(!moved.is_empty());
    // Below `span-6`: `tap-4`'s modems.
    assert!(
        moved.iter().all(|m| m == "modem-7" || m == "modem-8"),
        "{moved:?}"
    );
}

#[test]
fn a_misalignment_reaches_only_what_is_below_its_amp() {
    let (plant, library, profile) = base();
    let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
    let environment = Environment {
        schema_version: schema::environment::SCHEMA_VERSION.into(),
        samples: vec![sample(0)],
        ambient_sources: Vec::new(),
    };
    let at = |faults: &FaultSet| {
        simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults,
            seed: 5,
            options: SimOptions::default(),
            link: None,
        })
        .unwrap()
        .remove(0)
    };
    let before = at(&no_faults());
    let after = at(&FaultSet {
        faults: vec![Fault {
            onset: Onset::Instant,
            kind: FaultKind::Misalignment {
                active_id: ElementId::new("amp-2"),
                pad_delta_db: 3.0,
                eq_delta_db: 0.0,
                return_pad_delta_db: 0.0,
            },
        }],
        ..no_faults()
    });
    let moved = changed(&before, &after);
    assert!(!moved.is_empty());
    let below = ["modem-5", "modem-6", "modem-7", "modem-8"];
    assert!(
        moved.iter().all(|m| below.contains(&m.as_str())),
        "{moved:?}"
    );
}

/// Everything task 22 added, at once: the as-built plant with a link under fiber faults, an ambient source, wind and sun, a coupled
/// connector, a misalignment, tolerance draws and the plant-side readings. Same inputs and seed, same bytes.
#[test]
fn everything_new_at_once_is_deterministic() {
    let plant: Plant = read("one_street_as_built/plant_lowsplit_iccap.json");
    let mut library: ComponentLibrary = read("library/generic.json");
    for part in &mut library.components {
        if part.id.as_str() == "modem-generic-d31" {
            part.tolerance = Some(schema::component::Tolerance {
                loss_db: None,
                gain_db: None,
                r_loop_pct: None,
                return_loss_db: None,
                nf_db: Some(1.0),
                impl_mer_db: Some(1.0),
            });
        }
    }
    let mut profile: Profile = read("profiles/d31_lowsplit_1002_iccap.json");
    profile.plant_reports = schema::profile::PlantReports {
        node_optical_rx: true,
        hub_optical_rx: true,
        rpd_optic_status: true,
        out_level_per_port: true,
    };
    let fiber: schema::fiber::FiberPlant = read("one_street_as_built/fiber.json");
    let fiber_faults = vec![schema::fiber_fault::FiberFault {
        onset: Onset::Ramp {
            start: Timestamp(0),
            end: Timestamp(600),
        },
        kind: schema::fiber_fault::FiberFaultKind::EventLoss {
            location: schema::fiber_fault::FiberLocation::Cable {
                cable: FiberCableId::new("fiber-2"),
                pos_ft: 200.0,
            },
            loss_db: 2.0,
        },
    }];
    let mut hourly_multipliers = [1.0; 24];
    hourly_multipliers[0] = 2.0;
    let environment = Environment {
        schema_version: schema::environment::SCHEMA_VERSION.into(),
        samples: (0..3)
            .map(|k| EnvironmentSample {
                wind_m_per_s: Some(3.0 * k as f64),
                solar_w_per_m2: Some(400.0 * k as f64),
                temp_c: 15.0 + 5.0 * k as f64,
                ..sample(300 * k)
            })
            .collect(),
        ambient_sources: vec![AmbientSource {
            id: "ambient-1".into(),
            lo_mhz: 5.0,
            hi_mhz: 42.0,
            level_dbmv_per_mhz: -20.0,
            schedule: Schedule { hourly_multipliers },
        }],
    };
    let faults = FaultSet {
        faults: vec![
            Fault {
                onset: Onset::Instant,
                kind: FaultKind::ConnectorReflection {
                    location: Location::Span {
                        span_id: EdgeId::new("span-6"),
                        pos_ft: 120.0,
                    },
                    return_loss_db: 16.0,
                    couplings: Some(ConnectorCouplings {
                        temp_db_per_c: Some(0.2),
                        wind_db_per_m_per_s: Some(-0.6),
                        ambient_coupling_db: Some(35.0),
                    }),
                },
            },
            Fault {
                onset: Onset::Instant,
                kind: FaultKind::Misalignment {
                    active_id: ElementId::new("amp-2"),
                    pad_delta_db: 1.0,
                    eq_delta_db: 2.0,
                    return_pad_delta_db: 1.0,
                },
            },
        ],
        ..no_faults()
    };
    let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
    let once = || {
        let link = sim::optics::node_link_drawn(
            &fiber,
            &plant,
            &library,
            &profile.channel_plan,
            &fiber_faults,
            Timestamp(300),
            Some(9),
        )
        .unwrap()
        .unwrap();
        let steps = simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults: &faults,
            seed: 9,
            options: SimOptions {
                detail: Detail::Full,
                randomize: Some(9),
                ..SimOptions::default()
            },
            link: Some(&link),
        })
        .unwrap();
        serde_json::to_string(&steps).unwrap()
    };
    let a = once();
    assert_eq!(a, once());
    // And it read what it was given.
    let steps: Vec<StepObservation> = serde_json::from_str(&a).unwrap();
    let optics = steps[1].service_group.optics.as_ref().unwrap();
    assert!(optics.node_rx_dbm.is_some() && optics.hub_rx_dbm.is_some());
    assert!(steps[0].actives.iter().any(|a| !a.out_levels.is_empty()));
}
