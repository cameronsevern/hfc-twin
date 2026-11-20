//! The likelihood: how well simulated telemetry under a hypothesis matches what was seen. σ is wide on
//! purpose. It has to cover domain randomization, measurement noise and everything the sim doesn't
//! know about this plant, or the fit chases noise.

use std::collections::BTreeMap;

use features::{ModemFeatures, StepFeatures};
use schema::common::ElementId;
use serde::{Deserialize, Serialize};

/// σ for one modem's features.
const LEVEL_SIGMA_DB: f64 = 1.2;
const LEVEL_SHAPE_SIGMA_DB: f64 = 0.8;
const SLOPE_SIGMA_DB: f64 = 0.4;
const TX_SIGMA_DB: f64 = 1.2;
const MER_SIGMA_DB: f64 = 0.9;
const MARGIN_SIGMA_DB: f64 = 1.5;
const MARGIN_BUCKET_SIGMA_DB: f64 = 2.0;
const NMTER_SIGMA_DB: f64 = 1.5;
/// Neighbouring buckets move together, so 64 of them aren't 64 independent facts. They count for
/// this share, or a shape fit would outvote every other kind of evidence.
const BUCKET_WEIGHT: f64 = 0.25;
const TAP_SIGMA_DB: f64 = 2.0;
const ECHO_SIGMA_FT: f64 = 40.0;
const RIPPLE_SIGMA_DB: f64 = 0.06;
const LOG_RATE_SIGMA: f64 = 1.0;
/// NMTER below this is too small to compare: a healthy channel's value depends on tiny echoes.
const NMTER_FLOOR_DB: f64 = -38.0;
/// Rates below this read as zero.
const RATE_FLOOR: f64 = 1e-7;
/// A modem up where the hypothesis has it down, or the reverse: a count of σ, squared.
const STATE_MISS: f64 = 100.0;
const SNR_SIGMA_DB: f64 = 1.0;
const VOLT_SIGMA: f64 = 1.5;
const OUT_LEVEL_SIGMA_DB: f64 = 1.0;

/// How one hypothesis fits.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FitResult {
    /// The whole misfit.
    pub chi2: f64,
    /// How many terms went into it.
    pub dof: usize,
    /// Per modem, so a residual can be located.
    pub per_modem: BTreeMap<ElementId, f64>,
}

#[derive(Default)]
struct Sum {
    chi2: f64,
    dof: usize,
}

impl Sum {
    fn add(&mut self, observed: Option<f64>, predicted: Option<f64>, sigma: f64) {
        if let (Some(a), Some(b)) = (observed, predicted) {
            let r = (a - b) / sigma;
            // A reading with no finite value says nothing to compare.
            if r.is_finite() {
                self.chi2 += r * r;
                self.dof += 1;
            }
        }
    }
}

fn log_rate(rate: Option<f64>) -> Option<f64> {
    rate.map(|r| r.max(RATE_FLOOR).log10())
}

fn nmter(value: Option<f64>) -> Option<f64> {
    value.map(|v| v.max(NMTER_FLOOR_DB))
}

fn fit_modem(a: &ModemFeatures, b: &ModemFeatures) -> Sum {
    let mut s = Sum::default();
    if a.operational != b.operational {
        s.chi2 += STATE_MISS;
        s.dof += 1;
        return s;
    }
    if !a.operational {
        s.dof += 1;
        return s;
    }
    s.add(a.level_drift_db, b.level_drift_db, LEVEL_SIGMA_DB);
    s.add(
        a.level_drift_slope_db_per_100mhz,
        b.level_drift_slope_db_per_100mhz,
        SLOPE_SIGMA_DB,
    );
    // The shape, with the mean taken out: amp alignment and tap tolerance move every bucket alike,
    // and a fault's shape is what can't be confused with them.
    if let (Some(mean_a), Some(mean_b)) = (a.level_drift_db, b.level_drift_db) {
        for (x, y) in a.level_drift_buckets.iter().zip(&b.level_drift_buckets) {
            if let (Some(x), Some(y)) = (x, y) {
                let r = ((x - mean_a) - (y - mean_b)) / LEVEL_SHAPE_SIGMA_DB;
                s.chi2 += BUCKET_WEIGHT * r * r;
                s.dof += 1;
            }
        }
    }
    s.add(a.us_tx_drift_db, b.us_tx_drift_db, TX_SIGMA_DB);
    s.add(a.mer_drift_db, b.mer_drift_db, MER_SIGMA_DB);
    s.add(a.mer_margin_p2_db, b.mer_margin_p2_db, MARGIN_SIGMA_DB);
    if let (Some(x), Some(y)) = (&a.rxmer_margin_buckets, &b.rxmer_margin_buckets) {
        for (p, q) in x.iter().zip(y) {
            let r = (p - q) / MARGIN_BUCKET_SIGMA_DB;
            s.chi2 += BUCKET_WEIGHT * r * r;
            s.dof += 1;
        }
    }
    s.add(
        nmter(a.preeq_nmter_db),
        nmter(b.preeq_nmter_db),
        NMTER_SIGMA_DB,
    );
    if let (Some(x), Some(y)) = (&a.preeq_taps_db, &b.preeq_taps_db) {
        for (p, q) in x.iter().zip(y) {
            s.add(Some(*p), Some(*q), TAP_SIGMA_DB);
        }
    }
    s.add(a.chan_est_ripple_db, b.chan_est_ripple_db, RIPPLE_SIGMA_DB);
    // Delay only means something where a real echo stands out.
    let strong = |m: &ModemFeatures| m.ds_echo_strength_db.is_some_and(|x| x > 0.15);
    if strong(a) && strong(b) {
        s.add(a.ds_echo_ft, b.ds_echo_ft, ECHO_SIGMA_FT);
    } else if strong(a) != strong(b) {
        s.add(a.ds_echo_strength_db, b.ds_echo_strength_db, 0.1);
    }
    s.add(
        log_rate(a.uncorrectable_rate),
        log_rate(b.uncorrectable_rate),
        LOG_RATE_SIGMA,
    );
    s
}

/// `observed` against `predicted`. Modems match by id.
pub fn fit(observed: &StepFeatures, predicted: &StepFeatures) -> FitResult {
    let predicted_modems: BTreeMap<&ElementId, &ModemFeatures> = predicted
        .modems
        .iter()
        .map(|m| (&m.element_id, m))
        .collect();
    let mut total = Sum::default();
    let mut per_modem = BTreeMap::new();
    for modem in &observed.modems {
        if let Some(other) = predicted_modems.get(&modem.element_id) {
            let s = fit_modem(modem, other);
            per_modem.insert(modem.element_id.clone(), s.chi2);
            total.chi2 += s.chi2;
            total.dof += s.dof;
        }
    }
    total.add(
        observed.service_group.us_snr_drift_min_db,
        predicted.service_group.us_snr_drift_min_db,
        SNR_SIGMA_DB,
    );
    total.add(
        observed.service_group.us_noise_rise_db,
        predicted.service_group.us_noise_rise_db,
        2.0 * SNR_SIGMA_DB,
    );
    let predicted_actives: BTreeMap<&ElementId, _> = predicted
        .actives
        .iter()
        .map(|a| (&a.element_id, a))
        .collect();
    for active in &observed.actives {
        if let Some(other) = predicted_actives.get(&active.element_id) {
            total.add(active.v_drift, other.v_drift, VOLT_SIGMA);
            total.add(
                active.out_level_drift_db,
                other.out_level_drift_db,
                OUT_LEVEL_SIGMA_DB,
            );
            if active.silent != other.silent {
                total.chi2 += STATE_MISS;
                total.dof += 1;
            }
        }
    }
    // An RPD's states are reports, not estimates, so a mismatch weighs like a modem's state.
    if let (Some(a), Some(b)) = (&observed.rpd, &predicted.rpd) {
        let misses = [
            a.top_level_state != b.top_level_state,
            a.gcp_status != b.gcp_status,
            a.ptp_result != b.ptp_result,
            a.rf_output_up != b.rf_output_up,
            a.drawing_power != b.drawing_power,
        ];
        total.chi2 += misses.iter().filter(|m| **m).count() as f64 * STATE_MISS;
        total.dof += misses.len();
    }
    FitResult {
        chi2: total.chi2,
        dof: total.dof.max(1),
        per_modem,
    }
}
