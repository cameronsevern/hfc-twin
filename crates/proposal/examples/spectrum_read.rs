//! One forward pass's latency, written for the decision rule, and what the model says about the one real capture
//! (`fixtures/field/README.md`). That file's origin is unverified, and it's one file, so this is a sanity check.
//!
//!     cargo run --release -p proposal --example spectrum_read [artifacts dir]
//!
//! The directory defaults to `ml/artifacts`; task 24's model is under `ml/artifacts/as_built`.

use std::fs;
use std::time::Instant;

use features::spectrum::{BINS, bin_curve};
use pnm_wire::pnm::decode_rxmer;
use proposal::spectrum::{SpectrumModel, input_channels};

type Failure = Box<dyn std::error::Error>;

fn main() -> Result<(), Failure> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ml/artifacts".to_string());
    let model: SpectrumModel =
        serde_json::from_str(&fs::read_to_string(format!("{dir}/spectrum_model.json"))?)?;
    model.check()?;
    let file = decode_rxmer(&fs::read("fixtures/field/pypnm_rxmer.bin")?)?;
    let first_mhz = f64::from(file.axis.subcarrier_zero_hz) / 1e6
        + f64::from(file.axis.first_active) * f64::from(file.axis.spacing_khz) / 1000.0;
    let binned = bin_curve(first_mhz, f64::from(file.axis.spacing_khz), &file.data);
    let (low, mean) = binned.to_bytes();
    let input = input_channels(binned.first_mhz, &low, &mean);

    let runs = 20;
    let start = Instant::now();
    let mut total = 0.0;
    for _ in 0..runs {
        total += model.forward(&input)[BINS - 1];
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(runs);
    fs::write(
        format!("{dir}/spectrum_latency.json"),
        serde_json::to_string(&serde_json::json!({"ms": ms, "runs": runs, "threads": 1}))?,
    )?;
    println!("{ms:.1} ms per forward pass ({total:.3})");
    match model.read(&binned) {
        Some(reading) => println!("real capture: normal {}, {reading:?}", reading.normal()),
        None => println!("no thresholds chosen yet"),
    }
    Ok(())
}
