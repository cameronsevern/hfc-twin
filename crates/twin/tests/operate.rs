//! Task 28's additions to the ops tools, on a small generated region: a scenario starts written into both worlds, the layout
//! draws the region, and the state carries the sky, the chart's series, trucks' times and the actions taken. All synthetic.

use std::path::Path;

use serde_json::{Value, json};
use twin::{Twin, call_tool, load_one_street};

fn twin() -> Twin {
    load_one_street(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"),
        1,
    )
    .unwrap()
}

const WORLDS: [&str; 2] = ["reactive", "twin"];

fn call(t: &mut Twin, tool: &str, arguments: Value) -> Value {
    call_tool(t, tool, &arguments).unwrap_or_else(|e| panic!("{tool}: {e:?}"))
}

fn started(scenario: &str) -> (Twin, Value) {
    let mut t = twin();
    let map = call(
        &mut t,
        "generate_layout",
        json!({"family": "Suburban", "seed": 1, "homes": 200}),
    );
    let controls = json!({"homes_per_node": 120, "cascade_limit": null, "take_rate": 0.6, "d30_share": 0.3,
                          "max_drop_ft": 250.0, "pinned_nodes": [], "seed": 1});
    call(
        &mut t,
        "design_region",
        json!({"map_id": map["map_id"], "profile": "d31_lowsplit_1002_iccap", "controls": controls}),
    );
    let start = call(&mut t, "ops_start", json!({"scenario": scenario}));
    (t, start)
}

#[test]
fn the_scenarios_are_listed_and_one_starts_with_its_seed_and_intensity() {
    let mut t = twin();
    let list = call(&mut t, "ops_scenarios", json!({}));
    let ids: Vec<&str> = list["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"stormy-monday"), "{ids:?}");
    let (_, start) = started("stormy-monday");
    assert_eq!(start["seed"], json!(26));
    assert_eq!(start["intensity"], json!(50.0));
    assert_eq!(start["scenario"]["id"], json!("stormy-monday"));
    assert_eq!(start["origin"], json!("Synthetic"));
}

#[test]
fn an_unknown_scenario_is_refused() {
    let mut t = twin();
    assert!(call_tool(&mut t, "ops_start", &json!({"scenario": "no-such-day"})).is_err());
}

#[test]
fn the_layout_draws_the_region() {
    let (mut t, start) = started("stormy-monday");
    let layout = call(&mut t, "ops_layout", json!({}));
    assert_eq!(
        layout["homes"].as_array().unwrap().len() as u64,
        start["homes"].as_u64().unwrap()
    );
    assert!(!layout["streets"]["features"].as_array().unwrap().is_empty());
    assert!(!layout["spans"]["features"].as_array().unwrap().is_empty());
    assert!(!layout["nodes"].as_array().unwrap().is_empty());
    assert_eq!(layout["real_geography"], json!(false));
    let (lo, hi) = (&layout["bounds"][0], &layout["bounds"][1]);
    assert!(lo[0].as_f64().unwrap() < hi[0].as_f64().unwrap());
    assert!(lo[1].as_f64().unwrap() < hi[1].as_f64().unwrap());
}

#[test]
fn the_state_carries_the_sky_the_series_and_the_actions_taken() {
    let (mut t, _) = started("stormy-monday");
    call(&mut t, "ops_run", json!({"minutes": 30}));
    let st = call(&mut t, "ops_state", json!({}));
    assert_eq!(st["clock"], json!("Mon 06:30"));
    assert_eq!(st["sky"]["hour"], json!(6.5));
    // The scenario's storm began at its first minute and runs six hours.
    assert_eq!(st["sky"]["storm_until_s"], json!(60 + 6 * 3600));
    assert_eq!(st["sky"]["wind_m_per_s"], json!(25.0));
    assert_eq!(
        st["actions"].as_array().unwrap().len(),
        1,
        "the noon actions aren't taken yet"
    );
    let times: Vec<i64> = st["series_reactive"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s[0].as_i64().unwrap())
        .collect();
    assert_eq!(times, vec![0, 600, 1200, 1800]);
    assert!(st["worlds"]["reactive"].is_object() && st["worlds"]["twin"].is_object());
    assert!(st["worlds"]["twin"]["trips"].is_array());
    for truck in st["worlds"]["twin"]["trucks"].as_array().unwrap() {
        assert!(truck.get("from_t_s").is_some() && truck.get("to_t_s").is_some());
    }
    let later = call(
        &mut t,
        "ops_state",
        json!({"world": "twin", "series_since_s": 1200}),
    );
    assert!(later["worlds"].get("reactive").is_none());
    assert_eq!(later["series_twin"].as_array().unwrap().len(), 2);
    // A drive's route is in the same [lon, lat] pairs as a truck's, so a client reads both one way.
    call(
        &mut t,
        "ops_act",
        json!({"action": {"CarHit": {"near": null}}}),
    );
    call(&mut t, "ops_run", json!({"minutes": 600}));
    let late = call(&mut t, "ops_state", json!({"since_s": 0}));
    let trips: Vec<&Value> = WORLDS
        .iter()
        .flat_map(|w| late["worlds"][w]["trips"].as_array().unwrap())
        .collect();
    assert!(
        !trips.is_empty(),
        "a car into the plant rolls a truck within ten hours"
    );
    for trip in trips {
        let first = &trip["path"][0];
        assert!(first[0].is_f64() && first[1].is_f64(), "{trip}");
    }
}

#[test]
fn an_action_asked_for_in_the_past_happens_at_the_next_minute() {
    let (mut t, _) = started("stormy-monday");
    call(&mut t, "ops_run", json!({"minutes": 10}));
    let done = call(&mut t, "ops_act", json!({"action": "FiberCut", "at_s": 0}));
    assert_eq!(done["applies_at_s"], json!(660));
    let ahead = call(
        &mut t,
        "ops_act",
        json!({"action": {"DigIn": {"near": null}}, "at_s": 3600}),
    );
    assert_eq!(ahead["applies_at_s"], json!(3600));
}

#[test]
fn a_region_whose_map_isnt_loaded_isnt_started_and_says_what_to_do() {
    // The fixture twin's own region has no map, so there's nothing to run until an area is designed.
    let mut t = twin();
    let refused = call_tool(&mut t, "ops_start", &json!({})).unwrap_err();
    assert!(
        format!("{refused:?}").contains("design an area first"),
        "{refused:?}"
    );
    let (_, start) = started("stormy-monday");
    assert!(start["map_id"].as_str().unwrap().starts_with("generated-"));
}
