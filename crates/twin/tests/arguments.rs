//! Out-of-range arguments are refused in words that name the field, and in-range ones pass. All synthetic.

use std::path::Path;

use serde_json::{Value, json};
use twin::{ToolError, Twin, call_tool, load_one_street};

fn twin() -> Twin {
    load_one_street(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"),
        1,
    )
    .unwrap()
}

fn refused(t: &mut Twin, tool: &str, arguments: Value, field: &str) {
    match call_tool(t, tool, &arguments) {
        Err(ToolError::BadArguments(message)) => {
            assert!(message.contains(field), "{tool}: {message}")
        }
        other => panic!("{tool} {arguments} should be refused, got {other:?}"),
    }
}

fn controls(homes_per_node: u32, take_rate: f64, d30_share: f64, max_drop_ft: f64) -> Value {
    json!({"homes_per_node": homes_per_node, "cascade_limit": null, "take_rate": take_rate, "d30_share": d30_share,
           "max_drop_ft": max_drop_ft, "pinned_nodes": [], "seed": 1})
}

#[test]
fn environment_out_of_range_is_refused() {
    let mut t = twin();
    refused(
        &mut t,
        "set_environment",
        json!({"temp_c": 200.0}),
        "temp_c",
    );
    refused(
        &mut t,
        "set_environment",
        json!({"temp_c": -60.0}),
        "temp_c",
    );
    refused(
        &mut t,
        "set_environment",
        json!({"ground_temp_c": 99.0}),
        "ground_temp_c",
    );
    refused(
        &mut t,
        "set_environment",
        json!({"rain_mm_per_hr": -5.0}),
        "rain_mm_per_hr",
    );
    refused(&mut t, "set_environment", json!({"t": -1}), "t is");
    // A refused call changes nothing.
    assert_eq!(t.sample.temp_c, 20.0);
    call_tool(
        &mut t,
        "set_environment",
        &json!({"temp_c": -20.0, "rain_mm_per_hr": 10.0}),
    )
    .unwrap();
    assert_eq!(t.sample.temp_c, -20.0);
}

#[test]
fn layout_size_is_bounded() {
    let mut t = twin();
    refused(
        &mut t,
        "generate_layout",
        json!({"family": "Suburban", "seed": 1, "homes": 0}),
        "homes",
    );
    refused(
        &mut t,
        "generate_layout",
        json!({"family": "Suburban", "seed": 1, "homes": 1_000_000}),
        "homes",
    );
    call_tool(
        &mut t,
        "generate_layout",
        &json!({"family": "Suburban", "seed": 1, "homes": 200}),
    )
    .unwrap();
}

#[test]
fn design_controls_are_checked() {
    let mut t = twin();
    let map = call_tool(
        &mut t,
        "generate_layout",
        &json!({"family": "Suburban", "seed": 1, "homes": 200}),
    )
    .unwrap();
    let map_id = map["map_id"].clone();
    let profile = "d31_lowsplit_1002_iccap";
    let design = |c: Value| json!({"map_id": map_id, "profile": profile, "controls": c});
    refused(
        &mut t,
        "design_region",
        design(controls(0, 0.6, 0.3, 250.0)),
        "homes_per_node",
    );
    refused(
        &mut t,
        "design_region",
        design(controls(120, 5.0, 0.3, 250.0)),
        "take_rate",
    );
    refused(
        &mut t,
        "design_region",
        design(controls(120, 0.0, 0.3, 250.0)),
        "take_rate",
    );
    refused(
        &mut t,
        "design_region",
        design(controls(120, 0.6, 1.5, 250.0)),
        "d30_share",
    );
    refused(
        &mut t,
        "design_region",
        design(controls(120, 0.6, 0.3, 0.0)),
        "max_drop_ft",
    );
    // Nothing was designed by the refusals.
    assert!(
        !t.nodes
            .keys()
            .any(|id| id.as_str().starts_with(map_id.as_str().unwrap()))
    );
    call_tool(
        &mut t,
        "design_region",
        &design(controls(120, 0.6, 0.3, 250.0)),
    )
    .unwrap();
}

#[test]
fn switch_test_says_whether_there_is_noise_to_find() {
    let mut t = twin();
    let found = call_tool(
        &mut t,
        "localize_ingress",
        &json!({"service_group": "sg-one-street-highsplit"}),
    )
    .unwrap();
    assert_eq!(found["upstream_degraded"], json!(false));
    assert_eq!(found["noise_is_below"], Value::Null);
}

#[test]
fn a_map_says_what_locks_it() {
    let mut t = twin();
    let map = call_tool(
        &mut t,
        "generate_layout",
        &json!({"family": "Suburban", "seed": 1, "homes": 200}),
    )
    .unwrap();
    let map_id = map["map_id"].clone();
    let designed = call_tool(
        &mut t,
        "design_region",
        &json!({"map_id": map_id, "profile": "d31_lowsplit_1002_iccap", "controls": controls(120, 0.6, 0.3, 250.0)}),
    )
    .unwrap();
    let first = designed["nodes"][0]["service_group"].clone();
    let got = call_tool(&mut t, "get_map", &json!({"map_id": map_id})).unwrap();
    assert_eq!(got["locked"], Value::Null);
    call_tool(&mut t, "mark_built", &json!({"service_group": first})).unwrap();
    let got = call_tool(&mut t, "get_map", &json!({"map_id": map_id})).unwrap();
    assert_eq!(got["locked"]["service_group"], first);
    assert_eq!(got["locked"]["reason"], json!("has built elements"));
}
