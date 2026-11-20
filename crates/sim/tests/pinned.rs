//! Pinned digests of the sim's output on both `one_street` plants (`docs/tasks/21-as-built-schemas.md`, "Before
//! anything else"). Version 2 of the plant and component schemas is additive, so a plant with none of the new parts
//! must simulate to the same bytes as before (`docs/as-built-plan.md`, rule 2). A change that moves a digest here is a
//! bug, not a reason to update it.
//!
//! The digest hashes the JSON with every `schema_version` key removed, since those strings move with a schema bump
//! and nothing else may. It uses FNV-1a (`sim::rng::hash_str`), which is stable across Rust releases where `std`'s
//! hasher isn't.

use std::fs;

use plant_graph::PlantGraph;
use schema::common::Timestamp;
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::FaultSet;
use schema::plant::Plant;
use schema::profile::Profile;
use serde_json::Value;
use sim::rng::hash_str;
use sim::telemetry::{Detail, Scenario, SimOptions, simulate};

/// Pinned at `40d0133` as `bb42282f8209344b`, before any edit for task 21, and held through task 19. Moved by task 27 (an
/// SC-QAM modem's equalizer takes out an echo within its span, `docs/tasks/27-scqam-equalizer.md`), and by nothing else.
const LOW_DIGEST: &str = "9c8afd723a327e07";
/// Pinned at `40d0133` as `4ccfe14f3dd83669`, before any edit for task 21, and held through task 19. Moved by task 27, and by
/// nothing else.
const HIGH_DIGEST: &str = "05023d694cea75a9";

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(&full).unwrap())
        .unwrap_or_else(|e| panic!("{path}: {e}"))
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

/// The stable digest of a value's JSON, with its version strings removed.
pub fn digest(value: &impl serde::Serialize) -> u64 {
    let mut value = serde_json::to_value(value).unwrap();
    strip_versions(&mut value);
    hash_str(&serde_json::to_string(&value).unwrap())
}

/// Cold, mild, hot and wet, five minutes apart, so modem state carries from one step to the next.
fn environment() -> Environment {
    let sample = |i: i64, temp_c, ground_temp_c, rain_mm_per_hr| EnvironmentSample {
        t: Timestamp(300 * i),
        temp_c,
        ground_temp_c,
        rain_mm_per_hr,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    };
    Environment {
        schema_version: "1".into(),
        samples: vec![
            sample(0, -20.0, 2.0, 0.0),
            sample(1, 20.0, 15.0, 0.0),
            sample(2, 45.0, 28.0, 0.0),
            sample(3, 12.0, 14.0, 25.0),
        ],
        ambient_sources: Vec::new(),
    }
}

/// One plant's digest: healthy, then each example fault alone, each with randomization off and then on.
fn plant_digest(plant_file: &str, profile_file: &str, fault_files: &[&str]) -> String {
    let plant: Plant = read(plant_file);
    let library: ComponentLibrary = read("library/generic.json");
    let profile: Profile = read(profile_file);
    let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
    let environment = environment();

    let mut sets = vec![FaultSet {
        schema_version: "1".into(),
        seed: 7,
        faults: Vec::new(),
    }];
    for file in fault_files {
        let examples: FaultSet = read(file);
        for fault in examples.faults {
            sets.push(FaultSet {
                schema_version: examples.schema_version.clone(),
                seed: examples.seed,
                faults: vec![fault],
            });
        }
    }

    let mut runs = String::new();
    for faults in &sets {
        for randomize in [None, Some(3)] {
            let options = SimOptions {
                detail: Detail::Full,
                randomize,
                ..SimOptions::default()
            };
            let scenario = Scenario {
                graph: &graph,
                environment: &environment,
                faults,
                seed: 11,
                options,
                link: None,
            };
            let run = match simulate(&scenario) {
                Ok(steps) => digest(&steps),
                Err(error) => hash_str(&format!("{error:?}")),
            };
            runs.push_str(&format!("{run:016x}\n"));
        }
    }
    format!("{:016x}", hash_str(&runs))
}

#[test]
fn one_street_low_split_is_pinned() {
    let found = plant_digest(
        "one_street/plant_lowsplit_iccap.json",
        "profiles/d31_lowsplit_1002_iccap.json",
        &["one_street/fault_examples.json"],
    );
    assert_eq!(found, LOW_DIGEST, "the sim's output on one_street moved");
}

#[test]
fn one_street_high_split_is_pinned() {
    let found = plant_digest(
        "one_street/plant_highsplit_rpd.json",
        "profiles/d31_highsplit_1218_rpd.json",
        &[
            "one_street/fault_examples.json",
            "one_street/fault_examples_rpd.json",
        ],
    );
    assert_eq!(found, HIGH_DIGEST, "the sim's output on one_street moved");
}

#[test]
fn the_digest_ignores_version_strings_and_nothing_else() {
    let a = serde_json::json!({"schema_version": "1", "x": [{"schema_version": "1", "y": 1.5}]});
    let b = serde_json::json!({"schema_version": "2", "x": [{"schema_version": "2", "y": 1.5}]});
    let c = serde_json::json!({"schema_version": "1", "x": [{"schema_version": "1", "y": 1.25}]});
    assert_eq!(digest(&a), digest(&b));
    assert_ne!(digest(&a), digest(&c));
}
