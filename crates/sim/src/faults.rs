//! §8: a fault set at one time step, as an overlay on the walk. Severity comes from onset, schedule
//! and environment. Nothing here knows what a fault looks like in telemetry (invariant 5).

use plant_graph::PlantGraph;
use schema::common::{EdgeId, Timestamp};
use schema::environment::AmbientSource;
use schema::environment::EnvironmentSample;
use schema::fault::{
    AmpState, Burst, ConnectorCouplings, CutEnd, Fault, FaultKind, FaultSet, Location,
    NodeFailureMode, Onset, Schedule,
};
use schema::plant::{EdgeKind, PortRole};

use crate::cable::exposure_temp_c;
use crate::error::PhysicsError;
use crate::overlay::{
    Ingress, Overlay, PointImpairment, ReturnLoss, ShapedLoss, ShortPoint, WetSection,
};
use crate::rng::{Rng, hash_str};
use crate::units::db;

/// §8: the width of each step of a common-path product's spectrum, MHz.
const CPD_BIN_MHZ: f64 = 0.5;
/// §8: the resolution the forward plan's spectrum is convolved at, MHz.
const CPD_GRID_MHZ: f64 = 0.25;
/// §8: an SC-QAM's occupied width within its channel, as a share (approx).
const SC_OCCUPIED_SHARE: f64 = 0.86;

/// §20.1: what crosses a cut, dB: nothing a modem or an amp can use (approx; a number, since the walk adds dB).
pub const CUT_LOSS_DB: f64 = 300.0;
/// §20.1: a cut end reflects everything.
const CUT_RL_DB: f64 = 0.0;
/// §20.1: a cut whose center conductor lies against the shield, ohms (approx).
pub const CUT_SHORT_OHM: f64 = 0.1;
/// §20.4: the line's impedance, ohms.
const Z0_OHM: f64 = 75.0;
/// §20.5: a damaged device's CPD, third order against second, dB (approx) [inferred].
const DAMAGE_CPD_THIRD_DB: f64 = -10.0;

/// §20: seconds since an event began, while it lasts: from its onset's start until its repair, if it has one.
pub fn event_elapsed_s(onset: &Onset, t: Timestamp, repair_s: Option<f64>) -> Option<f64> {
    let start = match onset {
        Onset::Instant => 0,
        Onset::Ramp { start, .. } => start.0,
    };
    let elapsed = (t.0 - start) as f64;
    if elapsed < 0.0 || repair_s.is_some_and(|r| elapsed >= r) {
        return None;
    }
    Some(elapsed)
}

/// §20.3: water's loss at a point, `k·√(f/1000)` dB, as straight lines every 25 MHz to 3 GHz.
fn sqrt_loss(db_at_1ghz: f64) -> ShapedLoss {
    let points = (0..=120)
        .map(|i| {
            let f = 25.0 * i as f64;
            (f, db_at_1ghz * (f / 1000.0).sqrt())
        })
        .collect();
    ShapedLoss::Piecewise { points }
}

/// §20.4: a shunt resistance `r` across a matched line: the loss through it and the return loss of its inverted
/// reflection, dB. `T = 2r/(2r + Z0)`, `Γ = −Z0/(2r + Z0)`.
pub fn shunt_db(r_ohm: f64) -> (f64, f64) {
    let r = r_ohm.max(1e-6);
    let t = 2.0 * r / (2.0 * r + Z0_OHM);
    let gamma = Z0_OHM / (2.0 * r + Z0_OHM);
    (-20.0 * t.log10(), -20.0 * gamma.log10())
}

/// §20.1: a cut at a point: nothing passes, and the live side ends in a full reflection, inverted for a short.
fn cut_point(end: CutEnd) -> PointImpairment {
    PointImpairment {
        loss: Some(ShapedLoss::Flat {
            loss_db: CUT_LOSS_DB,
        }),
        reflector: Some(ReturnLoss::Flat { rl_db: CUT_RL_DB }),
        inverted: end == CutEnd::Shorted,
        ..Default::default()
    }
}

/// The return path's band, MHz.
fn return_band(profile: &schema::profile::Profile) -> (f64, f64) {
    profile
        .band_plan
        .iter()
        .find(|b| b.direction == schema::profile::Direction::Us)
        .map_or((5.0, 42.0), |b| (b.lo_mhz, b.hi_mhz))
}

/// §8: the forward plan's occupied spectrum on a grid from 0 MHz.
fn forward_mask(profile: &schema::profile::Profile) -> Vec<f64> {
    let plan = &profile.channel_plan;
    let top_mhz = plan
        .ds_sc_qam
        .iter()
        .map(|c| c.center_mhz + c.width_mhz / 2.0)
        .chain(plan.ofdm.iter().map(|c| c.last_active_mhz))
        .fold(0.0, f64::max);
    let n = (top_mhz / CPD_GRID_MHZ) as usize + 2;
    let mut mask = vec![0.0; n];
    for (lo, hi) in plan
        .ds_sc_qam
        .iter()
        .map(|c| {
            let half = c.width_mhz * SC_OCCUPIED_SHARE / 2.0;
            (c.center_mhz - half, c.center_mhz + half)
        })
        .chain(
            plan.ofdm
                .iter()
                .map(|c| (c.first_active_mhz, c.last_active_mhz)),
        )
    {
        let (a, b) = ((lo / CPD_GRID_MHZ) as usize, (hi / CPD_GRID_MHZ) as usize);
        for value in mask.iter_mut().take(b.min(n)).skip(a) {
            *value = 1.0;
        }
    }
    mask
}

/// §8: autocorrelation of a spectrum at `lag` grid steps: the difference products at that spacing.
fn autocorrelation(mask: &[f64], lag: usize) -> f64 {
    mask.iter()
        .zip(mask.iter().skip(lag))
        .map(|(a, b)| a * b)
        .sum::<f64>()
        * CPD_GRID_MHZ
}

/// §8: a common-path product's density across the return band, dBmV per MHz. The shape comes from
/// the forward plan alone: second order is its autocorrelation, third that mixed with the plan again.
pub fn cpd_density_per_mhz(
    profile: &schema::profile::Profile,
    lo_mhz: f64,
    hi_mhz: f64,
    second_order_dbmv_per_mhz: f64,
    third_over_second_db: f64,
) -> Vec<(f64, f64)> {
    let mask = forward_mask(profile);
    let largest = autocorrelation(&mask, 0).max(f64::MIN_POSITIVE);
    let third_share = crate::units::lin(third_over_second_db);
    let mut out = Vec::new();
    let mut f_mhz = lo_mhz + CPD_BIN_MHZ / 2.0;
    while f_mhz < hi_mhz {
        let lag = (f_mhz / CPD_GRID_MHZ).round() as usize;
        let second = autocorrelation(&mask, lag) / largest;
        // Third order at this offset spreads a second-order product over one more channel's width: smooth it.
        let reach = (6.0 / CPD_GRID_MHZ) as usize / 2;
        let third = (lag.saturating_sub(reach)..=lag + reach)
            .map(|l| autocorrelation(&mask, l) / largest)
            .sum::<f64>()
            / (2 * reach + 1) as f64;
        let density = second + third_share * third;
        if density > 0.0 {
            out.push((f_mhz, second_order_dbmv_per_mhz + db(density)));
        }
        f_mhz += CPD_BIN_MHZ;
    }
    out
}

/// How far along its onset a fault is at `t`, 0 to 1.
pub fn onset_share(onset: &Onset, t: Timestamp) -> f64 {
    match onset {
        Onset::Instant => 1.0,
        Onset::Ramp { start, end } => {
            // A ramp of no length is a step at its start, as `Instant` is at 0: an event at a world's time T (task 26).
            if end.0 <= start.0 {
                if t.0 >= start.0 { 1.0 } else { 0.0 }
            } else if t.0 <= start.0 {
                0.0
            } else if t.0 >= end.0 {
                1.0
            } else {
                (t.0 - start.0) as f64 / (end.0 - start.0) as f64
            }
        }
    }
}

/// §8: hour = (t / 3600) mod 24, with t = 0 at midnight.
pub fn schedule_multiplier(schedule: &Schedule, t: Timestamp) -> f64 {
    let hour = (t.0.div_euclid(3600)).rem_euclid(24) as usize;
    schedule.hourly_multipliers[hour]
}

/// A connector's return loss part way through its onset: the reflection's amplitude scales.
fn ramped_rl_db(rl_db: f64, share: f64) -> f64 {
    if share <= 0.0 {
        f64::INFINITY
    } else {
        rl_db - 20.0 * share.log10()
    }
}

/// Water's impedance steps: about one per 10 ft, kept few enough to stay cheap.
const WET_STEP_FT: f64 = 10.0;
const WET_STEPS_MAX: usize = 16;

fn add_point(
    overlay: &mut Overlay,
    graph: &PlantGraph<'_>,
    location: &Location,
    point: PointImpairment,
) -> Result<(), PhysicsError> {
    match location {
        Location::Span { span_id, pos_ft } => {
            graph.edge(span_id)?;
            let points = overlay.span_points.entry(span_id.clone()).or_default();
            points.push((*pos_ft, point));
            points.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        Location::Port { element_id, port } => {
            graph.element(element_id)?;
            overlay
                .port_points
                .entry((element_id.clone(), port.clone()))
                .or_default()
                .push(point);
        }
    }
    Ok(())
}

fn ingress(
    f_mhz: f64,
    bw_mhz: f64,
    power_dbmv: f64,
    multiplier: f64,
    burst: Option<&Burst>,
) -> Option<Ingress> {
    if multiplier <= 0.0 {
        return None;
    }
    // A burst's time-average power is its duty cycle down from its power while on (§8).
    let duty = burst.map_or(1.0, |b| {
        (b.duration_ms * b.rate_per_s / 1000.0).clamp(1e-6, 1.0)
    });
    Some(Ingress {
        f_mhz,
        bw_mhz,
        power_dbmv: power_dbmv + db(multiplier) + db(duty),
        peak_over_avg_db: -db(duty),
    })
}

/// §19: a connector fault's return loss at a sample, from its 20 °C, calm value and its couplings, held at 0 dB or more.
pub fn connector_rl_db(
    rl_0_db: f64,
    couplings: &ConnectorCouplings,
    temp_c: f64,
    wind_m_per_s: f64,
) -> f64 {
    let rl_db = rl_0_db
        + couplings.temp_db_per_c.unwrap_or(0.0) * (temp_c - 20.0)
        + couplings.wind_db_per_m_per_s.unwrap_or(0.0) * wind_m_per_s;
    rl_db.max(0.0)
}

/// §19: the temperature at a fault's place: a span's own, or at a port the edge's into that element. The root has none
/// above it, and is in air.
fn location_temp_c(
    graph: &PlantGraph<'_>,
    location: &Location,
    sample: &EnvironmentSample,
) -> Result<f64, PhysicsError> {
    let edge = match location {
        Location::Span { span_id, .. } => Some(graph.edge(span_id)?),
        Location::Port { element_id, .. } => graph.parent(element_id)?.map(|(_, edge)| edge),
    };
    let exposure = match edge {
        Some(edge) => crate::transfer::edge_cable(graph, edge)?.map(|(_, exposure)| exposure),
        None => None,
    };
    Ok(exposure_temp_c(
        sample,
        exposure.unwrap_or(crate::cable::Exposure::Air),
    ))
}

/// §18: the environment's ambient sources at time `t`, at zero coupling: each its band's total power after its schedule. A
/// source its schedule turns off at this hour is left out.
pub fn ambient_at(sources: &[AmbientSource], t: Timestamp) -> Vec<Ingress> {
    sources
        .iter()
        .filter_map(|source| {
            let multiplier = schedule_multiplier(&source.schedule, t);
            let bw_mhz = source.hi_mhz - source.lo_mhz;
            (multiplier > 0.0 && bw_mhz > 0.0).then(|| Ingress {
                f_mhz: (source.lo_mhz + source.hi_mhz) / 2.0,
                bw_mhz,
                power_dbmv: source.level_dbmv_per_mhz + db(bw_mhz) + db(multiplier),
                peak_over_avg_db: 0.0,
            })
        })
        .collect()
}

impl Overlay {
    /// Builds the overlay for `faults` at one environment sample. `seed` is the fault set's, and
    /// only sets where a wet cable's impedance steps fall.
    pub fn build(
        graph: &PlantGraph<'_>,
        faults: &FaultSet,
        sample: &EnvironmentSample,
    ) -> Result<Overlay, PhysicsError> {
        crate::support::check_supported(graph)?;
        let mut overlay = Overlay::default();
        for fault in &faults.faults {
            overlay.add_fault(graph, fault, sample, faults.seed)?;
        }
        Ok(overlay)
    }

    fn add_fault(
        &mut self,
        graph: &PlantGraph<'_>,
        fault: &Fault,
        sample: &EnvironmentSample,
        seed: u64,
    ) -> Result<(), PhysicsError> {
        let share = onset_share(&fault.onset, sample.t);
        if share <= 0.0 {
            return Ok(());
        }
        match &fault.kind {
            FaultKind::WaterIngress {
                span_id,
                pos_ft,
                affected_len_ft,
                k_w_db_per_100ft,
                rain_coupling_per_mm_per_hr,
                impedance_variation_pct,
            } => {
                let k_w_eff = k_w_db_per_100ft
                    * (1.0 + rain_coupling_per_mm_per_hr * sample.rain_mm_per_hr)
                    * share;
                self.wet
                    .entry(span_id.clone())
                    .or_default()
                    .push(WetSection {
                        pos_ft: *pos_ft,
                        len_ft: *affected_len_ft,
                        k_w_eff_db_per_100ft: k_w_eff,
                    });
                self.add_wet_steps(
                    graph,
                    span_id,
                    *pos_ft,
                    *affected_len_ft,
                    *impedance_variation_pct * share,
                    seed,
                )?;
            }
            FaultKind::ConnectorReflection {
                location,
                return_loss_db,
                couplings,
            } => {
                // §19: its return loss moves with the temperature at its place and the wind.
                let rl_db = match couplings {
                    Some(c) if c.temp_db_per_c.is_some() || c.wind_db_per_m_per_s.is_some() => {
                        connector_rl_db(
                            *return_loss_db,
                            c,
                            location_temp_c(graph, location, sample)?,
                            sample.wind_m_per_s.unwrap_or(0.0),
                        )
                    }
                    _ => *return_loss_db,
                };
                let point = PointImpairment {
                    reflector: Some(ReturnLoss::Flat {
                        rl_db: ramped_rl_db(rl_db, share),
                    }),
                    // §18: an opened shield lets ambient in, more as the fault comes on.
                    ambient_coupling_db: couplings
                        .as_ref()
                        .and_then(|c| c.ambient_coupling_db)
                        .filter(|_| share > 0.0)
                        .map(|c| c - db(share)),
                    ..Default::default()
                };
                add_point(self, graph, location, point)?;
            }
            FaultKind::Suckout {
                location,
                f0_mhz,
                depth_db,
                width_mhz,
            } => {
                let point = PointImpairment {
                    loss: Some(ShapedLoss::Gaussian {
                        f0_mhz: *f0_mhz,
                        depth_db: depth_db * share,
                        width_mhz: *width_mhz,
                    }),
                    ..Default::default()
                };
                add_point(self, graph, location, point)?;
            }
            FaultKind::FrequencySelectiveLoss {
                location,
                shape,
                f_lo_mhz,
                f_hi_mhz,
                depth_db,
                stop_return_loss_db,
                skirt_mhz,
            } => {
                let depth = depth_db * share;
                let loss = match skirt_mhz {
                    Some(e) => ShapedLoss::skirted(*shape, *f_lo_mhz, *f_hi_mhz, depth, *e),
                    None => ShapedLoss::selective(*shape, *f_lo_mhz, *f_hi_mhz, depth),
                };
                let point = PointImpairment {
                    reflector: Some(ReturnLoss::StopBand {
                        pass_rl_db: crate::overlay::PASS_RL_DB,
                        stop_rl_db: ramped_rl_db(*stop_return_loss_db, share),
                        shape: loss.clone(),
                        depth_db: depth,
                    }),
                    loss: Some(loss),
                    ..Default::default()
                };
                add_point(self, graph, location, point)?;
            }
            FaultKind::AmpDrift { amp_id, state } => {
                graph.element(amp_id)?;
                let entry = self.amps.entry(amp_id.clone()).or_default();
                match state {
                    AmpState::Failed => {
                        if share >= 1.0 {
                            entry.failed = true;
                        }
                    }
                    AmpState::Drift {
                        gain_delta_db,
                        tilt_delta_db,
                        compression_onset_dbmv,
                    } => {
                        entry.gain_delta_db += gain_delta_db * share;
                        entry.tilt_delta_db += tilt_delta_db * share;
                        entry.compression_onset_dbmv = *compression_onset_dbmv;
                    }
                }
            }
            FaultKind::Misalignment {
                active_id,
                pad_delta_db,
                eq_delta_db,
                return_pad_delta_db,
            } => {
                graph.element(active_id)?;
                let entry = self.amps.entry(active_id.clone()).or_default();
                let (pad, eq, ret) = entry.misalignment.unwrap_or((0.0, 0.0, 0.0));
                entry.misalignment = Some((
                    pad + pad_delta_db * share,
                    eq + eq_delta_db * share,
                    ret + return_pad_delta_db * share,
                ));
            }
            FaultKind::ResonantPeak {
                amp_id,
                f0_mhz,
                gain_db,
                width_mhz,
            } => {
                graph.element(amp_id)?;
                self.amps
                    .entry(amp_id.clone())
                    .or_default()
                    .resonance
                    .push(ShapedLoss::Gaussian {
                        f0_mhz: *f0_mhz,
                        depth_db: -gain_db * share,
                        width_mhz: *width_mhz,
                    });
            }
            FaultKind::AdjacentLevelOffset {
                f_lo_mhz,
                f_hi_mhz,
                offset_db,
            } => {
                self.source_offsets
                    .push((*f_lo_mhz, *f_hi_mhz, offset_db * share));
            }
            FaultKind::ForwardOptics { cnr_loss_db } => {
                self.optics_cnr_loss_db += cnr_loss_db * share;
            }
            FaultKind::CommonPathDistortion {
                location,
                second_order_dbmv_per_mhz,
                third_over_second_db,
                schedule,
            } => {
                let multiplier = schedule_multiplier(schedule, sample.t) * share;
                if multiplier > 0.0 {
                    let (lo_mhz, hi_mhz) = return_band(graph.profile());
                    let products = cpd_density_per_mhz(
                        graph.profile(),
                        lo_mhz,
                        hi_mhz,
                        *second_order_dbmv_per_mhz,
                        *third_over_second_db,
                    );
                    for (f_mhz, dbmv_per_mhz) in products {
                        if let Some(ingress) = ingress(
                            f_mhz,
                            CPD_BIN_MHZ,
                            dbmv_per_mhz + db(CPD_BIN_MHZ),
                            multiplier,
                            None,
                        ) {
                            let point = PointImpairment {
                                us_ingress: Some(ingress),
                                ..Default::default()
                            };
                            add_point(self, graph, location, point)?;
                        }
                    }
                }
            }
            FaultKind::UpstreamIngress {
                entry_location,
                f_mhz,
                bw_mhz,
                power_dbmv,
                schedule,
                burst,
            } => {
                let multiplier = schedule_multiplier(schedule, sample.t) * share;
                if let Some(ingress) =
                    ingress(*f_mhz, *bw_mhz, *power_dbmv, multiplier, burst.as_ref())
                {
                    let point = PointImpairment {
                        us_ingress: Some(ingress),
                        ..Default::default()
                    };
                    add_point(self, graph, entry_location, point)?;
                }
            }
            FaultKind::DownstreamIngress {
                entry_location,
                f_mhz,
                bw_mhz,
                power_dbmv,
                schedule,
            } => {
                let multiplier = schedule_multiplier(schedule, sample.t) * share;
                if let Some(ingress) = ingress(*f_mhz, *bw_mhz, *power_dbmv, multiplier, None) {
                    let point = PointImpairment {
                        ds_ingress: Some(ingress),
                        ..Default::default()
                    };
                    add_point(self, graph, entry_location, point)?;
                }
            }
            FaultKind::PowerSupplySag {
                psu_id,
                v_delta,
                schedule,
            } => {
                let change = v_delta * schedule_multiplier(schedule, sample.t) * share;
                *self.psu_v_delta.entry(psu_id.clone()).or_insert(0.0) += change;
            }
            FaultKind::RfmMisalignment {
                rpd_id,
                ds_gain_delta_db,
                tilt_delta_db,
            } => {
                graph.element(rpd_id)?;
                let entry = self.amps.entry(rpd_id.clone()).or_default();
                entry.gain_delta_db += ds_gain_delta_db * share;
                entry.tilt_delta_db += tilt_delta_db * share;
            }
            // Their effect on RF is derived in `crate::rpd`, from time and each other.
            FaultKind::RpdFailure { .. }
            | FaultKind::CinLinkCut { .. }
            | FaultKind::GcpLoss { .. }
            | FaultKind::TimingLoss { .. } => {}
            FaultKind::SpanCut {
                span_id,
                pos_ft,
                end,
                repair_s,
            } => {
                if event_elapsed_s(&fault.onset, sample.t, *repair_s).is_some() {
                    let location = Location::Span {
                        span_id: span_id.clone(),
                        pos_ft: *pos_ft,
                    };
                    add_point(self, graph, &location, cut_point(*end))?;
                    self.ac_open.insert(span_id.clone());
                    if *end == CutEnd::Shorted {
                        self.shorts.push(ShortPoint {
                            span_id: span_id.clone(),
                            pos_ft: *pos_ft,
                            r_ohm: CUT_SHORT_OHM,
                        });
                    }
                }
            }
            FaultKind::AnalogNodeFailure {
                node_id,
                mode,
                repair_s,
            } => {
                graph.element(node_id)?;
                if event_elapsed_s(&fault.onset, sample.t, *repair_s).is_some() {
                    match mode {
                        NodeFailureMode::Power => self.down.insert(node_id.clone()),
                        NodeFailureMode::RfSection => self.rf_off.insert(node_id.clone()),
                    };
                }
            }
            FaultKind::WaterInDevice {
                element_id,
                loss_db_at_1ghz,
                return_loss_db,
                rain_coupling_per_mm_per_hr,
                failed_after_s,
            } => {
                let element = graph.element(element_id)?;
                // §20.3: at the device's input, or, for a node, which is fed by fiber and has none, at each RF output.
                let mut ports: Vec<&schema::plant::Port> = element
                    .ports
                    .iter()
                    .filter(|p| p.role == PortRole::Input)
                    .take(1)
                    .collect();
                if ports.is_empty() {
                    ports = element
                        .ports
                        .iter()
                        .filter(|p| p.role == PortRole::Output)
                        .collect();
                }
                if ports.is_empty() {
                    return Err(PhysicsError::BadComponent(element_id.clone()));
                }
                let failed = failed_after_s.is_some_and(|after| {
                    event_elapsed_s(&fault.onset, sample.t, None).is_some_and(|e| e >= after)
                });
                if failed && is_active_kind(&element.kind) {
                    self.down.insert(element_id.clone());
                }
                for port in ports {
                    let location = Location::Port {
                        element_id: element_id.clone(),
                        port: port.id.clone(),
                    };
                    if failed {
                        add_point(self, graph, &location, cut_point(CutEnd::Open))?;
                    } else {
                        let k = loss_db_at_1ghz
                            * (1.0 + rain_coupling_per_mm_per_hr * sample.rain_mm_per_hr)
                            * share;
                        let point = PointImpairment {
                            loss: Some(sqrt_loss(k)),
                            reflector: Some(ReturnLoss::Flat {
                                rl_db: ramped_rl_db(*return_loss_db, share),
                            }),
                            ..Default::default()
                        };
                        add_point(self, graph, &location, point)?;
                    }
                }
            }
            FaultKind::PowerShort {
                span_id,
                pos_ft,
                fault_resistance_ohm,
                repair_s,
            } => {
                if event_elapsed_s(&fault.onset, sample.t, *repair_s).is_some() {
                    let (loss_db, rl_db) = shunt_db(*fault_resistance_ohm);
                    let point = PointImpairment {
                        loss: Some(ShapedLoss::Flat { loss_db }),
                        reflector: Some(ReturnLoss::Flat { rl_db }),
                        inverted: true,
                        ..Default::default()
                    };
                    let location = Location::Span {
                        span_id: span_id.clone(),
                        pos_ft: *pos_ft,
                    };
                    add_point(self, graph, &location, point)?;
                    self.shorts.push(ShortPoint {
                        span_id: span_id.clone(),
                        pos_ft: *pos_ft,
                        r_ohm: *fault_resistance_ohm,
                    });
                }
            }
            FaultKind::DamagedDevice {
                element_id,
                through_loss_db,
                return_loss_db,
                ports_lost,
                ingress_power_dbmv,
                cpd_second_order_dbmv_per_mhz,
            } => {
                // One blow, its effects together: each is the existing fault at the same point (§20.5), so a
                // damaged device with one effect is that fault.
                let element = graph.element(element_id)?;
                let port = element
                    .ports
                    .iter()
                    .find(|p| p.role == PortRole::Input)
                    .ok_or_else(|| PhysicsError::BadComponent(element_id.clone()))?;
                let location = Location::Port {
                    element_id: element_id.clone(),
                    port: port.id.clone(),
                };
                let point = PointImpairment {
                    loss: (*through_loss_db > 0.0).then(|| ShapedLoss::Flat {
                        loss_db: through_loss_db * share,
                    }),
                    reflector: Some(ReturnLoss::Flat {
                        rl_db: ramped_rl_db(*return_loss_db, share),
                    }),
                    ..Default::default()
                };
                add_point(self, graph, &location, point)?;
                let flat = Schedule {
                    hourly_multipliers: [1.0; 24],
                };
                if let Some(power_dbmv) = ingress_power_dbmv {
                    let (lo_mhz, hi_mhz) = return_band(graph.profile());
                    self.add_fault(
                        graph,
                        &Fault {
                            onset: fault.onset.clone(),
                            kind: FaultKind::UpstreamIngress {
                                entry_location: location.clone(),
                                f_mhz: (lo_mhz + hi_mhz) / 2.0,
                                bw_mhz: hi_mhz - lo_mhz,
                                power_dbmv: *power_dbmv,
                                schedule: flat.clone(),
                                burst: None,
                            },
                        },
                        sample,
                        seed,
                    )?;
                }
                if let Some(second) = cpd_second_order_dbmv_per_mhz {
                    self.add_fault(
                        graph,
                        &Fault {
                            onset: fault.onset.clone(),
                            kind: FaultKind::CommonPathDistortion {
                                location: location.clone(),
                                second_order_dbmv_per_mhz: *second,
                                third_over_second_db: DAMAGE_CPD_THIRD_DB,
                                schedule: flat,
                            },
                        },
                        sample,
                        seed,
                    )?;
                }
                for lost in ports_lost {
                    for edge in graph
                        .child_edges(element_id)
                        .iter()
                        .filter(|e| &e.from.port == lost)
                    {
                        let at = Location::Span {
                            span_id: edge.id.clone(),
                            pos_ft: 0.0,
                        };
                        add_point(self, graph, &at, cut_point(CutEnd::Open))?;
                        self.ac_open.insert(edge.id.clone());
                    }
                }
            }
            FaultKind::UtilityOutage {
                center,
                radius_ft,
                repair_s,
            } => {
                if let Some(elapsed) = event_elapsed_s(&fault.onset, sample.t, *repair_s) {
                    let inside = |geo: Option<schema::common::LonLat>| {
                        geo.is_some_and(|g| dist_ft(*center, g) <= *radius_ft)
                    };
                    for supply in &graph.plant().power_supplies {
                        if inside(supply.geo) {
                            self.supplies_out.insert(supply.id.clone(), elapsed);
                        }
                    }
                    for modem in graph.modems() {
                        if inside(modem.geo) {
                            self.modems_off.insert(modem.id.clone());
                        }
                    }
                }
            }
            FaultKind::BadDrop {
                modem_id,
                extra_loss_db,
                return_loss_db,
            } => {
                let element = graph.element(modem_id)?;
                let port = element
                    .ports
                    .iter()
                    .find(|p| p.role == PortRole::Input)
                    .ok_or_else(|| PhysicsError::BadComponent(modem_id.clone()))?;
                let point = PointImpairment {
                    loss: Some(ShapedLoss::Flat {
                        loss_db: extra_loss_db * share,
                    }),
                    reflector: return_loss_db.map(|rl| ReturnLoss::Flat {
                        rl_db: ramped_rl_db(rl, share),
                    }),
                    ..Default::default()
                };
                self.port_points
                    .entry((modem_id.clone(), port.id.clone()))
                    .or_default()
                    .push(point);
            }
        }
        Ok(())
    }

    /// §8: seeded per-segment impedance variation. Each step is an ordinary mismatch, `Γ ≈ ΔZ/2Z`,
    /// so the non-periodic waves come out of the same cavities as any other echo.
    fn add_wet_steps(
        &mut self,
        graph: &PlantGraph<'_>,
        span_id: &EdgeId,
        pos_ft: f64,
        len_ft: f64,
        variation_pct: f64,
        seed: u64,
    ) -> Result<(), PhysicsError> {
        let edge = graph.edge(span_id)?;
        if matches!(edge.kind, EdgeKind::Direct) {
            return Ok(());
        }
        if variation_pct <= 0.0 {
            return Ok(());
        }
        let steps = ((len_ft / WET_STEP_FT) as usize).clamp(2, WET_STEPS_MAX);
        let mut rng = Rng::derive(seed, &[hash_str(span_id.as_str()), pos_ft.to_bits()]);
        let points = self.span_points.entry(span_id.clone()).or_default();
        for _ in 0..steps {
            let at_ft = pos_ft + rng.uniform() * len_ft;
            let gamma = (variation_pct / 100.0 / 2.0) * rng.range(0.5, 1.5);
            points.push((
                at_ft,
                PointImpairment {
                    reflector: Some(ReturnLoss::Flat {
                        rl_db: -20.0 * gamma.log10(),
                    }),
                    ..Default::default()
                },
            ));
        }
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(())
    }

    /// §8: domain randomization. Jitters each part by its tolerance, per seed: ±0.5 dB on losses, ±1
    /// dB on amp gain, ±10 % on loop resistance, and the same ±0.5 dB over a span's loss as a share.
    pub fn randomize(&mut self, graph: &PlantGraph<'_>, seed: u64) {
        // §8: a part's stated tolerance, where it states one. Each draw below the version 1 ones has a stream of its own by element
        // and slot, so a part that states none draws nothing and moves nothing else.
        let tolerance = |id: &schema::common::ComponentId| {
            graph.component(id).and_then(|c| c.tolerance.as_ref())
        };
        for element in &graph.plant().elements {
            let mut rng = Rng::derive(seed, &[hash_str(element.id.as_str()), 1]);
            let stated = tolerance(&element.component_ref);
            match element.kind {
                schema::plant::ElementKind::Amp { .. } => {
                    let range_db = stated.and_then(|t| t.gain_db).unwrap_or(1.0);
                    self.amps
                        .entry(element.id.clone())
                        .or_default()
                        .gain_delta_db += rng.range(-range_db, range_db);
                }
                schema::plant::ElementKind::Tap
                | schema::plant::ElementKind::Splitter
                | schema::plant::ElementKind::DirectionalCoupler => {
                    let range_db = stated.and_then(|t| t.loss_db).unwrap_or(0.5);
                    self.element_offset_db
                        .insert(element.id.clone(), rng.range(-range_db, range_db));
                }
                schema::plant::ElementKind::Modem => {
                    let nf = stated.and_then(|t| t.nf_db);
                    let ceiling = stated.and_then(|t| t.impl_mer_db);
                    if nf.is_some() || ceiling.is_some() {
                        let id = hash_str(element.id.as_str());
                        let draw = |range: Option<f64>, slot: u64| {
                            range.map_or(0.0, |r| Rng::derive(seed, &[id, 4, slot]).range(-r, r))
                        };
                        self.modem_draws
                            .insert(element.id.clone(), (draw(nf, 0), draw(ceiling, 1)));
                    }
                }
                _ => {}
            }
            if let Some(schema::plant::Fitted::Active(fitted)) = &element.fitted {
                let id = hash_str(element.id.as_str());
                let draw = |part: &Option<schema::common::ComponentId>, slot: u64| {
                    part.as_ref()
                        .and_then(tolerance)
                        .and_then(|t| t.loss_db)
                        .map_or(0.0, |r| Rng::derive(seed, &[id, 5, slot]).range(-r, r))
                };
                let mut draws = crate::overlay::PlugInDraws {
                    input_pad_db: draw(&fitted.input_pad, 0),
                    input_eq_db: draw(&fitted.input_eq, 1),
                    return_pad_db: draw(&fitted.return_pad, 2),
                    return_eq_db: draw(&fitted.return_eq, 3),
                    outputs: std::collections::BTreeMap::new(),
                };
                for (k, output) in fitted.outputs.iter().enumerate() {
                    let k = k as u64;
                    let pair = (draw(&output.pad, 10 + 2 * k), draw(&output.eq, 11 + 2 * k));
                    if pair != (0.0, 0.0) {
                        draws.outputs.insert(output.port.clone(), pair);
                    }
                }
                if draws != crate::overlay::PlugInDraws::default() {
                    self.plugin_draws.insert(element.id.clone(), draws);
                }
            }
        }
        for edge in &graph.plant().edges {
            let mut rng = Rng::derive(seed, &[hash_str(edge.id.as_str()), 2]);
            self.edge_loss_factor
                .insert(edge.id.clone(), 1.0 + rng.range(-0.03, 0.03));
            for (k, fitting) in edge.kind.fittings().iter().enumerate() {
                if let Some(range) =
                    tolerance(&fitting.component_ref).and_then(|t| t.return_loss_db)
                {
                    let draw = Rng::derive(seed, &[hash_str(edge.id.as_str()), 6, k as u64])
                        .range(-range, range);
                    self.fitting_rl_draws.insert((edge.id.clone(), k), draw);
                }
            }
        }
        let mut rng = Rng::derive(seed, &[3]);
        self.r_loop_factor = 1.0 + rng.range(-0.1, 0.1);
    }
}

/// Whether an element is an active: it draws power and stops without it.
fn is_active_kind(kind: &schema::plant::ElementKind) -> bool {
    matches!(
        kind,
        schema::plant::ElementKind::Node { .. }
            | schema::plant::ElementKind::Rpd { .. }
            | schema::plant::ElementKind::Amp { .. }
    )
}

/// Feet between two points, flat-earth over a neighbourhood: a degree of latitude is 364,000 ft, of longitude that by the
/// cosine of the latitude (approx, as `design::geo`).
pub fn dist_ft(a: schema::common::LonLat, b: schema::common::LonLat) -> f64 {
    const FT_PER_DEG_LAT: f64 = 364_000.0;
    let lat = ((a.lat_deg + b.lat_deg) / 2.0).to_radians();
    let dx = (a.lon_deg - b.lon_deg) * FT_PER_DEG_LAT * lat.cos();
    let dy = (a.lat_deg - b.lat_deg) * FT_PER_DEG_LAT;
    (dx * dx + dy * dy).sqrt()
}
