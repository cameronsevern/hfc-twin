//! Which observation fields exist, and how they are read. A profile holds the plant-side
//! set and a modem its own; generations mix on one node, so what a given modem reports
//! is the intersection (§9).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How an adapter reaches a device's data.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessPath {
    /// SNMP to the modem's own address.
    CmSnmp,
    /// A PNM file the modem uploads as a TFTP client.
    CmTftp,
    /// SNMP to the CMTS, for what it holds about a modem or channel.
    CmtsSnmp,
}

/// Full-band capture support.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FbcCapability {
    /// Available as a PNM file.
    pub file: bool,
    /// Available over SNMP.
    pub snmp: bool,
    /// Lowest covered frequency, MHz.
    pub lo_mhz: f64,
    /// Highest covered frequency, MHz.
    pub hi_mhz: f64,
}

/// Which observation fields exist, and how.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CapabilitySet {
    /// Downstream RxMER per subcarrier, by file.
    pub ds_rxmer_file: bool,
    /// Downstream RxMER mean, standard deviation and 2nd percentile.
    pub ds_rxmer_summary: bool,
    /// Downstream channel estimate coefficients, by file.
    pub ds_chan_est_file: bool,
    /// Downstream channel estimate `ResponseSummary`.
    pub ds_chan_est_summary: bool,
    /// OFDMA pre-eq coefficients, by file.
    pub ofdma_preeq_file: bool,
    /// OFDMA pre-eq `ResponseSummary`.
    pub ofdma_preeq_summary: bool,
    /// SC-QAM pre-eq taps.
    pub sc_qam_preeq: bool,
    /// Full-band capture, or `None` if unsupported.
    pub fbc: Option<FbcCapability>,
    /// OFDM codeword counts per data profile.
    pub per_profile_fec: bool,
    /// OFDM PLC and NCP counters.
    pub plc_ncp_counters: bool,
    /// OFDM receive power per 6 MHz band.
    pub band_power_6mhz: bool,
    /// Upstream RxMER per subcarrier, by file.
    pub us_rxmer_file: bool,
    /// Upstream RxMER channel mean.
    pub us_rxmer_summary: bool,
    /// Upstream spectrum capture.
    pub us_spectrum_capture: bool,
    /// Active reports input voltage.
    pub v_in: bool,
    /// Active reports current draw.
    pub i_draw: bool,
    /// Active reports temperature.
    pub temp_c: bool,
    /// Active reports operating status.
    pub status: bool,
    /// Active reports an output level.
    pub out_level: bool,
    /// Ways to reach the data.
    pub access: Vec<AccessPath>,
}

impl CapabilitySet {
    /// True if any field that only exists on OFDM or OFDMA channels is present.
    pub fn has_ofdm_fields(&self) -> bool {
        self.ds_rxmer_file
            || self.ds_rxmer_summary
            || self.ds_chan_est_file
            || self.ds_chan_est_summary
            || self.ofdma_preeq_file
            || self.ofdma_preeq_summary
            || self.per_profile_fec
            || self.plc_ncp_counters
            || self.band_power_6mhz
            || self.us_rxmer_file
            || self.us_rxmer_summary
    }
}

/// What one modem reports on one profile: the intersection of the two sets. Service
/// group and active fields come from the profile's set directly, since no modem reports
/// them.
pub fn effective(profile: &CapabilitySet, modem: &CapabilitySet) -> CapabilitySet {
    CapabilitySet {
        ds_rxmer_file: profile.ds_rxmer_file && modem.ds_rxmer_file,
        ds_rxmer_summary: profile.ds_rxmer_summary && modem.ds_rxmer_summary,
        ds_chan_est_file: profile.ds_chan_est_file && modem.ds_chan_est_file,
        ds_chan_est_summary: profile.ds_chan_est_summary && modem.ds_chan_est_summary,
        ofdma_preeq_file: profile.ofdma_preeq_file && modem.ofdma_preeq_file,
        ofdma_preeq_summary: profile.ofdma_preeq_summary && modem.ofdma_preeq_summary,
        sc_qam_preeq: profile.sc_qam_preeq && modem.sc_qam_preeq,
        fbc: effective_fbc(profile.fbc, modem.fbc),
        per_profile_fec: profile.per_profile_fec && modem.per_profile_fec,
        plc_ncp_counters: profile.plc_ncp_counters && modem.plc_ncp_counters,
        band_power_6mhz: profile.band_power_6mhz && modem.band_power_6mhz,
        us_rxmer_file: profile.us_rxmer_file && modem.us_rxmer_file,
        us_rxmer_summary: profile.us_rxmer_summary && modem.us_rxmer_summary,
        us_spectrum_capture: profile.us_spectrum_capture && modem.us_spectrum_capture,
        v_in: profile.v_in && modem.v_in,
        i_draw: profile.i_draw && modem.i_draw,
        temp_c: profile.temp_c && modem.temp_c,
        status: profile.status && modem.status,
        out_level: profile.out_level && modem.out_level,
        access: profile
            .access
            .iter()
            .copied()
            .filter(|path| modem.access.contains(path))
            .collect(),
    }
}

fn effective_fbc(a: Option<FbcCapability>, b: Option<FbcCapability>) -> Option<FbcCapability> {
    let (a, b) = (a?, b?);
    let fbc = FbcCapability {
        file: a.file && b.file,
        snmp: a.snmp && b.snmp,
        lo_mhz: a.lo_mhz.max(b.lo_mhz),
        hi_mhz: a.hi_mhz.min(b.hi_mhz),
    };
    ((fbc.file || fbc.snmp) && fbc.lo_mhz < fbc.hi_mhz).then_some(fbc)
}
