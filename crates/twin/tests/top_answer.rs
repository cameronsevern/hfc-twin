//! A reflection at an amp's input is found, and the answer a diagnosis leads with is the one its work order is for. All synthetic.

use std::path::Path;

use serde_json::{Value, json};
use twin::{Twin, call_tool, load_one_street};

fn twin_with_model() -> Twin {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut t = load_one_street(&root.join("fixtures"), 1).unwrap();
    let text = std::fs::read_to_string(root.join("ml/artifacts/proposal_gbdt.json")).unwrap();
    t.model = Some(proposal::Model::from_json(&text).unwrap());
    t
}

const NODE: &str = "sg-one-street-highsplit";

/// A 6 dB connector reflection at amp-1's input, the far end of span-1 (1200 ft from rpd-1). The modems' pre-eq echo reads one tap
/// either side of 1200 ft. Found by hand on the page, when no candidate sat at a span's end and healthy led at 63 %.
fn reflection_at_amp_input(t: &mut Twin) -> Value {
    let fault = json!({"onset": "Instant", "kind": {"ConnectorReflection": {
        "location": {"Port": {"element_id": "amp-1", "port": "in"}}, "return_loss_db": 6.0}}});
    call_tool(
        t,
        "inject_fault",
        &json!({"service_group": NODE, "fault": fault}),
    )
    .unwrap();
    call_tool(t, "diagnose", &json!({"service_group": NODE})).unwrap()
}

fn components(h: &Value) -> Vec<String> {
    h["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_lead_answer_is_the_most_likely_one() {
    let mut t = twin_with_model();
    let a = reflection_at_amp_input(&mut t);
    let posteriors: Vec<f64> = a["hypotheses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["posterior"].as_f64().unwrap())
        .collect();
    assert!(
        posteriors.windows(2).all(|w| w[0] >= w[1]),
        "hypotheses must come most likely first: {posteriors:?}"
    );
}

#[test]
fn a_reflection_at_an_amp_input_is_found() {
    let mut t = twin_with_model();
    let a = reflection_at_amp_input(&mut t);
    let affected = a["who"]["affected"].as_array().unwrap().len();
    assert_eq!(affected, 8, "every modem is below amp-1");
    let top = &a["hypotheses"][0];
    let second = &a["hypotheses"][1];
    assert_eq!(top["class"], json!("ConnectorReflection"), "{top}");
    // The same cavity echoes whichever end is bad, so both ends of span-1 are answers, and the telemetry can't split them.
    let ends: Vec<Vec<String>> = vec![components(top), components(second)];
    assert!(ends.contains(&vec!["amp-1".to_string()]), "{ends:?}");
    assert!(ends.contains(&vec!["rpd-1".to_string()]), "{ends:?}");
    assert_eq!(top["affected_modems"], json!(8));
    let rl = top["faults"][0]["kind"]["ConnectorReflection"]["return_loss_db"]
        .as_f64()
        .unwrap();
    assert!((rl - 6.0).abs() <= 1.5, "return loss {rl} dB, injected 6");
    // The order is for the answer led with.
    let order = &a["work_orders"][0];
    assert_eq!(order["element_id"].as_str().unwrap(), components(top)[0]);
}
