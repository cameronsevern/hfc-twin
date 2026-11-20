//! Class and component accuracy per fault class on the one-street fixture, with domain randomization
//! and measurement noise on. `cargo run --release -p diagnosis --example accuracy [rows-per-class]`.
//! All synthetic (invariant 11).

use std::collections::BTreeMap;
use std::fs;

use diagnosis::{DiagnoserConfig, diagnose};
use features::DesignReference;
use features::sampler::{CLASSES, sample_fault};
use features::sweep::{observe, random_sample};
use plant_graph::PlantGraph;
use schema::component::ComponentLibrary;
use schema::fault::FaultSet;
use schema::plant::Plant;
use schema::profile::Profile;
use schema::work_order::CauseClass;
use sim::rng::Rng;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).expect("fixture")).expect("parse")
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let library: ComponentLibrary = read("library/generic.json");
    let config = DiagnoserConfig::default();
    println!("| Profile | Class | Top class | Top component | Top-3 component | Sims per run |");
    println!("|---|---|---|---|---|---|");
    for (plant_file, profile_file) in [
        (
            "one_street/plant_lowsplit_iccap.json",
            "profiles/d31_lowsplit_1002_iccap.json",
        ),
        (
            "one_street/plant_highsplit_rpd.json",
            "profiles/d31_highsplit_1218_rpd.json",
        ),
    ] {
        let plant: Plant = read(plant_file);
        let profile: Profile = read(profile_file);
        let graph = PlantGraph::new(&plant, &library, &profile).expect("valid");
        let reference = DesignReference::compute(&graph).expect("reference");
        for class in CLASSES {
            let mut row: BTreeMap<&str, f64> = BTreeMap::new();
            for i in 0..n {
                let seed = 1000 + i as u64;
                let mut rng = Rng::derive(seed, &[class as u64 + 1]);
                let Some(fault) = sample_fault(&graph, class, &mut rng) else {
                    continue;
                };
                let truth = diagnosis::engine::fault_component(&fault);
                let faults = FaultSet {
                    schema_version: "1".into(),
                    seed,
                    faults: vec![fault],
                };
                let sample = random_sample(&mut rng, 0);
                let observed =
                    observe(&graph, &reference, &faults, sample.clone(), seed).expect("sim");
                let answer =
                    diagnose(&graph, &reference, &observed, &sample, &config).expect("diagnose");
                let top = answer.hypotheses.first();
                *row.entry("runs").or_default() += 1.0;
                *row.entry("sims").or_default() += answer.simulations as f64;
                if top.and_then(|h| h.class) == Some(class) {
                    *row.entry("class").or_default() += 1.0;
                }
                // Upstream ingress can't be placed from modem data (§7.3), so a class hit counts.
                let placeable = class != CauseClass::UpstreamIngress;
                if top.is_some_and(|h| h.components.contains(&truth)) || !placeable {
                    *row.entry("component").or_default() += 1.0;
                }
                if answer
                    .hypotheses
                    .iter()
                    .take(3)
                    .any(|h| h.components.contains(&truth))
                    || !placeable
                {
                    *row.entry("top3").or_default() += 1.0;
                }
            }
            let runs = row.get("runs").copied().unwrap_or(1.0);
            let share = |key: &str| row.get(key).copied().unwrap_or(0.0) / runs * 100.0;
            println!(
                "| {} | {} | {:.0} % | {:.0} % | {:.0} % | {:.0} |",
                profile.id,
                class.name(),
                share("class"),
                share("component"),
                share("top3"),
                row.get("sims").copied().unwrap_or(0.0) / runs,
            );
        }
    }
}
