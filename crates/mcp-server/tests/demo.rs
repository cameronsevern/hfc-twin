//! The lifecycle demo, through `tools/call` and nothing else, on a simulated region of at least 20 nodes
//! (`docs/DESIGN.md`, "The demo"). Prints how long each step takes. All synthetic, and every step says so.

use std::path::Path;
use std::time::Instant;

use mcp_server::handle;
use serde_json::{Value, json};
use twin::{Twin, load_one_street};

fn call(twin: &mut Twin, name: &str, arguments: Value) -> Value {
    let reply = handle(
        twin,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
    )
    .unwrap();
    assert_eq!(reply["result"]["isError"], json!(false), "{name}: {reply}");
    reply["result"]["structuredContent"].clone()
}

fn timed(label: &str, f: impl FnOnce() -> Value) -> Value {
    let start = Instant::now();
    let out = f();
    println!("{label}: {:?}", start.elapsed());
    out
}

#[test]
fn the_whole_lifecycle_on_a_region() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut t = load_one_street(&fixtures, 1).unwrap();

    // Design a region, half low-split I-CCAP and half high-split RPD.
    let low = "d31_lowsplit_1002_iccap";
    let high = "d31_highsplit_1218_rpd";
    let mut nodes: Vec<String> = Vec::new();
    for (family, profile, seed) in [("Suburban", low, 1), ("UrbanGrid", high, 2)] {
        let map = call(
            &mut t,
            "generate_layout",
            json!({"family": family, "seed": seed, "homes": 1600}),
        );
        assert_eq!(map["source"]["Generated"]["family"], json!(family));
        let map_id = map["map_id"].clone();
        let design = timed("design a map", || {
            call(
                &mut t,
                "design_region",
                json!({"map_id": map_id, "profile": profile, "controls": {
                    "homes_per_node": 120, "cascade_limit": null, "take_rate": 0.6, "d30_share": 0.3,
                    "max_drop_ft": 250.0, "pinned_nodes": [], "seed": seed,
                }}),
            )
        });
        assert_eq!(design["origin"], json!("Synthetic"));
        for node in design["nodes"].as_array().unwrap() {
            assert_eq!(
                node["physics_issues"],
                json!([]),
                "{}",
                node["service_group"]
            );
            assert_eq!(
                node["topology_issues"],
                json!([]),
                "{}",
                node["service_group"]
            );
            assert!(!node["parts"].as_array().unwrap().is_empty());
            nodes.push(node["service_group"].as_str().unwrap().to_string());
        }
    }
    println!("{} designed nodes", nodes.len());
    assert!((20..=80).contains(&nodes.len()), "{} nodes", nodes.len());

    // Mark them built.
    timed("mark every node built", || {
        for sg in &nodes {
            let built = call(&mut t, "mark_built", json!({"service_group": sg}));
            assert!(built["records"]["Designed"].is_null());
        }
        Value::Null
    });

    // Watch it run: nothing looks wrong on a healthy region.
    let status = timed("first read of the region", || {
        call(&mut t, "get_region_status", json!({}))
    });
    let designed = |status: &Value| -> Vec<Value> {
        status["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| {
                nodes
                    .iter()
                    .any(|s| s == n["service_group"].as_str().unwrap())
            })
            .cloned()
            .collect()
    };
    let healthy = designed(&status);
    assert_eq!(healthy.len(), nodes.len());
    let down: u64 = healthy
        .iter()
        .map(|n| n["out_of_service"].as_u64().unwrap())
        .sum();
    assert_eq!(down, 0);

    // Inject a fault on one node.
    let sg = nodes[nodes.len() / 2].clone();
    let plant = call(&mut t, "get_plant", json!({"service_group": sg}));
    let span = plant["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| {
            f["properties"]["kind"] == json!("Span")
                && f["properties"]["len_ft"].as_f64().unwrap() > 200.0
        })
        .unwrap()["properties"]
        .clone();
    let span_id = span["id"].clone();
    let fault = json!({"onset": "Instant", "kind": {"ConnectorReflection": {"location": {"Span": {"span_id": span_id, "pos_ft": span["len_ft"].as_f64().unwrap() / 2.0}}, "return_loss_db": 5.0}}});
    call(
        &mut t,
        "inject_fault",
        json!({"service_group": sg, "fault": fault}),
    );

    // Only that node recomputes.
    let before = call(&mut t, "get_recompute_stats", json!({}));
    let status = timed("read of the region after one change", || {
        call(&mut t, "get_region_status", json!({}))
    });
    let after = call(&mut t, "get_recompute_stats", json!({}));
    let counts = |stats: &Value, id: &str| stats["recomputed"][id].as_u64().unwrap_or(0);
    assert_eq!(counts(&after, &sg), counts(&before, &sg) + 1);
    for other in nodes.iter().filter(|n| **n != sg) {
        assert_eq!(
            counts(&after, other),
            counts(&before, other),
            "{other} recomputed"
        );
    }
    let this = designed(&status)
        .into_iter()
        .find(|n| n["service_group"] == json!(sg))
        .unwrap();
    assert!(this["affected"].as_u64().unwrap() > 0);

    // Diagnose it.
    let answer = timed("diagnose", || {
        call(&mut t, "diagnose", json!({"service_group": sg}))
    });
    assert_eq!(answer["origin"], json!("Synthetic"));
    assert_eq!(
        answer["hypotheses"][0]["class"],
        json!("ConnectorReflection"),
        "{}",
        answer["hypotheses"][0]
    );
    // The floor's answer is there to compare, whether or not a model's loaded.
    assert!(!answer["no_ml"]["hypotheses"].as_array().unwrap().is_empty());
    let order = answer["work_orders"][0].clone();
    assert_eq!(order["status"], json!("Proposed"));
    assert!(order["geo"].is_object());
    println!("work order: {}", order["action"]);

    // Issue it, and prove the fix.
    let id = order["id"].clone();
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
    let verdict = timed("verify from telemetry", || {
        call(
            &mut t,
            "verify_work_order",
            json!({"service_group": sg, "id": id}),
        )
    });
    assert_eq!(verdict["status"], json!("Verified"), "{verdict}");
    assert_eq!(
        call(&mut t, "list_verified_labels", json!({}))["labels"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Migrate a low-split node to high-split RPD.
    let low_node = nodes
        .iter()
        .find(|n| {
            let listed = call_ref(&mut t);
            listed["service_groups"]
                .as_array()
                .unwrap()
                .iter()
                .any(|g| g["service_group"] == json!(n) && g["profile"] == json!(low))
        })
        .unwrap()
        .clone();
    let plan = timed("plan a migration", || {
        call(
            &mut t,
            "plan_migration",
            json!({"service_group": low_node, "to_profile": high}),
        )
    });
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
    println!(
        "migration: the typical modem's upstream {:.0} to {:.0} Mbit/s, {} modems lose service, {} issues to fix",
        plan["us_capacity_before_mbps"].as_f64().unwrap(),
        plan["us_capacity_after_mbps"].as_f64().unwrap(),
        plan["modems_losing_service"],
        plan["issues_after"].as_array().unwrap().len(),
    );
    let applied = call(
        &mut t,
        "apply_migration",
        json!({"service_group": low_node, "to_profile": high}),
    );
    assert_eq!(applied["profile"], json!(high));
    timed("read of the region after the migration", || {
        call(&mut t, "get_region_status", json!({}))
    });
}

fn call_ref(t: &mut Twin) -> Value {
    call(t, "list_service_groups", json!({}))
}
