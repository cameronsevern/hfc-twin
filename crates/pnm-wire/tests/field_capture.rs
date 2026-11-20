//! A real RxMER file from a public project, `fixtures/field/README.md`, through our decoder.

use std::fs;

use pnm_wire::pnm::decode_rxmer;

#[test]
fn a_real_capture_decodes_to_what_it_reads() {
    let path = format!(
        "{}/../../fixtures/field/pypnm_rxmer.bin",
        env!("CARGO_MANIFEST_DIR")
    );
    let file = decode_rxmer(&fs::read(path).unwrap()).unwrap();
    let values: Vec<f64> = file.data.iter().map(|v| v.unwrap()).collect();
    assert_eq!(values.len(), 7480);
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    assert!((mean - 40.42).abs() < 0.01, "{mean}");
    let lowest = values.iter().copied().fold(f64::MAX, f64::min);
    assert!((lowest - 28.25).abs() < 1e-9, "{lowest}");
}
