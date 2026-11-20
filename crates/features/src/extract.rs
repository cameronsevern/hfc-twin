//! Features from one time step's observations. Arrays stay arrays, normalized the same way, for the
//! spectrum encoder; a field is `None` where a modem's capability or state gives no value.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use plant_graph::PlantGraph;
use schema::common::{ChannelId, ElementId};
use schema::component::{ComponentSpec, ModemGeneration};
use schema::observation::{DsRxLevels, ModemObservation, RegState, ServiceGroupObservation};
use schema::profile::Profile;
use serde::{Deserialize, Serialize};
use sim::cable::C_FT_PER_US;
use sim::telemetry::StepObservation;

use crate::reference::{DesignReference, band_key};

/// How many buckets the margin and level arrays hold, whatever the subcarrier count: the OFDM spectrum
/// for margin and the downstream band for level, each split evenly in frequency.
pub const MARGIN_BUCKETS: usize = 64;

/// Tap energy below this is too small to compare, dB relative to the main tap.
const TAP_FLOOR_DB: f64 = -50.0;

/// Foam hardline's velocity factor, for turning a delay into feet of cavity (§1.3).
const V_F: f64 = 0.87;

/// What one modem shows at one step, relative to its design and its neighbours.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ModemFeatures {
    /// The modem.
    pub element_id: ElementId,
    /// Its generation.
    pub is_d31: bool,
    /// Registered and passing traffic.
    pub operational: bool,
    /// Its parent, normally a tap.
    pub parent: ElementId,
    /// The tap its drop leaves, found up through its home's premises: its siblings are the other homes on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tap: Option<ElementId>,
    /// Amps above it.
    pub cascade: usize,
    /// Mean level against design, dB.
    pub level_drift_db: Option<f64>,
    /// Lowest channel or band against design, dB.
    pub level_drift_min_db: Option<f64>,
    /// Least-squares slope of level drift, dB per 100 MHz.
    pub level_drift_slope_db_per_100mhz: Option<f64>,
    /// Level drift less the median of modems on the same tap, dB.
    pub sibling_level_dev_db: Option<f64>,
    /// Mean transmit power against design, dB.
    pub us_tx_drift_db: Option<f64>,
    /// Transmit drift less the median of modems on the same tap, dB.
    pub sibling_us_tx_dev_db: Option<f64>,
    /// Level drift less the mean of the other modems in the same home, dB: one device in a home and not another points
    /// inside it [PNM-HFC §8.8.9.3]. `None` with no other modem in the home.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_level_dev_db: Option<f64>,
    /// RxMER mean over the highest profile's threshold, dB.
    pub mer_margin_mean_db: Option<f64>,
    /// 2nd-percentile RxMER over the same threshold, dB.
    pub mer_margin_p2_db: Option<f64>,
    /// RxMER mean against design, dB.
    pub mer_drift_db: Option<f64>,
    /// RxMER standard deviation, dB.
    pub mer_stddev_db: Option<f64>,
    /// Channel-estimate magnitude ripple, RMS, dB.
    pub chan_est_ripple_db: Option<f64>,
    /// That ripple against the design's own healthy prediction for this modem, dB: a home's splitters and cable ripple a
    /// healthy channel estimate, so the ripple a fault adds is what's over it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chan_est_ripple_drift_db: Option<f64>,
    /// Channel-estimate group delay ripple, peak to peak, ns.
    pub chan_est_gd_ripple_ns: Option<f64>,
    /// Strongest periodic ripple in the channel estimate: its delay, µs.
    pub ds_echo_delay_us: Option<f64>,
    /// That delay as feet of foam hardline cavity (§5.1).
    pub ds_echo_ft: Option<f64>,
    /// Its strength, as the ripple's amplitude in dB.
    pub ds_echo_strength_db: Option<f64>,
    /// Worst SC-QAM NMTER across channels, dB.
    pub preeq_nmter_db: Option<f64>,
    /// NMTER against design, dB.
    pub preeq_nmter_drift_db: Option<f64>,
    /// Delay of the post-main tap that rose most above design, µs.
    pub preeq_echo_delay_us: Option<f64>,
    /// That delay as feet of foam hardline cavity.
    pub preeq_echo_ft: Option<f64>,
    /// How far that tap rose, dB.
    pub preeq_echo_rise_db: Option<f64>,
    /// Tap energy averaged over SC-QAM channels, dB relative to the main tap, floored at −50 dB.
    pub preeq_taps_db: Option<Vec<f64>>,
    /// OFDMA pre-eq magnitude ripple, RMS, dB.
    pub ofdma_preeq_ripple_db: Option<f64>,
    /// Uncorrectable codewords per codeword.
    pub uncorrectable_rate: Option<f64>,
    /// Corrected codewords per codeword.
    pub corrected_rate: Option<f64>,
    /// T3 expiries in this interval.
    pub t3_delta: u64,
    /// T4 expiries in this interval.
    pub t4_delta: u64,
    /// Share of downstream channels unusable.
    pub ds_unusable_share: f64,
    /// Share of upstream channels unusable.
    pub us_unusable_share: f64,
    /// Share of data profiles failed.
    pub profiles_failed_share: Option<f64>,
    /// RxMER margin per bucket across the OFDM spectrum: lowest in each bucket, dB.
    pub rxmer_margin_buckets: Option<Vec<f64>>,
    /// Level against design per bucket across the downstream band, lowest in each, dB. `None` where
    /// no channel falls.
    pub level_drift_buckets: Vec<Option<f64>>,
}

/// What the service group shows at one step.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ServiceGroupFeatures {
    /// Lowest upstream SNR against design, dB.
    pub us_snr_drift_min_db: Option<f64>,
    /// Lowest upstream SNR, dB.
    pub us_snr_min_db: Option<f64>,
    /// Upstream uncorrectable rate.
    pub us_uncorrectable_rate: f64,
    /// How far the capture's noise floor sits above its median at its worst, per port, dB.
    pub us_noise_rise_db: Option<f64>,
    /// Frequency of that peak, MHz.
    pub us_noise_peak_mhz: Option<f64>,
    /// Share of modems not operational.
    pub modems_down_share: f64,
    /// Light into the node's forward receiver or RPD optic against the link's design value, dB (§16), where the plant
    /// reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_rx_drift_db: Option<f64>,
    /// Light into the hub's return receiver against the link's design value, dB (§16), where the plant reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_rx_drift_db: Option<f64>,
}

/// One step's features.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct StepFeatures {
    /// Per modem.
    pub modems: Vec<ModemFeatures>,
    /// The service group.
    pub service_group: ServiceGroupFeatures,
    /// Per active: voltage above its minimum, volts, and drift in its output level, dB.
    pub actives: Vec<ActiveFeatures>,
    /// The RPD's state, on a DAA node.
    #[serde(default)]
    pub rpd: Option<schema::rpd::RpdObservation>,
}

/// What an active reports, relative to what it should.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ActiveFeatures {
    /// The active.
    pub element_id: ElementId,
    /// Volts above `v_min`.
    pub v_margin: Option<f64>,
    /// Input voltage against design, volts.
    pub v_drift: Option<f64>,
    /// Output level against the profile's target, dB.
    pub out_level_drift_db: Option<f64>,
    /// Whether it reports no output.
    pub silent: bool,
    /// Each output port's level against its set point, the design's own level there, dB (§15), where the active reports
    /// one per port.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub port_level_drift_db: Vec<PortLevelDrift>,
}

/// One output port's level against its set point (§15).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PortLevelDrift {
    /// The port.
    pub port: schema::common::PortId,
    /// Reported less the design's level there, dB.
    pub drift_db: f64,
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    Some(sorted[sorted.len() / 2])
}

/// §5.4: the strongest periodic ripple in a response's magnitude: its delay and amplitude in dB. A
/// scan, rather than an FFT, because the delays worth finding are known and few.
pub fn strongest_ripple(mag_db: &[f64], spacing_mhz: f64) -> Option<(f64, f64)> {
    let n = mag_db.len();
    if n < 16 {
        return None;
    }
    // Detrend, so slope doesn't read as a long delay.
    let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let (slope, _, _) = sim::preeq::slope_and_ripple(&x, mag_db);
    let mean = mag_db.iter().sum::<f64>() / n as f64;
    let mid = (n as f64 - 1.0) / 2.0;
    let detrended: Vec<f64> = mag_db
        .iter()
        .enumerate()
        .map(|(i, y)| y - mean - slope * (i as f64 - mid))
        .collect();
    let span_mhz = spacing_mhz * n as f64;
    let (mut best_tau, mut best_power) = (0.0, 0.0);
    // Shorter than one period across the channel isn't a ripple, and past the Nyquist delay aliases.
    let tau_min_us = 2.0 / span_mhz;
    let tau_max_us = 0.45 / spacing_mhz;
    let mut tau = tau_min_us;
    while tau < tau_max_us {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, y) in detrended.iter().enumerate() {
            let phase = 2.0 * PI * tau * i as f64 * spacing_mhz;
            re += y * phase.cos();
            im += y * phase.sin();
        }
        let power = re * re + im * im;
        if power > best_power {
            best_power = power;
            best_tau = tau;
        }
        tau += 0.5 / span_mhz;
    }
    (best_power > 0.0).then(|| (best_tau, 2.0 * best_power.sqrt() / n as f64))
}

fn cavity_ft(tau_us: f64) -> f64 {
    tau_us * V_F * C_FT_PER_US / 2.0
}

/// The threshold features compare RxMER against: the highest profile in the channel plan.
fn target_thr_db(profile: &Profile) -> f64 {
    profile
        .channel_plan
        .ofdm
        .iter()
        .flat_map(|c| c.data_profiles.iter())
        .filter_map(|p| sim::fec::thr_db(profile, p.modulation))
        .fold(f64::NEG_INFINITY, f64::max)
}

/// §5.3: tap energy averaged over a modem's SC-QAM channels, dB relative to the main tap. The delay of
/// an echo doesn't depend on the channel, and its phase does, so power is what averages.
pub fn mean_taps_db(modem: &ModemObservation) -> Option<Vec<f64>> {
    let records = modem.preeq.sc_qam.as_ref()?;
    let first = records.first()?;
    let mut sum = vec![0.0; first.taps.len()];
    for record in records {
        let main = record.taps.get(record.main_tap_index as usize - 1)?;
        let main_energy = (main.re * main.re + main.im * main.im).max(1e-30);
        for (slot, tap) in sum.iter_mut().zip(&record.taps) {
            *slot += (tap.re * tap.re + tap.im * tap.im) / main_energy;
        }
    }
    let n = records.len() as f64;
    Some(
        sum.into_iter()
            .map(|e| (10.0 * (e / n).max(1e-30).log10()).max(TAP_FLOOR_DB))
            .collect(),
    )
}

/// The span of a profile's OFDM channels, MHz.
pub fn ofdm_span_mhz(profile: &Profile) -> (f64, f64) {
    let channels = &profile.channel_plan.ofdm;
    let lo = channels
        .iter()
        .map(|c| c.first_active_mhz)
        .fold(f64::INFINITY, f64::min);
    let hi = channels
        .iter()
        .map(|c| c.last_active_mhz)
        .fold(f64::NEG_INFINITY, f64::max);
    (lo, hi)
}

/// Which bucket a frequency falls in, across `lo..hi`.
pub fn bucket_of(f_mhz: f64, lo_mhz: f64, hi_mhz: f64) -> usize {
    let share = (f_mhz - lo_mhz) / (hi_mhz - lo_mhz).max(1e-9);
    ((share * MARGIN_BUCKETS as f64) as usize).min(MARGIN_BUCKETS - 1)
}

/// A bucket's centre, across `lo..hi`, MHz.
pub fn bucket_center(bucket: usize, lo_mhz: f64, hi_mhz: f64) -> f64 {
    lo_mhz + (bucket as f64 + 0.5) / MARGIN_BUCKETS as f64 * (hi_mhz - lo_mhz)
}

struct Baseline<'a> {
    t3: &'a BTreeMap<ChannelId, u64>,
}

/// The tap a modem's drop leaves: up through its home's premises to the first tap, or `None` where an active comes first.
pub fn tap_of(graph: &PlantGraph<'_>, modem: &ElementId) -> Option<ElementId> {
    let mut at = modem.clone();
    while let Ok(Some((parent, _))) = graph.parent(&at) {
        match parent.kind {
            schema::plant::ElementKind::Tap => return Some(parent.id.clone()),
            schema::plant::ElementKind::Node { .. }
            | schema::plant::ElementKind::Rpd { .. }
            | schema::plant::ElementKind::Amp { .. } => return None,
            _ => at = parent.id.clone(),
        }
    }
    None
}

/// The largest channel-estimate magnitude ripple a modem reports across its OFDM channels, RMS, dB.
pub fn chan_est_ripple(modem: &ModemObservation) -> Option<f64> {
    modem
        .ds_chan_est
        .iter()
        .flatten()
        .filter_map(|r| r.summary.map(|s| s.mag_ripple_rms_db))
        .reduce(f64::max)
}

fn one_modem(
    graph: &PlantGraph<'_>,
    modem: &ModemObservation,
    reference: &DesignReference,
    previous: Option<&ModemObservation>,
    thr_db: f64,
) -> ModemFeatures {
    let design = reference.modems.get(&modem.element_id);
    let (parent, cascade) = match graph.parent(&modem.element_id) {
        Ok(Some((parent, _))) => (
            parent.id.clone(),
            graph.cascade(&modem.element_id).map_or(0, |c| c.len()),
        ),
        _ => (modem.element_id.clone(), 0),
    };
    let tap = tap_of(graph, &modem.element_id);

    // Level drift, channel by channel against the design.
    let mut drifts: Vec<(f64, f64)> = Vec::new();
    for record in &modem.ds_rx {
        match &record.ds_rx_dbmv {
            DsRxLevels::ScQam { level_dbmv } => {
                if let Some(base) =
                    design.and_then(|d| d.ds_level_dbmv.get(&(record.channel_id.clone(), 0)))
                {
                    let center = graph
                        .profile()
                        .channel_plan
                        .ds_sc_qam
                        .iter()
                        .find(|c| c.channel_id == record.channel_id)
                        .map_or(0.0, |c| c.center_mhz);
                    drifts.push((center, level_dbmv - base));
                }
            }
            DsRxLevels::Ofdm { bands, .. } => {
                for band in bands {
                    if let Some(base) = design.and_then(|d| {
                        d.ds_level_dbmv
                            .get(&(record.channel_id.clone(), band_key(band.center_mhz)))
                    }) {
                        drifts.push((band.center_mhz, band.level_dbmv - base));
                    }
                }
            }
        }
    }
    let values: Vec<f64> = drifts.iter().map(|d| d.1).collect();
    let (ds_lo_mhz, ds_hi_mhz) = sim::walk::ds_band(graph.profile());
    let mut level_drift_buckets: Vec<Option<f64>> = vec![None; MARGIN_BUCKETS];
    for (f_mhz, drift) in &drifts {
        let slot = &mut level_drift_buckets[bucket_of(*f_mhz, ds_lo_mhz, ds_hi_mhz)];
        *slot = Some(slot.map_or(*drift, |x: f64| x.min(*drift)));
    }
    let level_drift_db =
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64);
    let level_drift_min_db = values.iter().copied().reduce(f64::min);
    let level_drift_slope_db_per_100mhz = (values.len() >= 2).then(|| {
        let x: Vec<f64> = drifts.iter().map(|d| d.0 / 100.0).collect();
        sim::preeq::slope_and_ripple(&x, &values).0
    });

    let tx: Vec<f64> = modem
        .us_tx
        .iter()
        .filter_map(|r| {
            design
                .and_then(|d| d.us_tx_dbmv.get(&r.channel_id))
                .map(|base| r.psd_dbmv_per_1p6mhz - base)
        })
        .collect();
    let us_tx_drift_db = (!tx.is_empty()).then(|| tx.iter().sum::<f64>() / tx.len() as f64);

    // RxMER.
    let (mut mean_sum, mut p2_min, mut std_max, mut mer_n, mut drift_sum) =
        (0.0, f64::INFINITY, 0f64, 0usize, 0.0);
    let mut buckets: Option<Vec<f64>> = None;
    let (ofdm_lo_mhz, ofdm_hi_mhz) = ofdm_span_mhz(graph.profile());
    for record in modem.rxmer.iter().flatten() {
        if let (Some(mean), Some(p2), Some(std)) = (
            record.rxmer_mean_db,
            record.rxmer_p2_db,
            record.rxmer_stddev_db,
        ) {
            mean_sum += mean;
            p2_min = p2_min.min(p2);
            std_max = std_max.max(std);
            mer_n += 1;
            if let Some(base) = design.and_then(|d| d.rxmer_mean_db.get(&record.channel_id)) {
                drift_sum += mean - base;
            }
        }
        if let Some(values) = &record.rxmer_db {
            let out = buckets.get_or_insert_with(|| vec![f64::INFINITY; MARGIN_BUCKETS]);
            let axis = record.axis;
            for (i, value) in values.iter().enumerate() {
                if let Some(mer) = value {
                    let f_mhz = axis.zero_freq_mhz
                        + (axis.first_active_index as usize + i) as f64 * axis.spacing_khz / 1000.0;
                    let bucket = bucket_of(f_mhz, ofdm_lo_mhz, ofdm_hi_mhz);
                    out[bucket] = out[bucket].min(mer - thr_db);
                }
            }
        }
    }
    if let Some(values) = &mut buckets {
        values.iter_mut().for_each(|x| {
            if !x.is_finite() {
                *x = 0.0;
            }
        });
    }

    // Channel estimate.
    let ripple_drift = chan_est_ripple(modem)
        .zip(design.and_then(|d| d.chan_est_ripple_db))
        .map(|(a, b)| a - b);
    let (mut ripple, mut gd_ripple): (Option<f64>, Option<f64>) = (None, None);
    let mut echo: Option<(f64, f64)> = None;
    for record in modem.ds_chan_est.iter().flatten() {
        if let Some(summary) = record.summary {
            ripple = Some(ripple.unwrap_or(0.0).max(summary.mag_ripple_rms_db));
            gd_ripple = Some(gd_ripple.unwrap_or(0.0).max(summary.gd_ripple_pp_ns));
        }
        if let Some(coefficients) = &record.coefficients {
            let mags: Vec<f64> = coefficients
                .iter()
                .flatten()
                .map(|c| 10.0 * (c.re * c.re + c.im * c.im).log10())
                .collect();
            if let Some(found) = strongest_ripple(&mags, record.axis.spacing_khz / 1000.0)
                && echo.is_none_or(|e| found.1 > e.1)
            {
                echo = Some(found);
            }
        }
    }

    // Pre-eq.
    let mut nmter: Option<f64> = None;
    let mut nmter_drift: Option<f64> = None;
    for record in modem.preeq.sc_qam.iter().flatten() {
        if nmter.is_none_or(|n| record.nmter_db > n) {
            nmter = Some(record.nmter_db);
            nmter_drift = design
                .and_then(|d| d.nmter_db.get(&record.channel_id))
                .map(|base| record.nmter_db - base);
        }
    }
    let preeq_taps_db = mean_taps_db(modem);
    let symbol_us = 1.0
        / graph
            .profile()
            .channel_plan
            .us_sc_qam
            .first()
            .map_or(5.12, |c| c.symbol_rate_msym_per_s);
    let main = sim::preeq::MAIN_TAP_INDEX - 1;
    let rise = preeq_taps_db
        .as_ref()
        .zip(design.and_then(|d| d.preeq_taps_db.as_ref()))
        .and_then(|(now, base)| {
            now.iter()
                .zip(base)
                .enumerate()
                .skip(main + 1)
                .map(|(i, (a, b))| (i, a - b))
                .max_by(|x, y| x.1.total_cmp(&y.1))
        });
    let preeq_delay = rise.map(|(i, _)| (i - main) as f64 * symbol_us);
    let ofdma_ripple = modem
        .preeq
        .ofdma
        .iter()
        .flatten()
        .filter_map(|r| r.summary.map(|s| s.mag_ripple_rms_db))
        .reduce(f64::max);

    let (mut total, mut corrected, mut uncorrectable) = (0u64, 0u64, 0u64);
    for record in &modem.codewords {
        total += record.total;
        corrected += record.corrected;
        uncorrectable += record.uncorrectable;
    }
    let rate = |count: u64| (total > 0).then(|| count as f64 / total as f64);

    let baseline_t3: BTreeMap<ChannelId, u64> = previous
        .map(|p| {
            p.t3_timeouts
                .iter()
                .map(|t| (t.channel_id.clone(), t.count))
                .collect()
        })
        .unwrap_or_default();
    let baseline_t4: BTreeMap<ChannelId, u64> = previous
        .map(|p| {
            p.t4_timeouts
                .iter()
                .map(|t| (t.channel_id.clone(), t.count))
                .collect()
        })
        .unwrap_or_default();
    let baseline = Baseline { t3: &baseline_t3 };
    let t3_delta = modem
        .t3_timeouts
        .iter()
        .map(|t| {
            t.count
                .saturating_sub(baseline.t3.get(&t.channel_id).copied().unwrap_or(0))
        })
        .sum();
    let t4_delta = modem
        .t4_timeouts
        .iter()
        .map(|t| {
            t.count
                .saturating_sub(baseline_t4.get(&t.channel_id).copied().unwrap_or(0))
        })
        .sum();

    let unusable_share = |states: &[schema::observation::ChannelStateRecord]| {
        if states.is_empty() {
            return 0.0;
        }
        states
            .iter()
            .filter(|s| s.state == schema::observation::ChannelUsability::Unusable)
            .count() as f64
            / states.len() as f64
    };
    let profiles_failed_share = modem.partial_channel.as_ref().map(|records| {
        let failed: usize = records.iter().map(|r| r.failed_data_profiles.len()).sum();
        let all: usize = graph
            .profile()
            .channel_plan
            .ofdm
            .iter()
            .map(|c| c.data_profiles.len())
            .sum();
        failed as f64 / all.max(1) as f64
    });

    ModemFeatures {
        element_id: modem.element_id.clone(),
        is_d31: modem.generation == ModemGeneration::D31,
        operational: modem.reg_state == RegState::Operational,
        parent,
        tap,
        cascade,
        level_drift_db,
        level_drift_min_db,
        level_drift_slope_db_per_100mhz,
        sibling_level_dev_db: None,
        us_tx_drift_db,
        sibling_us_tx_dev_db: None,
        home_level_dev_db: None,
        mer_margin_mean_db: (mer_n > 0).then(|| mean_sum / mer_n as f64 - thr_db),
        mer_margin_p2_db: (mer_n > 0).then_some(p2_min - thr_db),
        mer_drift_db: (mer_n > 0).then(|| drift_sum / mer_n as f64),
        mer_stddev_db: (mer_n > 0).then_some(std_max),
        chan_est_ripple_db: ripple,
        chan_est_ripple_drift_db: ripple_drift,
        chan_est_gd_ripple_ns: gd_ripple,
        ds_echo_delay_us: echo.map(|e| e.0),
        ds_echo_ft: echo.map(|e| cavity_ft(e.0)),
        ds_echo_strength_db: echo.map(|e| e.1),
        preeq_nmter_db: nmter,
        preeq_nmter_drift_db: nmter_drift,
        preeq_echo_delay_us: preeq_delay,
        preeq_echo_ft: preeq_delay.map(cavity_ft),
        preeq_echo_rise_db: rise.map(|r| r.1),
        preeq_taps_db,
        ofdma_preeq_ripple_db: ofdma_ripple,
        uncorrectable_rate: rate(uncorrectable),
        corrected_rate: rate(corrected),
        t3_delta,
        t4_delta,
        ds_unusable_share: unusable_share(&modem.ds_channel_state),
        us_unusable_share: unusable_share(&modem.us_channel_state),
        profiles_failed_share,
        rxmer_margin_buckets: buckets,
        level_drift_buckets,
    }
}

fn service_group(
    observation: &ServiceGroupObservation,
    reference: &DesignReference,
    modems: &[ModemFeatures],
) -> ServiceGroupFeatures {
    let us_snr_drift_min_db = observation
        .us_snr
        .iter()
        .filter_map(|r| {
            reference
                .us_snr_db
                .get(&r.channel_id)
                .map(|base| r.us_snr_db - base)
        })
        .reduce(f64::min);
    let us_snr_min_db = observation
        .us_snr
        .iter()
        .map(|r| r.us_snr_db)
        .reduce(f64::min);
    let (mut total, mut uncorrectable) = (0u64, 0u64);
    for record in &observation.us_codewords {
        total += record.total;
        uncorrectable += record.uncorrectable;
    }
    let mut noise_rise: Option<(f64, f64)> = None;
    for record in observation.us_spectrum.iter().flatten() {
        if let Some(floor) = median(&record.us_spectrum_dbmv) {
            for (i, level) in record.us_spectrum_dbmv.iter().enumerate() {
                let rise = level - floor;
                if noise_rise.is_none_or(|n| rise > n.0) {
                    let f_mhz =
                        record.first_bin_center_mhz + i as f64 * record.bin_width_khz / 1000.0;
                    noise_rise = Some((rise, f_mhz));
                }
            }
        }
    }
    let down = modems.iter().filter(|m| !m.operational).count();
    ServiceGroupFeatures {
        us_snr_drift_min_db,
        us_snr_min_db,
        us_uncorrectable_rate: if total > 0 {
            uncorrectable as f64 / total as f64
        } else {
            0.0
        },
        us_noise_rise_db: noise_rise.map(|n| n.0),
        us_noise_peak_mhz: noise_rise.map(|n| n.1),
        modems_down_share: down as f64 / modems.len().max(1) as f64,
        node_rx_drift_db: observation
            .optics
            .as_ref()
            .and_then(|o| o.node_rx_dbm)
            .zip(reference.node_rx_dbm)
            .map(|(seen, design)| seen - design),
        hub_rx_drift_db: observation
            .optics
            .as_ref()
            .and_then(|o| o.hub_rx_dbm)
            .zip(reference.hub_rx_dbm)
            .map(|(seen, design)| seen - design),
    }
}

/// Features for one step. `previous` is the step before, for counters that accumulate.
pub fn features_for(
    graph: &PlantGraph<'_>,
    step: &StepObservation,
    reference: &DesignReference,
    previous: Option<&StepObservation>,
) -> StepFeatures {
    let thr_db = target_thr_db(graph.profile());
    let previous_by_id: BTreeMap<&ElementId, &ModemObservation> = previous
        .map(|p| p.modems.iter().map(|m| (&m.element_id, m)).collect())
        .unwrap_or_default();
    let mut modems: Vec<ModemFeatures> = step
        .modems
        .iter()
        .map(|m| {
            one_modem(
                graph,
                m,
                reference,
                previous_by_id.get(&m.element_id).copied(),
                thr_db,
            )
        })
        .collect();

    // Sibling deviation: what's left when the shared path is taken out. Siblings share a tap, through their homes'
    // premises; a modem on its drop has the tap as its parent, so plant without premises reads as before.
    let mut by_parent: BTreeMap<ElementId, Vec<usize>> = BTreeMap::new();
    for (i, modem) in modems.iter().enumerate() {
        let key = modem.tap.clone().unwrap_or_else(|| modem.parent.clone());
        by_parent.entry(key).or_default().push(i);
    }
    for siblings in by_parent.values() {
        let levels: Vec<f64> = siblings
            .iter()
            .filter_map(|&i| modems[i].level_drift_db)
            .collect();
        let txs: Vec<f64> = siblings
            .iter()
            .filter_map(|&i| modems[i].us_tx_drift_db)
            .collect();
        let (level_mid, tx_mid) = (median(&levels), median(&txs));
        for &i in siblings {
            modems[i].sibling_level_dev_db =
                modems[i].level_drift_db.zip(level_mid).map(|(a, b)| a - b);
            modems[i].sibling_us_tx_dev_db =
                modems[i].us_tx_drift_db.zip(tx_mid).map(|(a, b)| a - b);
        }
    }

    // Deviation within a home: each modem against the mean of the others behind the same drop.
    let mut by_home: BTreeMap<schema::common::EdgeId, Vec<usize>> = BTreeMap::new();
    for (i, modem) in modems.iter().enumerate() {
        if let Some(home) = graph.premises_of(&modem.element_id).ok().flatten() {
            by_home.entry(home.drop.id.clone()).or_default().push(i);
        }
    }
    for members in by_home.values().filter(|m| m.len() > 1) {
        for &i in members {
            let others: Vec<f64> = members
                .iter()
                .filter(|&&j| j != i)
                .filter_map(|&j| modems[j].level_drift_db)
                .collect();
            modems[i].home_level_dev_db = modems[i]
                .level_drift_db
                .filter(|_| !others.is_empty())
                .map(|own| own - others.iter().sum::<f64>() / others.len() as f64);
        }
    }

    let group = service_group(&step.service_group, reference, &modems);
    let target = graph.profile().design_levels.out_dbmv_at_hi;
    let actives = step
        .actives
        .iter()
        .map(|active| {
            let v_min = graph
                .element(&active.element_id)
                .ok()
                .and_then(|e| graph.component(&e.component_ref))
                .and_then(|c| match &c.spec {
                    ComponentSpec::Node(n) => Some(n.active.v_min),
                    ComponentSpec::Rpd(r) => Some(r.active.v_min),
                    ComponentSpec::Amp(a) => Some(a.v_min),
                    _ => None,
                });
            ActiveFeatures {
                element_id: active.element_id.clone(),
                v_margin: active.v_in.zip(v_min).map(|(v, min)| v - min),
                v_drift: active
                    .v_in
                    .zip(reference.v_in.get(&active.element_id))
                    .map(|(v, base)| v - base),
                out_level_drift_db: active.out_level.map(|o| o.out_level_dbmv - target),
                silent: active.out_level.is_none(),
                port_level_drift_db: active
                    .out_levels
                    .iter()
                    .filter_map(|p| {
                        reference
                            .port_out_dbmv
                            .get(&(active.element_id.clone(), p.port.clone()))
                            .map(|design| PortLevelDrift {
                                port: p.port.clone(),
                                drift_db: p.out_level_dbmv - design,
                            })
                    })
                    .collect(),
            }
        })
        .collect();
    StepFeatures {
        modems,
        service_group: group,
        actives,
        rpd: step.rpd.clone(),
    }
}
