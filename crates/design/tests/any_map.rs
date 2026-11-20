//! Task 25: the version 2 designer on real streets, with the page's controls, leaves no issue on any node it builds. Each
//! fixture under `fixtures/osm/any_map/` is a cut of a real town where a cause first showed: apartment blocks of 31 to 268 units
//! (Seattle Ballard), an amp cascade outside N/10 + 3 dB and a modem over the spread limit (Albany), and modems told to
//! transmit under their floor (Brattleboro). The streets are real; the plant on them is invented (invariant 11).

use std::collections::BTreeSet;
use std::fs;

use design::osm::read_overpass;
use schema::component::ComponentLibrary;
use schema::design_rules::DesignRules;
use schema::plant::Installation;
use schema::profile::Profile;
use schema::region::{DesignControls, DesignMode, StreetMap};

fn text(path: &str) -> String {
    fs::read_to_string(format!(
        "{}/../../fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    serde_json::from_str(&text(path)).unwrap()
}

fn library() -> ComponentLibrary {
    schema::component::migrate(read("library/generic.json")).unwrap()
}

fn profile(name: &str) -> Profile {
    schema::profile::migrate(read(&format!("profiles/{name}.json"))).unwrap()
}

fn map(name: &str) -> StreetMap {
    let (map, _) = read_overpass(
        name,
        &text(&format!("osm/any_map/{name}.json")),
        Installation::Aerial,
    )
    .unwrap();
    map
}

/// The page's controls (`ui/index.html`): 120 homes a node, take rate 0.6, the generic rules, homes over the ceiling built.
fn controls(rules: &DesignRules) -> DesignControls {
    DesignControls {
        homes_per_node: 120,
        take_rate: 0.6,
        d30_share: 0.3,
        max_drop_ft: 250.0,
        seed: 1,
        rules_id: Some(rules.id.clone()),
        mode: Some(DesignMode::Greenfield),
        build_over_ceiling: true,
        ..DesignControls::default()
    }
}

fn holds(name: &str, profile_name: &str) {
    let library = library();
    let rules: DesignRules = read("design_rules/generic.json");
    let map = map(name);
    let profile = profile(profile_name);
    let d = design::v2::design(&map, &profile, &library, &rules, &controls(&rules)).unwrap();
    assert!(
        !d.nodes.is_empty(),
        "{name} on {profile_name}: nothing built"
    );
    for node in &d.nodes {
        assert!(
            node.issues.is_empty(),
            "{name} on {profile_name}, {}: {:?}",
            node.plant.service_group,
            &node.issues[..node.issues.len().min(4)]
        );
    }
    // Every home is built, or unserved and said so; one over the ceiling is built and marked.
    let built: u32 = d.nodes.iter().map(|n| n.homes_passed).sum();
    assert_eq!(
        built as usize + d.unserved.len(),
        map.homes.len(),
        "{name} on {profile_name}"
    );
    let unserved: BTreeSet<&String> = d.unserved.iter().collect();
    for over in &d.over_ceiling {
        assert!(over.homes.iter().all(|h| !unserved.contains(h)));
    }
}

#[test]
fn an_apartment_block_designs_without_issues_on_both_profiles() {
    holds("seattle_ballard_block_wa", "d31_lowsplit_1002_iccap");
    holds("seattle_ballard_block_wa", "d31_highsplit_1218_rpd");
}

#[test]
fn a_long_cascade_designs_without_issues() {
    holds("albany_center_ny", "d31_lowsplit_1002_iccap");
}

#[test]
fn modems_near_their_amp_transmit_above_their_floor() {
    holds("brattleboro_center_vt", "d31_highsplit_1218_rpd");
}

#[test]
fn an_apartment_block_is_fed_a_group_at_a_time() {
    let library = library();
    let rules: DesignRules = read("design_rules/generic.json");
    let map = map("seattle_ballard_block_wa");
    let profile = profile("d31_lowsplit_1002_iccap");
    let d = design::v2::design(&map, &profile, &library, &rules, &controls(&rules)).unwrap();
    // The 268-unit building: its units' modems hang in groups behind a ground block, never more than 8 to a feed.
    let block = |id: &str| id.starts_with("building-217360877-");
    let units: Vec<&String> = d
        .nodes
        .iter()
        .flat_map(|n| n.templates.iter())
        .filter(|(home, _)| block(home))
        .map(|(_, t)| t)
        .collect();
    assert!(!units.is_empty());
    assert!(units.iter().all(|t| t.starts_with("mdu-")), "{units:?}");
    // Its ground blocks, one a group with service, named by the group's first unit: `<node>-h<unit>-gb`.
    let unit_of = |id: &str| -> Option<usize> {
        let rest = id.strip_suffix("-gb")?;
        rest.rsplit_once("-h")?.1.parse().ok()
    };
    let drops_to_block = d
        .nodes
        .iter()
        .flat_map(|n| &n.plant.elements)
        .filter(|e| e.kind == schema::plant::ElementKind::GroundBlock)
        .filter_map(|e| unit_of(e.id.as_str()))
        .filter(|h| block(&map.homes[*h].id))
        .count();
    // 268 units in groups of 8 is 34 feeds, less any group where no unit takes service.
    assert!((30..=34).contains(&drops_to_block), "{drops_to_block}");
}

#[test]
fn the_same_seed_designs_the_same_bytes() {
    let library = library();
    let rules: DesignRules = read("design_rules/generic.json");
    let map = map("seattle_ballard_block_wa");
    let profile = profile("d31_highsplit_1218_rpd");
    let a = design::v2::design(&map, &profile, &library, &rules, &controls(&rules)).unwrap();
    let b = design::v2::design(&map, &profile, &library, &rules, &controls(&rules)).unwrap();
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}
