//! Fault parameterizations, one variant per row of `docs/physics.md` §8. A fault is
//! physical parameters that modify the plant; its telemetry signature is always computed
//! by the sim (invariant 5).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{EdgeId, ElementId, LonLat, PortId, Timestamp};

/// Version of this schema.
pub const SCHEMA_VERSION: &str = "5";

/// The faults applied to one plant, and the seed behind their random parts.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FaultSet {
    /// Version of this schema.
    pub schema_version: String,
    /// Seed for every random draw the faults need (invariant 4).
    pub seed: u64,
    /// The faults; the sim applies them all (§8, compound faults).
    pub faults: Vec<Fault>,
}

/// One fault.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Fault {
    /// When the fault appears.
    pub onset: Onset,
    /// The fault class and its parameters.
    pub kind: FaultKind,
}

/// How a fault appears over time.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum Onset {
    /// Present at full severity from the start of the run.
    Instant,
    /// Severity rises linearly from zero to full.
    Ramp {
        /// Simulated time severity starts rising, seconds.
        start: Timestamp,
        /// Simulated time severity reaches full, seconds.
        end: Timestamp,
    },
}

/// A point in the plant (§8).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum Location {
    /// A point along a cabled edge, either a span or a drop.
    Span {
        /// The edge.
        span_id: EdgeId,
        /// Distance from the edge's upstream end, feet.
        pos_ft: f64,
    },
    /// A device port.
    Port {
        /// The device.
        element_id: ElementId,
        /// The port on it.
        port: PortId,
    },
}

/// A time-of-day profile on a fault's severity (§8).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    /// Severity multiplier per hour, dimensionless, with hour = (t / 3600) mod 24.
    pub hourly_multipliers: [f64; 24],
}

/// The fault classes of §8.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum FaultKind {
    /// Water in a span: added loss and non-periodic impedance variation.
    WaterIngress {
        /// The span.
        span_id: EdgeId,
        /// Where the affected length starts, feet from the span's upstream end.
        pos_ft: f64,
        /// Affected length, feet.
        affected_len_ft: f64,
        /// `k_w`: added loss at 1 GHz, dB per 100 ft.
        k_w_db_per_100ft: f64,
        /// Rise in `k_w` per unit of rainfall, per mm per hour.
        rain_coupling_per_mm_per_hr: f64,
        /// Size of the seeded impedance variation, percent.
        impedance_variation_pct: f64,
    },
    /// A mismatch at a connector; echoes follow from §5.1.
    ConnectorReflection {
        /// Where the mismatch is.
        location: Location,
        /// Return loss of the mismatch, dB, at 20 °C and calm.
        return_loss_db: f64,
        /// How the mismatch moves with the environment, and whether it lets ambient in (§18, §19). Version 4.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        couplings: Option<ConnectorCouplings>,
    },
    /// A narrow minimum-phase loss.
    Suckout {
        /// Where the loss is.
        location: Location,
        /// Centre frequency, MHz.
        f0_mhz: f64,
        /// Loss at the centre, dB.
        depth_db: f64,
        /// Offset from centre at which the dB depth falls to 1/e, MHz.
        width_mhz: f64,
    },
    /// Shaped minimum-phase loss: traps, filters, pullout and under-rated parts.
    FrequencySelectiveLoss {
        /// Where the loss is.
        location: Location,
        /// The shape of the loss.
        shape: LossShape,
        /// Lower edge, MHz.
        f_lo_mhz: f64,
        /// Upper edge, MHz.
        f_hi_mhz: f64,
        /// Loss in the stop band, dB.
        depth_db: f64,
        /// Return loss in the stop band, dB.
        stop_return_loss_db: f64,
        /// Width over which the loss ramps in, MHz, or `None` for a sharp edge.
        #[serde(default)]
        skirt_mhz: Option<f64>,
    },
    /// An amp drifting out of alignment or failing.
    AmpDrift {
        /// The amp.
        amp_id: ElementId,
        /// What has happened to it.
        state: AmpState,
    },
    /// Noise injected into the return path; funnels per §7.3.
    UpstreamIngress {
        /// Where the noise enters.
        entry_location: Location,
        /// Centre frequency, MHz.
        f_mhz: f64,
        /// Bandwidth, MHz.
        bw_mhz: f64,
        /// Total power in `bw_mhz`, dBmV.
        power_dbmv: f64,
        /// Time-of-day profile.
        schedule: Schedule,
        /// Burst structure, or `None` for continuous noise.
        burst: Option<Burst>,
    },
    /// Noise injected into the forward path (§4.5).
    DownstreamIngress {
        /// Where the noise enters.
        entry_location: Location,
        /// Centre frequency, MHz.
        f_mhz: f64,
        /// Bandwidth, MHz.
        bw_mhz: f64,
        /// Total power in `bw_mhz`, dBmV.
        power_dbmv: f64,
        /// Time-of-day profile.
        schedule: Schedule,
    },
    /// A power supply running low; brownouts follow per §6.3.
    PowerSupplySag {
        /// The power supply.
        psu_id: ElementId,
        /// Fall in supply voltage, volts.
        v_delta: f64,
        /// Time-of-day profile.
        schedule: Schedule,
    },
    /// Loss on one drop only.
    BadDrop {
        /// The modem at the end of the drop.
        modem_id: ElementId,
        /// Added loss, dB.
        extra_loss_db: f64,
        /// Return loss of an added mismatch, dB, or `None` for no mismatch.
        return_loss_db: Option<f64>,
    },
    /// Frequency-specific gain from feedback inside an amp [PNM-HFC §7.1.5].
    ResonantPeak {
        /// The amp.
        amp_id: ElementId,
        /// Centre frequency, MHz.
        f0_mhz: f64,
        /// Rise at the centre, dB.
        gain_db: f64,
        /// Offset from centre at which the dB rise falls to 1/e, MHz.
        width_mhz: f64,
    },
    /// A block of channels combined at the wrong level at the head end or node [PNM-HFC §7.1.1].
    AdjacentLevelOffset {
        /// Lower edge, MHz.
        f_lo_mhz: f64,
        /// Upper edge, MHz.
        f_hi_mhz: f64,
        /// Level change, dB.
        offset_db: f64,
    },
    /// A worse forward optical link: laser, connector or clipping, seen as noise (§8).
    ForwardOptics {
        /// Fall in `CNR_opt`, dB.
        cnr_loss_db: f64,
    },
    /// A corroded contact mixing the forward signals into the return path [PNM-HFC §8.16.1.1].
    CommonPathDistortion {
        /// Where the contact is.
        location: Location,
        /// Second-order product density at its largest, dBmV per MHz.
        second_order_dbmv_per_mhz: f64,
        /// Third-order density at its largest against the second order's, dB.
        third_over_second_db: f64,
        /// Time-of-day profile.
        schedule: Schedule,
    },
    /// An RPD stopped: power, crash or reset [R-PHY §6.3, §8].
    RpdFailure {
        /// The RPD.
        rpd_id: ElementId,
        /// How it failed.
        mode: RpdFailureMode,
    },
    /// The fiber to an RPD is cut. It stays alive and powered, and its RF stops.
    CinLinkCut {
        /// The RPD.
        rpd_id: ElementId,
        /// Seconds until it's repaired, or `None` if it isn't.
        repair_s: Option<f64>,
    },
    /// The control plane to an RPD is lost. No RF effect until retries run out [R-PHY §7.1.3].
    GcpLoss {
        /// The RPD.
        rpd_id: ElementId,
        /// How long it lasts, seconds.
        duration_s: f64,
    },
    /// An RPD loses its PTP sync and runs on holdover [R-PHY §6.9].
    TimingLoss {
        /// The RPD.
        rpd_id: ElementId,
        /// How long it lasts, seconds.
        duration_s: f64,
    },
    /// A wrong pad or equalizer at a fitted active, as its difference from what's fitted (§15): more pad is more loss, more
    /// equalizer more loss at the bottom of its slope. Version 4.
    Misalignment {
        /// The active.
        active_id: ElementId,
        /// Forward input pad, dB more than fitted.
        pad_delta_db: f64,
        /// Forward input equalizer, dB more than fitted.
        eq_delta_db: f64,
        /// Return pad, dB more than fitted.
        return_pad_delta_db: f64,
    },
    /// An RPD's RF output is set wrong, as can happen at cutover.
    RfmMisalignment {
        /// The RPD.
        rpd_id: ElementId,
        /// Output change at the band's top, dB.
        ds_gain_delta_db: f64,
        /// Added tilt across the band, dB.
        tilt_delta_db: f64,
    },
    /// §20.1: a span or drop cut through at a point. Nothing passes it, RF or AC, and the live side ends in an open or a
    /// short. Version 5.
    SpanCut {
        /// The span or drop.
        span_id: EdgeId,
        /// Where, feet from its upstream end.
        pos_ft: f64,
        /// How the live side ends.
        end: CutEnd,
        /// Seconds from the onset until it's spliced, or `None` while it stays cut.
        repair_s: Option<f64>,
    },
    /// §20.2: an analog node stops. Version 5.
    AnalogNodeFailure {
        /// The node.
        node_id: ElementId,
        /// What stops.
        mode: NodeFailureMode,
        /// Seconds from the onset until it's repaired, or `None` while it stays down.
        repair_s: Option<f64>,
    },
    /// §20.3: water inside a device: loss and a mismatch at its input, and, after a while, the device fails. Version 5.
    WaterInDevice {
        /// The device.
        element_id: ElementId,
        /// Added loss at 1 GHz, dB, at full severity and no rain; it goes as √f.
        loss_db_at_1ghz: f64,
        /// Return loss at its input, dB.
        return_loss_db: f64,
        /// Rise in the loss per unit of rainfall, per mm per hour, as §8's water in a span.
        rain_coupling_per_mm_per_hr: f64,
        /// Seconds from the onset until it fails outright, or `None` if it doesn't.
        failed_after_s: Option<f64>,
    },
    /// §20.4: a low-resistance path from center conductor to shield on a span. Version 5.
    PowerShort {
        /// The span.
        span_id: EdgeId,
        /// Where, feet from its upstream end.
        pos_ft: f64,
        /// The path's resistance, ohms.
        fault_resistance_ohm: f64,
        /// Seconds from the onset until it's repaired, or `None`.
        repair_s: Option<f64>,
    },
    /// §20.5: one blow to a device, as a car into a tap: its through path loses, its input mismatches, some of its ports
    /// are lost, and its opened housing lets ingress and corrosion's CPD in, all at once. Version 5.
    DamagedDevice {
        /// The device.
        element_id: ElementId,
        /// Added through loss, dB.
        through_loss_db: f64,
        /// Return loss at its input, dB.
        return_loss_db: f64,
        /// Ports cut off, each as an open.
        ports_lost: Vec<PortId>,
        /// Upstream ingress entering there, total in the return band, dBmV; `None` for none.
        ingress_power_dbmv: Option<f64>,
        /// Second-order CPD density entering there, dBmV per MHz; `None` for none.
        cpd_second_order_dbmv_per_mhz: Option<f64>,
    },
    /// §20.6: the utility fails over an area. Supplies inside it run on their batteries for as long as §20.6 says, then
    /// stop; homes inside it lose house power, and their modems with it. Version 5.
    UtilityOutage {
        /// The area's centre.
        center: LonLat,
        /// Its radius, feet.
        radius_ft: f64,
        /// Seconds from the onset until power is back, or `None`.
        repair_s: Option<f64>,
    },
}

/// How a cut cable's live side ends (§20.1). Version 5.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutEnd {
    /// The conductors parted: an open, reflecting in phase.
    Open,
    /// Center conductor against the shield: a short, reflecting inverted, and a path for the AC (§20.4).
    Shorted,
}

/// What stops when an analog node fails (§20.2). Version 5.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeFailureMode {
    /// It loses power: no RF either way, and nothing it powers runs.
    Power,
    /// Its RF section fails: powered, and no forward out and no return in.
    RfSection,
}

/// A connector fault's couplings to the environment (§18, §19). Each absent is none. Version 4.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct ConnectorCouplings {
    /// Change in return loss per °C above 20 °C at the connector, dB: positive for a joint that loosens as it cools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temp_db_per_c: Option<f64>,
    /// Change in return loss per m/s of wind, dB: negative for a joint that moves when the cable does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_db_per_m_per_s: Option<f64>,
    /// Coupling of ambient sources in at this point, dB, where the defect has opened the shield (§18).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ambient_coupling_db: Option<f64>,
}

/// The shape of a frequency-selective loss.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossShape {
    /// Loss between the edges.
    BandStop,
    /// Loss below the lower edge.
    HighPass,
    /// Loss above the upper edge.
    LowPass,
}

/// The state of a drifting or failed amp.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum AmpState {
    /// Output and, above an onset level, distortion have moved (§4.4).
    Drift {
        /// Change in gain, dB.
        gain_delta_db: f64,
        /// Change in tilt, dB.
        tilt_delta_db: f64,
        /// Output level above which CIN degrades further, dBmV per 6 MHz at the band's
        /// top frequency, or `None` for no compression.
        compression_onset_dbmv: Option<f64>,
    },
    /// No output. An explicit state, because JSON cannot carry −∞.
    Failed,
}

/// The time structure of bursty ingress.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Burst {
    /// Length of one burst, milliseconds.
    pub duration_ms: f64,
    /// Bursts per second.
    pub rate_per_s: f64,
}

/// How an RPD failed.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
pub enum RpdFailureMode {
    /// Hard reset: full init from local boot.
    HardReset {
        /// Seconds to come back.
        recovery_s: f64,
    },
    /// Soft reset: it keeps its frequency and warm-starts PTP.
    SoftReset {
        /// Seconds to come back.
        recovery_s: f64,
    },
    /// Doesn't come back.
    Dead,
}

/// Whether a model trained on `version` can read this schema's faults. Versions 3 to 5 added faults and an optional
/// field a version 2 model never saw and changed none it did. The sim moved under it too, so scores it reports are stale until it's
/// retrained, but what it reads still means the same.
pub fn model_can_read(version: &str) -> bool {
    matches!(version, "2" | "3" | "4" | "5")
}

/// Brings a fault set from an earlier version to this one. Versions 2 to 5 only added variants and optional fields that
/// default to what the sim did before (version 4: a connector's couplings; version 5: task 19's damage events), so earlier
/// data means the same.
pub fn migrate(mut set: FaultSet) -> Result<FaultSet, String> {
    match set.schema_version.as_str() {
        "1" | "2" | "3" | "4" | "5" => {
            set.schema_version = SCHEMA_VERSION.to_string();
            Ok(set)
        }
        other => Err(format!("no migration from fault schema version {other}")),
    }
}
