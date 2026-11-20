//! State survives a restart, against a real Postgres. Runs when `HFC_TEST_DATABASE_URL` is set, and says so when it isn't.

use std::path::Path;

use mcp_server::handle;
use mcp_server::persist::{Store, Touched, touched_by};
use serde_json::{Value, json};
use twin::{Twin, load_one_street, tool_specs};

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

/// Every tool has to be either known to write or known to read, so a new one can't quietly lose its changes on restart.
#[test]
fn every_tool_is_classified() {
    const READ_ONLY: [&str; 22] = [
        "list_service_groups",
        "get_optical_budget",
        "get_plant",
        "get_parts_list",
        "get_fiber_plant",
        "get_level_budget",
        "get_design_issues",
        "list_faults",
        "get_modem_telemetry",
        "get_status_map",
        "what_if",
        "list_work_orders",
        "get_modem_identity",
        "get_region_status",
        "get_recompute_stats",
        "localize_ingress",
        "list_verified_labels",
        "plan_migration",
        "list_maps",
        "get_map",
        "get_design_cost",
        "list_design_rules",
    ];
    // A running world (task 26) lives in memory only: it's a fold of the region, its seed and its log, and a restart starts
    // none. These change nothing the store keeps.
    const NOT_KEPT: [&str; 7] = [
        "ops_start",
        "ops_run",
        "ops_state",
        "ops_act",
        "ops_rates",
        "ops_layout",
        "ops_scenarios",
    ];
    let args = json!({"service_group": "x"});
    for tool in tool_specs() {
        let writes = !matches!(touched_by(tool.name, &args), Touched::Nothing);
        let quiet = READ_ONLY.contains(&tool.name) || NOT_KEPT.contains(&tool.name);
        assert_ne!(writes, quiet, "{}", tool.name);
    }
    // A fiber fault is the region's, so it's kept with the meta document.
    let region = json!({"region": "r"});
    assert!(matches!(touched_by("inject_fault", &region), Touched::Meta));
    assert!(matches!(touched_by("clear_faults", &region), Touched::Meta));
}

#[test]
fn state_survives_a_restart() {
    let Ok(url) = std::env::var("HFC_TEST_DATABASE_URL") else {
        eprintln!("HFC_TEST_DATABASE_URL isn't set, so Postgres wasn't exercised");
        return;
    };
    let mut store = Store::connect(&url).unwrap();
    store.wipe().unwrap();
    let sg = "sg-one-street-highsplit";
    let mut first = twin();
    let fault = json!({"onset": "Instant", "kind": {"BadDrop": {"modem_id": "modem-4", "extra_loss_db": 9.0, "return_loss_db": null}}});
    call(
        &mut first,
        "inject_fault",
        json!({"service_group": sg, "fault": fault}),
    );
    call(&mut first, "set_environment", json!({"temp_c": 33.0}));
    let answer = call(&mut first, "diagnose", json!({"service_group": sg}));
    let id = answer["work_orders"][0]["id"].clone();
    call(
        &mut first,
        "dispatch_work_order",
        json!({"service_group": sg, "id": id}),
    );
    store.save_all(&first).unwrap();
    drop(store);

    let mut store = Store::connect(&url).unwrap();
    let mut second = twin();
    assert_eq!(store.load_into(&mut second).unwrap(), 2);
    assert_eq!(second.sample.temp_c, 33.0);
    assert_eq!(
        call(&mut second, "list_faults", json!({"service_group": sg}))["faults"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // The record picks up where it left off, and derived values come back equal.
    assert_eq!(
        call(&mut second, "get_status_map", json!({"service_group": sg})),
        call(&mut first, "get_status_map", json!({"service_group": sg}))
    );
    call(
        &mut second,
        "complete_work_order",
        json!({"service_group": sg, "id": id}),
    );
    let verdict = call(
        &mut second,
        "verify_work_order",
        json!({"service_group": sg, "id": id}),
    );
    assert_eq!(verdict["status"], json!("Verified"));
    store.wipe().unwrap();
}
