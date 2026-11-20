//! The whole lifecycle through `tools/call`, on simulated plant.

use std::path::Path;

use mcp_server::handle;
use serde_json::{Value, json};
use twin::{Twin, load_one_street};

fn twin() -> Twin {
    load_one_street(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"),
        1,
    )
    .unwrap()
}

fn raw(twin: &mut Twin, name: &str, arguments: Value) -> Value {
    handle(
        twin,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
    )
    .unwrap()
}

fn call(twin: &mut Twin, name: &str, arguments: Value) -> Value {
    let reply = raw(twin, name, arguments);
    assert_eq!(reply["result"]["isError"], json!(false), "{name}: {reply}");
    reply["result"]["structuredContent"].clone()
}

fn refused(twin: &mut Twin, name: &str, arguments: Value) -> String {
    let reply = raw(twin, name, arguments);
    assert_eq!(reply["result"]["isError"], json!(true), "{name}: {reply}");
    reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string()
}

fn connector(span: &str, pos_ft: f64, rl: f64) -> Value {
    json!({"onset": "Instant", "kind": {"ConnectorReflection": {"location": {"Span": {"span_id": span, "pos_ft": pos_ft}}, "return_loss_db": rl}}})
}

fn bad_drop(modem: &str, loss: f64) -> Value {
    json!({"onset": "Instant", "kind": {"BadDrop": {"modem_id": modem, "extra_loss_db": loss, "return_loss_db": null}}})
}

const GROUPS: [&str; 2] = ["sg-one-street-lowsplit", "sg-one-street-highsplit"];

#[test]
fn the_right_fix_verifies_on_both_profiles() {
    for sg in GROUPS {
        let mut t = twin();
        call(
            &mut t,
            "inject_fault",
            json!({"service_group": sg, "fault": connector("span-3", 150.0, 6.0)}),
        );
        let answer = call(&mut t, "diagnose", json!({"service_group": sg}));
        let id = answer["work_orders"][0]["id"].clone();
        assert_eq!(answer["work_orders"][0]["status"], json!("Proposed"));

        let order = call(
            &mut t,
            "dispatch_work_order",
            json!({"service_group": sg, "id": id}),
        );
        assert_eq!(order["status"], json!("Dispatched"));
        call(
            &mut t,
            "complete_work_order",
            json!({"service_group": sg, "id": id}),
        );
        assert_eq!(
            call(&mut t, "list_faults", json!({"service_group": sg}))["faults"],
            json!([])
        );

        let result = call(
            &mut t,
            "verify_work_order",
            json!({"service_group": sg, "id": id}),
        );
        assert_eq!(result["status"], json!("Verified"), "{result}");
        assert_eq!(result["verdict"]["still_affected"], json!([]));
        assert!(!result["affected_at_issue"].as_array().unwrap().is_empty());

        let labels = call(&mut t, "list_verified_labels", json!({}));
        assert_eq!(labels["labels"].as_array().unwrap().len(), 1);
        assert_eq!(labels["labels"][0]["cause"], json!("ConnectorReflection"));
        assert_eq!(labels["labels"][0]["origin"], json!("Synthetic"));
    }
}

#[test]
fn a_wrong_diagnosis_fails_verification() {
    let sg = GROUPS[1];
    let mut t = twin();
    call(
        &mut t,
        "inject_fault",
        json!({"service_group": sg, "fault": bad_drop("modem-4", 10.0)}),
    );
    let answer = call(&mut t, "diagnose", json!({"service_group": sg}));
    let id = answer["work_orders"][0]["id"].clone();
    // The crew goes where the order says. Something else is wrong too, which stays.
    call(
        &mut t,
        "inject_fault",
        json!({"service_group": sg, "fault": bad_drop("modem-8", 10.0)}),
    );
    call(
        &mut t,
        "dispatch_work_order",
        json!({"service_group": sg, "id": id}),
    );
    call(
        &mut t,
        "complete_work_order",
        json!({"service_group": sg, "id": id}),
    );
    // Modem-4 is back, and modem-8 wasn't in the evidence, so this order did clear what it saw.
    let first = call(
        &mut t,
        "verify_work_order",
        json!({"service_group": sg, "id": id}),
    );
    assert_eq!(first["status"], json!("Verified"), "{first}");

    // Now one that names the wrong place: the symptoms are real and the crew's somewhere else.
    let answer = call(&mut t, "diagnose", json!({"service_group": sg}));
    let id = answer["work_orders"][0]["id"].clone();
    assert_eq!(answer["hypotheses"][0]["components"][0], json!("modem-8"));
    {
        let node = t
            .nodes
            .get_mut(&schema::common::ServiceGroupId::new(sg))
            .unwrap();
        let order = node
            .work_orders
            .iter_mut()
            .find(|o| serde_json::to_value(o.id).unwrap() == id)
            .unwrap();
        order.parameters = serde_json::from_value(bad_drop("modem-1", 10.0)).unwrap();
    }
    call(
        &mut t,
        "dispatch_work_order",
        json!({"service_group": sg, "id": id}),
    );
    call(
        &mut t,
        "complete_work_order",
        json!({"service_group": sg, "id": id}),
    );
    let result = call(
        &mut t,
        "verify_work_order",
        json!({"service_group": sg, "id": id}),
    );
    assert_eq!(result["status"], json!("NotVerified"), "{result}");
    assert_eq!(result["verdict"]["still_affected"], json!(["modem-8"]));
    // It's still there, and the label list hasn't gained a false one.
    assert_eq!(
        call(&mut t, "list_faults", json!({"service_group": sg}))["faults"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(&mut t, "list_verified_labels", json!({}))["labels"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn transitions_out_of_order_are_refused() {
    let sg = GROUPS[0];
    let mut t = twin();
    call(
        &mut t,
        "inject_fault",
        json!({"service_group": sg, "fault": bad_drop("modem-4", 10.0)}),
    );
    let answer = call(&mut t, "diagnose", json!({"service_group": sg}));
    let id = answer["work_orders"][0]["id"].clone();
    assert!(
        refused(
            &mut t,
            "complete_work_order",
            json!({"service_group": sg, "id": id})
        )
        .contains("Dispatched")
    );
    assert!(
        refused(
            &mut t,
            "verify_work_order",
            json!({"service_group": sg, "id": id})
        )
        .contains("Completed")
    );
    call(
        &mut t,
        "dispatch_work_order",
        json!({"service_group": sg, "id": id}),
    );
    assert!(
        refused(
            &mut t,
            "dispatch_work_order",
            json!({"service_group": sg, "id": id})
        )
        .contains("Proposed")
    );
    assert!(
        refused(
            &mut t,
            "dispatch_work_order",
            json!({"service_group": sg, "id": "00000000-0000-0000-0000-000000000000"})
        )
        .contains("no work order")
    );
    // The fault's still there: nothing's been repaired by asking out of turn.
    assert_eq!(
        call(&mut t, "list_faults", json!({"service_group": sg}))["faults"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn an_rpd_fault_verifies_from_its_own_state() {
    let sg = GROUPS[1];
    let mut t = twin();
    let fault = json!({"onset": "Instant", "kind": {"GcpLoss": {"rpd_id": "rpd-1", "duration_s": 100000.0}}});
    call(
        &mut t,
        "inject_fault",
        json!({"service_group": sg, "fault": fault}),
    );
    let answer = call(&mut t, "diagnose", json!({"service_group": sg}));
    assert_eq!(answer["hypotheses"][0]["class"], json!("GcpLoss"));
    let id = answer["work_orders"][0]["id"].clone();
    call(
        &mut t,
        "dispatch_work_order",
        json!({"service_group": sg, "id": id}),
    );
    call(
        &mut t,
        "complete_work_order",
        json!({"service_group": sg, "id": id}),
    );
    let result = call(
        &mut t,
        "verify_work_order",
        json!({"service_group": sg, "id": id}),
    );
    assert_eq!(result["status"], json!("Verified"), "{result}");
}

#[test]
fn design_to_built_and_migrated_through_tools() {
    let mut t = twin();
    let map = call(
        &mut t,
        "generate_layout",
        json!({"family": "Suburban", "seed": 3, "homes": 300}),
    );
    assert_eq!(map["source"]["Generated"]["family"], json!("Suburban"));
    let map_id = map["map_id"].clone();
    let design = call(
        &mut t,
        "design_region",
        json!({"map_id": map_id, "profile": "d31_lowsplit_1002_iccap"}),
    );
    assert_eq!(design["origin"], json!("Synthetic"));
    let nodes = design["nodes"].as_array().unwrap().clone();
    assert!(nodes.len() >= 2);
    for node in &nodes {
        assert_eq!(node["physics_issues"], json!([]));
        assert_eq!(node["topology_issues"], json!([]));
        assert!(!node["parts"].as_array().unwrap().is_empty());
    }
    let sg = nodes[0]["service_group"].clone();

    let plant = call(&mut t, "get_plant", json!({"service_group": sg}));
    assert!(
        plant["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["properties"]["record_status"] == json!("Designed"))
    );
    let built = call(&mut t, "mark_built", json!({"service_group": sg}));
    assert!(built["records"]["Designed"].is_null());
    assert!(built["records"]["Built"].as_u64().unwrap() > 0);

    let status = call(&mut t, "get_status_map", json!({"service_group": sg}));
    assert_eq!(status["who"]["out_of_service"], json!(0));

    let plan = call(
        &mut t,
        "plan_migration",
        json!({"service_group": sg, "to_profile": "d31_highsplit_1218_rpd"}),
    );
    assert!(plan["converted_plant"].is_null());
    assert!(
        plan["us_capacity_after_mbps"].as_f64().unwrap()
            > plan["us_capacity_before_mbps"].as_f64().unwrap()
    );
    assert!(
        plan["optics_what_if"]["label"]
            .as_str()
            .unwrap()
            .contains("What-if")
    );
    // Planning changes nothing.
    let listed = call(&mut t, "list_service_groups", json!({}));
    let this = |listed: &Value| {
        listed["service_groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["service_group"] == sg)
            .unwrap()
            .clone()
    };
    assert_eq!(this(&listed)["profile"], json!("d31_lowsplit_1002_iccap"));

    let applied = call(
        &mut t,
        "apply_migration",
        json!({"service_group": sg, "to_profile": "d31_highsplit_1218_rpd"}),
    );
    assert_eq!(applied["profile"], json!("d31_highsplit_1218_rpd"));
    let listed = call(&mut t, "list_service_groups", json!({}));
    assert_eq!(this(&listed)["profile"], json!("d31_highsplit_1218_rpd"));
    call(&mut t, "get_status_map", json!({"service_group": sg}));

    assert!(
        refused(
            &mut t,
            "design_region",
            json!({"map_id": "nope", "profile": "d31_lowsplit_1002_iccap"})
        )
        .contains("no map")
    );
}

/// An order is a truck roll. The floor names a fault on this healthy designed node at high posterior, and nobody reads worse than normal, so
/// it raises none.
#[test]
fn a_healthy_node_raises_no_work_order() {
    let mut t = twin();
    let map = call(
        &mut t,
        "generate_layout",
        json!({"family": "Suburban", "seed": 7, "homes": 1000}),
    );
    let design = call(
        &mut t,
        "design_region",
        json!({"map_id": map["map_id"], "profile": "d31_highsplit_1218_rpd", "controls": {
            "homes_per_node": 120, "cascade_limit": null, "take_rate": 0.6, "d30_share": 0.3,
            "max_drop_ft": 250.0, "pinned_nodes": [], "seed": 7,
        }}),
    );
    let sg = design["nodes"][0]["service_group"].clone();
    let answer = call(&mut t, "diagnose", json!({"service_group": sg}));
    assert!(
        answer["no_ml"]["hypotheses"][0]["class"].is_string(),
        "this test needs a false alarm to hold back"
    );
    assert_eq!(answer["who"]["affected"], json!([]));
    assert_eq!(answer["work_orders"], json!([]));
    assert_eq!(answer["order_withheld"], json!(true));
}
