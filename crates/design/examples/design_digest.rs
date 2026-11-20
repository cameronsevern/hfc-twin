//! A digest of every generated design, to show a change to the optimizer left the training layouts alone.
//! `cargo run --release -p design --example design_digest`.
//!
//! The first line is the digest task 14 held, `7298d7aca5f41752`, over the designs with their version strings set back to `"1"`:
//! the plant and region schemas moved to version 2 in task 21 and nothing else in a design did. It uses `std`'s hasher, which
//! promises nothing across Rust releases, so the second line is the stable digest `tests/pinned.rs` holds: FNV-1a over the
//! JSON with every version string removed.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};

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
    let mut all = DefaultHasher::new();
    let mut stable = String::new();
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
            for seed in 1..=8 {
                for homes in [300, 1000] {
                    let map = generate(family, seed, homes);
                    let controls = DesignControls {
                        seed,
                        ..DesignControls::default()
                    };
                    let design =
                        design_region(&map, &profile, &library, &controls, &DesignParts::default())
                            .expect("design");
                    let mut as_v1 = design.clone();
                    as_v1.region.schema_version = "1".into();
                    for node in &mut as_v1.nodes {
                        node.plant.schema_version = "1".into();
                    }
                    serde_json::to_string(&as_v1).expect("json").hash(&mut all);
                    let mut value = serde_json::to_value(&design).expect("json");
                    strip_versions(&mut value);
                    let one = sim::rng::hash_str(&serde_json::to_string(&value).expect("json"));
                    stable.push_str(&format!("{one:016x}\n"));
                }
            }
        }
    }
    println!("{:016x}", all.finish());
    println!("{:016x}", sim::rng::hash_str(&stable));
}

/// Removes every `schema_version` key, at any depth.
fn strip_versions(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("schema_version");
            map.values_mut().for_each(strip_versions);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_versions),
        _ => {}
    }
}
