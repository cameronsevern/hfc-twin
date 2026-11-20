//! The whole v0 loop, through JSON-RPC and nothing else. All synthetic (invariant 11).

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

fn call(twin: &mut Twin, name: &str, arguments: Value) -> Value {
    let reply = handle(
        twin,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
    )
    .unwrap();
    assert_eq!(reply["result"]["isError"], json!(false), "{name}: {reply}");
    reply["result"]["structuredContent"].clone()
}

#[test]
fn initialize_and_list() {
    let mut t = twin();
    let reply = handle(
        &mut t,
        &json!({"jsonrpc": "2.0", "id": 7, "method": "initialize", "params": {}}),
    )
    .unwrap();
    assert_eq!(reply["id"], json!(7));
    assert!(reply["result"]["capabilities"]["tools"].is_object());
    assert_eq!(
        reply["result"]["protocolVersion"],
        json!(mcp_server::PROTOCOL_VERSION)
    );

    let reply = handle(
        &mut t,
        &json!({"jsonrpc": "2.0", "id": 8, "method": "tools/list"}),
    )
    .unwrap();
    let tools = reply["result"]["tools"].as_array().unwrap();
    assert!(tools.len() >= 15);
    for tool in tools {
        assert_eq!(
            tool["inputSchema"]["type"],
            json!("object"),
            "{}",
            tool["name"]
        );
        assert!(tool["description"].as_str().unwrap().len() > 10);
    }
}

#[test]
fn errors_and_notifications() {
    let mut t = twin();
    assert!(
        handle(
            &mut t,
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .is_none()
    );
    let reply = handle(
        &mut t,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "nope"}),
    )
    .unwrap();
    assert_eq!(reply["error"]["code"], json!(-32601));
    let reply = handle(
        &mut t,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "nope"}}),
    )
    .unwrap();
    assert_eq!(reply["error"]["code"], json!(-32602));
    let reply = handle(&mut t, &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "get_plant", "arguments": {"service_group": 3}}})).unwrap();
    assert_eq!(reply["result"]["isError"], json!(true));
    let reply = handle(&mut t, &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "get_plant", "arguments": {"service_group": "nope"}}})).unwrap();
    assert_eq!(reply["result"]["isError"], json!(true));
}

fn connector(span: &str, pos_ft: f64, rl: f64) -> Value {
    json!({"onset": "Instant", "kind": {"ConnectorReflection": {"location": {"Span": {"span_id": span, "pos_ft": pos_ft}}, "return_loss_db": rl}}})
}

#[test]
fn the_v0_loop_on_both_profiles() {
    let mut t = twin();
    let groups = call(&mut t, "list_service_groups", json!({}));
    let groups = groups["service_groups"].as_array().unwrap().clone();
    assert_eq!(groups.len(), 2);
    for group in groups {
        assert_eq!(group["origin"], json!("Synthetic"));
        assert_eq!(group["modems"], json!(8));
        let sg = group["service_group"].clone();

        let plant = call(&mut t, "get_plant", json!({"service_group": sg}));
        assert_eq!(plant["type"], json!("FeatureCollection"));
        assert_eq!(plant["origin"], json!("Synthetic"));
        assert!(plant["features"].as_array().unwrap().len() > 20);

        let issues = call(&mut t, "get_design_issues", json!({"service_group": sg}));
        assert_eq!(issues["physics"], json!([]));
        assert_eq!(issues["topology"], json!([]));

        let parts = call(&mut t, "get_parts_list", json!({"service_group": sg}));
        assert!(!parts["parts"].as_array().unwrap().is_empty());

        let status = call(&mut t, "get_status_map", json!({"service_group": sg}));
        assert_eq!(status["who"]["affected"], json!([]));

        call(
            &mut t,
            "inject_fault",
            json!({"service_group": sg, "fault": connector("span-3", 150.0, 6.0)}),
        );
        let status = call(&mut t, "get_status_map", json!({"service_group": sg}));
        assert!(!status["who"]["affected"].as_array().unwrap().is_empty());

        let answer = call(&mut t, "diagnose", json!({"service_group": sg}));
        assert_eq!(answer["origin"], json!("Synthetic"));
        assert_eq!(
            answer["hypotheses"][0]["class"],
            json!("ConnectorReflection")
        );
        assert_eq!(answer["hypotheses"][0]["components"][0], json!("span-3"));
        let order = &answer["work_orders"][0];
        assert_eq!(order["cause"], json!("ConnectorReflection"));
        assert_eq!(order["status"], json!("Proposed"));
        assert!(order["action"].as_str().unwrap().contains("connector"));

        let orders = call(&mut t, "list_work_orders", json!({"service_group": sg}));
        assert_eq!(orders["work_orders"].as_array().unwrap().len(), 1);

        call(&mut t, "clear_faults", json!({"service_group": sg}));
        let status = call(&mut t, "get_status_map", json!({"service_group": sg}));
        assert_eq!(status["who"]["affected"], json!([]));
    }
}

#[test]
fn what_if_changes_nothing() {
    let mut t = twin();
    let sg = json!("sg-one-street-highsplit");
    let before = call(&mut t, "get_status_map", json!({"service_group": sg}));
    let after_fault = call(
        &mut t,
        "what_if",
        json!({"service_group": sg, "fault": {"onset": "Instant", "kind": {"BadDrop": {"modem_id": "modem-4", "extra_loss_db": 9.0, "return_loss_db": null}}}}),
    );
    assert_ne!(before, after_fault);
    let after = call(&mut t, "get_status_map", json!({"service_group": sg}));
    assert_eq!(before, after);
    assert_eq!(
        call(&mut t, "list_faults", json!({"service_group": sg}))["faults"],
        json!([])
    );
}

#[test]
fn bad_faults_are_refused() {
    let mut t = twin();
    let reply = handle(&mut t, &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "inject_fault", "arguments": {"service_group": "sg-one-street-highsplit", "fault": connector("span-3", 5000.0, 6.0)}}})).unwrap();
    assert_eq!(reply["result"]["isError"], json!(true));
    assert_eq!(
        call(
            &mut t,
            "list_faults",
            json!({"service_group": "sg-one-street-highsplit"})
        )["faults"],
        json!([])
    );
}

#[test]
fn identity_is_gated() {
    let mut t = twin();
    let arguments = json!({"service_group": "sg-one-street-highsplit", "modem": "modem-1"});
    let reply = handle(&mut t, &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "get_modem_identity", "arguments": arguments}})).unwrap();
    assert_eq!(reply["result"]["isError"], json!(true));
    assert!(
        reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("gated")
    );
    t.identity_allowed = true;
    call(&mut t, "get_modem_identity", arguments);
}

#[test]
fn telemetry_is_in_the_observation_schema_and_repeats() {
    let mut t = twin();
    let arguments = json!({"service_group": "sg-one-street-highsplit", "modem": "modem-4"});
    let a = call(&mut t, "get_modem_telemetry", arguments.clone());
    let b = call(&mut t, "get_modem_telemetry", arguments);
    assert_eq!(a, b);
    let parsed: schema::observation::ModemObservation = serde_json::from_value(a).unwrap();
    assert_eq!(parsed.origin, schema::common::Origin::Synthetic);
    assert!(parsed.rxmer.is_some());
}

#[test]
fn environment_moves_what_is_read() {
    let mut t = twin();
    let sg = json!("sg-one-street-highsplit");
    let budget = |t: &mut Twin| {
        call(t, "get_level_budget", json!({"service_group": sg}))["modems"][3]["level_min_dbmv"]
            .as_f64()
            .unwrap()
    };
    let mild = budget(&mut t);
    call(
        &mut t,
        "set_environment",
        json!({"temp_c": 45.0, "ground_temp_c": 30.0}),
    );
    assert!(budget(&mut t) < mild);
}
