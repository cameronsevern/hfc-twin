//! A pinned digest of every generated design (`docs/tasks/21-as-built-schemas.md`, "Before anything else"). The
//! version 1 designer is frozen (`docs/as-built-plan.md`, rule 7): the shipped models were trained on its layouts, so
//! a change that moves this digest is a bug, not a reason to update it.
//!
//! It covers what `examples/design_digest.rs` covers, and hashes the JSON with every `schema_version` key removed,
//! using FNV-1a (`sim::rng::hash_str`), which is stable across Rust releases where `std`'s hasher isn't.

use std::fs;

use design::{DesignParts, design_region, generate};
use schema::component::ComponentLibrary;
use schema::profile::Profile;
use schema::region::{DesignControls, LayoutFamily};
use serde_json::Value;
use sim::rng::hash_str;

/// Pinned on the tree at commit `40d0133`, before any edit for task 21.
const DESIGN_DIGEST: &str = "3caa35431bc358fd";

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
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

#[test]
fn every_generated_design_is_pinned() {
    let library: ComponentLibrary = read("library/generic.json");
    let mut designs = String::new();
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
                            .unwrap();
                    let mut value = serde_json::to_value(&design).unwrap();
                    strip_versions(&mut value);
                    let one = hash_str(&serde_json::to_string(&value).unwrap());
                    designs.push_str(&format!("{one:016x}\n"));
                }
            }
        }
    }
    let found = format!("{:016x}", hash_str(&designs));
    assert_eq!(found, DESIGN_DIGEST, "a generated design moved");
}
