//! Prints what the diagnoser made of a few draws of one class. `inspect <ClassName> [n]`.

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
use sim::rng::Rng;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).expect("fixture")).expect("parse")
}

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ConnectorReflection".into());
    let n: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);
    let library: ComponentLibrary = read("library/generic.json");
    let config = DiagnoserConfig::default();
    let plant: Plant = read("one_street/plant_highsplit_rpd.json");
    let profile: Profile = read("profiles/d31_highsplit_1218_rpd.json");
    let graph = PlantGraph::new(&plant, &library, &profile).expect("valid");
    let reference = DesignReference::compute(&graph).expect("reference");
    let class = *CLASSES.iter().find(|c| c.name() == name).expect("class");
    for i in 0..n {
        let seed = 1000 + i as u64;
        let mut rng = Rng::derive(seed, &[class as u64 + 1]);
        let fault = sample_fault(&graph, class, &mut rng).expect("fault");
        println!("TRUE {:?}", fault.kind);
        let faults = FaultSet {
            schema_version: "1".into(),
            seed,
            faults: vec![fault],
        };
        let sample = random_sample(&mut rng, 0);
        println!(
            "  env {:.0}C rain {:.1}",
            sample.temp_c, sample.rain_mm_per_hr
        );
        let observed = observe(&graph, &reference, &faults, sample.clone(), seed).expect("sim");
        let answer = diagnose(&graph, &reference, &observed, &sample, &config).expect("diagnose");
        println!(
            "  who {:?} us_deg {} active {:?}",
            answer.who.affected, answer.who.upstream_degraded, answer.who.actives
        );
        for h in answer.hypotheses.iter().take(3) {
            let what: Vec<String> = h
                .faults
                .iter()
                .map(|f| format!("{:?}", f.kind).chars().take(110).collect())
                .collect();
            println!(
                "  {:.2} chi2 {:.0}/{} {:?}",
                h.posterior, h.chi2, h.dof, what
            );
        }
    }
}
