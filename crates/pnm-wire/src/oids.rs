//! The objects the adapter and emulator exchange.
//!
//! **Which of these are confirmed.** The subtree roots and the objects marked "sourced" are printed in the
//! documents `docs/reference/collection-management.md` §3 cites. No source there prints an OID for the CM test
//! tables, so those arcs are placeholders under the right root. They're named once, here, so an adapter
//! pointed at a real modem changes this file alone. That is the first thing to check against the MIB files.

use crate::snmp::Oid;

/// `sysDescr.0`. Standard.
pub fn sys_descr() -> Oid {
    Oid::parse("1.3.6.1.2.1.1.1.0")
}

fn pnm(arcs: &str) -> Oid {
    Oid::parse(&format!("1.3.6.1.4.1.4491.2.1.27.{arcs}"))
}

fn if3(arcs: &str) -> Oid {
    Oid::parse(&format!("1.3.6.1.4.1.4491.2.1.20.{arcs}"))
}

fn if31(arcs: &str) -> Oid {
    Oid::parse(&format!("1.3.6.1.4.1.4491.2.1.28.{arcs}"))
}

fn docs_if(arcs: &str) -> Oid {
    Oid::parse(&format!("1.3.6.1.2.1.10.127.{arcs}"))
}

/// `docsPnmBulkDestIpAddrType.0`. Sourced.
pub fn bulk_dest_ip_type() -> Oid {
    pnm("1.1.1.1.0")
}
/// `docsPnmBulkDestIpAddr.0`. Sourced.
pub fn bulk_dest_ip_addr() -> Oid {
    pnm("1.1.1.2.0")
}
/// `docsPnmBulkDestPath.0`. Sourced.
pub fn bulk_dest_path() -> Oid {
    pnm("1.1.1.3.0")
}
/// `docsPnmCmCtlTest.0`. Sourced.
pub fn cm_ctl_test() -> Oid {
    pnm("1.2.1.1.0")
}
/// `docsPnmCmCtlStatus.0`. Sourced.
pub fn cm_ctl_status() -> Oid {
    pnm("1.2.1.3.0")
}

/// One CM test table's objects, keyed by ifIndex where the table has one. Placeholder arcs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Test {
    /// `docsPnmCmDsOfdmRxMerTable`.
    DsRxmer,
    /// `docsPnmCmOfdmChanEstCoefTable`.
    ChanEst,
    /// `docsPnmCmUsPreEqTable`.
    UsPreeq,
    /// `docsIf3CmSpectrumAnalysisCtrlCmd`. Sourced root, scalars.
    Spectrum,
}

impl Test {
    /// The value `docsPnmCmCtlTest` reports for this test (placeholder).
    pub fn code(self) -> i64 {
        match self {
            Test::Spectrum => 2,
            Test::ChanEst => 4,
            Test::DsRxmer => 6,
            Test::UsPreeq => 10,
        }
    }

    fn table(self) -> u32 {
        match self {
            Test::DsRxmer => 21,
            Test::ChanEst => 22,
            Test::UsPreeq => 23,
            Test::Spectrum => 24,
        }
    }

    /// `...FileEnable`, or `...TrigEnable`, for a channel.
    pub fn enable(self, if_index: u32) -> Oid {
        pnm(&format!("1.2.{}.1.1.{if_index}", self.table()))
    }
    /// `...MeasStatus` for a channel.
    pub fn status(self, if_index: u32) -> Oid {
        pnm(&format!("1.2.{}.1.2.{if_index}", self.table()))
    }
    /// `...FileName` for a channel.
    pub fn file_name(self, if_index: u32) -> Oid {
        pnm(&format!("1.2.{}.1.3.{if_index}", self.table()))
    }
}

/// [CM-OSSI §D.2.1]: `MeasStatus`.
pub mod meas_status {
    /// Inactive.
    pub const INACTIVE: i64 = 2;
    /// Busy.
    pub const BUSY: i64 = 3;
    /// Sample ready.
    pub const SAMPLE_READY: i64 = 4;
    /// Error.
    pub const ERROR: i64 = 5;
}

/// [CM-OSSI §D.2.3]: `docsPnmCmCtlStatus`.
pub mod ctl_status {
    /// Ready.
    pub const READY: i64 = 2;
    /// A test is running.
    pub const TEST_IN_PROGRESS: i64 = 3;
    /// Refused for now.
    pub const TEMP_REJECT: i64 = 4;
}

/// `docsIf3CmStatusValue.<rf if>`: `CmRegState`. Placeholder arc under the sourced root.
pub fn cm_status_value() -> Oid {
    if3("1.1.1.1.1")
}
/// `docsIfDownChannelFrequency.<if>`. Sourced.
pub fn down_channel_frequency() -> Oid {
    docs_if("1.1.1.1.2")
}
/// `docsIfDownChannelPower.<if>`, tenth dBmV. Sourced.
pub fn down_channel_power() -> Oid {
    docs_if("1.1.1.1.6")
}
/// `docsIfSigQUnerroreds.<if>`. Sourced.
pub fn sigq_unerroreds() -> Oid {
    docs_if("1.1.4.1.2")
}
/// `docsIfSigQCorrecteds.<if>`. Sourced.
pub fn sigq_correcteds() -> Oid {
    docs_if("1.1.4.1.3")
}
/// `docsIfSigQUncorrectables.<if>`. Sourced.
pub fn sigq_uncorrectables() -> Oid {
    docs_if("1.1.4.1.4")
}
/// `docsIf3CmStatusUsTxPower.<us if>`, tenth dBmV total. Placeholder arc.
pub fn us_tx_power() -> Oid {
    if3("1.2.1.1.1")
}
/// `docsIf3CmStatusUsT3Timeouts.<us if>`. Placeholder arc.
pub fn us_t3() -> Oid {
    if3("1.2.1.1.2")
}
/// `docsIf3CmStatusUsT4Timeouts.<us if>`. Placeholder arc.
pub fn us_t4() -> Oid {
    if3("1.2.1.1.3")
}
/// `docsIf3CmStatusUsEqData.<us if>`. Placeholder arc.
pub fn us_eq_data() -> Oid {
    if3("1.2.1.1.4")
}
/// `docsIf3CmStatusUsRangingStatus.<us if>`. Placeholder arc.
pub fn us_ranging_status() -> Oid {
    if3("1.2.1.1.5")
}
/// `docsIf31CmDsOfdmChanSubcarrierZeroFreq.<if>`. Placeholder arc.
pub fn ofdm_sc0() -> Oid {
    if31("1.2.1.1.1")
}
/// `docsIf31CmDsOfdmProfileStatsTotalCodewords.<if>.<profile>`. Placeholder arc.
pub fn ofdm_profile_total() -> Oid {
    if31("1.2.2.1.1")
}
/// `docsIf31CmDsOfdmProfileStatsCorrectedCodewords.<if>.<profile>`. Placeholder arc.
pub fn ofdm_profile_corrected() -> Oid {
    if31("1.2.2.1.2")
}
/// `docsIf31CmDsOfdmProfileStatsUncorrectableCodewords.<if>.<profile>`. Placeholder arc.
pub fn ofdm_profile_uncorrectable() -> Oid {
    if31("1.2.2.1.3")
}
/// `docsIf31CmDsOfdmChannelPowerRxPower.<if>.<band>`, tenth dBmV. `band`'s centre is in the next object.
pub fn ofdm_band_power() -> Oid {
    if31("1.2.3.1.1")
}
/// `docsIf31CmDsOfdmChannelPowerCenterFrequency.<if>.<band>`, Hz.
pub fn ofdm_band_center() -> Oid {
    if31("1.2.3.1.2")
}
/// `docsIf31CmUsOfdmaChanTxPower.<us if>`, quarter dBmV per 1.6 MHz. Placeholder arc.
pub fn ofdma_tx_power() -> Oid {
    if31("1.3.1.1.1")
}
/// `docsIf31CmUsScQamChanTxPsd.<us if>`, quarter dBmV per 1.6 MHz [CM-OSSI §F.3.3.3.1.20]. Placeholder arc.
pub fn us_sc_tx_psd() -> Oid {
    if31("1.3.2.1.1")
}
/// `docsIf3CmtsCmRegStatusMacAddr.<id>`. Sourced.
pub fn cmts_cm_mac() -> Oid {
    if3("1.3.1.2")
}
/// `docsIf3CmtsCmRegStatusIPv4Addr.<id>`. Placeholder column of that sourced table.
pub fn cmts_cm_ip() -> Oid {
    if3("1.3.1.3")
}
/// Where the emulator puts each modem's SNMP port. Not a real object: a real CM is on UDP 161 at its own
/// address, and many emulated modems share one. Under a private arc so it can't be mistaken for one.
pub fn emulator_cm_port() -> Oid {
    Oid::parse("1.3.6.1.4.1.99999.1.1")
}
/// `docsIf3CmtsSignalQualityExtCNIR.<us if>`, tenth dB. Placeholder arc.
pub fn cmts_us_cnir() -> Oid {
    if3("1.4.1.1")
}
/// `docsIfSigQUnerroreds`, `Correcteds`, `Uncorrectables` on a CMTS upstream. Same objects as a modem's.
pub fn cmts_us_frequency() -> Oid {
    docs_if("1.1.2.1.2")
}
