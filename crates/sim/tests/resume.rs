//! Task 26's two promises to the world it runs: the fold carried across calls is the fold in one call, byte for byte, and an
//! event at world time T is the same event at 0, shifted by T. On `one_street`; all synthetic.

use std::fs;

use plant_graph::PlantGraph;
use schema::common::{EdgeId, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{CutEnd, Fault, FaultKind, FaultSet, Onset};
use schema::plant::Plant;
use schema::profile::Profile;
use sim::telemetry::{
    Detail, Scenario, SimOptions, StepObservation, fresh_states, simulate, simulate_from,
};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn at(t: i64) -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(t),
        temp_c: 20.0,
        ground_temp_c: 18.0,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

fn environment(times: &[i64]) -> Environment {
    Environment {
        schema_version: "1".into(),
        samples: times.iter().map(|t| at(*t)).collect(),
        ambient_sources: Vec::new(),
    }
}

fn cut(onset: Onset) -> FaultSet {
    FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.into(),
        seed: 9,
        faults: vec![Fault {
            onset,
            kind: FaultKind::SpanCut {
                span_id: EdgeId::new("span-5"),
                pos_ft: 120.0,
                end: CutEnd::Open,
                repair_s: Some(1800.0),
            },
        }],
    }
}

fn options() -> SimOptions {
    SimOptions {
        detail: Detail::Summary { stride: 16 },
        ..SimOptions::default()
    }
}

fn bytes(steps: &[StepObservation]) -> String {
    serde_json::to_string(steps).unwrap()
}

#[test]
fn a_run_split_anywhere_is_the_run_in_one() {
    let (plant, library, profile): (Plant, ComponentLibrary, Profile) = (
        read("one_street/plant_lowsplit_iccap.json"),
        read("library/generic.json"),
        read("profiles/d31_lowsplit_1002_iccap.json"),
    );
    let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
    let faults = cut(Onset::Instant);
    let times: Vec<i64> = (0..40).map(|i| i * 60).collect();
    let whole = simulate(&Scenario {
        graph: &graph,
        environment: &environment(&times),
        faults: &faults,
        seed: 9,
        options: options(),
        link: None,
    })
    .unwrap();
    let mut states = fresh_states(&graph);
    let mut pieces = Vec::new();
    let mut previous = None;
    for chunk in times.chunks(7) {
        pieces.extend(
            simulate_from(
                &Scenario {
                    graph: &graph,
                    environment: &environment(chunk),
                    faults: &faults,
                    seed: 9,
                    options: options(),
                    link: None,
                },
                &mut states,
                previous,
            )
            .unwrap(),
        );
        previous = chunk.last().map(|t| Timestamp(*t));
    }
    assert_eq!(bytes(&whole), bytes(&pieces));
}

#[test]
fn an_event_at_t_is_the_event_at_0_shifted() {
    let (plant, library, profile): (Plant, ComponentLibrary, Profile) = (
        read("one_street/plant_lowsplit_iccap.json"),
        read("library/generic.json"),
        read("profiles/d31_lowsplit_1002_iccap.json"),
    );
    let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
    let shift = 7200;
    let times: Vec<i64> = (0..40).map(|i| i * 60).collect();
    let later: Vec<i64> = times.iter().map(|t| t + shift).collect();
    let run = |faults: &FaultSet, times: &[i64]| {
        simulate(&Scenario {
            graph: &graph,
            environment: &environment(times),
            faults,
            seed: 9,
            options: options(),
            link: None,
        })
        .unwrap()
    };
    let at_zero = run(&cut(Onset::Instant), &times);
    let at_t = run(
        &cut(Onset::Ramp {
            start: Timestamp(shift),
            end: Timestamp(shift),
        }),
        &later,
    );
    // The same modems up, step by step, from onset through the repair: the states, not the bytes, since time is in them.
    let states = |steps: &[StepObservation]| -> Vec<Vec<schema::observation::RegState>> {
        steps
            .iter()
            .map(|s| s.modems.iter().map(|m| m.reg_state).collect())
            .collect()
    };
    assert_eq!(states(&at_zero), states(&at_t));
    assert!(
        states(&at_zero)[5]
            .iter()
            .any(|r| *r != schema::observation::RegState::Operational)
    );
    assert!(
        states(&at_zero)[39]
            .iter()
            .all(|r| *r == schema::observation::RegState::Operational)
    );
}
