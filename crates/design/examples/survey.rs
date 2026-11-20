//! How the optimizer does across families. `cargo run --release -p design --example survey`. All synthetic.

use std::fs;
use std::time::Instant;

use design::{DesignParts, design_region, generate};
use schema::component::ComponentLibrary;
use schema::profile::Profile;
use schema::region::{DesignControls, LayoutFamily};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).expect("fixture")).expect("parse")
}

fn main() {
    let library: ComponentLibrary = read("library/generic.json");
    for profile_file in [
        "profiles/d31_lowsplit_1002_iccap.json",
        "profiles/d31_highsplit_1218_rpd.json",
    ] {
        let profile: Profile = read(profile_file);
        for family in [
            LayoutFamily::UrbanGrid,
            LayoutFamily::Suburban,
            LayoutFamily::Rural,
        ] {
            for seed in 1..=2 {
                let map = generate(family, seed, 600);
                let start = Instant::now();
                let design = design_region(
                    &map,
                    &profile,
                    &library,
                    &DesignControls::default(),
                    &DesignParts::default(),
                )
                .expect("design");
                let issues: usize = design
                    .nodes
                    .iter()
                    .map(|n| n.topology_issues.len() + n.physics_issues.len())
                    .sum();
                let amps: u32 = design.nodes.iter().map(|n| n.amps).sum();
                let cable: f64 = design.nodes.iter().map(|n| n.hardline_ft).sum();
                let cascade = design
                    .nodes
                    .iter()
                    .map(|n| n.max_cascade)
                    .max()
                    .unwrap_or(0);
                println!(
                    "{} {family:?} seed {seed}: {} homes, {} nodes, {amps} amps, {:.1} mi hardline, N+{cascade}, {issues} issues, {} unserved, {:?}",
                    profile.id,
                    map.homes.len(),
                    design.nodes.len(),
                    cable / 5280.0,
                    design.unserved.len(),
                    start.elapsed()
                );
                let passed: u32 = design.nodes.iter().map(|n| n.homes_passed).sum();
                let modems: usize = design
                    .nodes
                    .iter()
                    .map(|n| {
                        n.plant
                            .elements
                            .iter()
                            .filter(|e| matches!(e.kind, schema::plant::ElementKind::Modem))
                            .count()
                    })
                    .sum();
                println!("    {passed} homes passed, {modems} modems");
                let mut shown = std::collections::BTreeMap::new();
                for node in &design.nodes {
                    for issue in node.physics_issues.iter().chain(&node.topology_issues) {
                        let kind = issue.split_whitespace().next().unwrap_or("?").to_string();
                        let entry = shown.entry(kind).or_insert((0, issue.clone()));
                        entry.0 += 1;
                    }
                }
                for (kind, (n, example)) in shown {
                    println!(
                        "   {n} × {kind}: {}",
                        example.chars().take(140).collect::<String>()
                    );
                }
            }
        }
    }
}
