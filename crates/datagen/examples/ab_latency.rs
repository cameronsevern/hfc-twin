//! Task 24: one forward pass of the version 2 graph model on designed version 2 nodes, rows excluded, as task 15's rule times
//! it: on the node whose candidate count is nearest "about 100", and on the largest, for the record. Synthetic plant.
use std::time::Instant;

use design::v2::design;
use features::DesignReference;
use features::sweep::{observe, random_sample};
use plant_graph::PlantGraph;
use proposal::{Proposer, component_rows};
use schema::component::ComponentLibrary;
use schema::design_rules::DesignRules;
use schema::fault::FaultSet;
use schema::profile::Profile;
use schema::region::{DesignControls, LayoutFamily};
use sim::rng::Rng;

type Error = Box<dyn std::error::Error + Send + Sync>;

const PASSES: u32 = 20;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, Error> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn main() -> Result<(), Error> {
    let library: ComponentLibrary = read("fixtures/library/generic.json")?;
    let rules: DesignRules = read("fixtures/design_rules/generic.json")?;
    let profile: Profile =
        schema::profile::migrate(read("fixtures/profiles/d31_highsplit_1218_rpd.json")?)?;
    let model = Proposer::from_json(&std::fs::read_to_string(
        "ml/artifacts/as_built/gnn_maps.json",
    )?)?;
    let mut timed = Vec::new();
    for family in [
        LayoutFamily::Suburban,
        LayoutFamily::Rural,
        LayoutFamily::UrbanGrid,
    ] {
        let map = design::generate(family, 1, 600);
        let controls = DesignControls {
            seed: 1,
            rules_id: Some(rules.id.clone()),
            build_over_ceiling: true,
            ..DesignControls::default()
        };
        let d = design(&map, &profile, &library, &rules, &controls)?;
        for node in &d.nodes {
            let graph =
                PlantGraph::new(&node.plant, &library, &profile).map_err(|e| format!("{e:?}"))?;
            let reference = DesignReference::compute(&graph)?;
            let faults = FaultSet {
                schema_version: schema::fault::SCHEMA_VERSION.to_string(),
                seed: 1,
                faults: Vec::new(),
            };
            let mut rng = Rng::new(1);
            let observed = observe(&graph, &reference, &faults, random_sample(&mut rng, 0), 1)?;
            let rows = component_rows(&graph, &observed);
            let t = Instant::now();
            for _ in 0..PASSES {
                model.propose(&graph, &node.plant, &observed, &rows)?;
            }
            let ms = t.elapsed().as_secs_f64() * 1000.0 / f64::from(PASSES);
            timed.push((rows.len(), ms));
        }
    }
    let near = timed
        .iter()
        .min_by_key(|(n, _)| n.abs_diff(100))
        .copied()
        .ok_or("no node")?;
    let largest = timed
        .iter()
        .max_by_key(|(n, _)| *n)
        .copied()
        .ok_or("no node")?;
    println!(
        "graph model: {:.2} ms on {} candidates, {:.2} ms on the largest, {} (synthetic plant)",
        near.1, near.0, largest.1, largest.0
    );
    std::fs::write(
        "ml/artifacts/as_built/gnn_latency.json",
        serde_json::json!({
            "origin": "Synthetic",
            "candidates": near.0,
            "gnn_ms": near.1,
            "largest_candidates": largest.0,
            "largest_ms": largest.1,
        })
        .to_string(),
    )?;
    Ok(())
}
