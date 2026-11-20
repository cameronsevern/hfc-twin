//! Version 2 parts in the sim (`docs/tasks/21-as-built-schemas.md`, "Readers"). Task 21 refused the as-built fixture, naming each
//! part it couldn't price; task 22 priced them all, so the first test turned over and the fixture simulates. Parts with no RF or
//! power effect change nothing: a plant with only those simulates to the same output as without them (`docs/as-built-plan.md`,
//! rule 2).
//! All synthetic.

use std::fs;

use plant_graph::PlantGraph;
use schema::common::{ComponentId, StructureId, Timestamp};
use schema::component::{ComponentLibrary, StructureKind, Tolerance};
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{FaultSet, Location};
use schema::plant::{BatteryBank, EdgeKind, Plant, RecordStatus, RoutePoint, Structure};
use schema::profile::Profile;
use sim::PhysicsError;
use sim::telemetry::{Detail, Scenario, SimOptions, StepObservation, simulate};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn environment() -> Environment {
    Environment {
        schema_version: "1".into(),
        samples: vec![
            EnvironmentSample {
                t: Timestamp(0),
                temp_c: 30.0,
                ground_temp_c: 18.0,
                rain_mm_per_hr: 0.0,
                solar_w_per_m2: None,
                wind_m_per_s: None,
            },
            EnvironmentSample {
                t: Timestamp(300),
                temp_c: -5.0,
                ground_temp_c: 4.0,
                rain_mm_per_hr: 10.0,
                solar_w_per_m2: None,
                wind_m_per_s: None,
            },
        ],
        ambient_sources: Vec::new(),
    }
}

fn faults() -> FaultSet {
    read("one_street/fault_examples.json")
}

fn run(
    plant: &Plant,
    library: &ComponentLibrary,
    profile: &Profile,
    randomize: Option<u64>,
) -> Result<Vec<StepObservation>, PhysicsError> {
    let graph = PlantGraph::new(plant, library, profile).unwrap();
    let mut faults = faults();
    faults.faults.truncate(3);
    simulate(&Scenario {
        graph: &graph,
        environment: &environment(),
        faults: &faults,
        seed: 5,
        options: SimOptions {
            detail: Detail::Full,
            randomize,
            ..SimOptions::default()
        },
        link: None,
    })
}

fn pairs() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "plant_lowsplit_iccap.json",
            "profiles/d31_lowsplit_1002_iccap.json",
        ),
        (
            "plant_highsplit_rpd.json",
            "profiles/d31_highsplit_1218_rpd.json",
        ),
    ]
}

/// Task 22 prices every part task 21 added, so the test that refused the as-built fixture turns over: nothing is named, and every
/// entry point that refused it answers, on both profiles, at a cold wet sample and a hot dry one.
#[test]
fn the_as_built_fixture_simulates_with_nothing_refused() {
    let library: ComponentLibrary = read("library/generic.json");
    for (plant_file, profile_file) in pairs() {
        let plant: Plant = read(&format!("one_street_as_built/{plant_file}"));
        let profile: Profile = read(profile_file);
        let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
        assert_eq!(sim::support::unsupported(&graph), Vec::<String>::new());
        assert!(!plant.version_2_parts(&library).is_empty());
        let empty = FaultSet {
            schema_version: schema::fault::SCHEMA_VERSION.into(),
            seed: 1,
            faults: Vec::new(),
        };
        let steps = simulate(&Scenario {
            graph: &graph,
            environment: &environment(),
            faults: &empty,
            seed: 5,
            options: SimOptions::default(),
            link: None,
        })
        .unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].modems.len(), 9, "{plant_file}");
        // With faults too.
        run(&plant, &library, &profile, None).unwrap();

        let sample = &environment().samples[0];
        sim::Overlay::build(&graph, &empty, sample).unwrap();
        sim::ac::solve_ac(&graph, &sim::Overlay::default()).unwrap();
        let modem = Location::Port {
            element_id: schema::common::ElementId::new("modem-1"),
            port: schema::common::PortId::new("rf"),
        };
        let root = Location::Port {
            element_id: graph.root().id.clone(),
            port: schema::common::PortId::new("rf-1"),
        };
        sim::transfer::transfer(&graph, sample, &root, &modem, 600.0).unwrap();
    }
}

/// `one_street` with every version 2 part that has no RF or power effect: a structure under every element, strand or conduit on
/// every span, a path through poles, record statuses on edges and the supply, and batteries in a cabinet.
fn with_neutral_parts(mut plant: Plant) -> Plant {
    plant.schema_version = schema::plant::SCHEMA_VERSION.into();
    for (i, element) in plant.elements.iter_mut().enumerate() {
        let id = StructureId::new(format!("pole-{i}"));
        plant.structures.push(Structure {
            id: id.clone(),
            kind: StructureKind::Pole,
            component_ref: ComponentId::new("structure-generic-pole"),
            geo: element.geo,
            record_status: RecordStatus::Built,
        });
        element.structure = Some(id);
    }
    plant.structures.push(Structure {
        id: StructureId::new("cabinet-1"),
        kind: StructureKind::Cabinet,
        component_ref: ComponentId::new("structure-generic-cabinet"),
        geo: None,
        record_status: RecordStatus::Built,
    });
    for edge in &mut plant.edges {
        edge.record_status = Some(RecordStatus::Built);
        match &mut edge.kind {
            EdgeKind::Span { support, path, .. } => {
                *support = Some(ComponentId::new("strand-generic"));
                path.push(RoutePoint {
                    geo: schema::common::LonLat {
                        lon_deg: -105.098,
                        lat_deg: 39.9,
                    },
                    structure: Some(StructureId::new("pole-0")),
                });
            }
            EdgeKind::Drop { installation, .. } => {
                *installation = Some(schema::plant::Installation::Aerial);
            }
            _ => {}
        }
    }
    let supply = &mut plant.power_supplies[0];
    supply.batteries = Some(BatteryBank {
        component_ref: ComponentId::new("battery-generic-12v"),
        per_string: 3,
        strings: 1,
    });
    supply.structure = Some(StructureId::new("cabinet-1"));
    supply.record_status = Some(RecordStatus::Built);
    plant
}

#[test]
fn parts_with_no_rf_change_nothing() {
    let library: ComponentLibrary = read("library/generic.json");
    for (plant_file, profile_file) in pairs() {
        let plant: Plant = read(&format!("one_street/{plant_file}"));
        let profile: Profile = read(profile_file);
        let neutral = with_neutral_parts(plant.clone());
        assert!(!neutral.version_2_parts(&library).is_empty());
        for randomize in [None, Some(9)] {
            let before = run(&plant, &library, &profile, randomize).unwrap();
            let after = run(&neutral, &library, &profile, randomize).unwrap();
            assert_eq!(
                serde_json::to_string(&before).unwrap(),
                serde_json::to_string(&after).unwrap(),
                "{plant_file}"
            );
        }
    }
}

/// §8's randomization draws within each part's stated tolerance (task 22), so a run that randomizes reads it and a run that doesn't
/// is unchanged by it. Task 21 refused such a part when randomizing; this test turned over with task 22.
#[test]
fn a_stated_tolerance_is_drawn_within_only_when_randomizing() {
    let mut library: ComponentLibrary = read("library/generic.json");
    let plant: Plant = read("one_street/plant_lowsplit_iccap.json");
    let profile: Profile = read("profiles/d31_lowsplit_1002_iccap.json");
    let plain = run(&plant, &library, &profile, None).unwrap();
    let drawn = run(&plant, &library, &profile, Some(9)).unwrap();
    for part in &mut library.components {
        if part.id.as_str() == "amp-generic-1218" {
            part.tolerance = Some(Tolerance {
                loss_db: None,
                gain_db: Some(0.5),
                r_loop_pct: None,
                return_loss_db: None,
                nf_db: None,
                impl_mer_db: None,
            });
        }
    }
    assert_eq!(run(&plant, &library, &profile, None).unwrap(), plain);
    let within = run(&plant, &library, &profile, Some(9)).unwrap();
    assert_ne!(within, drawn);
    assert_eq!(run(&plant, &library, &profile, Some(9)).unwrap(), within);
}
