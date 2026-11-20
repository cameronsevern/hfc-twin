//! Architecture profiles: a data file that selects among implemented mechanisms. A profile
//! holds targets; what a part can deliver is in its component entry.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::capability::CapabilitySet;
use crate::common::{ChannelId, ProfileId};

/// Version of this schema.
pub const SCHEMA_VERSION: &str = "2";

/// DOCSIS version and duplex mode.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Docsis {
    /// DOCSIS 3.0.
    D30,
    /// DOCSIS 3.1.
    D31,
    /// DOCSIS 4.0, extended spectrum.
    D40Esd,
    /// DOCSIS 4.0, full duplex.
    D40Fdx,
}

/// Direction of a band segment.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Upstream only.
    Us,
    /// Downstream only.
    Ds,
    /// Shared, under FDX.
    Both,
}

/// One contiguous part of a band plan. Segments rather than one upstream and one
/// downstream range, since D4.0 has two upstream bands and FDX shares one.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BandSegment {
    /// Lower edge, MHz.
    pub lo_mhz: f64,
    /// Upper edge, MHz.
    pub hi_mhz: f64,
    /// Which way it carries signal.
    pub direction: Direction,
}

impl BandSegment {
    /// Whether this segment carries signal in `direction`.
    pub fn carries(&self, direction: Direction) -> bool {
        self.direction == direction || self.direction == Direction::Both
    }
}

/// The kind of head-end device at a tree's root.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// Analog fiber node, fed from an I-CCAP.
    Analog,
    /// Remote PHY device.
    Rpd,
}

/// A constellation. These are the orders with §4.6 thresholds.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qam {
    /// 16-QAM.
    Qam16,
    /// 64-QAM.
    Qam64,
    /// 128-QAM.
    Qam128,
    /// 256-QAM.
    Qam256,
    /// 512-QAM.
    Qam512,
    /// 1024-QAM.
    Qam1024,
    /// 2048-QAM.
    Qam2048,
    /// 4096-QAM.
    Qam4096,
}

/// NCP modulation. The PLC is always 16-QAM, so it has no field (§4.6).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NcpModulation {
    /// QPSK.
    Qpsk,
    /// 16-QAM.
    Qam16,
    /// 64-QAM.
    Qam64,
}

/// A downstream SC-QAM channel.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DsScQamChannel {
    /// Stable channel id.
    pub channel_id: ChannelId,
    /// Centre frequency, MHz.
    pub center_mhz: f64,
    /// Occupied width, MHz.
    pub width_mhz: f64,
    /// Modulation.
    pub modulation: Qam,
}

/// An upstream SC-QAM channel.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UsScQamChannel {
    /// Stable channel id.
    pub channel_id: ChannelId,
    /// Centre frequency, MHz.
    pub center_mhz: f64,
    /// Occupied width, MHz.
    pub width_mhz: f64,
    /// Symbol rate, Msym/s (§5.3).
    pub symbol_rate_msym_per_s: f64,
    /// Modulation.
    pub modulation: Qam,
}

/// A data profile on an OFDM or OFDMA channel, with one modulation across it.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DataProfile {
    /// Downstream profile id (0 is Profile A) or upstream data IUC.
    pub data_profile_id: u8,
    /// Modulation.
    pub modulation: Qam,
}

/// A downstream OFDM channel.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OfdmChannel {
    /// Stable channel id.
    pub channel_id: ChannelId,
    /// First active subcarrier, MHz.
    pub first_active_mhz: f64,
    /// Last active subcarrier, MHz.
    pub last_active_mhz: f64,
    /// FFT size: 4096 or 8192.
    pub fft_size: u32,
    /// Subcarrier spacing, kHz; 50 at 4K and 25 at 8K.
    pub subcarrier_spacing_khz: f64,
    /// Cyclic prefix, µs.
    pub cp_us: f64,
    /// Window roll-off, µs.
    pub rolloff_us: f64,
    /// Data profiles.
    pub data_profiles: Vec<DataProfile>,
    /// NCP modulation.
    pub ncp_modulation: NcpModulation,
}

/// An upstream OFDMA channel.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OfdmaChannel {
    /// Stable channel id.
    pub channel_id: ChannelId,
    /// First active subcarrier, MHz.
    pub first_active_mhz: f64,
    /// Last active subcarrier, MHz.
    pub last_active_mhz: f64,
    /// FFT size: 2048 or 4096.
    pub fft_size: u32,
    /// Subcarrier spacing, kHz; 50 at 2K and 25 at 4K.
    pub subcarrier_spacing_khz: f64,
    /// Cyclic prefix, µs.
    pub cp_us: f64,
    /// Window roll-off, µs.
    pub rolloff_us: f64,
    /// Data profiles.
    pub data_profiles: Vec<DataProfile>,
}

/// Every channel the profile runs.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ChannelPlan {
    /// Downstream SC-QAM channels.
    pub ds_sc_qam: Vec<DsScQamChannel>,
    /// Downstream OFDM channels.
    pub ofdm: Vec<OfdmChannel>,
    /// Upstream SC-QAM channels.
    pub us_sc_qam: Vec<UsScQamChannel>,
    /// Upstream OFDMA channels.
    pub ofdma: Vec<OfdmaChannel>,
}

/// The lowest downstream level a modem may run at a given highest modulation (§3.3).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct QamFloor {
    /// Highest modulation in use.
    pub qam: Qam,
    /// Input floor, dBmV per 6 MHz.
    pub min_dbmv: f64,
}

/// Design targets and limits (§3).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignLevels {
    /// Active output at the downstream band's top frequency, dBmV per 6 MHz (§3.1).
    pub out_dbmv_at_hi: f64,
    /// Output tilt across the downstream band, dB (§3.1).
    pub tilt_db: f64,
    /// Most the average power per MHz at a modem may exceed the average of its
    /// lowest-power 24 MHz, dB (§3.3).
    pub psd_spread_max_db: f64,
    /// Modem input floor by highest modulation (§3.3).
    pub modem_in_min_dbmv_by_qam: Vec<QamFloor>,
    /// Modem input ceiling, dBmV per 6 MHz (§3.3).
    pub modem_in_max_dbmv: f64,
    /// Modem total input ceiling, dBmV over the whole band (§3.3).
    pub modem_in_total_max_dbmv: f64,
    /// Level the node's return input needs, dBmV per 6.4 MHz (§7.2).
    pub us_node_in_target_dbmv: f64,
}

/// FEC figures for one modulation (§4.6).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FecRow {
    /// Modulation.
    pub qam: Qam,
    /// OFDM failure threshold `thr`, where uncorrectables begin, dB.
    pub thr_db: f64,
    /// Required CNR in AWGN, dB.
    pub req_cnr_db: f64,
    /// Required CNR from 1002 to 1218 MHz where it differs, dB.
    pub req_cnr_above_1002_db: Option<f64>,
    /// SC-QAM failure threshold where a source gives one, dB.
    pub sc_qam_thr_db: Option<f64>,
}

/// What a modem does when asked for more transmit power than it has (§7.2).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxSaturation {
    /// Transmits at its limit, so the CMTS hears it low by the deficit.
    Clamp,
}

/// Modem and CMTS behaviour that sources leave to vendors (§7.2, §7.4).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModemBehavior {
    /// Above the transmit limit.
    pub tx_saturation: TxSaturation,
    /// Station maintenance spacing, seconds.
    pub ranging_spacing_s: f64,
    /// Time from a usable channel to registration, seconds.
    pub reregistration_s: f64,
}

/// An architecture profile.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Version of this schema.
    pub schema_version: String,
    /// Stable id, referenced by plants.
    pub id: ProfileId,
    /// DOCSIS version and duplex mode.
    pub docsis: Docsis,
    /// Spectrum by direction.
    pub band_plan: Vec<BandSegment>,
    /// Channels in that spectrum.
    pub channel_plan: ChannelPlan,
    /// Most amps from the root to any modem.
    pub max_cascade: u8,
    /// Most homes passed per node.
    pub max_homes_passed: u32,
    /// Design targets and limits.
    pub design_levels: DesignLevels,
    /// FEC figures by modulation.
    pub fec: Vec<FecRow>,
    /// Vendor-dependent modem and CMTS behaviour.
    pub modem_behavior: ModemBehavior,
    /// The kind of device a plant on this profile must have at its root.
    pub node_kind: NodeKind,
    /// The plant-side capability set.
    pub capability: CapabilitySet,
    /// What the plant's optics and actives report beyond `capability` (§16, observation version 2). Version 2.
    #[serde(default, skip_serializing_if = "PlantReports::is_none")]
    pub plant_reports: PlantReports,
}

/// Plant-side readings observation version 2 added, each its own flag. Absent is none, as version 1 reported. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct PlantReports {
    /// An analog node's forward receiver power (`fnOpticalReceiverPower`), or an RPD optic's receive power.
    #[serde(default)]
    pub node_optical_rx: bool,
    /// The hub's return receiver input power (`heOpRxInputPower`), or the hub optic's receive power.
    #[serde(default)]
    pub hub_optical_rx: bool,
    /// An RPD's optic status, from its pluggable's diagnostics.
    #[serde(default)]
    pub rpd_optic_status: bool,
    /// An active's output level per output port (`rfAmpRFPortOutputRFLevel`).
    #[serde(default)]
    pub out_level_per_port: bool,
}

impl PlantReports {
    /// Whether it reports none of them.
    pub fn is_none(&self) -> bool {
        *self == PlantReports::default()
    }
}

/// Brings a profile from an earlier version to this one. Version 2 only added `plant_reports`, absent as none, so a version 1
/// profile means the same.
pub fn migrate(mut profile: Profile) -> Result<Profile, String> {
    match profile.schema_version.as_str() {
        "1" | "2" => {
            profile.schema_version = SCHEMA_VERSION.to_string();
            Ok(profile)
        }
        other => Err(format!("no migration from profile schema version {other}")),
    }
}

/// Cyclic prefix options, downstream, µs [PHY Tables 39–40].
const DS_CP_US: [f64; 5] = [0.9375, 1.25, 2.5, 3.75, 5.0];
/// Roll-off options, downstream, µs [PHY Tables 39–40].
const DS_ROLLOFF_US: [f64; 5] = [0.0, 0.3125, 0.625, 0.9375, 1.25];
/// Cyclic prefix options, upstream, µs [PHY Tables 9–10].
const US_CP_US: [f64; 11] = [
    0.9375, 1.25, 1.5625, 1.875, 2.1875, 2.5, 2.8125, 3.125, 3.75, 5.0, 6.25,
];
/// Roll-off options, upstream, µs [PHY Tables 9–10].
const US_ROLLOFF_US: [f64; 8] = [0.0, 0.3125, 0.625, 0.9375, 1.25, 1.5625, 1.875, 2.1875];
/// Largest encompassed spectrum of an OFDM channel, MHz [PHY §7.3.1].
const MAX_OFDM_MHZ: f64 = 190.0;
/// Largest encompassed spectrum of an OFDMA channel, MHz [PHY §7.3.2].
const MAX_OFDMA_MHZ: f64 = 95.0;

/// A problem with a profile.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ProfileError {
    /// Two channels share an id.
    #[error("duplicate channel id {0}")]
    DuplicateChannel(ChannelId),
    /// FFT size and subcarrier spacing disagree for the channel's direction.
    #[error("channel {channel}: FFT size {fft_size} doesn't give {spacing_khz} kHz spacing")]
    FftSpacingMismatch {
        /// The channel.
        channel: ChannelId,
        /// Its FFT size.
        fft_size: u32,
        /// Its subcarrier spacing, kHz.
        spacing_khz: f64,
    },
    /// Cyclic prefix or roll-off isn't an allowed value, or roll-off isn't below prefix.
    #[error("channel {channel}: cyclic prefix {cp_us} µs with roll-off {rolloff_us} µs")]
    BadCpRolloff {
        /// The channel.
        channel: ChannelId,
        /// Its cyclic prefix, µs.
        cp_us: f64,
        /// Its roll-off, µs.
        rolloff_us: f64,
    },
    /// Active subcarriers are in the wrong order or span too much spectrum.
    #[error("channel {0}: active subcarrier range is empty or too wide")]
    BadActiveRange(ChannelId),
    /// An active subcarrier isn't on a multiple of the spacing.
    #[error("channel {0}: active subcarriers aren't on the subcarrier grid")]
    OffGrid(ChannelId),
    /// A channel falls outside the band plan for its direction.
    #[error("channel {0} is outside the band plan")]
    OutsideBandPlan(ChannelId),
}

/// Whether `value` is one of `options`, which are exact binary fractions.
fn is_option(options: &[f64], value: f64) -> bool {
    options.iter().any(|option| (option - value).abs() < 1e-9)
}

impl Profile {
    /// Top of upstream and bottom of downstream, which together name a diplexer split.
    pub fn split_edges(&self) -> Option<(f64, f64)> {
        let edge = |direction, pick: fn(&BandSegment) -> f64, better: fn(f64, f64) -> f64| {
            self.band_plan
                .iter()
                .filter(|segment| segment.carries(direction))
                .map(pick)
                .reduce(better)
        };
        Some((
            edge(Direction::Us, |s| s.hi_mhz, f64::max)?,
            edge(Direction::Ds, |s| s.lo_mhz, f64::min)?,
        ))
    }

    fn inside_band(&self, direction: Direction, lo_mhz: f64, hi_mhz: f64) -> bool {
        self.band_plan.iter().any(|segment| {
            segment.carries(direction) && segment.lo_mhz <= lo_mhz && hi_mhz <= segment.hi_mhz
        })
    }

    /// Checks the channel plan against the numerology of `docs/reference/phy-docsis31.md`
    /// §4–§5 and the band plan.
    pub fn validate(&self) -> Result<(), Vec<ProfileError>> {
        let mut errors = Vec::new();
        let plan = &self.channel_plan;

        let mut seen = BTreeSet::new();
        let ids = plan
            .ds_sc_qam
            .iter()
            .map(|c| &c.channel_id)
            .chain(plan.ofdm.iter().map(|c| &c.channel_id))
            .chain(plan.us_sc_qam.iter().map(|c| &c.channel_id))
            .chain(plan.ofdma.iter().map(|c| &c.channel_id));
        for id in ids {
            if !seen.insert(id) {
                errors.push(ProfileError::DuplicateChannel(id.clone()));
            }
        }

        for c in &plan.ds_sc_qam {
            let half_mhz = c.width_mhz / 2.0;
            if !self.inside_band(
                Direction::Ds,
                c.center_mhz - half_mhz,
                c.center_mhz + half_mhz,
            ) {
                errors.push(ProfileError::OutsideBandPlan(c.channel_id.clone()));
            }
        }
        for c in &plan.us_sc_qam {
            let half_mhz = c.width_mhz / 2.0;
            if !self.inside_band(
                Direction::Us,
                c.center_mhz - half_mhz,
                c.center_mhz + half_mhz,
            ) {
                errors.push(ProfileError::OutsideBandPlan(c.channel_id.clone()));
            }
        }
        for c in &plan.ofdm {
            let spacing_khz = match c.fft_size {
                4096 => Some(50.0),
                8192 => Some(25.0),
                _ => None,
            };
            self.check_multicarrier(
                &mut errors,
                MulticarrierCheck {
                    channel: &c.channel_id,
                    direction: Direction::Ds,
                    first_active_mhz: c.first_active_mhz,
                    last_active_mhz: c.last_active_mhz,
                    fft_size: c.fft_size,
                    spacing_khz: c.subcarrier_spacing_khz,
                    expected_spacing_khz: spacing_khz,
                    cp_us: c.cp_us,
                    rolloff_us: c.rolloff_us,
                    cp_options: &DS_CP_US,
                    rolloff_options: &DS_ROLLOFF_US,
                    max_encompassed_mhz: MAX_OFDM_MHZ,
                },
            );
        }
        for c in &plan.ofdma {
            let spacing_khz = match c.fft_size {
                2048 => Some(50.0),
                4096 => Some(25.0),
                _ => None,
            };
            self.check_multicarrier(
                &mut errors,
                MulticarrierCheck {
                    channel: &c.channel_id,
                    direction: Direction::Us,
                    first_active_mhz: c.first_active_mhz,
                    last_active_mhz: c.last_active_mhz,
                    fft_size: c.fft_size,
                    spacing_khz: c.subcarrier_spacing_khz,
                    expected_spacing_khz: spacing_khz,
                    cp_us: c.cp_us,
                    rolloff_us: c.rolloff_us,
                    cp_options: &US_CP_US,
                    rolloff_options: &US_ROLLOFF_US,
                    max_encompassed_mhz: MAX_OFDMA_MHZ,
                },
            );
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn check_multicarrier(&self, errors: &mut Vec<ProfileError>, c: MulticarrierCheck<'_>) {
        let channel = || c.channel.clone();
        if c.expected_spacing_khz != Some(c.spacing_khz) {
            errors.push(ProfileError::FftSpacingMismatch {
                channel: channel(),
                fft_size: c.fft_size,
                spacing_khz: c.spacing_khz,
            });
        }
        let cp_ok = is_option(c.cp_options, c.cp_us);
        let rolloff_ok = is_option(c.rolloff_options, c.rolloff_us);
        if !cp_ok || !rolloff_ok || c.rolloff_us >= c.cp_us {
            errors.push(ProfileError::BadCpRolloff {
                channel: channel(),
                cp_us: c.cp_us,
                rolloff_us: c.rolloff_us,
            });
        }
        // Encompassed spectrum is highest minus lowest centre, plus one spacing.
        let spacing_mhz = c.spacing_khz / 1000.0;
        let encompassed_mhz = c.last_active_mhz - c.first_active_mhz + spacing_mhz;
        if c.last_active_mhz <= c.first_active_mhz || encompassed_mhz > c.max_encompassed_mhz + 1e-9
        {
            errors.push(ProfileError::BadActiveRange(channel()));
        }
        // Every subcarrier centre is a whole multiple of the spacing.
        let on_grid = |f_mhz: f64| {
            let steps = f_mhz / spacing_mhz;
            spacing_mhz > 0.0 && (steps - steps.round()).abs() < 1e-6
        };
        if !on_grid(c.first_active_mhz) || !on_grid(c.last_active_mhz) {
            errors.push(ProfileError::OffGrid(channel()));
        }
        if !self.inside_band(c.direction, c.first_active_mhz, c.last_active_mhz) {
            errors.push(ProfileError::OutsideBandPlan(channel()));
        }
    }
}

struct MulticarrierCheck<'a> {
    channel: &'a ChannelId,
    direction: Direction,
    first_active_mhz: f64,
    last_active_mhz: f64,
    fft_size: u32,
    spacing_khz: f64,
    expected_spacing_khz: Option<f64>,
    cp_us: f64,
    rolloff_us: f64,
    cp_options: &'a [f64],
    rolloff_options: &'a [f64],
    max_encompassed_mhz: f64,
}
