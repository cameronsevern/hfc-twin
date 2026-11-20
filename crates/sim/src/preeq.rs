//! Pre-equalization and response summaries (§5.3–§5.5).

use std::f64::consts::PI;

use schema::observation::ResponseSummary;

use crate::units::{C64, db};

/// §5.3: taps in a DOCSIS 3.0 pre-equalizer.
pub const SC_QAM_TAPS: usize = 24;
/// §5.3: the main tap, 1-based, as a real modem usually places it.
pub const MAIN_TAP_INDEX: usize = 8;

/// §5.3: taps and metrics for one SC-QAM channel.
#[derive(Debug, Clone, PartialEq)]
pub struct ScQamPreeq {
    /// The 24 symbol-spaced taps, scaled to unit total energy.
    pub taps: Vec<C64>,
    /// 1-based position of the main tap.
    pub main_tap_index: u8,
    /// Main tap compression, dB.
    pub mtc_db: f64,
    /// Main tap ratio, dB.
    pub mtr_db: f64,
    /// Non-main tap to total energy, dB.
    pub nmter_db: f64,
}

/// One sample of a channel's response for `sc_qam_preeq`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreeqSample {
    /// Which of the 24 T-spaced frequency bins it folds into.
    pub bin: usize,
    /// Its raised-cosine weight.
    pub weight: f64,
    /// Where to read the channel, MHz.
    pub f_mhz: f64,
}

/// §5.3: DOCSIS 3.0 SC-QAM roll-off.
const ROLLOFF: f64 = 0.25;

/// §5.3: raised cosine: a T-spaced equalizer sees the channel folded through the matched filter, so
/// the images in the roll-off share a bin. Weights in a bin sum to 1.
fn raised_cosine(offset_mhz: f64, symbol_rate_msym_per_s: f64) -> f64 {
    let f = offset_mhz.abs();
    let lo = (1.0 - ROLLOFF) * symbol_rate_msym_per_s / 2.0;
    let hi = (1.0 + ROLLOFF) * symbol_rate_msym_per_s / 2.0;
    if f <= lo {
        1.0
    } else if f >= hi {
        0.0
    } else {
        0.5 * (1.0 + (PI / (ROLLOFF * symbol_rate_msym_per_s) * (f - lo)).cos())
    }
}

/// §5.3: where to sample a channel's response, ascending in frequency.
pub fn preeq_samples(center_mhz: f64, symbol_rate_msym_per_s: f64) -> Vec<PreeqSample> {
    let n = SC_QAM_TAPS;
    let mut out = Vec::new();
    for image in [-1.0, 0.0, 1.0] {
        for k in 0..n {
            let offset = ((k as f64 - n as f64 / 2.0) / n as f64 + image) * symbol_rate_msym_per_s;
            let weight = raised_cosine(offset, symbol_rate_msym_per_s);
            if weight > 0.0 {
                out.push(PreeqSample {
                    bin: k,
                    weight,
                    f_mhz: center_mhz + offset,
                });
            }
        }
    }
    out.sort_by(|a, b| a.f_mhz.total_cmp(&b.f_mhz));
    out
}

/// Removes a response's best-fit delay. Ranging takes out bulk delay, so only what varies across a
/// channel is left for the equalizer to undo.
pub fn remove_bulk_delay(h: &[C64]) -> Vec<C64> {
    let phase = unwrap_phase(&h.iter().map(|z| z.arg()).collect::<Vec<_>>());
    let x: Vec<f64> = (0..h.len()).map(|i| i as f64).collect();
    let (slope, _, _) = slope_and_ripple(&x, &phase);
    let x_mid = (h.len() as f64 - 1.0) / 2.0;
    h.iter()
        .enumerate()
        .map(|(i, z)| *z * C64::from_polar(1.0, -slope * (i as f64 - x_mid)))
        .collect()
}

/// §5.3: the T-spaced inverse of the folded response, read at `preeq_samples`. An echo between tap
/// positions spreads across its neighbours.
pub fn sc_qam_preeq_from_samples(samples: &[PreeqSample], response: &[C64]) -> ScQamPreeq {
    let n = SC_QAM_TAPS;
    let mut folded = vec![C64::ZERO; n];
    for (sample, h) in samples.iter().zip(response) {
        folded[sample.bin] = folded[sample.bin] + h.scale(sample.weight);
    }
    let inverse: Vec<C64> = folded.iter().map(|z| z.inv()).collect();
    let mut taps = vec![C64::ZERO; n];
    for (m, tap) in taps.iter_mut().enumerate() {
        // Position m is `m − (main − 1)` symbols after the main tap.
        let delay_symbols = m as f64 - (MAIN_TAP_INDEX as f64 - 1.0);
        let mut sum = C64::ZERO;
        for (k, c) in inverse.iter().enumerate() {
            let offset = (k as f64 - n as f64 / 2.0) / n as f64;
            sum = sum + *c * C64::from_polar(1.0, 2.0 * PI * offset * delay_symbols);
        }
        *tap = sum.scale(1.0 / n as f64);
    }
    let (mtc_db, mtr_db, nmter_db) = tap_metrics(&taps, MAIN_TAP_INDEX);
    let total: f64 = taps.iter().map(|t| t.norm_sqr()).sum();
    let scale = if total > 0.0 { 1.0 / total.sqrt() } else { 1.0 };
    ScQamPreeq {
        taps: taps.into_iter().map(|t| t.scale(scale)).collect(),
        main_tap_index: MAIN_TAP_INDEX as u8,
        mtc_db,
        mtr_db,
        nmter_db,
    }
}

/// §5.3: as `sc_qam_preeq_from_samples`, given the channel response. `h` takes MHz.
pub fn sc_qam_preeq(
    center_mhz: f64,
    symbol_rate_msym_per_s: f64,
    h: impl Fn(f64) -> C64,
) -> ScQamPreeq {
    let samples = preeq_samples(center_mhz, symbol_rate_msym_per_s);
    let response: Vec<C64> = samples.iter().map(|s| h(s.f_mhz)).collect();
    sc_qam_preeq_from_samples(&samples, &response)
}

/// §5.3: (MTC, MTR, NMTER) in dB, from the taps and the 1-based main tap.
pub fn tap_metrics(taps: &[C64], main_tap_index: usize) -> (f64, f64, f64) {
    let total: f64 = taps.iter().map(|t| t.norm_sqr()).sum();
    let main = taps
        .get(main_tap_index.saturating_sub(1))
        .map_or(0.0, |t| t.norm_sqr());
    let non_main = (total - main).max(1e-30);
    (db(total / main), db(main / non_main), db(non_main / total))
}

/// §5.4: `c_k ∝ 1/H(f_k)`, normalized so `mean(|c_k|²) = 1`, with I and Q clipped at s2.13's limit.
pub fn ofdma_coefficients(h: &[C64]) -> Vec<C64> {
    const LIMIT: f64 = 3.9998;
    let inverse: Vec<C64> = h.iter().map(|z| z.inv()).collect();
    let mean: f64 = inverse.iter().map(|c| c.norm_sqr()).sum::<f64>() / inverse.len().max(1) as f64;
    let scale = if mean > 0.0 { 1.0 / mean.sqrt() } else { 1.0 };
    inverse
        .into_iter()
        .map(|c| C64 {
            re: (c.re * scale).clamp(-LIMIT, LIMIT),
            im: (c.im * scale).clamp(-LIMIT, LIMIT),
        })
        .collect()
}

/// §5.5: the downstream channel estimate: `H` scaled to an average power of 1, so slope and ripple
/// survive and flat loss does not.
pub fn normalized_response(h: &[C64]) -> Vec<C64> {
    let mean: f64 = h.iter().map(|z| z.norm_sqr()).sum::<f64>() / h.len().max(1) as f64;
    let scale = if mean > 0.0 { 1.0 / mean.sqrt() } else { 1.0 };
    h.iter().map(|z| z.scale(scale)).collect()
}

/// §5.5: (mean, population standard deviation, 2nd percentile) of RxMER in dB.
pub fn rxmer_summary(rxmer_db: &[f64]) -> Option<(f64, f64, f64)> {
    if rxmer_db.is_empty() {
        return None;
    }
    let n = rxmer_db.len() as f64;
    let mean = rxmer_db.iter().sum::<f64>() / n;
    let var = rxmer_db.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    let mut sorted = rxmer_db.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = ((n * 0.02).floor() as usize).max(1) - 1;
    Some((mean, var.sqrt(), sorted[index]))
}

/// §5.5: (slope, RMS ripple, peak-to-peak ripple) of `y` against `x` by least squares.
pub fn slope_and_ripple(x: &[f64], y: &[f64]) -> (f64, f64, f64) {
    let n = x.len().min(y.len());
    if n < 2 {
        return (0.0, 0.0, 0.0);
    }
    let x_mean = x[..n].iter().sum::<f64>() / n as f64;
    let y_mean = y[..n].iter().sum::<f64>() / n as f64;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for i in 0..n {
        sxx += (x[i] - x_mean).powi(2);
        sxy += (x[i] - x_mean) * (y[i] - y_mean);
    }
    let slope = if sxx > 0.0 { sxy / sxx } else { 0.0 };
    let (mut sum_sq, mut lo, mut hi) = (0.0, f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..n {
        let r = y[i] - (y_mean + slope * (x[i] - x_mean));
        sum_sq += r * r;
        lo = lo.min(r);
        hi = hi.max(r);
    }
    (slope, (sum_sq / n as f64).sqrt(), hi - lo)
}

/// §5.5: unwrap a phase in radians.
pub fn unwrap_phase(phase_rad: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(phase_rad.len());
    let mut offset = 0.0;
    for (i, &p) in phase_rad.iter().enumerate() {
        if i > 0 {
            let step = p - phase_rad[i - 1];
            if step > PI {
                offset -= 2.0 * PI;
            } else if step < -PI {
                offset += 2.0 * PI;
            }
        }
        out.push(p + offset);
    }
    out
}

/// §5.5: group delay from an unwrapped phase, ns, with `spacing_hz` between subcarriers. N − 1 values.
pub fn group_delay_ns(unwrapped_rad: &[f64], spacing_hz: f64) -> Vec<f64> {
    unwrapped_rad
        .windows(2)
        .map(|w| -(w[1] - w[0]) / (2.0 * PI * spacing_hz) * 1e9)
        .collect()
}

/// §5.5: the summary metrics of a response, at subcarrier frequencies `f_mhz`.
pub fn response_summary(coefficients: &[C64], f_mhz: &[f64], spacing_hz: f64) -> ResponseSummary {
    let mag_db: Vec<f64> = coefficients.iter().map(|c| db(c.norm_sqr())).collect();
    let (mag_slope, mag_rms, mag_pp) = slope_and_ripple(f_mhz, &mag_db);
    let phase: Vec<f64> = coefficients.iter().map(|c| c.arg()).collect();
    let gd = group_delay_ns(&unwrap_phase(&phase), spacing_hz);
    let gd_f = &f_mhz[..f_mhz.len().saturating_sub(1)];
    let (gd_slope, gd_rms, gd_pp) = slope_and_ripple(gd_f, &gd);
    ResponseSummary {
        mag_mean_db: mag_db.iter().sum::<f64>() / mag_db.len().max(1) as f64,
        mag_slope_db_per_mhz: mag_slope,
        mag_ripple_rms_db: mag_rms,
        mag_ripple_pp_db: mag_pp,
        gd_mean_ns: gd.iter().sum::<f64>() / gd.len().max(1) as f64,
        gd_slope_ns_per_mhz: gd_slope,
        gd_ripple_rms_ns: gd_rms,
        gd_ripple_pp_ns: gd_pp,
    }
}
