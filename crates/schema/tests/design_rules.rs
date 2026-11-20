//! `fixtures/design_rules/`: the design rules as data (`docs/tasks/21-as-built-schemas.md`, deliverable 5; version 2 in
//! `docs/tasks/23-as-built-designer.md`). Each file parses without loss, names only parts the generic library has, has every
//! value the version 2 designer reads, and every value it calls sourced names a reference note section that exists
//! (`docs/as-built-plan.md`, rule 6).

use std::fs;
use std::path::{Path, PathBuf};

use schema::component::ComponentLibrary;
use schema::design_rules::{Basis, DesignRules};
use serde_json::Value;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load<T: serde::de::DeserializeOwned + serde::Serialize>(file: &str) -> T {
    let text = fs::read_to_string(repo().join(file)).unwrap();
    let value: T = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{file}: {e}"));
    let raw: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        serde_json::to_value(&value).unwrap(),
        raw,
        "{file} doesn't round-trip"
    );
    value
}

/// The rules files, each with the library it's checked against.
const FILES: [&str; 2] = [
    "fixtures/design_rules/generic.json",
    "fixtures/design_rules/legacy-750.json",
];

#[test]
fn the_rules_validate_against_both_libraries_and_are_complete() {
    for file in FILES {
        let rules: DesignRules = load(file);
        assert_eq!(rules.schema_version, schema::design_rules::SCHEMA_VERSION);
        for library in [
            "fixtures/library/generic.json",
            "fixtures/library/sourced.json",
        ] {
            // The sourced library carries the sheets' own nulls, so it's read, not round-tripped.
            let text = fs::read_to_string(repo().join(library)).unwrap();
            let library: ComponentLibrary = serde_json::from_str(&text).unwrap();
            rules
                .validate(&library)
                .unwrap_or_else(|e| panic!("{file}: {e:?}"));
        }
        rules
            .for_design()
            .unwrap_or_else(|e| panic!("{file}: {e:?}"));
    }
}

#[test]
fn the_legacy_profile_reads() {
    let profile: schema::profile::Profile = load("fixtures/profiles/d30_lowsplit_750_legacy.json");
    assert_eq!(profile.schema_version, schema::profile::SCHEMA_VERSION);
    assert_eq!(profile.docsis, schema::profile::Docsis::D30);
}

/// A rule with no basis doesn't parse, and a sourced basis that isn't `file.md §n` fails validation.
#[test]
fn a_rule_with_no_basis_fails() {
    let text = fs::read_to_string(repo().join(FILES[0])).unwrap();
    let mut raw: Value = serde_json::from_str(&text).unwrap();
    raw["node"]["max_actives"]
        .as_object_mut()
        .unwrap()
        .remove("basis");
    assert!(serde_json::from_value::<DesignRules>(raw).is_err());

    let library: ComponentLibrary = load("fixtures/library/generic.json");
    let mut rules: DesignRules = load(FILES[0]);
    rules.node.max_cascade.basis = Basis::Sourced {
        note: "from memory".into(),
    };
    let errors = rules.validate(&library).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, schema::design_rules::RulesError::BadNote { .. })),
        "{errors:?}"
    );
}

#[test]
fn every_sourced_value_names_a_section_that_exists() {
    for file in FILES {
        every_sourced_value_in(file);
    }
}

fn every_sourced_value_in(file: &str) {
    let rules: DesignRules = load(file);
    let mut sourced = 0;
    for (rule, basis) in rules.bases() {
        let Basis::Sourced { note } = basis else {
            continue;
        };
        sourced += 1;
        let (file, section) = note
            .split_once(" §")
            .unwrap_or_else(|| panic!("{rule}: {note} isn't `file.md §n`"));
        let text = fs::read_to_string(repo().join("docs/reference").join(file))
            .unwrap_or_else(|e| panic!("{rule}: {file}: {e}"));
        let heading = |prefix: &str| {
            text.lines().any(|l| {
                l.starts_with(&format!("{prefix} {section}."))
                    || l.starts_with(&format!("{prefix} {section} "))
            })
        };
        assert!(
            heading("##") || heading("###"),
            "{rule}: no section {section} in {file}"
        );
    }
    assert!(sourced > 0);
}

#[test]
fn validate_rejects_broken_rules() {
    let library: ComponentLibrary = load("fixtures/library/generic.json");
    let mut rules: DesignRules = load("fixtures/design_rules/generic.json");
    rules.roles[0].cable_ref.value = schema::common::ComponentId::new("drop-rg6");
    rules.premises[0].share.value += 0.5;
    rules.drops.reverse();
    let tap = schema::common::ComponentId::new("tap-generic-23db-2p-fp");
    rules.tap_families[1]
        .parts
        .as_mut()
        .unwrap()
        .value
        .push(tap);
    rules
        .costs
        .parts
        .retain(|c| c.component_ref.as_str() != "pi-generic-1218");
    rules.profiles.as_mut().unwrap()[0]
        .premises
        .value
        .push("no-such-home".into());
    let errors = rules.validate(&library).unwrap_err();
    use schema::design_rules::RulesError::*;
    assert!(
        errors.iter().any(|e| matches!(e, WrongClass { .. })),
        "{errors:?}"
    );
    assert!(errors.iter().any(|e| matches!(e, Shares(_))), "{errors:?}");
    assert!(errors.iter().any(|e| matches!(e, DropOrder)), "{errors:?}");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Misfit { why, .. } if why.contains("2-port"))),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Misfit { why, .. } if why.contains("no-such-home"))),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, MissingCost(p) if p == "pi-generic-1218")),
        "{errors:?}"
    );
}
