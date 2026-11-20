//! The spectrum CNN in Rust against the Python that trained it.

use std::fs;

use features::spectrum::{PADDING, bin_curve};
use proposal::model::ModelError;
use proposal::spectrum::{SpectrumModel, input_channels};
use serde::Deserialize;

fn artifact(name: &str) -> String {
    let path = format!("{}/../../ml/artifacts/{name}", env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(path).unwrap()
}

fn model() -> SpectrumModel {
    serde_json::from_str(&artifact("spectrum_model.json")).unwrap()
}

#[derive(Deserialize)]
struct Case {
    first_mhz: f64,
    min: Vec<u8>,
    mean: Vec<u8>,
    outputs: Vec<Vec<f64>>,
}

#[derive(Deserialize)]
struct Parity {
    origin: String,
    cases: Vec<Case>,
}

#[test]
fn the_network_matches_python() {
    let m = model();
    m.check().unwrap();
    assert_eq!(m.origin, "Synthetic");
    let parity: Parity = serde_json::from_str(&artifact("spectrum_parity.json")).unwrap();
    assert_eq!(parity.origin, "Synthetic");
    assert!(!parity.cases.is_empty());
    // The channel summary has to leave padding out, so the file must hold some.
    assert!(parity.cases.iter().any(|c| c.mean.contains(&PADDING)));
    for case in &parity.cases {
        let got = m.forward(&input_channels(case.first_mhz, &case.min, &case.mean));
        let want: Vec<f64> = case.outputs.iter().flatten().copied().collect();
        assert_eq!(got.len(), want.len());
        for (a, b) in got.iter().zip(&want) {
            assert!((a - b).abs() < 1e-9, "{a} vs {b}");
        }
    }
}

#[test]
fn it_gives_the_same_bytes_twice() {
    let m = model();
    let curve = vec![Some(40.0); 3800];
    let binned = bin_curve(811.0, 50.0, &curve);
    let (low, mean) = binned.to_bytes();
    let input = input_channels(binned.first_mhz, &low, &mean);
    assert_eq!(m.forward(&input), m.forward(&input));
}

#[test]
fn a_model_from_another_schema_is_refused() {
    let good = model();
    let mut other = good.clone();
    other.observation_schema = "0".into();
    assert!(matches!(
        other.check(),
        Err(ModelError::ObservationSchema { .. })
    ));
    let mut other = good.clone();
    other.fault_schema = "0".into();
    assert!(matches!(other.check(), Err(ModelError::FaultSchema { .. })));
    let mut other = good.clone();
    other.spectrum_schema = "0".into();
    assert!(matches!(other.check(), Err(ModelError::Shape(_))));
    let mut other = good.clone();
    other.architecture = "dilated-cnn-1".into();
    assert!(matches!(other.check(), Err(ModelError::Shape(_))));
    let mut other = good.clone();
    other.mix.b.pop();
    assert!(matches!(other.check(), Err(ModelError::Shape(_))));
    let mut other = good;
    other.stem.b.pop();
    assert!(matches!(other.check(), Err(ModelError::Shape(_))));
}

#[derive(Deserialize)]
struct EditedParity {
    origin: String,
    artifact: String,
    cases: Vec<Case>,
}

/// Task 18: every edited artifact with a parity file reads in Rust unchanged and runs to Python's numbers.
#[test]
fn edited_artifacts_match_python() {
    let dir = format!("{}/../../ml/artifacts/edits", env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".parity.json") {
            continue;
        }
        let parity: EditedParity =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parity.origin, "Synthetic");
        let m: SpectrumModel = serde_json::from_str(
            &fs::read_to_string(format!("{dir}/{}", parity.artifact)).unwrap(),
        )
        .unwrap();
        m.check().unwrap();
        assert_eq!(m.origin, "Synthetic");
        assert!(m.thresholds.is_some());
        for case in &parity.cases {
            let got = m.forward(&input_channels(case.first_mhz, &case.min, &case.mean));
            let want: Vec<f64> = case.outputs.iter().flatten().copied().collect();
            assert_eq!(got.len(), want.len());
            for (a, b) in got.iter().zip(&want) {
                assert!((a - b).abs() < 1e-9, "{}: {a} vs {b}", parity.artifact);
            }
            assert_eq!(
                got,
                m.forward(&input_channels(case.first_mhz, &case.min, &case.mean))
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no edited artifact has a parity file");
}
