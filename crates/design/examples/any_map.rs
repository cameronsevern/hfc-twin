//! The version 2 designer on real OpenStreetMap extracts, with the page's controls: does any map design without issues?
//! `cargo run --release -p design --example any_map -- [--lift] [--homes-per-node N] FILE.json...` prints one JSON line
//! per map and profile. `--lift` builds the nodes the rules' cost ceiling puts over it. The streets are real; the plant on
//! them is invented and every number is synthetic.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::Instant;

use design::osm::read_overpass;
use schema::component::ComponentLibrary;
use schema::design_rules::DesignRules;
use schema::plant::{Installation, Plant};
use schema::profile::Profile;
use schema::region::{DesignControls, DesignMode};
use serde_json::{Value, json};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).expect("fixture")).expect("parse")
}

/// The issue's kind: the first word of its text.
fn kind(issue: &str) -> String {
    issue
        .split([' ', '{', '(', ':'])
        .next()
        .unwrap_or("")
        .to_string()
}

fn main() {
    let mut lift = false;
    let mut only: Option<String> = None;
    let mut library_path: Option<String> = None;
    let mut homes_per_node = 120;
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--lift" => lift = true,
            "--only" => only = args.next(),
            "--library" => library_path = args.next(),
            "--homes-per-node" => {
                homes_per_node = args.next().and_then(|n| n.parse().ok()).expect("a number")
            }
            _ => files.push(a),
        }
    }
    let library: ComponentLibrary = schema::component::migrate(match &library_path {
        Some(path) => {
            serde_json::from_str(&fs::read_to_string(path).expect("library")).expect("parse")
        }
        None => read("library/generic.json"),
    })
    .expect("library");
    let rules: DesignRules = read("design_rules/generic.json");
    for file in &files {
        let name = Path::new(file)
            .file_stem()
            .map_or(file.clone(), |s| s.to_string_lossy().to_string());
        let text = fs::read_to_string(file).expect("read");
        let (map, _) = match read_overpass(&name, &text, Installation::Aerial) {
            Ok(m) => m,
            Err(e) => {
                println!("{}", json!({"map": name, "error": format!("import: {e}")}));
                continue;
            }
        };
        for profile_file in [
            "profiles/d31_lowsplit_1002_iccap.json",
            "profiles/d31_highsplit_1218_rpd.json",
        ] {
            if only
                .as_ref()
                .is_some_and(|o| !profile_file.contains(o.as_str()))
            {
                continue;
            }
            let profile: Profile = schema::profile::migrate(read(profile_file)).expect("profile");
            let controls = DesignControls {
                homes_per_node,
                take_rate: 0.6,
                d30_share: 0.3,
                max_drop_ft: 250.0,
                seed: 1,
                rules_id: Some(rules.id.clone()),
                mode: Some(DesignMode::Greenfield),
                build_over_ceiling: lift,
                ..DesignControls::default()
            };
            let start = Instant::now();
            let result = design::v2::design(&map, &profile, &library, &rules, &controls);
            let took_s = start.elapsed().as_secs_f64();
            let d = match result {
                Ok(d) => d,
                Err(e) => {
                    println!(
                        "{}",
                        json!({"map": name, "profile": profile.id.to_string(), "homes": map.homes.len(), "error": e.to_string(), "took_s": took_s})
                    );
                    continue;
                }
            };
            if std::env::var_os("HFC_NOTES").is_some() {
                for n in d.nodes.iter().filter(|n| !n.issues.is_empty()) {
                    eprintln!(
                        "NODE {} homes {} issues {} notes {:#?}",
                        n.plant.service_group,
                        n.homes_passed,
                        n.issues.len(),
                        n.notes
                    );
                }
            }
            let kit = design::v2::kit::Kit::new(&rules, &library, &profile, None).expect("kit");
            let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
            let mut by_node: BTreeMap<String, usize> = BTreeMap::new();
            let mut examples: Vec<String> = Vec::new();
            for n in &d.nodes {
                for i in &n.issues {
                    *by_kind.entry(kind(i)).or_default() += 1;
                    *by_node
                        .entry(n.plant.service_group.to_string())
                        .or_default() += 1;
                    if examples
                        .iter()
                        .filter(|e: &&String| e.contains(&format!(": {}", kind(i))))
                        .count()
                        < 3
                    {
                        examples.push(format!("{}: {i}", n.plant.service_group));
                    }
                }
            }
            let plants: Vec<Plant> = d.nodes.iter().map(|n| n.plant.clone()).collect();
            let bars = design::v2::bars::all(
                &design::v2::bars::Design {
                    kit: &kit,
                    region: &d.region,
                    plants: &plants,
                    fiber: &d.fiber,
                },
                &BTreeSet::new(),
            );
            let failed: Vec<Value> = bars
                .iter()
                .filter(|b| !b.met)
                .map(|b| json!({"bar": b.name, "failed": b.failed, "checked": b.checked, "detail": b.detail}))
                .collect();
            // Why each unserved home is: no structure within a drop of it, a piece of street the hub doesn't reach, or an
            // area the designer couldn't split under the rules' cascade or active limit.
            let mut unserved_by: BTreeMap<&str, usize> = BTreeMap::new();
            {
                let s = design::v2::pipeline::place_structures(&kit, &map, &controls)
                    .expect("structures");
                let hub = s.nearest(map.headend);
                let dist = hub.map(|h| design::v2::nodes::dijkstra(&s, h).0);
                let index: BTreeMap<&str, usize> = map
                    .homes
                    .iter()
                    .enumerate()
                    .map(|(i, h)| (h.id.as_str(), i))
                    .collect();
                for id in &d.unserved {
                    let why = match index.get(id.as_str()).and_then(|i| s.homes[*i]) {
                        None => "no structure within a drop",
                        Some(at) if dist.as_ref().is_none_or(|d| !d[at.site].is_finite()) => {
                            "no street the hub reaches"
                        }
                        Some(_) => "an area that couldn't split",
                    };
                    *unserved_by.entry(why).or_default() += 1;
                }
            }
            let over: usize = d.over_ceiling.iter().map(|o| o.homes.len()).sum();
            let built: u32 = d.nodes.iter().map(|n| n.homes_passed).sum();
            println!(
                "{}",
                json!({
                    "map": name,
                    "profile": profile.id.to_string(),
                    "lift": lift,
                    "homes": map.homes.len(),
                    "nodes": d.nodes.len(),
                    "homes_built": built,
                    "over_ceiling": over,
                    "unserved": d.unserved.len(),
                    "unserved_by": unserved_by,
                    "issues": by_kind.values().sum::<usize>(),
                    "by_kind": by_kind,
                    "nodes_with_issues": by_node.len(),
                    "examples": examples,
                    "bars_failed": failed,
                    "candidates": d.candidates.iter().map(|c| json!([c.express_cable.to_string(), c.step, c.nodes, c.issues, c.usd_per_home.round(), c.kept])).collect::<Vec<_>>(),
                    "notes": d.nodes.iter().map(|n| n.notes.len()).sum::<usize>(),
                    "took_s": (took_s * 10.0).round() / 10.0,
                })
            );
        }
    }
}
