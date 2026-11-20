//! The twin on version 2 (`docs/tasks/21-as-built-schemas.md`, "Readers"): version 1 documents load on the current version, a
//! plant with version 2 parts is diagnosed by the no-ML floor and says why, and `get_plant`, `get_parts_list` and
//! `get_fiber_plant` carry the new parts. All synthetic.

use std::path::{Path, PathBuf};

use schema::common::{ComponentId, ServiceGroupId, StructureId};
use schema::component::StructureKind;
use schema::plant::{EdgeKind, Plant, RecordStatus, Structure};
use serde_json::{Value, json};
use twin::{Twin, TwinError, call_tool, load_one_street};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn read<T: serde::de::DeserializeOwned>(file: &str) -> T {
    serde_json::from_str(&std::fs::read_to_string(fixtures().join(file)).unwrap()).unwrap()
}

#[test]
fn version_1_documents_load_on_the_current_version() {
    let t = load_one_street(&fixtures(), 1).unwrap();
    assert_eq!(t.library.schema_version, schema::component::SCHEMA_VERSION);
    for node in t.nodes.values() {
        assert_eq!(node.plant.schema_version, schema::plant::SCHEMA_VERSION);
    }
    // A node stored before task 21 is a version 1 plant. It comes forward unchanged apart from its version.
    let stored: Plant = read("one_street/plant_highsplit_rpd.json");
    assert_eq!(stored.schema_version, "1");
    let loaded = twin::state::migrate_plant(stored.clone()).unwrap();
    assert_eq!(
        Plant {
            schema_version: "1".into(),
            ..loaded
        },
        stored
    );
    let mut future = stored;
    future.schema_version = "99".into();
    assert!(matches!(
        twin::state::migrate_plant(future),
        Err(TwinError::Migration(_))
    ));
}

/// `one_street`'s high-split plant with a pole under its root and strand on its first span: version 2 parts the sim prices as
/// before, so the floor can still run.
fn with_a_pole(t: &Twin) -> Plant {
    let mut plant = t.nodes[&ServiceGroupId::new("sg-one-street-highsplit")]
        .plant
        .clone();
    plant.service_group = ServiceGroupId::new("sg-with-a-pole");
    plant.structures.push(Structure {
        id: StructureId::new("pole-1"),
        kind: StructureKind::Pole,
        component_ref: ComponentId::new("structure-generic-pole"),
        geo: None,
        record_status: RecordStatus::Built,
    });
    plant.elements[0].structure = Some(StructureId::new("pole-1"));
    if let EdgeKind::Span { support, .. } = &mut plant.edges[0].kind {
        *support = Some(ComponentId::new("strand-generic"));
    }
    plant
}

#[test]
fn a_plant_with_version_2_parts_gets_the_floor_and_says_why() {
    let root = fixtures().join("..");
    let mut t = load_one_street(&fixtures(), 1).unwrap();
    let text = std::fs::read_to_string(root.join("ml/artifacts/proposal_gbdt.json")).unwrap();
    t.model = Some(proposal::Model::from_json(&text).unwrap());
    let plant = with_a_pole(&t);
    t.add_plant(plant).unwrap();

    let reply = call_tool(
        &mut t,
        "diagnose",
        &json!({"service_group": "sg-with-a-pole"}),
    )
    .unwrap();
    assert!(reply["model"]["proposals"].is_null(), "{}", reply["model"]);
    let reason = reply["model"]["withheld_reason"].as_str().unwrap();
    assert!(reason.contains("version 2"), "{reason}");
    assert!(reason.contains("structure record: pole-1"), "{reason}");
    assert!(reason.contains("support: span-1"), "{reason}");
    assert!(
        reply["hypotheses"]
            .as_array()
            .is_some_and(|h| !h.is_empty())
    );

    // The same plant without them still gets the model.
    let reply = call_tool(
        &mut t,
        "diagnose",
        &json!({"service_group": "sg-one-street-highsplit"}),
    )
    .unwrap();
    assert!(reply["model"]["withheld_reason"].is_null());
    assert!(!reply["model"]["proposals"].is_null());
}

fn kinds(plant: &Value) -> Vec<String> {
    plant["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["properties"]["kind"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn reads_carry_the_new_parts() {
    let mut t = load_one_street(&fixtures(), 1).unwrap();
    let plant: Plant = read("one_street_as_built/plant_lowsplit_iccap.json");
    t.add_plant(plant).unwrap();
    let group = json!({"service_group": "sg-as-built-lowsplit"});

    let geojson = call_tool(&mut t, "get_plant", &group).unwrap();
    let kinds = kinds(&geojson);
    for kind in [
        "GroundBlock",
        "DropAmp",
        "Filter",
        "Cpe",
        "LineEqualizer",
        "House",
        "Structure",
    ] {
        assert!(kinds.iter().any(|k| k == kind), "no {kind} in {kinds:?}");
    }
    let amp = geojson["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["properties"]["id"] == "amp-1")
        .unwrap();
    // What `sim::align` picks (§15).
    assert_eq!(
        amp["properties"]["fitted"]["Active"]["input_eq"],
        "eq-generic-20db"
    );
    let span = geojson["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["properties"]["id"] == "span-2")
        .unwrap();
    assert_eq!(span["properties"]["fittings"].as_array().unwrap().len(), 3);
    assert_eq!(span["properties"]["support"], "strand-generic");
    // Two poles between its ends, so four points.
    assert_eq!(span["geometry"]["coordinates"].as_array().unwrap().len(), 4);

    let parts = call_tool(&mut t, "get_parts_list", &group).unwrap();
    let battery = parts["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["component_ref"] == "battery-generic-12v")
        .unwrap();
    assert_eq!(battery["quantity"], 3);

    // The fiber plant, read only, with whether each node it serves has a link each way.
    t.add_fiber_plant(read("one_street_as_built/fiber.json"))
        .unwrap();
    let region: schema::region::Region = read("one_street_as_built/region.json");
    t.regions.insert(region.id.clone(), region);
    let mut high: Plant = read("one_street_as_built/plant_highsplit_rpd.json");
    high.service_group = ServiceGroupId::new("sg-as-built-highsplit");
    t.add_plant(high).unwrap();
    let fiber = call_tool(
        &mut t,
        "get_fiber_plant",
        &json!({"fiber_id": "fiber-as-built"}),
    )
    .unwrap();
    assert_eq!(fiber["origin"], "Synthetic");
    assert_eq!(fiber["splices"], 4);
    assert_eq!(fiber["regions"][0]["issues"], json!([]));
    assert_eq!(fiber["fiber_plant"]["links"].as_array().unwrap().len(), 4);
    assert!(call_tool(&mut t, "get_fiber_plant", &json!({"fiber_id": "nothing"})).is_err());

    // Task 22 prices every part, so a read that needs the sim answers on the as-built plant.
    let status = call_tool(&mut t, "get_status_map", &group).unwrap();
    assert_eq!(status["origin"], "Synthetic");
}

/// Task 22: the twin on the as-built fixture with its fiber plant: a node's optical budget, a fiber fault at the region that reaches
/// the nodes whose links cross it, recomputing those and no others, and the level budget through the home.
#[test]
fn fiber_faults_reach_the_nodes_whose_links_cross_them() {
    let mut t = twin::load_as_built(&fixtures(), 1).unwrap();
    let low = json!({"service_group": "sg-as-built-lowsplit"});
    let high = json!({"service_group": "sg-as-built-highsplit"});
    let budget = call_tool(&mut t, "get_optical_budget", &low).unwrap();
    assert_eq!(budget["origin"], "Synthetic");
    assert_eq!(budget["region"], "region-as-built");
    let forward = &budget["link"]["forward"];
    assert_eq!(forward["up"], true);
    assert_eq!(forward["analog"], true);
    // §16's oracle, before this node's own draws within tolerance: no part on the link states one, so it's exact.
    assert!((forward["received_dbm"].as_f64().unwrap() + 2.1252).abs() < 1e-3);
    assert_eq!(budget["issues"], json!([]));
    let rpd = call_tool(&mut t, "get_optical_budget", &high).unwrap();
    assert_eq!(rpd["link"]["forward"]["analog"], false);
    assert!((rpd["link"]["forward"]["margin_db"].as_f64().unwrap() - 9.2294).abs() < 1e-3);

    // Both nodes' status, cached.
    call_tool(&mut t, "get_region_status", &json!({})).unwrap();
    let before = t.cache.stats();
    // A cut on `fiber-2`: only the low-split node's links cross it.
    let cut = json!({
        "region": "region-as-built",
        "fiber_fault": {"onset": "Instant", "kind": {"FiberCut": {"cable": "fiber-2", "pos_ft": 400.0, "repair_s": null}}},
    });
    let injected = call_tool(&mut t, "inject_fault", &cut).unwrap();
    assert_eq!(injected["fiber_faults"].as_array().unwrap().len(), 1);
    let listed = call_tool(&mut t, "list_faults", &high).unwrap();
    assert_eq!(listed["fiber_faults"].as_array().unwrap().len(), 1);
    let dark = call_tool(&mut t, "get_optical_budget", &low).unwrap();
    assert_eq!(dark["link"]["forward"]["up"], false);
    assert!(dark["link"]["forward"]["received_dbm"].is_null());
    assert!(!dark["issues"].as_array().unwrap().is_empty());
    assert_eq!(
        call_tool(&mut t, "get_optical_budget", &high).unwrap()["link"],
        rpd["link"]
    );
    call_tool(&mut t, "get_region_status", &json!({})).unwrap();
    let after = t.cache.stats();
    let recomputed = |s: &twin::cache::CacheStats, g: &str| {
        s.recomputed
            .get(&ServiceGroupId::new(g))
            .copied()
            .unwrap_or(0)
    };
    assert_eq!(
        recomputed(&after, "sg-as-built-lowsplit"),
        recomputed(&before, "sg-as-built-lowsplit") + 1
    );
    assert_eq!(
        recomputed(&after, "sg-as-built-highsplit"),
        recomputed(&before, "sg-as-built-highsplit")
    );

    // A fiber fault on a cable the region doesn't have is refused, and clearing the region brings it back.
    let bad = json!({
        "region": "region-as-built",
        "fiber_fault": {"onset": "Instant", "kind": {"FiberCut": {"cable": "nothing", "pos_ft": 1.0, "repair_s": null}}},
    });
    assert!(call_tool(&mut t, "inject_fault", &bad).is_err());
    assert!(
        call_tool(
            &mut t,
            "inject_fault",
            &json!({"region": "region-as-built"})
        )
        .is_err()
    );
    call_tool(
        &mut t,
        "clear_faults",
        &json!({"region": "region-as-built"}),
    )
    .unwrap();
    assert_eq!(
        call_tool(&mut t, "get_optical_budget", &low).unwrap()["link"],
        budget["link"]
    );

    // The level budget reads through each home: from its tap port to the modem.
    let levels = call_tool(&mut t, "get_level_budget", &low).unwrap();
    let modem_2 = levels["modems"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["modem"] == "modem-2")
        .unwrap();
    assert_eq!(modem_2["premises"]["ground_block"], "gb-2");
    assert_eq!(modem_2["premises"]["tap"], "tap-1");
    let at_hi = modem_2["premises"]["loss_from_tap_port_db"]["at_hi"]
        .as_f64()
        .unwrap();
    assert!(at_hi > 10.0 && at_hi < 25.0, "{at_hi}");
}

/// Task 24's wiring: a model trained on version 2 plant answers for a plant with version 2 parts, and the shipped model still
/// answers for one without, each refused on the other's. The model here is a stand-in with no trees, enough to show which
/// one the twin asks.
#[test]
fn each_plant_gets_the_model_trained_on_its_kind() {
    let root = fixtures().join("..");
    let mut t = load_one_street(&fixtures(), 1).unwrap();
    let text = std::fs::read_to_string(root.join("ml/artifacts/proposal_gbdt.json")).unwrap();
    t.model = Some(proposal::Model::from_json(&text).unwrap());
    let stand_in = json!({
        "name": "stand-in", "observation_schema": schema::observation::SCHEMA_VERSION,
        "fault_schema": schema::fault::SCHEMA_VERSION, "trained_on": "Synthetic", "features": ["k_premises"],
        "component": {"baseline": [0.0], "trees": []}, "class": {"baseline": [0.0], "trees": []},
        "classes": ["InHomeWiring"], "temperature": 1.0, "plant_schema": "2"
    });
    t.as_built_model = Some(proposal::Proposer::from_json(&stand_in.to_string()).unwrap());
    t.add_plant(with_a_pole(&t)).unwrap();
    let ask =
        |t: &mut Twin, id: &str| call_tool(t, "diagnose", &json!({"service_group": id})).unwrap();
    let as_built = ask(&mut t, "sg-with-a-pole");
    assert!(
        as_built["model"]["withheld_reason"].is_null(),
        "{}",
        as_built["model"]
    );
    assert!(!as_built["model"]["proposals"].is_null());
    let shipped = ask(&mut t, "sg-one-street-highsplit");
    assert!(!shipped["model"]["proposals"].is_null());
    // Without the version 2 model, the floor again, and why.
    t.as_built_model = None;
    let floor = ask(&mut t, "sg-with-a-pole");
    assert!(
        floor["model"]["withheld_reason"]
            .as_str()
            .is_some_and(|r| r.contains("version 2"))
    );
    assert!(proposal::trained_for(Some("2"), true) && !proposal::trained_for(Some("2"), false));
    assert!(proposal::trained_for(None, false) && !proposal::trained_for(None, true));
}

/// Task 24's model itself answers for the hand-built as-built node, and the shipped one isn't asked about it.
#[test]
fn the_as_built_model_answers_for_an_as_built_plant() {
    let root = fixtures().join("..");
    let mut t = load_one_street(&fixtures(), 1).unwrap();
    let shipped = std::fs::read_to_string(root.join("ml/artifacts/proposal_gbdt.json")).unwrap();
    t.model = Some(proposal::Model::from_json(&shipped).unwrap());
    let text = std::fs::read_to_string(root.join("ml/artifacts/as_built/gbdt_maps.json")).unwrap();
    let model = proposal::Proposer::from_json(&text).unwrap();
    assert_eq!(model.plant_schema(), Some("2"));
    t.as_built_model = Some(model);
    let plant: Plant = read("one_street_as_built/plant_lowsplit_iccap.json");
    t.add_plant(plant).unwrap();
    let reply = call_tool(
        &mut t,
        "diagnose",
        &json!({"service_group": "sg-as-built-lowsplit"}),
    )
    .unwrap();
    assert!(
        reply["model"]["withheld_reason"].is_null(),
        "{}",
        reply["model"]
    );
    let proposals = reply["model"]["proposals"].as_array().unwrap();
    assert!(!proposals.is_empty());
    // It ranks the version 2 places too: a home's parts and a drop that records its fittings are candidates.
    let total: f64 = proposals
        .iter()
        .filter_map(|p| p["probability"].as_f64())
        .sum();
    assert!(total > 0.0 && total <= 1.0 + 1e-9, "{total}");
}
