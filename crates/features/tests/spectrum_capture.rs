//! The one real capture on the grid: `fixtures/field/README.md`.

use features::spectrum::{BINS, bin_curve};
use pnm_wire::pnm::decode_rxmer;

#[test]
fn the_real_capture_bins_as_expected() {
    let path = format!(
        "{}/../../fixtures/field/pypnm_rxmer.bin",
        env!("CARGO_MANIFEST_DIR")
    );
    let file = decode_rxmer(&std::fs::read(path).unwrap()).unwrap();
    let first_mhz = f64::from(file.axis.subcarrier_zero_hz) / 1e6
        + f64::from(file.axis.first_active) * f64::from(file.axis.spacing_khz) / 1000.0;
    let binned = bin_curve(first_mhz, f64::from(file.axis.spacing_khz), &file.data);
    assert_eq!(binned.min_db.len(), BINS);
    // 187 MHz wide on a 0.2 MHz grid.
    assert_eq!(binned.present.iter().filter(|p| **p).count(), 935);
    // The spike at 750 MHz survives in the lowest curve, and is gone from the mean.
    let bin = ((750.0 - first_mhz) / 0.2) as usize;
    assert!(
        (binned.min_db[bin] - 28.25).abs() < 0.01,
        "{}",
        binned.min_db[bin]
    );
    assert!(binned.mean_db[bin] > 34.0, "{}", binned.mean_db[bin]);
}
