//! Designing on a map through `tools/call`. The geography in the extract is real; every plant is invented (invariant 11).

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;

use mcp_server::handle;
use mcp_server::overpass::{Prepared, Source, fetch, prepare};
use serde_json::{Value, json};
use twin::{Twin, load_one_street};

const PROFILE: &str = "d31_lowsplit_1002_iccap";

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn twin() -> Twin {
    load_one_street(&fixtures(), 1).unwrap()
}

fn extract() -> String {
    fs::read_to_string(fixtures().join("osm/real_broomfield_co.json")).unwrap()
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

fn refusal(twin: &mut Twin, name: &str, arguments: Value) -> String {
    let reply = raw(twin, name, arguments);
    assert_eq!(reply["result"]["isError"], json!(true), "{name}: {reply}");
    reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string()
}

fn controls(homes_per_node: u32, pins: Value) -> Value {
    json!({"homes_per_node": homes_per_node, "cascade_limit": null, "take_rate": 0.6, "d30_share": 0.3, "max_drop_ft": 250.0, "pinned_nodes": pins, "seed": 1})
}

fn imported(twin: &mut Twin) -> Value {
    call(
        twin,
        "import_osm",
        json!({"map_id": "real", "overpass_json": extract(), "headend": {"lon_deg": -105.09, "lat_deg": 39.902}}),
    )
}

fn design(twin: &mut Twin, pins: Value) -> Value {
    call(
        twin,
        "design_region",
        json!({"map_id": "real", "profile": PROFILE, "controls": controls(30, pins)}),
    )
}

fn node_ids(twin: &mut Twin) -> Vec<String> {
    let groups = call(twin, "list_service_groups", json!({}));
    groups["service_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["service_group"].as_str().unwrap().to_string())
        .filter(|id| id.starts_with("real/"))
        .collect()
}

#[test]
fn an_import_says_what_it_is_and_what_it_left_out() {
    let mut twin = twin();
    let map = imported(&mut twin);
    assert_eq!(map["homes"], json!(69));
    assert_eq!(map["real_geography"], json!(true));
    assert_eq!(map["sparse"], json!(false));
    assert!(map["label"].as_str().unwrap().contains("invented"));
    assert!(map["label"].as_str().unwrap().contains("OpenStreetMap"));
    assert!(map["report"]["apartments_counted_as_one"].as_u64().unwrap() > 0);
    let listed = call(&mut twin, "list_maps", json!({}));
    assert_eq!(listed["maps"][0]["map_id"], json!("real"));

    let shown = call(&mut twin, "get_map", json!({"map_id": "real"}));
    assert_eq!(
        shown["home_points"]["features"].as_array().unwrap().len(),
        69
    );
    assert!(
        !shown["street_lines"]["features"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(shown["headend"]["lon_deg"], json!(-105.09));

    // Reading it back needs no extract, and an unknown map with none is refused.
    let again = call(&mut twin, "import_osm", json!({"map_id": "real"}));
    assert_eq!(again["homes"], json!(69));
    assert!(refusal(&mut twin, "import_osm", json!({"map_id": "nope"})).contains("no map"));
    assert!(
        refusal(
            &mut twin,
            "import_osm",
            json!({"map_id": "x", "overpass_json": "not json"})
        )
        .contains("couldn't read")
    );
}

#[test]
fn a_map_of_few_homes_says_so_and_invents_none() {
    let mut twin = twin();
    let text = json!({"elements": [
        {"type": "node", "id": 1, "lat": 40.0, "lon": -80.0},
        {"type": "node", "id": 2, "lat": 40.0, "lon": -79.9},
        {"type": "way", "id": 10, "nodes": [1, 2], "tags": {"highway": "residential"}},
    ]})
    .to_string();
    let map = call(
        &mut twin,
        "import_osm",
        json!({"map_id": "empty", "overpass_json": text}),
    );
    assert_eq!(map["sparse"], json!(true));
    assert_eq!(map["homes"], json!(0));
    let refused = refusal(
        &mut twin,
        "design_region",
        json!({"map_id": "empty", "profile": PROFILE}),
    );
    assert!(refused.contains("nothing to design"), "{refused}");
}

#[test]
fn a_re_solve_replaces_the_nodes_and_keeps_pins_and_inputs() {
    let mut twin = twin();
    imported(&mut twin);
    let first = design(&mut twin, json!([]));
    let first_count = first["nodes"].as_array().unwrap().len();
    assert!(first_count >= 2);
    assert_eq!(node_ids(&mut twin).len(), first_count);

    // One big node leaves no stale sg-1 behind.
    call(
        &mut twin,
        "design_region",
        json!({"map_id": "real", "profile": PROFILE, "controls": controls(250, json!([]))}),
    );
    assert_eq!(node_ids(&mut twin), vec!["real/sg-0".to_string()]);

    let pin = json!({"geo": {"lon_deg": -105.0905, "lat_deg": 39.9025}});
    design(&mut twin, json!([pin]));
    let shown = call(&mut twin, "get_map", json!({"map_id": "real"}));
    assert_eq!(shown["design_input"]["profile"], json!(PROFILE));
    assert_eq!(
        shown["design_input"]["controls"]["pinned_nodes"][0],
        json!({"geo": {"lon_deg": -105.0905, "lat_deg": 39.9025}})
    );
    let sites = shown["node_sites"].as_array().unwrap();
    assert_eq!(sites.len(), node_ids(&mut twin).len());
    assert_eq!(sites.iter().filter(|n| n["pin"] == json!(0)).count(), 1);
    // The pinned site is close to where it was pinned, on the street nearest it.
    let site = &sites.iter().find(|n| n["pin"] == json!(0)).unwrap()["site"];
    assert!((site["lon_deg"].as_f64().unwrap() + 105.0905).abs() < 0.002);
    assert!((site["lat_deg"].as_f64().unwrap() - 39.9025).abs() < 0.002);
    assert!(
        !shown["span_lines"]["features"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(shown["fiber"]["route_ft"].as_f64().unwrap() > 0.0);
}

#[test]
fn a_re_solve_stops_at_anything_recorded() {
    let mut twin = twin();
    imported(&mut twin);
    design(&mut twin, json!([]));
    let id = node_ids(&mut twin)[0].clone();

    // A built element.
    call(&mut twin, "mark_built", json!({"service_group": id}));
    let refused = refusal(
        &mut twin,
        "design_region",
        json!({"map_id": "real", "profile": PROFILE, "controls": controls(30, json!([]))}),
    );
    assert!(refused.contains("built elements"), "{refused}");
    assert!(refused.contains(&id));
    let refused = refusal(&mut twin, "discard_map", json!({"map_id": "real"}));
    assert!(refused.contains("built elements"), "{refused}");
    // And nothing moved.
    assert!(node_ids(&mut twin).contains(&id));
}

#[test]
fn a_re_solve_stops_at_a_fault_and_a_migration() {
    let mut twin = twin();
    imported(&mut twin);
    design(&mut twin, json!([]));
    let id = node_ids(&mut twin)[0].clone();
    let plant = call(&mut twin, "get_plant", json!({"service_group": id}));
    let span = plant["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["properties"]["kind"] == json!("Span"))
        .unwrap()["properties"]["id"]
        .clone();
    call(
        &mut twin,
        "inject_fault",
        json!({"service_group": id, "fault": {"onset": "Instant", "kind": {"ConnectorReflection": {"location": {"Span": {"span_id": span, "pos_ft": 1.0}}, "return_loss_db": 6.0}}}}),
    );
    let again = json!({"map_id": "real", "profile": PROFILE, "controls": controls(30, json!([]))});
    assert!(refusal(&mut twin, "design_region", again.clone()).contains("fault"));
    call(&mut twin, "clear_faults", json!({"service_group": id}));
    call(&mut twin, "design_region", again.clone());

    call(
        &mut twin,
        "apply_migration",
        json!({"service_group": id, "to_profile": "d31_highsplit_1218_rpd"}),
    );
    assert!(refusal(&mut twin, "design_region", again).contains("migrated"));
}

#[test]
fn a_proposed_order_doesnt_block_but_one_acted_on_does_until_forced() {
    let mut twin = twin();
    imported(&mut twin);
    design(&mut twin, json!([]));
    let id = node_ids(&mut twin)[0].clone();
    let plant = call(&mut twin, "get_plant", json!({"service_group": id}));
    // A span with modems below it: a leg can end at a tap whose home drew no modem at this take rate.
    let inject = plant["features"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["properties"]["kind"] == json!("Span"))
        .map(|f| {
            let p = &f["properties"];
            json!({"service_group": id, "fault": {"onset": "Instant", "kind": {"Suckout": {"location": {"Span": {"span_id": p["id"], "pos_ft": p["len_ft"].as_f64().unwrap() / 2.0}}, "f0_mhz": 650.0, "depth_db": 15.0, "width_mhz": 40.0}}}})
        })
        .find(|fault| {
            !call(&mut twin, "what_if", fault.clone())["who"]["affected"]
                .as_array()
                .unwrap()
                .is_empty()
        })
        .unwrap();
    call(&mut twin, "inject_fault", inject.clone());
    let answer = call(&mut twin, "diagnose", json!({"service_group": id}));
    assert!(
        !answer["work_orders"].as_array().unwrap().is_empty(),
        "{}",
        answer["who"]
    );
    call(&mut twin, "clear_faults", json!({"service_group": id}));

    // Only a proposal: the design can still move, and it goes with the node.
    let again = json!({"map_id": "real", "profile": PROFILE, "controls": controls(30, json!([]))});
    call(&mut twin, "design_region", again.clone());

    call(&mut twin, "inject_fault", inject);
    let answer = call(&mut twin, "diagnose", json!({"service_group": id}));
    let order = answer["work_orders"][0]["id"].clone();
    call(
        &mut twin,
        "dispatch_work_order",
        json!({"service_group": id, "id": order}),
    );
    call(&mut twin, "clear_faults", json!({"service_group": id}));
    assert!(refusal(&mut twin, "design_region", again).contains("acted on"));
    assert!(refusal(&mut twin, "discard_map", json!({"map_id": "real"})).contains("acted on"));

    // Someone who means it isn't stuck.
    call(
        &mut twin,
        "discard_map",
        json!({"map_id": "real", "force": true}),
    );
    assert!(node_ids(&mut twin).is_empty());
}

#[test]
fn the_headend_moves_fiber_and_no_plant() {
    let mut twin = twin();
    imported(&mut twin);
    design(&mut twin, json!([]));
    let before = call(&mut twin, "get_map", json!({"map_id": "real"}));
    let id = node_ids(&mut twin)[0].clone();
    let plant_before = call(&mut twin, "get_plant", json!({"service_group": id}));

    let moved = call(
        &mut twin,
        "set_headend",
        json!({"map_id": "real", "headend": {"lon_deg": -105.0945, "lat_deg": 39.9005}}),
    );
    assert_ne!(moved["fiber"]["route_ft"], before["fiber"]["route_ft"]);
    assert_eq!(
        call(&mut twin, "get_plant", json!({"service_group": id})),
        plant_before
    );
    let after = call(&mut twin, "get_map", json!({"map_id": "real"}));
    assert_eq!(after["headend"]["lat_deg"], json!(39.9005));
    assert!(
        refusal(
            &mut twin,
            "set_headend",
            json!({"map_id": "x", "headend": {"lon_deg": 0.0, "lat_deg": 0.0}})
        )
        .contains("no map")
    );
}

#[test]
fn discarding_removes_everything_and_leaves_other_maps_alone() {
    let mut twin = twin();
    imported(&mut twin);
    design(&mut twin, json!([]));
    let before = call(&mut twin, "list_service_groups", json!({}))["service_groups"]
        .as_array()
        .unwrap()
        .len();
    let on_it = node_ids(&mut twin).len();
    let gone = call(&mut twin, "discard_map", json!({"map_id": "real"}));
    assert_eq!(gone["nodes_removed"], json!(on_it));
    assert!(node_ids(&mut twin).is_empty());
    assert_eq!(
        call(&mut twin, "list_service_groups", json!({}))["service_groups"]
            .as_array()
            .unwrap()
            .len(),
        before - on_it
    );
    assert_eq!(call(&mut twin, "list_maps", json!({}))["maps"], json!([]));
    assert!(refusal(&mut twin, "get_map", json!({"map_id": "real"})).contains("no map"));
}

#[test]
fn every_plant_on_a_real_map_stays_synthetic() {
    let mut twin = twin();
    imported(&mut twin);
    let designed = design(&mut twin, json!([]));
    assert_eq!(designed["origin"], json!("Synthetic"));
    for id in node_ids(&mut twin) {
        let plant = call(&mut twin, "get_plant", json!({"service_group": id}));
        assert_eq!(plant["origin"], json!("Synthetic"));
    }
}

fn area_call(north_deg: f64) -> Value {
    json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "import_osm_area", "arguments": {
        "south_deg": 39.900, "west_deg": -105.095, "north_deg": north_deg, "east_deg": -105.088,
    }}})
}

fn source(url: String) -> Source {
    Source {
        urls: vec![url],
        tries: 2,
        wait: Duration::from_millis(1),
        timeout: Duration::from_secs(5),
        deadline: Duration::from_secs(10),
    }
}

/// Answers each request with the next of these statuses, and the extract on a 200.
fn stub(statuses: Vec<u16>, body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/interpreter", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for status in statuses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut chunk = [0; 4096];
            // The whole request: closing on unread bytes resets the connection, which can lose the reply on its way.
            let body_at = loop {
                if let Some(at) = seen.windows(4).position(|w| w == b"\r\n\r\n") {
                    break at + 4;
                }
                let n = stream.read(&mut chunk).unwrap();
                seen.extend_from_slice(&chunk[..n]);
            };
            let headers = String::from_utf8_lossy(&seen[..body_at]).to_lowercase();
            let length: usize = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            while seen.len() < body_at + length {
                let n = stream.read(&mut chunk).unwrap();
                seen.extend_from_slice(&chunk[..n]);
            }
            let text = if status == 200 { body.as_str() } else { "" };
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            );
        }
    });
    url
}

fn failure(prepared: Prepared) -> String {
    match prepared {
        Prepared::Reply(reply) => {
            assert_eq!(reply["id"], json!(7));
            assert_eq!(reply["result"]["isError"], json!(true));
            reply["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .to_string()
        }
        Prepared::Call(call) => panic!("expected a failure, got {call}"),
    }
}

#[test]
fn an_area_is_fetched_retried_and_imported() {
    let url = stub(vec![502, 200], extract());
    let Prepared::Call(call_message) = prepare(&area_call(39.905), &source(url), |_| false) else {
        panic!("expected a call");
    };
    let mut twin = twin();
    let reply = handle(&mut twin, &call_message).unwrap();
    assert_eq!(reply["result"]["isError"], json!(false), "{reply}");
    let map = &reply["result"]["structuredContent"];
    assert_eq!(map["homes"], json!(69));
    assert_eq!(
        map["map_id"],
        json!("osm_39.9000_-105.0950_39.9050_-105.0880")
    );
    // The headend starts in the middle of the box.
    let shown = call(&mut twin, "get_map", json!({"map_id": map["map_id"]}));
    assert!((shown["headend"]["lat_deg"].as_f64().unwrap() - 39.9025).abs() < 1e-9);
}

#[test]
fn asking_for_the_same_area_again_doesnt_fetch() {
    // Nothing is listening, so a fetch would fail.
    let source = source("http://127.0.0.1:9/none".to_string());
    let Prepared::Call(call_message) = prepare(&area_call(39.905), &source, |_| true) else {
        panic!("expected a call");
    };
    assert_eq!(
        call_message["params"]["arguments"]["overpass_json"],
        Value::Null
    );
}

#[test]
fn a_failed_fetch_says_so_and_a_bad_box_never_goes_out() {
    let url = stub(vec![504, 504], String::new());
    let text = failure(prepare(&area_call(39.905), &source(url), |_| false));
    assert!(text.contains("504"), "{text}");
    assert!(text.contains("Nothing was imported"), "{text}");

    // Over the limit, inside out and unreadable, all without a request.
    let nowhere = source("http://127.0.0.1:9/none".to_string());
    assert!(failure(prepare(&area_call(41.5), &nowhere, |_| false)).contains("km²"));
    assert!(failure(prepare(&area_call(39.0), &nowhere, |_| false)).contains("isn't a box"));
    let unreadable = json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "import_osm_area", "arguments": {"south_deg": "x"}}});
    assert!(failure(prepare(&unreadable, &nowhere, |_| false)).contains("bad arguments"));

    // Without the fetching layer, the twin says what to use instead.
    let mut twin = twin();
    let refused = refusal(
        &mut twin,
        "import_osm_area",
        json!({"south_deg": 39.9, "west_deg": -105.0, "north_deg": 39.91, "east_deg": -104.99}),
    );
    assert!(refused.contains("import_osm"), "{refused}");
}

#[test]
fn a_server_that_never_answers_runs_out_the_deadline_not_the_clock() {
    // It accepts and says nothing.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/interpreter", listener.local_addr().unwrap());
    let source = Source {
        urls: vec![url.clone(), url],
        tries: 3,
        wait: Duration::from_millis(1),
        timeout: Duration::from_secs(60),
        deadline: Duration::from_millis(300),
    };
    let started = std::time::Instant::now();
    let text = failure(prepare(&area_call(39.905), &source, |_| false));
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(text.contains("Nothing was imported"), "{text}");
    drop(listener);
}

#[test]
fn other_messages_pass_through_untouched() {
    let message =
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "list_maps"}});
    let nowhere = source("http://127.0.0.1:9/none".to_string());
    let Prepared::Call(same) = prepare(&message, &nowhere, |_| false) else {
        panic!("expected a call");
    };
    assert_eq!(same, message);
}

/// ureq's `into_string` stops at 10 MB, and a few blocks of a dense city answer with more (task 29: a user's import failed with
/// "response too big for into_string"). The whole reply is read.
#[test]
fn a_reply_over_ten_megabytes_is_read_whole() {
    let body = format!(
        "{{\"elements\":[],\"pad\":\"{}\"}}",
        "x".repeat(11 * 1024 * 1024)
    );
    let url = stub(vec![200], body.clone());
    let area: twin::tools::AreaArgs = serde_json::from_value(json!({
        "south_deg": 39.900, "west_deg": -105.095, "north_deg": 39.905, "east_deg": -105.088,
    }))
    .unwrap();
    let text = fetch(&source(url), &area).unwrap();
    assert_eq!(text.len(), body.len());
}
