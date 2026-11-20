//! How the optimizer does on a real OpenStreetMap extract. `cargo run --release -p design --example osm_survey -- FILE.json`.
//! The streets are real and the plant on them is invented.

use std::fs;
use std::time::Instant;

use design::osm::read_overpass;
use design::{DesignParts, design_region};
use schema::component::ComponentLibrary;
use schema::plant::Installation;
use schema::profile::Profile;
use schema::region::DesignControls;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).expect("fixture")).expect("parse")
}

fn main() {
    let file = std::env::args().nth(1).expect("an Overpass JSON file");
    let text = fs::read_to_string(file).expect("read");
    let (map, report) = read_overpass("survey", &text, Installation::Aerial).expect("extract");
    println!(
        "{} streets, {} homes, {report:?}",
        map.streets.len(),
        map.homes.len()
    );
    let library: ComponentLibrary = read("library/generic.json");
    for profile_file in [
        "profiles/d31_lowsplit_1002_iccap.json",
        "profiles/d31_highsplit_1218_rpd.json",
    ] {
        let profile: Profile = read(profile_file);
        for homes_per_node in [60, 120, 250] {
            let controls = DesignControls {
                homes_per_node,
                ..DesignControls::default()
            };
            let start = Instant::now();
            let design =
                design_region(&map, &profile, &library, &controls, &DesignParts::default());
            let took = start.elapsed().as_secs_f64();
            match design {
                Ok(d) => {
                    let issues: usize = d
                        .nodes
                        .iter()
                        .map(|n| n.topology_issues.len() + n.physics_issues.len())
                        .sum();
                    let smallest = d.nodes.iter().map(|n| n.homes_passed).min().unwrap_or(0);
                    let amps: u32 = d.nodes.iter().map(|n| n.amps).sum();
                    let miles = d.nodes.iter().map(|n| n.hardline_ft).sum::<f64>() / 5280.0;
                    println!(
                        "{} per node {homes_per_node}: {} nodes, smallest {smallest} homes, {amps} amps over {miles:.1} mi, {} unserved, {issues} issues, {took:.2} s",
                        profile.id,
                        d.nodes.len(),
                        d.unserved.len(),
                    );
                    if let Some(first) = d
                        .nodes
                        .iter()
                        .find(|n| !n.physics_issues.is_empty() || !n.topology_issues.is_empty())
                    {
                        for issue in first
                            .topology_issues
                            .iter()
                            .chain(&first.physics_issues)
                            .take(4)
                        {
                            println!("    {issue}");
                        }
                    }
                }
                Err(e) => println!("{} per node {homes_per_node}: refused: {e}", profile.id),
            }
        }
    }
}
