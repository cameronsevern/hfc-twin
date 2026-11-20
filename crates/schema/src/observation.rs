//! What the sim emits, per `docs/physics.md` §9. The sim owns this schema; adapters map
//! vendor data into it (invariant 2). Values are absolute, as a device reports them; the
//! feature layer (step 4) derives what models read.
//!
//! Channel records describe themselves, so a record can be read without the channel plan
//! then in force. A field is `None` when a capability mask removes it or, decided when
//! the value is emitted, when its condition doesn't hold (§11).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{
    ChannelId, Complex, ElementId, Origin, PortId, ProfileId, ServiceGroupId, Timestamp,
};
use crate::component::ModemGeneration;

/// Version of this schema. Version 2 added a node's optical readings and an active's output level per port, each optional
/// and each under a capability flag (`profile::PlantReports`).
pub const SCHEMA_VERSION: &str = "2";

/// Whether a model trained on observations of `version` can read this schema's. Version 2 only added optional fields, which
/// a version 1 model never read, as `fault::model_can_read` reads faults.
pub fn model_can_read(version: &str) -> bool {
    matches!(version, "1" | "2")
}

/// Brings a record's version from an earlier one to this one. Version 2 only added optional fields, so a version 1 record
/// means the same; every record kind carries its version the same way.
pub fn migrate_version(version: &str) -> Result<String, String> {
    match version {
        "1" | "2" => Ok(SCHEMA_VERSION.to_string()),
        other => Err(format!(
            "no migration from observation schema version {other}"
        )),
    }
}

/// Locates a per-subcarrier array in frequency:
/// `f = zero_freq_mhz + (first_active_index + i) · spacing_khz`.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SubcarrierAxis {
    /// Frequency of subcarrier index 0, MHz.
    pub zero_freq_mhz: f64,
    /// Index of the array's first entry.
    pub first_active_index: u32,
    /// Subcarrier spacing, kHz.
    pub spacing_khz: f64,
}

/// Summary metrics of a channel estimate or OFDMA pre-eq response (§5.5).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ResponseSummary {
    /// Mean magnitude, dB.
    pub mag_mean_db: f64,
    /// Magnitude slope, dB per MHz.
    pub mag_slope_db_per_mhz: f64,
    /// RMS magnitude ripple, dB.
    pub mag_ripple_rms_db: f64,
    /// Peak-to-peak magnitude ripple, dB.
    pub mag_ripple_pp_db: f64,
    /// Mean group delay, ns.
    pub gd_mean_ns: f64,
    /// Group delay slope, ns per MHz.
    pub gd_slope_ns_per_mhz: f64,
    /// RMS group delay ripple, ns.
    pub gd_ripple_rms_ns: f64,
    /// Peak-to-peak group delay ripple, ns.
    pub gd_ripple_pp_ns: f64,
}

/// Downstream RxMER for one OFDM channel (§4.5, §5.5).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DsRxmerRecord {
    /// The OFDM channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Where `rxmer_db` lies in frequency.
    pub axis: SubcarrierAxis,
    /// RxMER per subcarrier, dB. An entry is `None` where the device has no value.
    pub rxmer_db: Option<Vec<Option<f64>>>,
    /// Mean over active subcarriers, dB.
    pub rxmer_mean_db: Option<f64>,
    /// Population standard deviation over active subcarriers, dB.
    pub rxmer_stddev_db: Option<f64>,
    /// 2nd-percentile value, dB.
    pub rxmer_p2_db: Option<f64>,
}

/// One full-band capture. A bin level isn't per 6 MHz, so the record carries what's needed
/// to convert it.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FbcRecord {
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Centre of the first bin, MHz.
    pub first_bin_center_mhz: f64,
    /// Bin spacing, kHz.
    pub bin_width_khz: f64,
    /// Equivalent noise bandwidth of one bin, kHz.
    pub noise_bandwidth_khz: f64,
    /// Level per bin, dBmV in `noise_bandwidth_khz`.
    pub fbc_dbmv: Vec<f64>,
}

/// Downstream channel estimate for one OFDM channel (§5.5).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DsChanEstRecord {
    /// The OFDM channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Where `coefficients` lie in frequency.
    pub axis: SubcarrierAxis,
    /// Per-subcarrier response, scaled to average power of about 1. An entry is `None`
    /// where the device has no value.
    pub coefficients: Option<Vec<Option<Complex>>>,
    /// Summary metrics.
    pub summary: Option<ResponseSummary>,
}

/// Receive power in one 6 MHz band of an OFDM channel.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DsBandLevel {
    /// Centre of the band, MHz.
    pub center_mhz: f64,
    /// Level, dBmV per 6 MHz, computed from active subcarriers.
    pub level_dbmv: f64,
}

/// Downstream receive power on one channel.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum DsRxLevels {
    /// One value for the channel.
    ScQam {
        /// Level, dBmV per 6 MHz.
        level_dbmv: f64,
    },
    /// One value per 6 MHz band, plus the PLC band.
    Ofdm {
        /// The 6 MHz bands.
        bands: Vec<DsBandLevel>,
        /// The PLC band, dBmV per 6 MHz.
        plc_band_dbmv: f64,
    },
}

/// `ds_rx_dbmv` for one downstream channel. OFDM channels appear only where the capability
/// `band_power_6mhz` allows.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DsRxRecord {
    /// The channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// The levels.
    pub ds_rx_dbmv: DsRxLevels,
}

/// Downstream codeword counts for one channel and data profile, per interval (§4.6).
/// OFDM channels appear only where the capability `per_profile_fec` allows.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CodewordRecord {
    /// The channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// The data profile, or `None` on SC-QAM, which has one.
    pub data_profile_id: Option<u8>,
    /// Codewords received.
    pub total: u64,
    /// Codewords corrected.
    pub corrected: u64,
    /// Codewords uncorrectable.
    pub uncorrectable: u64,
}

/// PLC counts for one OFDM channel, per interval (§4.6).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PlcRecord {
    /// The OFDM channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Codewords received.
    pub total: u64,
    /// Codewords that failed.
    pub unreliable: u64,
}

/// NCP counts for one OFDM channel, per interval (§4.6).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NcpRecord {
    /// The OFDM channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Fields received.
    pub total: u64,
    /// Fields that failed their CRC.
    pub crc_failures: u64,
}

/// SC-QAM pre-eq for one upstream channel (§5.3).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScQamPreeqRecord {
    /// The upstream channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// The 24 symbol-spaced taps.
    pub taps: Vec<Complex>,
    /// 1-based position of the main tap.
    pub main_tap_index: u8,
    /// Main tap compression, dB.
    pub mtc_db: f64,
    /// Main tap ratio, dB.
    pub mtr_db: f64,
    /// Non-main tap to total energy ratio, dB.
    pub nmter_db: f64,
}

/// OFDMA pre-eq for one upstream channel (§5.4, §5.5).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OfdmaPreeqRecord {
    /// The upstream channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Where `coefficients` lie in frequency.
    pub axis: SubcarrierAxis,
    /// Per-subcarrier coefficients, normalized so mean(|c|²) = 1.
    pub coefficients: Option<Vec<Complex>>,
    /// Summary metrics.
    pub summary: Option<ResponseSummary>,
}

/// `preeq`: SC-QAM, OFDMA or both.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Preeq {
    /// SC-QAM channels, where the capability allows.
    pub sc_qam: Option<Vec<ScQamPreeqRecord>>,
    /// OFDMA channels, where the capability allows.
    pub ofdma: Option<Vec<OfdmaPreeqRecord>>,
}

/// `us_tx` for one upstream channel (§7.2).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UsTxRecord {
    /// The upstream channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Transmit power, dBmV per 1.6 MHz.
    pub psd_dbmv_per_1p6mhz: f64,
    /// Total channel power, dBmV, on SC-QAM only: a 3.1 modem's SC-QAM total-power object
    /// stays total.
    pub total_dbmv: Option<f64>,
}

/// `us_rx_dbmv_per_1p6mhz` for one upstream channel (§7.2).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UsRxRecord {
    /// The upstream channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// This modem as the CMTS receives it, dBmV per 1.6 MHz.
    pub us_rx_dbmv_per_1p6mhz: f64,
}

/// Upstream RxMER for one modem on one OFDMA channel, measured at the CMTS (§7.3).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UsRxmerRecord {
    /// The OFDMA channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Where `us_rxmer_db` lies in frequency.
    pub axis: SubcarrierAxis,
    /// RxMER per subcarrier, dB. An entry is `None` where the device has no value.
    pub us_rxmer_db: Option<Vec<Option<f64>>>,
    /// Channel mean, dB.
    pub us_rxmer_mean_db: Option<f64>,
}

/// Registration state, as far as the §7.4 model moves through it. Names follow
/// `CmRegState` [CM-OSSI §F.2.2].
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegState {
    /// No downstream acquired.
    NotSynchronized,
    /// Downstream acquired.
    PhySynchronized,
    /// Ranging.
    RangingInProgress,
    /// Ranging done.
    RangingComplete,
    /// Registering.
    RegistrationInProgress,
    /// Registered and passing traffic.
    Operational,
}

/// Whether a channel can be used (§7.4). Unusable while another works is partial service.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelUsability {
    /// In use.
    Usable,
    /// Not usable.
    Unusable,
}

/// `ds_channel_state` or `us_channel_state` for one channel.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ChannelStateRecord {
    /// The channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Its state.
    pub state: ChannelUsability,
}

/// `partial_channel` for one OFDM channel: what has failed while its pilots and preambles
/// are fine (`docs/reference/mac-operations.md`).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PartialChannelRecord {
    /// The OFDM channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Data profiles that have failed.
    pub failed_data_profiles: Vec<u8>,
    /// The NCP has failed.
    pub ncp_failed: bool,
    /// The PLC has failed.
    pub plc_failed: bool,
}

/// A cumulative timeout count for one upstream channel [CM-OSSI §F.2.3.2].
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimeoutRecord {
    /// The upstream channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Timeouts since the modem started counting.
    pub count: u64,
}

/// The state of ranging on a channel [CM-OSSI §F.2.3.2].
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangingState {
    /// Aborted by the CMTS.
    Aborted,
    /// Ran out of retries.
    RetriesExceeded,
    /// Succeeded.
    Success,
    /// Still adjusting.
    Continue,
}

/// `ranging_status` for one upstream channel.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RangingStatusRecord {
    /// The upstream channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Its state.
    pub status: RangingState,
}

/// What one modem reports at one time step.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModemObservation {
    /// Version of this schema.
    pub schema_version: String,
    /// Simulated time, seconds.
    pub t: Timestamp,
    /// Whether this is simulated or measured (invariant 11).
    pub origin: Origin,
    /// The modem.
    pub element_id: ElementId,
    /// Its generation, so a reader knows which fields to expect.
    pub generation: ModemGeneration,

    /// Downstream RxMER per OFDM channel.
    pub rxmer: Option<Vec<DsRxmerRecord>>,
    /// Full-band capture.
    pub fbc: Option<FbcRecord>,
    /// Downstream channel estimate per OFDM channel.
    pub ds_chan_est: Option<Vec<DsChanEstRecord>>,
    /// Downstream receive power per channel.
    pub ds_rx: Vec<DsRxRecord>,
    /// Downstream codeword counts per channel and data profile.
    pub codewords: Vec<CodewordRecord>,
    /// PLC counts per OFDM channel.
    pub plc: Option<Vec<PlcRecord>>,
    /// NCP counts per OFDM channel.
    pub ncp: Option<Vec<NcpRecord>>,

    /// Upstream pre-equalization.
    pub preeq: Preeq,
    /// Transmit power per upstream channel.
    pub us_tx: Vec<UsTxRecord>,
    /// Power the CMTS receives per upstream channel.
    pub us_rx: Vec<UsRxRecord>,
    /// Upstream RxMER per OFDMA channel.
    pub us_rxmer: Option<Vec<UsRxmerRecord>>,

    /// Registration state.
    pub reg_state: RegState,
    /// Usability per downstream channel.
    pub ds_channel_state: Vec<ChannelStateRecord>,
    /// Usability per upstream channel.
    pub us_channel_state: Vec<ChannelStateRecord>,
    /// Partial channel state per OFDM channel.
    pub partial_channel: Option<Vec<PartialChannelRecord>>,
    /// Cumulative T3 timeouts per upstream channel.
    pub t3_timeouts: Vec<TimeoutRecord>,
    /// Cumulative T4 timeouts per upstream channel.
    pub t4_timeouts: Vec<TimeoutRecord>,
    /// Ranging status per upstream channel.
    pub ranging_status: Vec<RangingStatusRecord>,
}

/// An active's operating status.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveStatus {
    /// Running.
    Up,
    /// Not running: failed or browned out.
    Down,
}

/// An active's output level. No real source gives the level at the band's top, so the
/// record says where it was read.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OutLevel {
    /// Level at `f_ref_mhz`, dBmV per 6 MHz.
    pub out_level_dbmv: f64,
    /// Where it was read, MHz.
    pub f_ref_mhz: f64,
}

/// What one active reports at one time step, where the hardware reports it.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ActiveObservation {
    /// Version of this schema.
    pub schema_version: String,
    /// Simulated time, seconds.
    pub t: Timestamp,
    /// Whether this is simulated or measured (invariant 11).
    pub origin: Origin,
    /// The active.
    pub element_id: ElementId,
    /// Input voltage, volts.
    pub v_in: Option<f64>,
    /// Current draw, amps.
    pub i_draw: Option<f64>,
    /// Device temperature, °C.
    pub temp_c: Option<f64>,
    /// Operating status.
    pub status: Option<ActiveStatus>,
    /// Output level.
    pub out_level: Option<OutLevel>,
    /// Output level at each output port, where the active reports one per port (`rfAmpRFPortOutputRFLevel`). Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub out_levels: Vec<PortOutLevel>,
}

/// One output port's level (§15). Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PortOutLevel {
    /// The output port.
    pub port: PortId,
    /// Level at `f_ref_mhz`, dBmV per 6 MHz.
    pub out_level_dbmv: f64,
    /// Where it was read, MHz.
    pub f_ref_mhz: f64,
}

/// A node's optics as its devices report them (§16). Each field is `None` where its flag is off or nothing reports it.
/// Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpticsObservation {
    /// Light into the node's forward receiver or RPD optic, dBm: `fnOpticalReceiverPower` on an analog node, an RPD optic's
    /// receive power.
    pub node_rx_dbm: Option<f64>,
    /// Light into the hub's return receiver or optic, dBm: `heOpRxInputPower`, or the hub optic's receive power.
    pub hub_rx_dbm: Option<f64>,
    /// An RPD's primary optic.
    pub rpd_optic: Option<OpticStatus>,
}

/// One pluggable optic's status (§16). Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpticStatus {
    /// Whether its receive power is between sensitivity and overload.
    pub link_up: bool,
    /// Its launch power, dBm.
    pub tx_dbm: f64,
    /// Its receive power, dBm, or `None` with no light.
    pub rx_dbm: Option<f64>,
}

/// `us_snr_db` for one upstream channel (§7.3).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UsSnrRecord {
    /// The upstream channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// SNR at the CMTS, dB.
    pub us_snr_db: f64,
}

/// One upstream spectrum capture of `N_node(f)` and carriers, on one RF port (§7.3).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UsSpectrumRecord {
    /// The RF port on the root element.
    pub port: PortId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Centre of the first bin, MHz.
    pub first_bin_center_mhz: f64,
    /// Bin spacing, kHz.
    pub bin_width_khz: f64,
    /// Equivalent noise bandwidth of one bin, kHz.
    pub noise_bandwidth_khz: f64,
    /// Level per bin, dBmV in `noise_bandwidth_khz`.
    pub us_spectrum_dbmv: Vec<f64>,
}

/// `us_codewords` for one upstream channel, per interval, as the CMTS counts them.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UsCodewordRecord {
    /// The upstream channel.
    pub channel_id: ChannelId,
    /// The architecture profile in force.
    pub profile_ref: ProfileId,
    /// Codewords received.
    pub total: u64,
    /// Codewords corrected.
    pub corrected: u64,
    /// Codewords uncorrectable.
    pub uncorrectable: u64,
}

/// What one service group shows at one time step.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ServiceGroupObservation {
    /// Version of this schema.
    pub schema_version: String,
    /// Simulated time, seconds.
    pub t: Timestamp,
    /// Whether this is simulated or measured (invariant 11).
    pub origin: Origin,
    /// The service group.
    pub service_group: ServiceGroupId,
    /// SNR per upstream channel.
    pub us_snr: Vec<UsSnrRecord>,
    /// Upstream spectrum capture per RF port.
    pub us_spectrum: Option<Vec<UsSpectrumRecord>>,
    /// Codeword counts per upstream channel.
    pub us_codewords: Vec<UsCodewordRecord>,
    /// The node's optics, where the plant reports them (§16). Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optics: Option<OpticsObservation>,
}
