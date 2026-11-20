//! What a fault set does to the plant (§8), in the form the walk reads. No fault touches telemetry
//! directly: each becomes loss, phase, a mismatch or injected noise at a place in the tree.

use std::collections::{BTreeMap, BTreeSet};

use schema::common::{EdgeId, ElementId, PortId};
use schema::fault::LossShape;

/// Width of a frequency-selective loss's edge, MHz (approx). The guide shows sharp edges.
const FSL_EDGE_MHZ: f64 = 2.0;
/// Where a loss that runs to the top of the spectrum is tapered off, MHz. Far above any band, so it
/// only closes the integral for the minimum-phase term.
const FSL_TOP_MHZ: f64 = 4000.0;
/// dB per neper.
const DB_PER_NEPER: f64 = 8.685_889_638;

/// Dawson's integral, by Rybicki's method.
pub fn dawson(x: f64) -> f64 {
    const H: f64 = 0.4;
    const A1: f64 = 2.0 / 3.0;
    const A2: f64 = 0.4;
    const A3: f64 = 2.0 / 7.0;
    if x.abs() < 0.2 {
        let x2 = x * x;
        return x * (1.0 - A1 * x2 * (1.0 - A2 * x2 * (1.0 - A3 * x2)));
    }
    let xx = x.abs();
    let n0 = 2.0 * (0.5 * xx / H + 0.5).floor();
    let xp = xx - n0 * H;
    let mut e1 = (2.0 * xp * H).exp();
    let e2 = e1 * e1;
    let mut d1 = n0 + 1.0;
    let mut d2 = d1 - 2.0;
    let mut sum = 0.0;
    for i in 0..6 {
        let c = (-((2.0 * i as f64 + 1.0) * H).powi(2)).exp();
        sum += c * (e1 / d1 + 1.0 / (d2 * e1));
        d1 += 2.0;
        d2 -= 2.0;
        e1 *= e2;
    }
    0.564_189_583_5 * (-xp * xp).exp().copysign(x) * sum
}

/// A loss with a shape in frequency, and the minimum-phase phase that goes with it (§8).
#[derive(Debug, Clone, PartialEq)]
pub enum ShapedLoss {
    /// The same at every frequency, with no phase.
    Flat {
        /// Loss, dB.
        loss_db: f64,
    },
    /// §8's suckout: `depth·exp(−((f−f0)/width)²)`.
    Gaussian {
        /// Centre, MHz.
        f0_mhz: f64,
        /// Depth at the centre, dB.
        depth_db: f64,
        /// Where the dB depth falls to 1/e, MHz.
        width_mhz: f64,
    },
    /// Straight lines in dB between breakpoints, zero outside them.
    Piecewise {
        /// `(f_mhz, loss_db)`, ascending.
        points: Vec<(f64, f64)>,
    },
}

impl ShapedLoss {
    /// §8: the frequency-selective shapes as piecewise loss.
    pub fn selective(shape: LossShape, f_lo_mhz: f64, f_hi_mhz: f64, depth_db: f64) -> ShapedLoss {
        Self::skirted(shape, f_lo_mhz, f_hi_mhz, depth_db, FSL_EDGE_MHZ)
    }

    /// §8: the same, with loss ramping in over `e` MHz.
    pub fn skirted(
        shape: LossShape,
        f_lo_mhz: f64,
        f_hi_mhz: f64,
        depth_db: f64,
        e: f64,
    ) -> ShapedLoss {
        let points = match shape {
            LossShape::BandStop => vec![
                (f_lo_mhz - e, 0.0),
                (f_lo_mhz, depth_db),
                (f_hi_mhz, depth_db),
                (f_hi_mhz + e, 0.0),
            ],
            // Stops below the band, so the loss holds down to DC, where the mirror image takes over.
            LossShape::HighPass => vec![(0.0, depth_db), (f_lo_mhz, depth_db), (f_lo_mhz + e, 0.0)],
            LossShape::LowPass => vec![
                (f_hi_mhz - e, 0.0),
                (f_hi_mhz, depth_db),
                (FSL_TOP_MHZ, depth_db),
                (FSL_TOP_MHZ + e, 0.0),
            ],
        };
        ShapedLoss::Piecewise { points }
    }

    /// Loss at `f_mhz`, dB.
    pub fn loss_db(&self, f_mhz: f64) -> f64 {
        match self {
            ShapedLoss::Flat { loss_db } => *loss_db,
            ShapedLoss::Gaussian {
                f0_mhz,
                depth_db,
                width_mhz,
            } => depth_db * (-((f_mhz - f0_mhz) / width_mhz).powi(2)).exp(),
            ShapedLoss::Piecewise { points } => {
                let (Some(first), Some(last)) = (points.first(), points.last()) else {
                    return 0.0;
                };
                if f_mhz <= first.0 {
                    // A shape starting at DC holds to DC; any other is zero below its start.
                    return if first.0 == 0.0 { first.1 } else { 0.0 };
                }
                if f_mhz >= last.0 {
                    return last.1;
                }
                for pair in points.windows(2) {
                    if f_mhz <= pair[1].0 {
                        let share = (f_mhz - pair[0].0) / (pair[1].0 - pair[0].0);
                        return pair[0].1 + share * (pair[1].1 - pair[0].1);
                    }
                }
                0.0
            }
        }
    }

    /// The minimum-phase phase at `f_mhz`, radians: the Hilbert transform of the loss in nepers.
    /// That's what gives a notch its group delay (§8): positive on its skirts and at a stop band's
    /// edges, negative at its centre, as any minimum-phase notch has. A flat loss has none.
    pub fn phase_rad(&self, f_mhz: f64) -> f64 {
        match self {
            ShapedLoss::Flat { .. } => 0.0,
            ShapedLoss::Gaussian {
                f0_mhz,
                depth_db,
                width_mhz,
            } => {
                // H[exp(−x²)] = (2/√π)·D(x). The width is in dB, which is linear in nepers.
                let x = (f_mhz - f0_mhz) / width_mhz;
                (depth_db / DB_PER_NEPER) * std::f64::consts::FRAC_2_SQRT_PI * dawson(x)
            }
            ShapedLoss::Piecewise { points } => {
                // H of piecewise-linear α is (1/π)·Σ Δs_i·(f−b_i)·ln|f−b_i| over its slope changes,
                // with the negative-frequency mirror included so the response is real.
                let mut beta = 0.0;
                let mut prev_slope = 0.0;
                let n = points.len();
                let starts_at_dc = points.first().is_some_and(|p| p.0 == 0.0);
                for i in 0..n {
                    let slope = if i + 1 < n {
                        (points[i + 1].1 - points[i].1) / (points[i + 1].0 - points[i].0)
                    } else {
                        0.0
                    };
                    // A shape from DC mirrors to a flat top across zero, so no kink there.
                    let kink = if i == 0 && starts_at_dc {
                        0.0
                    } else {
                        slope - prev_slope
                    };
                    let b = points[i].0;
                    beta += kink * xlnx(f_mhz - b);
                    // The mirror is even in frequency, so its kink at −b has the same sign.
                    beta += kink * xlnx(f_mhz + b);
                    prev_slope = slope;
                }
                (beta / std::f64::consts::PI) / DB_PER_NEPER
            }
        }
    }
}

fn xlnx(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { x * x.abs().ln() }
}

/// A mismatch's return loss (§5.1).
#[derive(Debug, Clone, PartialEq)]
pub enum ReturnLoss {
    /// The same at every frequency.
    Flat {
        /// Return loss, dB.
        rl_db: f64,
    },
    /// §8: low return loss only where a selective loss stops the band. §13: a filter's, the same way.
    StopBand {
        /// Return loss outside the stop band, dB: `PASS_RL_DB` for a fault, a filter's own otherwise.
        pass_rl_db: f64,
        /// The stop band's return loss, dB.
        stop_rl_db: f64,
        /// The loss shape that says where the stop band is.
        shape: ShapedLoss,
        /// Depth at which the band counts as fully stopped, dB.
        depth_db: f64,
    },
    /// §14: a part's table, `(f_mhz, rl_db)` ascending, interpolated in dB and held flat past its ends.
    Table {
        /// The points.
        points: Vec<(f64, f64)>,
    },
    /// §14: a reflector behind a passive's loss to it, seen from the passive's input: its own return loss plus that loss
    /// twice over.
    Behind {
        /// The reflector itself.
        rl: Box<ReturnLoss>,
        /// The passive's loss to it, `(f_mhz, loss_db)` ascending, held flat past its ends.
        loss: Vec<(f64, f64)>,
    },
}

/// Linear in dB between points, flat past the ends; 0 with no points.
pub(crate) fn table_at(points: &[(f64, f64)], f_mhz: f64) -> f64 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return 0.0;
    };
    if f_mhz <= first.0 {
        return first.1;
    }
    if f_mhz >= last.0 {
        return last.1;
    }
    for pair in points.windows(2) {
        if f_mhz <= pair[1].0 {
            let share = (f_mhz - pair[0].0) / (pair[1].0 - pair[0].0);
            return pair[0].1 + share * (pair[1].1 - pair[0].1);
        }
    }
    last.1
}

/// Return loss outside any stop band: a transparent point, dB.
pub const PASS_RL_DB: f64 = 60.0;

impl ReturnLoss {
    /// Return loss at `f_mhz`, dB.
    pub fn at(&self, f_mhz: f64) -> f64 {
        match self {
            ReturnLoss::Flat { rl_db } => *rl_db,
            ReturnLoss::StopBand {
                pass_rl_db,
                stop_rl_db,
                shape,
                depth_db,
            } => {
                let share = if *depth_db > 0.0 {
                    (shape.loss_db(f_mhz) / depth_db).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                pass_rl_db + share * (stop_rl_db - pass_rl_db)
            }
            ReturnLoss::Table { points } => table_at(points, f_mhz),
            ReturnLoss::Behind { rl, loss } => rl.at(f_mhz) + 2.0 * table_at(loss, f_mhz),
        }
    }

    /// The strongest it gets anywhere, dB, for deciding whether an echo is worth computing.
    pub fn lowest_db(&self) -> f64 {
        match self {
            ReturnLoss::Flat { rl_db } => *rl_db,
            ReturnLoss::StopBand {
                pass_rl_db,
                stop_rl_db,
                ..
            } => stop_rl_db.min(*pass_rl_db),
            ReturnLoss::Table { points } => {
                points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min)
            }
            ReturnLoss::Behind { rl, loss } => {
                let least = loss.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
                rl.lowest_db() + 2.0 * if least.is_finite() { least } else { 0.0 }
            }
        }
    }
}

/// Ingress at a point, in the band `f_mhz ± bw_mhz/2`.
#[derive(Debug, Clone, PartialEq)]
pub struct Ingress {
    /// Centre, MHz.
    pub f_mhz: f64,
    /// Bandwidth, MHz.
    pub bw_mhz: f64,
    /// Time-average total power in the band at this time step, dBmV.
    pub power_dbmv: f64,
    /// Peak-to-average in dB: how far a burst sits above its time average. 0 for continuous.
    pub peak_over_avg_db: f64,
}

impl Ingress {
    /// Whether `f_mhz` falls in the band.
    pub fn covers(&self, f_mhz: f64) -> bool {
        (f_mhz - self.f_mhz).abs() <= self.bw_mhz / 2.0
    }
}

/// Something at a point in the plant.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PointImpairment {
    /// Added loss, with its phase.
    pub loss: Option<ShapedLoss>,
    /// Added mismatch.
    pub reflector: Option<ReturnLoss>,
    /// Noise entering the forward path here.
    pub ds_ingress: Option<Ingress>,
    /// Noise entering the return path here.
    pub us_ingress: Option<Ingress>,
    /// §18: ambient sources enter the return path here, this far below their level, where a defect has opened the shield.
    pub ambient_coupling_db: Option<f64>,
    /// §20.1: the mismatch reflects inverted, as a short does (Γ near −1) where an open reflects in phase.
    pub inverted: bool,
}

/// §20.4: a low-resistance path from center conductor to shield.
#[derive(Debug, Clone, PartialEq)]
pub struct ShortPoint {
    /// The span.
    pub span_id: EdgeId,
    /// Where, feet from its upstream end.
    pub pos_ft: f64,
    /// The path's resistance, ohms.
    pub r_ohm: f64,
}

/// A wet length of hardline (§8). Its seeded impedance steps are ordinary point reflectors.
#[derive(Debug, Clone, PartialEq)]
pub struct WetSection {
    /// Start, feet from the span's upstream end.
    pub pos_ft: f64,
    /// Length, feet.
    pub len_ft: f64,
    /// `k_w,eff`, dB/100 ft at 1 GHz, after rain and onset.
    pub k_w_eff_db_per_100ft: f64,
}

/// §8: what happens to an amp.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AmpOverride {
    /// Output change at the band's top, dB.
    pub gain_delta_db: f64,
    /// Added tilt across the band, dB.
    pub tilt_delta_db: f64,
    /// Output level above which distortion worsens, dBmV.
    pub compression_onset_dbmv: Option<f64>,
    /// §8: frequency-specific gain at the output, as a negative loss.
    pub resonance: Vec<ShapedLoss>,
    /// No output, in either direction.
    pub failed: bool,
    /// §15, §8: a wrong plug-in, as dB more than fitted: forward input pad, forward input equalizer, return pad.
    pub misalignment: Option<(f64, f64, f64)>,
}

/// A fault set at one time step, in the walk's terms. Empty is a healthy plant.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    /// Points on spans and drops, by edge, as `(pos_ft, impairment)`.
    pub span_points: BTreeMap<EdgeId, Vec<(f64, PointImpairment)>>,
    /// Points at a device's port.
    pub port_points: BTreeMap<(ElementId, PortId), Vec<PointImpairment>>,
    /// Wet sections, by edge.
    pub wet: BTreeMap<EdgeId, Vec<WetSection>>,
    /// Amp faults and brownouts, by element.
    pub amps: BTreeMap<ElementId, AmpOverride>,
    /// Actives with no power, in either direction (§6.3).
    pub down: BTreeSet<ElementId>,
    /// Actives powered and alive with no RF: an RPD whose fiber is cut, or held off by its Core.
    pub rf_off: BTreeSet<ElementId>,
    /// Supply voltage changes, by supply, volts.
    pub psu_v_delta: BTreeMap<ElementId, f64>,
    /// §8: blocks whose level at the node's output is off, as `(f_lo_mhz, f_hi_mhz, offset_db)`.
    pub source_offsets: Vec<(f64, f64, f64)>,
    /// §8: fall in the node's `CNR_opt`, dB.
    pub optics_cnr_loss_db: f64,
    /// §16: an analog node's forward link CNR from `sim::optics`, in place of its part's constant `CNR_opt`. `None`
    /// keeps the constant.
    pub optics_cnr_db: Option<f64>,
    /// §17: the source MER table of the CMTS behind an analog node, from its link state. Empty keeps PHY Table 41's floors.
    pub cmts_source_mer: Vec<schema::component::MerPoint>,
    /// §18: the environment's ambient sources at this step, at zero coupling, each its band's power after its schedule.
    pub ambient: Vec<Ingress>,
    /// §8: per-element loss offset, dB, from domain randomization. Real parts, so alignment saw it.
    pub element_offset_db: BTreeMap<ElementId, f64>,
    /// §8: per-edge cable loss factor, from domain randomization.
    pub edge_loss_factor: BTreeMap<EdgeId, f64>,
    /// §8: global loop resistance factor, from domain randomization.
    pub r_loop_factor: f64,
    /// §8: each modem's draws within its part's tolerance, as (noise figure, implementation ceiling) offsets, dB.
    pub modem_draws: BTreeMap<ElementId, (f64, f64)>,
    /// §8: each fitted active's plug-in values, drawn within their parts' tolerance.
    pub plugin_draws: BTreeMap<ElementId, PlugInDraws>,
    /// §8: a fitting's return loss offset, drawn within its part's tolerance, dB, by edge and the fitting's index on it.
    pub fitting_rl_draws: BTreeMap<(EdgeId, usize), f64>,
    /// §20.1: spans cut through, which carry no AC past the cut.
    pub ac_open: BTreeSet<EdgeId>,
    /// §20.4: shorts across the hardline, which draw current from the supply that reaches them.
    pub shorts: Vec<ShortPoint>,
    /// §20.6: supplies whose AC input has failed, with how long ago, seconds; their batteries carry them for a while.
    pub supplies_out: BTreeMap<ElementId, f64>,
    /// §20.6: modems with no house power.
    pub modems_off: BTreeSet<ElementId>,
}

/// §8: how far each of an active's fitted plug-ins sits from its stated value, dB.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlugInDraws {
    /// Forward input pad.
    pub input_pad_db: f64,
    /// Forward input equalizer.
    pub input_eq_db: f64,
    /// Return pad.
    pub return_pad_db: f64,
    /// Return equalizer.
    pub return_eq_db: f64,
    /// Each output's pad and equalizer, by port.
    pub outputs: BTreeMap<PortId, (f64, f64)>,
}

impl Default for Overlay {
    fn default() -> Self {
        Overlay {
            span_points: BTreeMap::new(),
            port_points: BTreeMap::new(),
            wet: BTreeMap::new(),
            amps: BTreeMap::new(),
            down: BTreeSet::new(),
            rf_off: BTreeSet::new(),
            psu_v_delta: BTreeMap::new(),
            source_offsets: Vec::new(),
            optics_cnr_loss_db: 0.0,
            optics_cnr_db: None,
            cmts_source_mer: Vec::new(),
            ambient: Vec::new(),
            element_offset_db: BTreeMap::new(),
            edge_loss_factor: BTreeMap::new(),
            r_loop_factor: 1.0,
            modem_draws: BTreeMap::new(),
            plugin_draws: BTreeMap::new(),
            fitting_rl_draws: BTreeMap::new(),
            ac_open: BTreeSet::new(),
            shorts: Vec::new(),
            supplies_out: BTreeMap::new(),
            modems_off: BTreeSet::new(),
        }
    }
}
