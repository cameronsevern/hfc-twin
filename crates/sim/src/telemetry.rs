//! What the sim emits per time step (§9): a fold over the environment series, deterministic given a
//! seed (invariant 4). Every signature comes out of the walk. Nothing here names a fault.
// Parallel per-frequency arrays read more plainly indexed.
#![allow(clippy::needless_range_loop)]

use std::collections::BTreeMap;

use plant_graph::PlantGraph;
use schema::capability::{CapabilitySet, effective};
use schema::common::{ChannelId, ElementId, Origin, PortId, ProfileId, Timestamp};
use schema::component::{ComponentSpec, ModemGeneration, ModemSpec};
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::FaultSet;
use schema::observation as obs;
use schema::profile::{NodeKind, OfdmChannel, OfdmaChannel, Qam};
use serde::{Deserialize, Serialize};

use crate::ac::solve_ac_at;
use crate::error::PhysicsError;
use crate::fec;
use crate::noise::{cnr_cm_db, inv_mer_isi_ofdm, inv_mer_isi_sc, n_floor_dbmv};
use crate::overlay::Overlay;
use crate::preeq;
use crate::rng::{Rng, hash_str};
use crate::state::{ModemState, StepInput};
use crate::units::{C64, amp, db, lin};
use crate::upstream::{p1p6hi_dbmv, per_1p6_from_per_6p4_db};
use crate::walk::{Band, Echo, Grid, ModemView, ReturnNoise, Walker, ds_band, us_band};

/// How much of each subcarrier array to compute and emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// Every subcarrier, as a real device reports.
    Full,
    /// Every Nth subcarrier and no full-band capture, for sweeps and for checking hypotheses.
    Summary {
        /// Keep one subcarrier in this many.
        stride: usize,
    },
}

/// Options for a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimOptions {
    /// How much to emit.
    pub detail: Detail,
    /// Seed for §8's domain randomization, if it's on.
    pub randomize: Option<u64>,
    /// Device measurement noise on RxMER and levels, dB standard deviation (approx). 0 for none.
    pub measurement_noise_db: f64,
    /// RxMER's scatter from one subcarrier to the next, on top of that, dB standard deviation. 0 for none.
    pub rxmer_scatter_db: f64,
}

impl Default for SimOptions {
    fn default() -> Self {
        SimOptions {
            detail: Detail::Full,
            randomize: None,
            measurement_noise_db: 0.0,
            rxmer_scatter_db: 0.0,
        }
    }
}

/// One simulation's inputs: everything an observation is derived from (invariant 1).
pub struct Scenario<'a> {
    /// Plant, library and profile.
    pub graph: &'a PlantGraph<'a>,
    /// The environment series.
    pub environment: &'a Environment,
    /// The faults.
    pub faults: &'a FaultSet,
    /// The seed for this service group's draws.
    pub seed: u64,
    /// Options.
    pub options: SimOptions,
    /// §16: the node's link state, from `sim::optics::node_link`. `None` keeps the root part's constant optics.
    pub link: Option<&'a crate::optics::NodeLink>,
}

/// Everything emitted at one time step.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct StepObservation {
    /// Simulated time, seconds.
    pub t: Timestamp,
    /// Per modem, in canonical order.
    pub modems: Vec<obs::ModemObservation>,
    /// Per active, in canonical order.
    pub actives: Vec<obs::ActiveObservation>,
    /// The service group.
    pub service_group: obs::ServiceGroupObservation,
    /// The RPD's own state, on a DAA node.
    #[serde(default)]
    pub rpd: Option<schema::rpd::RpdObservation>,
}

/// §4.5: the share of the modem's ceiling made after the echoes (approx, fitted to one lab modem).
const IMPL_SHAPED_SHARE: f64 = 0.65;
/// §4.5.1: how far off an SC-QAM still counts as adjacent to an OFDM channel, MHz (approx).
const ADJACENT_REACH_MHZ: f64 = 6.0;
/// §4.6: the PLC is always 16-QAM. Its band is 6 MHz at the channel's centre (placement approx).
const PLC_BAND_MHZ: f64 = 6.0;
/// §4.6: a vendor's limit on a profile's or the PLC's failure rate (approx).
const FAIL_RATE: f64 = 0.01;
/// §7.4: pilots and preambles go below about this RxMER (approx, below any data modulation).
const PILOT_LOSS_DB: f64 = 8.0;
/// §7.4: ranging is QPSK, so it survives where data doesn't (approx: 6 dB below 16-QAM's `thr`).
const RANGING_THR_DB: f64 = 6.0;
/// §5.1: 8K FFT guard, for the emitted axis' `first_active_index`, as a real channel reports.
const FIRST_ACTIVE_INDEX: u32 = 148;
/// FBC bin spacing, kHz.
const FBC_BIN_KHZ: f64 = 250.0;
/// Upstream spectrum capture bin spacing, kHz.
const US_SPECTRUM_BIN_KHZ: f64 = 250.0;
/// A SC-QAM channel is read at this many points across its width.
const SC_POINTS: usize = 5;
/// SC-QAM codeword rate, per second: 256-QAM at 38.8 Mbit/s, over RS(128,122) of 7-bit symbols.
const SC_CW_PER_S: f64 = 43_300.0;
/// Bits in an LDPC codeword [§4.6].
const LDPC_BITS: f64 = 16_200.0;
/// Codewords per second on the PLC and fields per second on the NCP (approx).
const PLC_CW_PER_S: f64 = 2_000.0;
const NCP_PER_S: f64 = 44_000.0;
/// A node's own return noise figure, when no amp sets a higher floor, dB (approx: SCTE 183's worked example).
const ROOT_RETURN_NF_DB: f64 = 10.0;
/// Upstream codewords per second (approx).
const US_CW_PER_S: f64 = 5_000.0;

/// One OFDM channel's stretch of a walk's grid.
struct Stretch<'p> {
    channel: &'p OfdmChannel,
    start: usize,
    len: usize,
    spacing_khz: f64,
}

struct ScStretch<'p> {
    channel: &'p schema::profile::DsScQamChannel,
    start: usize,
}

struct DownLayout<'p> {
    grid: Grid,
    ofdm: Vec<Stretch<'p>>,
    sc: Vec<ScStretch<'p>>,
    fbc: Option<(usize, usize, f64)>,
}

fn down_layout<'p>(graph: &PlantGraph<'p>, detail: Detail) -> DownLayout<'p> {
    let plan = &graph.profile().channel_plan;
    let stride = match detail {
        Detail::Full => 1,
        Detail::Summary { stride } => stride.max(1),
    };
    let mut f_mhz = Vec::new();
    let mut ofdm = Vec::new();
    for channel in &plan.ofdm {
        let spacing_mhz = channel.subcarrier_spacing_khz / 1000.0;
        let n = ((channel.last_active_mhz - channel.first_active_mhz) / spacing_mhz).round()
            as usize
            + 1;
        let start = f_mhz.len();
        let mut i = 0;
        while i < n {
            f_mhz.push(channel.first_active_mhz + i as f64 * spacing_mhz);
            i += stride;
        }
        ofdm.push(Stretch {
            channel,
            start,
            len: f_mhz.len() - start,
            spacing_khz: channel.subcarrier_spacing_khz * stride as f64,
        });
    }
    let mut sc = Vec::new();
    for channel in &plan.ds_sc_qam {
        let start = f_mhz.len();
        for k in 0..SC_POINTS {
            let offset = (k as f64 + 0.5) / SC_POINTS as f64 - 0.5;
            f_mhz.push(channel.center_mhz + offset * channel.width_mhz);
        }
        sc.push(ScStretch { channel, start });
    }
    let (ds_lo, ds_hi) = ds_band(graph.profile());
    let fbc = match (detail, &graph.profile().capability.fbc) {
        (Detail::Full, Some(cap)) => {
            let lo = cap.lo_mhz.max(ds_lo);
            let hi = cap.hi_mhz.min(ds_hi);
            let n = ((hi - lo) * 1000.0 / FBC_BIN_KHZ).floor() as usize;
            let start = f_mhz.len();
            f_mhz.extend((0..n).map(|i| lo + (i as f64 + 0.5) * FBC_BIN_KHZ / 1000.0));
            Some((start, n, lo))
        }
        _ => None,
    };
    DownLayout {
        grid: Grid::new(f_mhz),
        ofdm,
        sc,
        fbc,
    }
}

/// One OFDMA channel's stretch of a walk's grid.
struct UpStretch<'p> {
    channel: &'p OfdmaChannel,
    start: usize,
    len: usize,
    spacing_khz: f64,
}

struct UpScStretch<'p> {
    channel: &'p schema::profile::UsScQamChannel,
    start: usize,
    samples: Vec<preeq::PreeqSample>,
}

struct UpLayout<'p> {
    grid: Grid,
    ofdma: Vec<UpStretch<'p>>,
    sc: Vec<UpScStretch<'p>>,
    spectrum: Option<(usize, usize)>,
    spectrum_lo_mhz: f64,
}

fn up_layout<'p>(graph: &PlantGraph<'p>, detail: Detail) -> UpLayout<'p> {
    let plan = &graph.profile().channel_plan;
    let stride = match detail {
        Detail::Full => 1,
        Detail::Summary { stride } => stride.max(1),
    };
    let mut f_mhz = Vec::new();
    let mut ofdma = Vec::new();
    for channel in &plan.ofdma {
        let spacing_mhz = channel.subcarrier_spacing_khz / 1000.0;
        let n = ((channel.last_active_mhz - channel.first_active_mhz) / spacing_mhz).round()
            as usize
            + 1;
        let start = f_mhz.len();
        let mut i = 0;
        while i < n {
            f_mhz.push(channel.first_active_mhz + i as f64 * spacing_mhz);
            i += stride;
        }
        ofdma.push(UpStretch {
            channel,
            start,
            len: f_mhz.len() - start,
            spacing_khz: channel.subcarrier_spacing_khz * stride as f64,
        });
    }
    let mut sc = Vec::new();
    for channel in &plan.us_sc_qam {
        let start = f_mhz.len();
        let samples = preeq::preeq_samples(channel.center_mhz, channel.symbol_rate_msym_per_s);
        f_mhz.extend(samples.iter().map(|s| s.f_mhz));
        sc.push(UpScStretch {
            channel,
            start,
            samples,
        });
    }
    let (us_lo, us_hi) = us_band(graph.profile());
    let spectrum = if graph.profile().capability.us_spectrum_capture {
        let stride = if matches!(detail, Detail::Full) { 1 } else { 8 };
        let n = ((us_hi - us_lo) * 1000.0 / (US_SPECTRUM_BIN_KHZ * stride as f64)).floor() as usize;
        let start = f_mhz.len();
        f_mhz.extend(
            (0..n).map(|i| us_lo + (i as f64 + 0.5) * US_SPECTRUM_BIN_KHZ * stride as f64 / 1000.0),
        );
        Some((start, n))
    } else {
        None
    };
    UpLayout {
        grid: Grid::new(f_mhz),
        ofdma,
        sc,
        spectrum,
        spectrum_lo_mhz: us_lo,
    }
}

fn modem_spec(component: &schema::component::Component) -> Option<&ModemSpec> {
    match &component.spec {
        ComponentSpec::Modem(modem) => Some(modem),
        _ => None,
    }
}

fn subcarrier_axis(first_active_mhz: f64, spacing_khz: f64, detail: Detail) -> obs::SubcarrierAxis {
    let first_active_index = if matches!(detail, Detail::Full) {
        FIRST_ACTIVE_INDEX
    } else {
        0
    };
    obs::SubcarrierAxis {
        zero_freq_mhz: first_active_mhz - f64::from(first_active_index) * spacing_khz / 1000.0,
        first_active_index,
        spacing_khz,
    }
}

/// §4.5: the modem's ceiling at one frequency, in linear. The share made after the echoes ripples
/// with them, so `h_sqr` is |H|² there.
pub fn inv_impl_mer(impl_mer_db: f64, h_sqr: f64) -> f64 {
    lin(-impl_mer_db) * (IMPL_SHAPED_SHARE / h_sqr + (1.0 - IMPL_SHAPED_SHARE))
}

/// §4.5: the modem's whole noise sum at one frequency, in linear.
fn inv_mer(view: &ModemView<'_>, i: usize, nf_db: f64, impl_mer_db: f64, isi: f64) -> f64 {
    let h_sqr = view.h[i].norm_sqr();
    let level_dbmv = view.level_dbmv[i] + db(h_sqr);
    view.inv_plant[i] + lin(-cnr_cm_db(level_dbmv, nf_db)) + inv_impl_mer(impl_mer_db, h_sqr) + isi
}

/// §4.5.1: leakage into a subcarrier `x` subcarriers from an adjacent channel taken as wide,
/// relative to equal levels. `t_w_us` is the receive window's taper and `t_u_us` the useful symbol.
pub fn adjacent_leakage(x: f64, t_w_us: f64, t_u_us: f64) -> f64 {
    use std::f64::consts::PI;
    let x = x.max(1.0);
    let pi2 = PI * PI;
    let k_c = t_u_us / (2.0 * t_w_us.max(1e-3));
    if x < k_c {
        (1.0 / x - 1.0 / k_c) / (2.0 * pi2) + 1.0 / (20.0 * pi2 * k_c)
    } else {
        k_c.powi(4) / (20.0 * pi2 * x.powi(5))
    }
}

/// §4.5.1: adjacent channels' leakage across an OFDM channel, in linear, for this modem's levels.
fn inv_adjacent(
    ctx: &DownContext<'_, '_>,
    view: &ModemView<'_>,
    stretch: &Stretch<'_>,
) -> Vec<f64> {
    let channel = stretch.channel;
    let t_u_us = 1000.0 / channel.subcarrier_spacing_khz;
    let t_w_us = channel.cp_us - channel.rolloff_us;
    let spacing_mhz = channel.subcarrier_spacing_khz / 1000.0;
    let mut out = vec![0.0; stretch.len];
    for sc in &ctx.layout.sc {
        let lo_mhz = sc.channel.center_mhz - sc.channel.width_mhz / 2.0;
        let hi_mhz = sc.channel.center_mhz + sc.channel.width_mhz / 2.0;
        let below = hi_mhz <= channel.first_active_mhz
            && channel.first_active_mhz - hi_mhz <= ADJACENT_REACH_MHZ;
        let above = lo_mhz >= channel.last_active_mhz
            && lo_mhz - channel.last_active_mhz <= ADJACENT_REACH_MHZ;
        if !below && !above {
            continue;
        }
        let level_sc_dbmv = view.level_dbmv[sc.start + SC_POINTS / 2];
        for (k, value) in out.iter_mut().enumerate() {
            let i = stretch.start + k;
            let f = view.grid.f_mhz[i];
            let distance_mhz = if below { f - hi_mhz } else { lo_mhz - f };
            let ratio = lin(level_sc_dbmv - view.level_dbmv[i]);
            if ratio.is_finite() {
                *value += ratio * adjacent_leakage(distance_mhz / spacing_mhz, t_w_us, t_u_us);
            }
        }
    }
    out
}

fn isi_ofdm(echoes: &[Echo], channel: &OfdmChannel) -> f64 {
    let t_u_us = 1000.0 / channel.subcarrier_spacing_khz;
    echoes
        .iter()
        .map(|e| inv_mer_isi_ofdm(e.tau_us, channel.cp_us, e.rho, t_u_us))
        .sum()
}

/// §4.5: SC-QAM has no cyclic prefix; its equalizer takes echoes within its span down (task 27), and the largest residual
/// is what's left.
fn isi_sc(echoes: &[Echo]) -> f64 {
    echoes
        .iter()
        .map(|e| inv_mer_isi_sc(e.tau_us, e.rho))
        .fold(0.0, f64::max)
}

/// §5.5: `H` as the modem sees it, level slope included.
fn full_response(level_dbmv: &[f64], h: &[C64]) -> Vec<C64> {
    level_dbmv
        .iter()
        .zip(h)
        .map(|(l, z)| z.scale(amp(*l)))
        .collect()
}

fn no_signal(level_dbmv: f64) -> bool {
    !level_dbmv.is_finite()
}

/// Rounds to a tenth of a dB as a device reports, and adds seeded measurement noise.
fn read_db(value: f64, noise_db: f64, rng: &mut Rng) -> f64 {
    let noisy = if noise_db > 0.0 {
        value + noise_db * rng.gaussian()
    } else {
        value
    };
    (noisy * 10.0).round() / 10.0
}

/// Downstream results for one modem, before upstream joins them.
struct DownResult {
    rxmer: Vec<obs::DsRxmerRecord>,
    fbc: Option<obs::FbcRecord>,
    chan_est: Vec<obs::DsChanEstRecord>,
    ds_rx: Vec<obs::DsRxRecord>,
    codewords: Vec<obs::CodewordRecord>,
    plc: Vec<obs::PlcRecord>,
    ncp: Vec<obs::NcpRecord>,
    channel_state: Vec<obs::ChannelStateRecord>,
    partial: Vec<obs::PartialChannelRecord>,
    /// §7.4: the chance a MAC message is lost on the best channel with lock. 1 if none has.
    p_down: f64,
}

struct DownContext<'c, 'p> {
    layout: &'c DownLayout<'p>,
    profile_ref: &'c ProfileId,
    profile: &'c schema::profile::Profile,
    plant_capability: &'c CapabilitySet,
    detail: Detail,
    noise_db: f64,
    rxmer_scatter_db: f64,
    seed: u64,
    t: Timestamp,
    step_s: f64,
    /// §8: each modem's draws within its part's tolerance.
    modem_draws: &'c BTreeMap<ElementId, (f64, f64)>,
}

fn down_modem(ctx: &DownContext<'_, '_>, view: &ModemView<'_>) -> Result<DownResult, PhysicsError> {
    let stated = modem_spec(view.component)
        .ok_or_else(|| PhysicsError::BadComponent(view.modem.id.clone()))?;
    // §8: a modem drawn within its stated tolerance has its own noise figure and ceiling.
    let drawn;
    let modem = match ctx.modem_draws.get(&view.modem.id) {
        Some((nf_db, impl_mer_db)) => {
            drawn = ModemSpec {
                nf_db: stated.nf_db + nf_db,
                impl_mer_db: stated.impl_mer_db + impl_mer_db,
                ..stated.clone()
            };
            &drawn
        }
        None => stated,
    };
    let capability = effective(ctx.plant_capability, &modem.capability);
    let id_hash = hash_str(view.modem.id.as_str());
    let mut rng = Rng::derive(ctx.seed, &[ctx.t.0 as u64, id_hash, 10]);
    let mut out = DownResult {
        rxmer: Vec::new(),
        fbc: None,
        chan_est: Vec::new(),
        ds_rx: Vec::new(),
        codewords: Vec::new(),
        plc: Vec::new(),
        ncp: Vec::new(),
        channel_state: Vec::new(),
        partial: Vec::new(),
        p_down: 1.0,
    };

    // SC-QAM: every generation bonds these.
    for stretch in &ctx.layout.sc {
        let channel = stretch.channel;
        let isi = isi_sc(view.echoes);
        let (mut level_lin, mut inv_sum) = (0.0, 0.0);
        let mut missing = false;
        for k in 0..SC_POINTS {
            let i = stretch.start + k;
            missing |= no_signal(view.level_dbmv[i]);
            level_lin += lin(view.level_dbmv[i]) * view.h[i].norm_sqr();
            inv_sum += inv_mer(view, i, modem.nf_db, modem.impl_mer_db, isi);
        }
        let level_dbmv = db(level_lin / SC_POINTS as f64);
        let mer_db = -db(inv_sum / SC_POINTS as f64);
        let thr = fec::sc_qam_thr_db(ctx.profile, channel.modulation).unwrap_or(26.0);
        let margin = mer_db - thr;
        let p_uncorr = if missing {
            1.0
        } else {
            fec::p_uncorr_sc(margin)
        };
        let usable = !missing && p_uncorr < 0.5;
        out.channel_state.push(obs::ChannelStateRecord {
            channel_id: channel.channel_id.clone(),
            profile_ref: ctx.profile_ref.clone(),
            state: if usable {
                obs::ChannelUsability::Usable
            } else {
                obs::ChannelUsability::Unusable
            },
        });
        if !missing {
            out.p_down = out.p_down.min(p_uncorr);
            out.ds_rx.push(obs::DsRxRecord {
                channel_id: channel.channel_id.clone(),
                profile_ref: ctx.profile_ref.clone(),
                ds_rx_dbmv: obs::DsRxLevels::ScQam {
                    level_dbmv: read_db(level_dbmv, ctx.noise_db, &mut rng),
                },
            });
            let total = (SC_CW_PER_S * ctx.step_s) as u64;
            let uncorrectable = rng.binomial(total, p_uncorr);
            let corrected = rng.binomial(total - uncorrectable, fec::p_corr_sc(margin));
            out.codewords.push(obs::CodewordRecord {
                channel_id: channel.channel_id.clone(),
                profile_ref: ctx.profile_ref.clone(),
                data_profile_id: None,
                total,
                corrected,
                uncorrectable,
            });
        }
    }

    // OFDM: only where the part is D3.1 and its rating reaches the channel.
    let rated_hi = view.component.rated_hi_mhz.unwrap_or(f64::INFINITY);
    for stretch in &ctx.layout.ofdm {
        if modem.generation == ModemGeneration::D30 || stretch.channel.last_active_mhz > rated_hi {
            continue;
        }
        down_ofdm(ctx, view, modem, &capability, stretch, &mut rng, &mut out);
    }

    if let (Some((start, n, lo_mhz)), Some(fbc)) = (ctx.layout.fbc, &capability.fbc) {
        let first_mhz = lo_mhz + FBC_BIN_KHZ / 2000.0;
        let mut levels = Vec::with_capacity(n);
        let nbw_hz = FBC_BIN_KHZ * 1000.0;
        for k in 0..n {
            let i = start + k;
            let f = view.grid.f_mhz[i];
            if f > fbc.hi_mhz || f > rated_hi {
                break;
            }
            let signal_lin = if loaded(ctx.profile, f) && !no_signal(view.level_dbmv[i]) {
                lin(view.level_dbmv[i] + db(nbw_hz / 6e6)) * view.h[i].norm_sqr()
            } else {
                0.0
            };
            let plant_noise = if no_signal(view.level_dbmv[i]) {
                0.0
            } else {
                lin(view.level_dbmv[i] + db(nbw_hz / 6e6)) * view.inv_plant[i]
            };
            let modem_noise = lin(n_floor_dbmv(nbw_hz) + modem.nf_db);
            let bin = db(signal_lin + plant_noise + modem_noise);
            levels.push(read_db(bin, ctx.noise_db, &mut rng));
        }
        out.fbc = Some(obs::FbcRecord {
            profile_ref: ctx.profile_ref.clone(),
            first_bin_center_mhz: first_mhz,
            bin_width_khz: FBC_BIN_KHZ,
            noise_bandwidth_khz: FBC_BIN_KHZ,
            fbc_dbmv: levels,
        });
    }
    Ok(out)
}

/// Whether the channel plan puts a signal on `f_mhz`.
fn loaded(profile: &schema::profile::Profile, f_mhz: f64) -> bool {
    let plan = &profile.channel_plan;
    plan.ds_sc_qam
        .iter()
        .any(|c| (f_mhz - c.center_mhz).abs() <= c.width_mhz / 2.0)
        || plan
            .ofdm
            .iter()
            .any(|c| f_mhz >= c.first_active_mhz && f_mhz <= c.last_active_mhz)
}

#[allow(clippy::too_many_arguments)]
fn down_ofdm(
    ctx: &DownContext<'_, '_>,
    view: &ModemView<'_>,
    modem: &ModemSpec,
    capability: &CapabilitySet,
    stretch: &Stretch<'_>,
    rng: &mut Rng,
    out: &mut DownResult,
) {
    let channel = stretch.channel;
    let range = stretch.start..stretch.start + stretch.len;
    let isi = isi_ofdm(view.echoes, channel);
    let missing = range.clone().any(|i| no_signal(view.level_dbmv[i]));
    let channel_id = channel.channel_id.clone();
    let profile_ref = ctx.profile_ref.clone();
    let axis = subcarrier_axis(channel.first_active_mhz, stretch.spacing_khz, ctx.detail);

    if missing {
        out.channel_state.push(obs::ChannelStateRecord {
            channel_id,
            profile_ref,
            state: obs::ChannelUsability::Unusable,
        });
        return;
    }

    let adjacent = inv_adjacent(ctx, view, stretch);
    let mer_true: Vec<f64> = range
        .clone()
        .zip(&adjacent)
        .map(|(i, a)| -db(inv_mer(view, i, modem.nf_db, modem.impl_mer_db, isi) + a))
        .collect();
    let scatter_db = ctx.noise_db.hypot(ctx.rxmer_scatter_db);
    let mer_read: Vec<f64> = mer_true
        .iter()
        .map(|m| read_db(*m, scatter_db, rng))
        .collect();
    let mean_db = mer_true.iter().sum::<f64>() / mer_true.len().max(1) as f64;

    // §7.4: RxMER is measured on pilots and preambles, so it lasts as long as they do.
    let pilots_ok = mean_db > PILOT_LOSS_DB;
    if let Some((read_mean, read_std, read_p2)) = preeq::rxmer_summary(&mer_read)
        && pilots_ok
        && (capability.ds_rxmer_file || capability.ds_rxmer_summary)
    {
        out.rxmer.push(obs::DsRxmerRecord {
            channel_id: channel_id.clone(),
            profile_ref: profile_ref.clone(),
            axis,
            rxmer_db: capability
                .ds_rxmer_file
                .then(|| mer_read.iter().map(|m| Some(*m)).collect()),
            rxmer_mean_db: Some(read_mean),
            rxmer_stddev_db: Some(read_std),
            rxmer_p2_db: Some(read_p2),
        });
    }

    if capability.ds_chan_est_file || capability.ds_chan_est_summary {
        let levels = &view.level_dbmv[range.clone()];
        let response = preeq::normalized_response(&full_response(levels, &view.h[range.clone()]));
        let f = &view.grid.f_mhz[range.clone()];
        let summary = preeq::response_summary(&response, f, stretch.spacing_khz * 1000.0);
        out.chan_est.push(obs::DsChanEstRecord {
            channel_id: channel_id.clone(),
            profile_ref: profile_ref.clone(),
            axis,
            coefficients: capability.ds_chan_est_file.then(|| {
                response
                    .iter()
                    .map(|c| Some(schema::common::Complex::from(*c)))
                    .collect()
            }),
            summary: Some(summary),
        });
    }

    if capability.band_power_6mhz {
        let mut bands = Vec::new();
        let mut band_lo = channel.first_active_mhz;
        while band_lo < channel.last_active_mhz {
            let band_hi = band_lo + 6.0;
            let (mut sum, mut n) = (0.0, 0usize);
            for i in range.clone() {
                let f = view.grid.f_mhz[i];
                if f >= band_lo && f < band_hi {
                    sum += lin(view.level_dbmv[i]) * view.h[i].norm_sqr();
                    n += 1;
                }
            }
            if n > 0 {
                bands.push(obs::DsBandLevel {
                    center_mhz: band_lo + 3.0,
                    level_dbmv: read_db(db(sum / n as f64), ctx.noise_db, rng),
                });
            }
            band_lo = band_hi;
        }
        let plc_center = (channel.first_active_mhz + channel.last_active_mhz) / 2.0;
        let plc_level = band_mean_db(view, range.clone(), plc_center, PLC_BAND_MHZ);
        out.ds_rx.push(obs::DsRxRecord {
            channel_id: channel_id.clone(),
            profile_ref: profile_ref.clone(),
            ds_rx_dbmv: obs::DsRxLevels::Ofdm {
                bands,
                plc_band_dbmv: read_db(plc_level, ctx.noise_db, rng),
            },
        });
    }

    // §4.6: per data profile, from each one's own threshold.
    let t_sym_us = 1000.0 / channel.subcarrier_spacing_khz + channel.cp_us;
    let n_subcarriers = (channel.last_active_mhz - channel.first_active_mhz)
        / (channel.subcarrier_spacing_khz / 1000.0);
    let share = 1.0 / channel.data_profiles.len().max(1) as f64;
    let mut failed = Vec::new();
    let mut p_robust = 1.0f64;
    for profile in &channel.data_profiles {
        let thr = fec::thr_db(ctx.profile, profile.modulation).unwrap_or(24.0);
        let m = fec::effective_margin_db(&mer_true, thr);
        let k = fec::ofdm_k(profile.modulation);
        let p_uncorr = fec::p_uncorr_ofdm(m, k);
        let p_corr = fec::p_corr_ofdm(mean_db - thr, m, k);
        p_robust = p_robust.min(p_uncorr);
        if p_uncorr > FAIL_RATE {
            failed.push(profile.data_profile_id);
        }
        if capability.per_profile_fec {
            let bits = fec::bits_per_symbol(profile.modulation);
            let per_s = n_subcarriers * bits / (t_sym_us * 1e-6) / LDPC_BITS * share;
            let total = (per_s * ctx.step_s) as u64;
            let uncorrectable = rng.binomial(total, p_uncorr);
            let corrected = rng.binomial(total - uncorrectable, p_corr);
            out.codewords.push(obs::CodewordRecord {
                channel_id: channel_id.clone(),
                profile_ref: profile_ref.clone(),
                data_profile_id: Some(profile.data_profile_id),
                total,
                corrected,
                uncorrectable,
            });
        }
    }

    // §4.6: the PLC is 16-QAM and the NCP what its channel says, on the data curve at that modulation.
    let plc_mer: Vec<f64> = range
        .clone()
        .zip(&mer_true)
        .filter(|(i, _)| {
            let center = (channel.first_active_mhz + channel.last_active_mhz) / 2.0;
            (view.grid.f_mhz[*i] - center).abs() <= PLC_BAND_MHZ / 2.0
        })
        .map(|(_, m)| *m)
        .collect();
    let plc_thr = fec::thr_db(ctx.profile, Qam::Qam16).unwrap_or(12.0);
    let plc_m = fec::effective_margin_db(&plc_mer, plc_thr);
    let p_plc = fec::p_uncorr_ofdm(plc_m, fec::ofdm_k(Qam::Qam16));
    let ncp_qam = match channel.ncp_modulation {
        schema::profile::NcpModulation::Qam64 => Qam::Qam64,
        _ => Qam::Qam16,
    };
    let ncp_thr = fec::thr_db(ctx.profile, ncp_qam).unwrap_or(12.0);
    let p_ncp = fec::p_uncorr_ofdm(
        fec::effective_margin_db(&mer_true, ncp_thr),
        fec::ofdm_k(ncp_qam),
    );
    if capability.plc_ncp_counters {
        let total = (PLC_CW_PER_S * ctx.step_s) as u64;
        out.plc.push(obs::PlcRecord {
            channel_id: channel_id.clone(),
            profile_ref: profile_ref.clone(),
            total,
            unreliable: rng.binomial(total, p_plc),
        });
        let total = (NCP_PER_S * ctx.step_s) as u64;
        out.ncp.push(obs::NcpRecord {
            channel_id: channel_id.clone(),
            profile_ref: profile_ref.clone(),
            total,
            crc_failures: rng.binomial(total, p_ncp),
        });
    }

    let all_failed = failed.len() == channel.data_profiles.len();
    let usable = pilots_ok && !all_failed;
    out.channel_state.push(obs::ChannelStateRecord {
        channel_id: channel_id.clone(),
        profile_ref: profile_ref.clone(),
        state: if usable {
            obs::ChannelUsability::Usable
        } else {
            obs::ChannelUsability::Unusable
        },
    });
    out.partial.push(obs::PartialChannelRecord {
        channel_id,
        profile_ref,
        failed_data_profiles: failed,
        ncp_failed: p_ncp > FAIL_RATE,
        plc_failed: p_plc > FAIL_RATE,
    });
    // §7.4: with pilots held the channel stays in use, and MAC messages need its NCP and best profile.
    if pilots_ok {
        out.p_down = out.p_down.min(1.0 - (1.0 - p_robust) * (1.0 - p_ncp));
    }
}

fn band_mean_db(
    view: &ModemView<'_>,
    range: std::ops::Range<usize>,
    center_mhz: f64,
    width_mhz: f64,
) -> f64 {
    let (mut sum, mut n) = (0.0, 0usize);
    for i in range {
        if (view.grid.f_mhz[i] - center_mhz).abs() <= width_mhz / 2.0 {
            sum += lin(view.level_dbmv[i]) * view.h[i].norm_sqr();
            n += 1;
        }
    }
    db(sum / n.max(1) as f64)
}

/// Upstream results for one modem.
struct UpResult {
    preeq: obs::Preeq,
    us_tx: Vec<obs::UsTxRecord>,
    us_rx: Vec<obs::UsRxRecord>,
    us_rxmer: Vec<obs::UsRxmerRecord>,
    p_loss: BTreeMap<ChannelId, f64>,
    rx_per_channel_dbmv: BTreeMap<ChannelId, f64>,
}

struct UpContext<'c, 'p> {
    layout: &'c UpLayout<'p>,
    profile_ref: &'c ProfileId,
    plant_capability: &'c CapabilitySet,
    target_dbmv: f64,
    tx_limit: &'c dyn Fn(&ModemSpec) -> f64,
    noise: &'c ReturnNoise,
    optics_snr_db: Option<f64>,
    detail: Detail,
    meas_noise_db: f64,
    seed: u64,
    t: Timestamp,
}

/// §7.3: upstream SNR in linear, from power per 6.4 MHz and noise per Hz.
fn snr_inv(p_rx_dbmv: f64, n_per_hz: f64, optics_snr_db: Option<f64>) -> f64 {
    // §7.3 counts the amps in a node. A node with none still has its own receiver, which sets the floor. `N_coax`'s
    // oracle has amps, and they're far above this, so it doesn't move.
    let n_per_hz = n_per_hz.max(lin(crate::noise::THERMAL_DBMV_PER_HZ + ROOT_RETURN_NF_DB));
    let coax = lin(db(n_per_hz * 6.4e6) - p_rx_dbmv);
    coax + optics_snr_db.map_or(0.0, |o| lin(-o))
}

fn up_modem(ctx: &UpContext<'_, '_>, view: &ModemView<'_>) -> Result<UpResult, PhysicsError> {
    let modem = modem_spec(view.component)
        .ok_or_else(|| PhysicsError::BadComponent(view.modem.id.clone()))?;
    let capability = effective(ctx.plant_capability, &modem.capability);
    let mut rng = Rng::derive(
        ctx.seed,
        &[ctx.t.0 as u64, hash_str(view.modem.id.as_str()), 20],
    );
    let limit_per_1p6 = (ctx.tx_limit)(modem);
    let per_1p6 = per_1p6_from_per_6p4_db();
    let empty = Vec::new();
    let avg = ctx.noise.avg_per_hz.get(view.root_port).unwrap_or(&empty);
    let peak = ctx.noise.peak_per_hz.get(view.root_port).unwrap_or(&empty);
    let at = |v: &Vec<f64>, i: usize| v.get(i).copied().unwrap_or(0.0);

    let mut out = UpResult {
        preeq: obs::Preeq {
            sc_qam: None,
            ofdma: None,
        },
        us_tx: Vec::new(),
        us_rx: Vec::new(),
        us_rxmer: Vec::new(),
        p_loss: BTreeMap::new(),
        rx_per_channel_dbmv: BTreeMap::new(),
    };

    let mut sc_records = Vec::new();
    for stretch in &ctx.layout.sc {
        let channel = stretch.channel;
        let n = stretch.samples.len();
        let mid = stretch.start + n / 2;
        let required = ctx.target_dbmv + view.loss_up_db[mid];
        // §7.2: a modem clamps at its limit, so the CMTS hears it low by the deficit.
        let deficit = (required - per_1p6 - limit_per_1p6).max(0.0);
        let tx_dbmv = required - deficit;
        let p_rx_dbmv = if required.is_finite() {
            ctx.target_dbmv - deficit
        } else {
            f64::NEG_INFINITY
        };
        let snr_peak_inv = snr_inv(p_rx_dbmv, at(peak, mid), ctx.optics_snr_db);
        out.p_loss.insert(
            channel.channel_id.clone(),
            if p_rx_dbmv.is_finite() {
                fec::p_uncorr_sc(-db(snr_peak_inv) - RANGING_THR_DB)
            } else {
                1.0
            },
        );
        out.rx_per_channel_dbmv
            .insert(channel.channel_id.clone(), p_rx_dbmv);
        if !required.is_finite() {
            continue;
        }
        out.us_tx.push(obs::UsTxRecord {
            channel_id: channel.channel_id.clone(),
            profile_ref: ctx.profile_ref.clone(),
            psd_dbmv_per_1p6mhz: read_db(tx_dbmv - per_1p6, ctx.meas_noise_db, &mut rng),
            total_dbmv: Some(read_db(tx_dbmv, ctx.meas_noise_db, &mut rng)),
        });
        out.us_rx.push(obs::UsRxRecord {
            channel_id: channel.channel_id.clone(),
            profile_ref: ctx.profile_ref.clone(),
            us_rx_dbmv_per_1p6mhz: read_db(p_rx_dbmv - per_1p6, ctx.meas_noise_db, &mut rng),
        });
        if capability.sc_qam_preeq {
            let response: Vec<C64> = (0..n)
                .map(|k| {
                    let i = stretch.start + k;
                    view.h[i].scale(amp(-view.loss_up_db[i]))
                })
                .collect();
            let eq = preeq::sc_qam_preeq_from_samples(
                &stretch.samples,
                &preeq::remove_bulk_delay(&response),
            );
            sc_records.push(obs::ScQamPreeqRecord {
                channel_id: channel.channel_id.clone(),
                profile_ref: ctx.profile_ref.clone(),
                taps: eq.taps.iter().map(|t| (*t).into()).collect(),
                main_tap_index: eq.main_tap_index,
                mtc_db: eq.mtc_db,
                mtr_db: eq.mtr_db,
                nmter_db: eq.nmter_db,
            });
        }
    }
    if capability.sc_qam_preeq {
        out.preeq.sc_qam = Some(sc_records);
    }

    if modem.generation == ModemGeneration::D31 {
        let mut ofdma_records = Vec::new();
        for stretch in &ctx.layout.ofdma {
            let channel = stretch.channel;
            let range = stretch.start..stretch.start + stretch.len;
            let loss_mean =
                view.loss_up_db[range.clone()].iter().sum::<f64>() / stretch.len.max(1) as f64;
            let loss_max = view.loss_up_db[range.clone()]
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            let required_max = ctx.target_dbmv + loss_max;
            let deficit = (required_max - per_1p6 - limit_per_1p6).max(0.0);
            let reachable = loss_mean.is_finite();
            let p_rx_dbmv = if reachable {
                ctx.target_dbmv - deficit
            } else {
                f64::NEG_INFINITY
            };
            let mut worst_peak = 0.0f64;
            for i in range.clone() {
                worst_peak = worst_peak.max(snr_inv(p_rx_dbmv, at(peak, i), ctx.optics_snr_db));
            }
            out.p_loss.insert(
                channel.channel_id.clone(),
                if reachable {
                    fec::p_uncorr_sc(-db(worst_peak) - RANGING_THR_DB)
                } else {
                    1.0
                },
            );
            out.rx_per_channel_dbmv
                .insert(channel.channel_id.clone(), p_rx_dbmv);
            if !reachable {
                continue;
            }
            out.us_tx.push(obs::UsTxRecord {
                channel_id: channel.channel_id.clone(),
                profile_ref: ctx.profile_ref.clone(),
                psd_dbmv_per_1p6mhz: read_db(
                    ctx.target_dbmv + loss_mean - deficit - per_1p6,
                    ctx.meas_noise_db,
                    &mut rng,
                ),
                total_dbmv: None,
            });
            out.us_rx.push(obs::UsRxRecord {
                channel_id: channel.channel_id.clone(),
                profile_ref: ctx.profile_ref.clone(),
                us_rx_dbmv_per_1p6mhz: read_db(p_rx_dbmv - per_1p6, ctx.meas_noise_db, &mut rng),
            });
            let axis = subcarrier_axis(channel.first_active_mhz, stretch.spacing_khz, ctx.detail);
            if capability.ofdma_preeq_file || capability.ofdma_preeq_summary {
                let response: Vec<C64> = range
                    .clone()
                    .map(|i| view.h[i].scale(amp(-view.loss_up_db[i])))
                    .collect();
                let coefficients = preeq::ofdma_coefficients(&response);
                let f = &view.grid.f_mhz[range.clone()];
                let summary =
                    preeq::response_summary(&coefficients, f, stretch.spacing_khz * 1000.0);
                ofdma_records.push(obs::OfdmaPreeqRecord {
                    channel_id: channel.channel_id.clone(),
                    profile_ref: ctx.profile_ref.clone(),
                    axis,
                    coefficients: capability
                        .ofdma_preeq_file
                        .then(|| coefficients.iter().map(|c| (*c).into()).collect()),
                    summary: Some(summary),
                });
            }
            if capability.us_rxmer_file || capability.us_rxmer_summary {
                // §7.3: both referenced to the same bandwidth.
                let rxmer: Vec<f64> = range
                    .clone()
                    .map(|i| {
                        let value = -db(snr_inv(p_rx_dbmv, at(avg, i), ctx.optics_snr_db));
                        read_db(value, ctx.meas_noise_db, &mut rng)
                    })
                    .collect();
                let mean = rxmer.iter().sum::<f64>() / rxmer.len().max(1) as f64;
                out.us_rxmer.push(obs::UsRxmerRecord {
                    channel_id: channel.channel_id.clone(),
                    profile_ref: ctx.profile_ref.clone(),
                    axis,
                    us_rxmer_db: capability
                        .us_rxmer_file
                        .then(|| rxmer.iter().map(|m| Some(*m)).collect()),
                    us_rxmer_mean_db: Some(mean),
                });
            }
        }
        if capability.ofdma_preeq_file || capability.ofdma_preeq_summary {
            out.preeq.ofdma = Some(ofdma_records);
        }
    }
    Ok(out)
}

/// Runs the whole fold. Same inputs and seed give byte-identical output.
pub fn simulate(scenario: &Scenario<'_>) -> Result<Vec<StepObservation>, PhysicsError> {
    let mut states = fresh_states(scenario.graph);
    simulate_from(scenario, &mut states, None)
}

/// Every modem registered, as a run starts (§7.4).
pub fn fresh_states(graph: &PlantGraph<'_>) -> BTreeMap<ElementId, ModemState> {
    graph
        .modems()
        .into_iter()
        .map(|m| (m.id.clone(), ModemState::default()))
        .collect()
}

/// The fold carried on from where an earlier call left it (task 26): `states` are the modems' states after the step at
/// `previous`, and come back after the last sample. The modems' draws are keyed by seed, time and modem, so runs split
/// anywhere give the same bytes as one run over every sample.
pub fn simulate_from(
    scenario: &Scenario<'_>,
    states: &mut BTreeMap<ElementId, ModemState>,
    previous: Option<Timestamp>,
) -> Result<Vec<StepObservation>, PhysicsError> {
    let graph = scenario.graph;
    crate::support::check_supported(graph)?;
    for m in graph.modems() {
        states.entry(m.id.clone()).or_default();
    }
    let samples = &scenario.environment.samples;
    let mut out = Vec::with_capacity(samples.len());
    let layout_down = down_layout(graph, scenario.options.detail);
    let layout_up = up_layout(graph, scenario.options.detail);
    for (index, sample) in samples.iter().enumerate() {
        // §10: spacing sets the step. A lone sample stands for one ranging interval's worth.
        let step_s = if index > 0 {
            (sample.t.0 - samples[index - 1].t.0) as f64
        } else if let Some(before) = previous {
            (sample.t.0 - before.0) as f64
        } else if samples.len() > 1 {
            (samples[1].t.0 - sample.t.0) as f64
        } else {
            300.0
        };
        out.push(step(
            scenario,
            sample,
            step_s,
            &layout_down,
            &layout_up,
            states,
        )?);
    }
    Ok(out)
}

fn step(
    scenario: &Scenario<'_>,
    sample: &EnvironmentSample,
    step_s: f64,
    layout_down: &DownLayout<'_>,
    layout_up: &UpLayout<'_>,
    states: &mut BTreeMap<ElementId, ModemState>,
) -> Result<StepObservation, PhysicsError> {
    let graph = scenario.graph;
    let profile = graph.profile();
    let profile_ref = &profile.id;
    let mut overlay = Overlay::build(graph, scenario.faults, sample)?;
    if let Some(seed) = scenario.options.randomize {
        overlay.randomize(graph, seed);
    }
    // §18: the environment's ambient sources at this step.
    overlay.ambient = crate::faults::ambient_at(&scenario.environment.ambient_sources, sample.t);
    // §16: the forward link's CNR, and no RF at all from an analog node whose forward link is dark.
    if let Some(link) = scenario.link {
        overlay.optics_cnr_db = link.forward_cnr_db();
        overlay.cmts_source_mer = link.cmts_source_mer.clone();
        if link.forward_dark() {
            overlay.rf_off.insert(graph.root().id.clone());
        }
    }

    // §6.3: below V_min an active drops out, which is a fact about the plant at this step.
    let power = solve_ac_at(graph, &overlay, sample)?;
    overlay.down.extend(power.browned_out());

    // An RPD's own state decides whether it has RF, before anything reads the plant.
    let root = graph.root();
    let rpd = matches!(root.kind, schema::plant::ElementKind::Rpd { .. }).then(|| {
        crate::rpd::rpd_state(
            &root.id,
            &scenario.faults.faults,
            sample,
            overlay.down.contains(&root.id),
            scenario
                .link
                .is_some_and(crate::optics::NodeLink::digital_down),
        )
    });
    if let Some(state) = &rpd
        && !state.rf_output_up
        && !overlay.down.contains(&root.id)
    {
        overlay.rf_off.insert(root.id.clone());
    }

    let walker = Walker::new(graph, sample, &overlay)?;
    let noise_db = scenario.options.measurement_noise_db;

    let mut downs: BTreeMap<ElementId, DownResult> = BTreeMap::new();
    let ctx_down = DownContext {
        layout: layout_down,
        profile_ref,
        profile,
        plant_capability: &profile.capability,
        detail: scenario.options.detail,
        noise_db,
        rxmer_scatter_db: scenario.options.rxmer_scatter_db,
        seed: scenario.seed,
        t: sample.t,
        step_s,
        modem_draws: &overlay.modem_draws,
    };
    walker.run(&layout_down.grid, Band::Down, &mut |view: &ModemView<
        '_,
    >| {
        downs.insert(view.modem.id.clone(), down_modem(&ctx_down, view)?);
        Ok(())
    })?;

    // Return noise needs a whole pass before any modem can be read against it.
    let mut null = |_: &ModemView<'_>| Ok(());
    let return_noise = walker.run(&layout_up.grid, Band::Up, &mut null)?;

    let (us_lo, us_hi) = us_band(profile);
    let tx_limit = |modem: &ModemSpec| -> f64 {
        match modem.generation {
            ModemGeneration::D31 => p1p6hi_dbmv(modem.tx_max_total_dbmv, 0.0, us_hi - us_lo),
            ModemGeneration::D30 => {
                let channels = profile.channel_plan.us_sc_qam.len().max(1) as f64;
                modem.tx_max_total_dbmv - db(channels) - per_1p6_from_per_6p4_db()
            }
        }
    };
    let root_component = graph
        .component(&graph.root().component_ref)
        .ok_or_else(|| PhysicsError::BadComponent(graph.root().id.clone()))?;
    let optics_snr_db = match (&root_component.spec, profile.node_kind) {
        (ComponentSpec::Node(node), NodeKind::Analog) => Some(
            scenario
                .link
                .and_then(crate::optics::NodeLink::return_snr_db)
                .unwrap_or(node.us_optics_snr_db),
        ),
        _ => None,
    };
    // §17: the burst receiver's own ceiling, the RPD's or the bound CMTS's, combined with the optics.
    let ceiling_db = match &root_component.spec {
        ComponentSpec::Rpd(rpd) => rpd.us_rx_ceiling_db,
        ComponentSpec::Node(_) => scenario.link.and_then(|l| l.cmts_us_rx_ceiling_db),
        _ => None,
    };
    let optics_snr_db = match (optics_snr_db, ceiling_db) {
        (Some(optics), Some(ceiling)) => Some(-db(lin(-optics) + lin(-ceiling))),
        (optics, ceiling) => optics.or(ceiling),
    };
    let ctx_up = UpContext {
        layout: layout_up,
        profile_ref,
        plant_capability: &profile.capability,
        target_dbmv: profile.design_levels.us_node_in_target_dbmv,
        tx_limit: &tx_limit,
        noise: &return_noise,
        optics_snr_db,
        detail: scenario.options.detail,
        meas_noise_db: noise_db,
        seed: scenario.seed,
        t: sample.t,
    };
    let mut ups: BTreeMap<ElementId, UpResult> = BTreeMap::new();
    walker.run(&layout_up.grid, Band::Up, &mut |view: &ModemView<'_>| {
        ups.insert(view.modem.id.clone(), up_modem(&ctx_up, view)?);
        Ok(())
    })?;

    let modems = assemble_modems(scenario, sample, step_s, &mut downs, &mut ups, states)?;
    let actives = active_observations(graph, sample, &walker, &power, &overlay)?;
    let service_group = service_group_observation(
        scenario,
        sample,
        step_s,
        layout_up,
        &return_noise,
        optics_snr_db,
        overlay.down.contains(&graph.root().id) || overlay.rf_off.contains(&graph.root().id),
        if overlay.down.contains(&graph.root().id) {
            None
        } else {
            optics_observation(scenario)
        },
    )?;
    Ok(StepObservation {
        t: sample.t,
        modems,
        actives,
        service_group,
        rpd,
    })
}

fn assemble_modems(
    scenario: &Scenario<'_>,
    sample: &EnvironmentSample,
    step_s: f64,
    downs: &mut BTreeMap<ElementId, DownResult>,
    ups: &mut BTreeMap<ElementId, UpResult>,
    states: &mut BTreeMap<ElementId, ModemState>,
) -> Result<Vec<obs::ModemObservation>, PhysicsError> {
    let graph = scenario.graph;
    let profile = graph.profile();
    let behavior = &profile.modem_behavior;
    let mut out = Vec::new();
    for element in graph.modems() {
        let id = &element.id;
        let modem = graph
            .component(&element.component_ref)
            .and_then(modem_spec)
            .ok_or_else(|| PhysicsError::BadComponent(id.clone()))?;
        let capability = effective(&profile.capability, &modem.capability);
        let (Some(down), Some(up), Some(state)) =
            (downs.remove(id), ups.remove(id), states.get_mut(id))
        else {
            return Err(PhysicsError::BadComponent(id.clone()));
        };

        // §7.4: one exchange fails if the request or the reply is lost.
        let p_loss: BTreeMap<ChannelId, f64> = up
            .p_loss
            .iter()
            .map(|(c, p)| (c.clone(), 1.0 - (1.0 - p) * (1.0 - down.p_down)))
            .collect();
        let mut rng = Rng::derive(
            scenario.seed,
            &[sample.t.0 as u64, hash_str(id.as_str()), 30],
        );
        state.advance(
            &StepInput {
                step_s,
                p_down: down.p_down,
                p_loss,
                ranging_spacing_s: behavior.ranging_spacing_s,
                reregistration_s: behavior.reregistration_s,
            },
            &mut rng,
        );

        let operational = state.reg_state == obs::RegState::Operational;
        let profile_ref = &profile.id;
        let channel_records = |map: &BTreeMap<ChannelId, u64>| -> Vec<obs::TimeoutRecord> {
            map.iter()
                .map(|(channel_id, count)| obs::TimeoutRecord {
                    channel_id: channel_id.clone(),
                    profile_ref: profile_ref.clone(),
                    count: *count,
                })
                .collect()
        };
        let us_channels: Vec<&ChannelId> = state.t3.keys().collect();
        let us_channel_state = us_channels
            .iter()
            .map(|c| obs::ChannelStateRecord {
                channel_id: (*c).clone(),
                profile_ref: profile_ref.clone(),
                state: if state.us_unusable.contains(*c) {
                    obs::ChannelUsability::Unusable
                } else {
                    obs::ChannelUsability::Usable
                },
            })
            .collect();
        let ranging_status = us_channels
            .iter()
            .map(|c| obs::RangingStatusRecord {
                channel_id: (*c).clone(),
                profile_ref: profile_ref.clone(),
                status: if state.us_unusable.contains(*c) {
                    obs::RangingState::RetriesExceeded
                } else {
                    obs::RangingState::Success
                },
            })
            .collect();

        // A modem that isn't registered answers nothing but its counters (§11: validity at emit).
        let on = |present: bool| operational && present;
        out.push(obs::ModemObservation {
            schema_version: obs::SCHEMA_VERSION.to_string(),
            t: sample.t,
            origin: Origin::Synthetic,
            element_id: id.clone(),
            generation: modem.generation,
            rxmer: on(capability.ds_rxmer_file || capability.ds_rxmer_summary)
                .then_some(down.rxmer),
            fbc: if operational { down.fbc } else { None },
            ds_chan_est: on(capability.ds_chan_est_file || capability.ds_chan_est_summary)
                .then_some(down.chan_est),
            ds_rx: if operational { down.ds_rx } else { Vec::new() },
            codewords: if operational {
                down.codewords
            } else {
                Vec::new()
            },
            plc: on(capability.plc_ncp_counters).then_some(down.plc),
            ncp: on(capability.plc_ncp_counters).then_some(down.ncp),
            preeq: if operational {
                up.preeq
            } else {
                obs::Preeq {
                    sc_qam: None,
                    ofdma: None,
                }
            },
            us_tx: if operational { up.us_tx } else { Vec::new() },
            us_rx: if operational { up.us_rx } else { Vec::new() },
            us_rxmer: on(capability.us_rxmer_file || capability.us_rxmer_summary)
                .then_some(up.us_rxmer),
            reg_state: state.reg_state,
            ds_channel_state: down.channel_state,
            us_channel_state,
            partial_channel: on(capability.plc_ncp_counters).then_some(down.partial),
            t3_timeouts: channel_records(&state.t3),
            t4_timeouts: channel_records(&state.t4),
            ranging_status,
        });
    }
    Ok(out)
}

fn active_observations(
    graph: &PlantGraph<'_>,
    sample: &EnvironmentSample,
    walker: &Walker<'_>,
    power: &crate::ac::AcSolution,
    overlay: &Overlay,
) -> Result<Vec<obs::ActiveObservation>, PhysicsError> {
    let profile = graph.profile();
    let capability = &profile.capability;
    let (_, ds_hi) = ds_band(profile);
    let mut out = Vec::new();
    for element in graph.actives() {
        let state = power.actives.get(&element.id);
        // An RPD with its fiber cut still draws power and reports it. Only its RF is off.
        let no_rf = overlay.rf_off.contains(&element.id);
        let down = overlay.down.contains(&element.id)
            || overlay.amps.get(&element.id).is_some_and(|o| o.failed);
        let dev_db = walker.out_dev_at_pilot_db(&element.id);
        let level = dev_db
            .filter(|d| d.is_finite())
            .map(|d| profile.design_levels.out_dbmv_at_hi + d);
        out.push(obs::ActiveObservation {
            schema_version: obs::SCHEMA_VERSION.to_string(),
            t: sample.t,
            origin: Origin::Synthetic,
            element_id: element.id.clone(),
            v_in: if capability.v_in {
                state.map(|s| s.v_in)
            } else {
                None
            },
            i_draw: if capability.i_draw && !down {
                state.map(|s| s.i_draw)
            } else {
                None
            },
            temp_c: capability.temp_c.then_some(sample.temp_c),
            status: capability.status.then_some(if down {
                obs::ActiveStatus::Down
            } else {
                obs::ActiveStatus::Up
            }),
            out_level: if capability.out_level && !down && !no_rf {
                level.map(|out_level_dbmv| obs::OutLevel {
                    out_level_dbmv,
                    f_ref_mhz: ds_hi,
                })
            } else {
                None
            },
            out_levels: if profile.plant_reports.out_level_per_port && !down && !no_rf {
                level.map_or_else(Vec::new, |main_dbmv| {
                    port_levels(graph, element, main_dbmv, ds_hi)
                })
            } else {
                Vec::new()
            },
        });
    }
    Ok(out)
}

/// §15: each output port's level, from the main output's: less the port's loss from the main and what's fitted on it, at the
/// reading's frequency.
fn port_levels(
    graph: &PlantGraph<'_>,
    element: &schema::plant::Element,
    main_dbmv: f64,
    f_mhz: f64,
) -> Vec<obs::PortOutLevel> {
    let Some(active) = graph
        .component(&element.component_ref)
        .and_then(|c| c.spec.active())
    else {
        return Vec::new();
    };
    let fitted = match &element.fitted {
        Some(schema::plant::Fitted::Active(fitted)) => Some(fitted),
        _ => None,
    };
    let plugin_db = |slot: Option<&schema::common::ComponentId>| {
        slot.and_then(|id| graph.component(id))
            .and_then(|c| match &c.spec {
                ComponentSpec::PlugIn(spec) => Some(crate::passive::plugin_loss_db(spec, f_mhz)),
                _ => None,
            })
            .unwrap_or(0.0)
    };
    element
        .ports
        .iter()
        .filter(|p| p.role == schema::plant::PortRole::Output)
        .map(|port| {
            let from_main_db = active
                .outputs
                .iter()
                .find(|o| o.port == port.id)
                .map_or(0.0, |o| o.loss_from_main_db);
            let slot = fitted.and_then(|f| f.outputs.iter().find(|o| o.port == port.id));
            let fitted_db = plugin_db(slot.and_then(|o| o.pad.as_ref()))
                + plugin_db(slot.and_then(|o| o.eq.as_ref()));
            obs::PortOutLevel {
                port: port.id.clone(),
                out_level_dbmv: main_dbmv - from_main_db - fitted_db,
                f_ref_mhz: f_mhz,
            }
        })
        .collect()
}

/// A reading to 0.1 dB.
fn tenth_db(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// §16: what the node's optics report, under the profile's flags. An analog node reads its forward receiver in 0.1 mW
/// (`fnOpticalReceiverPower` [SCTE 38-5]), so faint light reads nothing; the hub's receiver and an RPD's optics read to 0.1 dB.
fn optics_observation(scenario: &Scenario<'_>) -> Option<obs::OpticsObservation> {
    let graph = scenario.graph;
    let reports = graph.profile().plant_reports;
    if !(reports.node_optical_rx || reports.hub_optical_rx || reports.rpd_optic_status) {
        return None;
    }
    let link = scenario.link?;
    let analog = matches!(
        graph
            .component(&graph.root().component_ref)
            .map(|c| &c.spec),
        Some(ComponentSpec::Node(_))
    );
    let hms_dbm = |dbm: f64| {
        let tenths_mw = (lin(dbm) / 0.1).round();
        (tenths_mw > 0.0).then(|| tenth_db(db(tenths_mw * 0.1)))
    };
    let forward_rx = link.forward.as_ref().and_then(|l| l.received_dbm);
    let node_rx_dbm = if reports.node_optical_rx {
        forward_rx.and_then(|p| {
            if analog {
                hms_dbm(p)
            } else {
                Some(tenth_db(p))
            }
        })
    } else {
        None
    };
    let hub_rx_dbm = if reports.hub_optical_rx {
        link.ret.as_ref().and_then(|l| l.received_dbm).map(tenth_db)
    } else {
        None
    };
    let rpd_optic = if reports.rpd_optic_status && !analog {
        link.ret.as_ref().map(|ret| obs::OpticStatus {
            link_up: !link.digital_down(),
            tx_dbm: tenth_db(ret.launch_dbm),
            rx_dbm: forward_rx.map(tenth_db),
        })
    } else {
        None
    };
    Some(obs::OpticsObservation {
        node_rx_dbm,
        hub_rx_dbm,
        rpd_optic,
    })
}

#[allow(clippy::too_many_arguments)]
fn service_group_observation(
    scenario: &Scenario<'_>,
    sample: &EnvironmentSample,
    step_s: f64,
    layout: &UpLayout<'_>,
    noise: &ReturnNoise,
    optics_snr_db: Option<f64>,
    root_off: bool,
    optics: Option<obs::OpticsObservation>,
) -> Result<obs::ServiceGroupObservation, PhysicsError> {
    let graph = scenario.graph;
    // A node that's off measures nothing, so there's no value to emit (§11). Its optics still report, unless it's unpowered.
    // §16: with its return link down, the CMTS hears nothing from it either.
    let return_dark = optics_snr_db == Some(f64::NEG_INFINITY);
    if root_off || return_dark {
        return Ok(obs::ServiceGroupObservation {
            schema_version: obs::SCHEMA_VERSION.to_string(),
            t: sample.t,
            origin: Origin::Synthetic,
            service_group: graph.plant().service_group.clone(),
            us_snr: Vec::new(),
            us_spectrum: None,
            us_codewords: Vec::new(),
            optics,
        });
    }
    let profile = graph.profile();
    let target = profile.design_levels.us_node_in_target_dbmv;
    let profile_ref = &profile.id;
    let mut rng = Rng::derive(scenario.seed, &[sample.t.0 as u64, 40]);

    // Each channel at the worst root port. Every port's noise is its own (§7.3).
    let worst_at = |map: &BTreeMap<PortId, Vec<f64>>, i: usize| -> f64 {
        map.values()
            .map(|v| v.get(i).copied().unwrap_or(0.0))
            .fold(0.0, f64::max)
    };
    let mut us_snr = Vec::new();
    let mut us_codewords = Vec::new();
    let mut channel_entry = |channel_id: &ChannelId,
                             modulation: Qam,
                             avg_per_hz: f64,
                             peak_per_hz: f64,
                             duty_hint: f64| {
        let snr_db = -db(snr_inv(target, avg_per_hz, optics_snr_db));
        let peak_db = -db(snr_inv(target, peak_per_hz, optics_snr_db));
        us_snr.push(obs::UsSnrRecord {
            channel_id: channel_id.clone(),
            profile_ref: profile_ref.clone(),
            us_snr_db: read_db(snr_db, scenario.options.measurement_noise_db, &mut rng),
        });
        let thr = fec::sc_qam_thr_db(profile, modulation).unwrap_or(20.0);
        // §8: FEC reads peak noise during a burst, and the average the rest of the time.
        let p = duty_hint * fec::p_uncorr_sc(peak_db - thr)
            + (1.0 - duty_hint) * fec::p_uncorr_sc(snr_db - thr);
        let total = (US_CW_PER_S * step_s) as u64;
        let uncorrectable = rng.binomial(total, p);
        let corrected = rng.binomial(total - uncorrectable, fec::p_corr_sc(snr_db - thr));
        us_codewords.push(obs::UsCodewordRecord {
            channel_id: channel_id.clone(),
            profile_ref: profile_ref.clone(),
            total,
            corrected,
            uncorrectable,
        });
    };
    for stretch in &layout.sc {
        let mid = stretch.start + stretch.samples.len() / 2;
        let (avg, peak) = (
            worst_at(&noise.avg_per_hz, mid),
            worst_at(&noise.peak_per_hz, mid),
        );
        let duty = if peak > avg && avg > 0.0 {
            avg / peak
        } else {
            0.0
        };
        channel_entry(
            &stretch.channel.channel_id,
            stretch.channel.modulation,
            avg,
            peak,
            duty,
        );
    }
    for stretch in &layout.ofdma {
        let range = stretch.start..stretch.start + stretch.len;
        let avg = range
            .clone()
            .map(|i| worst_at(&noise.avg_per_hz, i))
            .sum::<f64>()
            / stretch.len.max(1) as f64;
        let peak =
            range.map(|i| worst_at(&noise.peak_per_hz, i)).sum::<f64>() / stretch.len.max(1) as f64;
        let duty = if peak > avg && avg > 0.0 {
            avg / peak
        } else {
            0.0
        };
        let modulation = stretch
            .channel
            .data_profiles
            .last()
            .map_or(Qam::Qam256, |p| p.modulation);
        channel_entry(&stretch.channel.channel_id, modulation, avg, peak, duty);
    }

    let us_spectrum = layout.spectrum.map(|(start, n)| {
        noise
            .avg_per_hz
            .iter()
            .map(|(port, per_hz)| {
                let stride = if matches!(scenario.options.detail, Detail::Full) {
                    1.0
                } else {
                    8.0
                };
                let bin_khz = US_SPECTRUM_BIN_KHZ * stride;
                let nbw_hz = bin_khz * 1000.0;
                let bins: Vec<f64> = (0..n)
                    .map(|k| {
                        let i = start + k;
                        let f = layout.grid.f_mhz[i];
                        let carrier = if carried_up(profile, f) {
                            lin(target + db(nbw_hz / 6.4e6))
                        } else {
                            0.0
                        };
                        let noise_lin = per_hz.get(i).copied().unwrap_or(0.0) * nbw_hz;
                        (db(carrier + noise_lin) * 10.0).round() / 10.0
                    })
                    .collect();
                obs::UsSpectrumRecord {
                    port: port.clone(),
                    profile_ref: profile_ref.clone(),
                    first_bin_center_mhz: layout.spectrum_lo_mhz + bin_khz / 2000.0,
                    bin_width_khz: bin_khz,
                    noise_bandwidth_khz: bin_khz,
                    us_spectrum_dbmv: bins,
                }
            })
            .collect()
    });

    Ok(obs::ServiceGroupObservation {
        schema_version: obs::SCHEMA_VERSION.to_string(),
        t: sample.t,
        origin: Origin::Synthetic,
        service_group: graph.plant().service_group.clone(),
        us_snr,
        us_spectrum,
        us_codewords,
        optics,
    })
}

fn carried_up(profile: &schema::profile::Profile, f_mhz: f64) -> bool {
    let plan = &profile.channel_plan;
    plan.us_sc_qam
        .iter()
        .any(|c| (f_mhz - c.center_mhz).abs() <= c.width_mhz / 2.0)
        || plan
            .ofdma
            .iter()
            .any(|c| f_mhz >= c.first_active_mhz && f_mhz <= c.last_active_mhz)
}
