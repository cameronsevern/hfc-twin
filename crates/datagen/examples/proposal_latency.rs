//! Time for one forward pass of the shipped proposal model on one designed node, rows included. Synthetic plant.
use std::time::Instant;

use design::{DesignParts, design_region, generate};
use features::DesignReference;
use features::sweep::{observe, random_sample};
use plant_graph::PlantGraph;
use proposal::{Model, Proposer, component_rows};
use schema::component::ComponentLibrary;
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
    let profile: Profile = read("fixtures/profiles/d31_highsplit_1218_rpd.json")?;
    let model = Model::from_json(&std::fs::read_to_string("ml/artifacts/proposal_gbdt.json")?)?;
    let map = generate(LayoutFamily::Suburban, 1, 200);
    let controls = DesignControls {
        homes_per_node: 120,
        seed: 1,
        ..DesignControls::default()
    };
    let region = design_region(&map, &profile, &library, &controls, &DesignParts::default())?;
    let node = region.nodes.first().ok_or("no node")?;
    let graph = PlantGraph::new(&node.plant, &library, &profile).map_err(|e| format!("{e:?}"))?;
    let reference = DesignReference::compute(&graph)?;
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.to_string(),
        seed: 1,
        faults: Vec::new(),
    };
    let mut rng = Rng::new(1);
    let observed = observe(&graph, &reference, &faults, random_sample(&mut rng, 0), 1)?;
    let t = Instant::now();
    let mut places = 0;
    for _ in 0..PASSES {
        let rows = component_rows(&graph, &observed);
        places = rows.len();
        model.propose(&rows);
    }
    println!(
        "{places} candidate places: {:.2} ms per pass (synthetic plant)",
        t.elapsed().as_secs_f64() * 1000.0 / f64::from(PASSES)
    );
    // The graph model, if it's there, on the same node. `step 15`'s decision reads what this writes.
    if let Ok(text) = std::fs::read_to_string("ml/artifacts/gnn_maps.json") {
        let gnn = Proposer::from_json(&text)?;
        let rows = component_rows(&graph, &observed);
        let t = Instant::now();
        for _ in 0..PASSES {
            gnn.propose(&graph, &node.plant, &observed, &rows)?;
        }
        let gnn_ms = t.elapsed().as_secs_f64() * 1000.0 / f64::from(PASSES);
        println!("graph model: {gnn_ms:.2} ms per pass, rows excluded (synthetic plant)");
        std::fs::write(
            "ml/artifacts/gnn_latency.json",
            serde_json::json!({"origin": "Synthetic", "candidates": places, "gnn_ms": gnn_ms})
                .to_string(),
        )?;
    }
    Ok(())
}
