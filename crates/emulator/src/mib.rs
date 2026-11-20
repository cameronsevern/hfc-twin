//! Observations as MIB objects and PNM files, in the units and encodings real devices use, so what crosses
//! the wire loses precision as a real reading does.

use std::collections::BTreeMap;
use std::net::SocketAddr;

use pnm_wire::oids;
use pnm_wire::oids::Test;
use pnm_wire::pnm::{self, Axis};
use pnm_wire::snmp::{Oid, Value};
use schema::common::ChannelId;
use schema::observation::{ChannelUsability, DsRxLevels, ModemObservation, RangingState, RegState};
use schema::profile::Profile;
use sim::rng::hash_str;
use sim::telemetry::StepObservation;

/// A locally administered MAC derived from an element id, standing in for what's on the modem's label.
pub fn mac_of(id: &str) -> [u8; 6] {
    let hash = hash_str(id).to_be_bytes();
    [0x02, hash[0], hash[1], hash[2], hash[3], hash[4]]
}

/// How channels map to ifIndexes, and back. A real modem's differ; an adapter matches by frequency.
pub struct IfIndexes;

impl IfIndexes {
    /// A downstream SC-QAM channel's.
    pub fn ds_sc(profile: &Profile, id: &ChannelId) -> Option<u32> {
        let at = profile
            .channel_plan
            .ds_sc_qam
            .iter()
            .position(|c| &c.channel_id == id)?;
        Some(1000 + at as u32)
    }
    /// An OFDM channel's.
    pub fn ofdm(profile: &Profile, id: &ChannelId) -> Option<u32> {
        let at = profile
            .channel_plan
            .ofdm
            .iter()
            .position(|c| &c.channel_id == id)?;
        Some(3000 + at as u32)
    }
    /// An upstream SC-QAM channel's.
    pub fn us_sc(profile: &Profile, id: &ChannelId) -> Option<u32> {
        let at = profile
            .channel_plan
            .us_sc_qam
            .iter()
            .position(|c| &c.channel_id == id)?;
        Some(2000 + at as u32)
    }
    /// An OFDMA channel's.
    pub fn ofdma(profile: &Profile, id: &ChannelId) -> Option<u32> {
        let at = profile
            .channel_plan
            .ofdma
            .iter()
            .position(|c| &c.channel_id == id)?;
        Some(4000 + at as u32)
    }
    /// Whichever an upstream channel has.
    pub fn us(profile: &Profile, id: &ChannelId) -> Option<u32> {
        Self::us_sc(profile, id).or_else(|| Self::ofdma(profile, id))
    }
    /// Whichever a downstream channel has.
    pub fn ds(profile: &Profile, id: &ChannelId) -> Option<u32> {
        Self::ds_sc(profile, id).or_else(|| Self::ofdm(profile, id))
    }
}

/// `CmRegState` [CM-OSSI §F.2.2].
pub fn reg_state_code(state: RegState) -> i64 {
    match state {
        RegState::NotSynchronized => 3,
        RegState::PhySynchronized => 4,
        RegState::RangingInProgress => 22,
        RegState::RangingComplete => 6,
        RegState::RegistrationInProgress => 18,
        RegState::Operational => 12,
    }
}

fn tenth(value: f64) -> i64 {
    (value * 10.0).round() as i64
}

fn quarter(value: f64) -> i64 {
    (value * 4.0).round() as i64
}

/// A modem's status objects. Counters as they accumulate.
pub fn modem_objects(
    modem: &ModemObservation,
    profile: &Profile,
    table: &mut BTreeMap<Oid, Value>,
) {
    table.insert(
        oids::cm_status_value().child(&[1]),
        Value::Integer(reg_state_code(modem.reg_state)),
    );

    // Channels the modem is on: state first, since an unregistered modem reports only state.
    for record in &modem.ds_channel_state {
        if let Some(if_index) = IfIndexes::ds(profile, &record.channel_id) {
            let usable = record.state == ChannelUsability::Usable;
            table.insert(
                oids::cm_status_value().child(&[2, if_index]),
                Value::Integer(if usable { 1 } else { 2 }),
            );
        }
    }
    for channel in &profile.channel_plan.ds_sc_qam {
        if let Some(if_index) = IfIndexes::ds_sc(profile, &channel.channel_id) {
            table.insert(
                oids::down_channel_frequency().child(&[if_index]),
                Value::Integer((channel.center_mhz * 1e6).round() as i64),
            );
        }
    }
    for record in &modem.ds_rx {
        match &record.ds_rx_dbmv {
            DsRxLevels::ScQam { level_dbmv } => {
                if let Some(if_index) = IfIndexes::ds_sc(profile, &record.channel_id) {
                    table.insert(
                        oids::down_channel_power().child(&[if_index]),
                        Value::Integer(tenth(*level_dbmv)),
                    );
                }
            }
            DsRxLevels::Ofdm {
                bands,
                plc_band_dbmv,
            } => {
                if let Some(if_index) = IfIndexes::ofdm(profile, &record.channel_id) {
                    // [CM-OSSI §F.3.2.3.2]: band 0 is the PLC band.
                    table.insert(
                        oids::ofdm_band_power().child(&[if_index, 0]),
                        Value::Integer(tenth(*plc_band_dbmv)),
                    );
                    for (i, band) in bands.iter().enumerate() {
                        let n = i as u32 + 1;
                        table.insert(
                            oids::ofdm_band_power().child(&[if_index, n]),
                            Value::Integer(tenth(band.level_dbmv)),
                        );
                        table.insert(
                            oids::ofdm_band_center().child(&[if_index, n]),
                            Value::Integer((band.center_mhz * 1e6).round() as i64),
                        );
                    }
                }
            }
        }
    }
    for record in &modem.codewords {
        match record.data_profile_id {
            None => {
                if let Some(if_index) = IfIndexes::ds_sc(profile, &record.channel_id) {
                    table.insert(
                        oids::sigq_unerroreds().child(&[if_index]),
                        Value::Counter64(record.total - record.corrected - record.uncorrectable),
                    );
                    table.insert(
                        oids::sigq_correcteds().child(&[if_index]),
                        Value::Counter64(record.corrected),
                    );
                    table.insert(
                        oids::sigq_uncorrectables().child(&[if_index]),
                        Value::Counter64(record.uncorrectable),
                    );
                }
            }
            Some(id) => {
                if let Some(if_index) = IfIndexes::ofdm(profile, &record.channel_id) {
                    let key = [if_index, u32::from(id)];
                    table.insert(
                        oids::ofdm_profile_total().child(&key),
                        Value::Counter64(record.total),
                    );
                    table.insert(
                        oids::ofdm_profile_corrected().child(&key),
                        Value::Counter64(record.corrected),
                    );
                    table.insert(
                        oids::ofdm_profile_uncorrectable().child(&key),
                        Value::Counter64(record.uncorrectable),
                    );
                }
            }
        }
    }
    for record in &modem.us_tx {
        if let Some(if_index) = IfIndexes::us_sc(profile, &record.channel_id)
            && let Some(total) = record.total_dbmv
        {
            table.insert(
                oids::us_tx_power().child(&[if_index]),
                Value::Integer(tenth(total)),
            );
            table.insert(
                oids::us_sc_tx_psd().child(&[if_index]),
                Value::Integer(quarter(record.psd_dbmv_per_1p6mhz)),
            );
        }
        if let Some(if_index) = IfIndexes::ofdma(profile, &record.channel_id) {
            table.insert(
                oids::ofdma_tx_power().child(&[if_index]),
                Value::Integer(quarter(record.psd_dbmv_per_1p6mhz)),
            );
        }
    }
    for record in &modem.t3_timeouts {
        if let Some(if_index) = IfIndexes::us(profile, &record.channel_id) {
            table.insert(
                oids::us_t3().child(&[if_index]),
                Value::Counter32(record.count as u32),
            );
        }
    }
    for record in &modem.t4_timeouts {
        if let Some(if_index) = IfIndexes::us(profile, &record.channel_id) {
            table.insert(
                oids::us_t4().child(&[if_index]),
                Value::Counter32(record.count as u32),
            );
        }
    }
    for record in &modem.ranging_status {
        if let Some(if_index) = IfIndexes::us(profile, &record.channel_id) {
            let code = match record.status {
                RangingState::Aborted => 2,
                RangingState::RetriesExceeded => 3,
                RangingState::Success => 4,
                RangingState::Continue => 5,
            };
            table.insert(
                oids::us_ranging_status().child(&[if_index]),
                Value::Integer(code),
            );
        }
    }
    for record in modem.preeq.sc_qam.iter().flatten() {
        if let Some(if_index) = IfIndexes::us_sc(profile, &record.channel_id) {
            let taps: Vec<(f64, f64)> = record.taps.iter().map(|t| (t.re, t.im)).collect();
            table.insert(
                oids::us_eq_data().child(&[if_index]),
                Value::Octets(pnm::encode_equalizer(record.main_tap_index, &taps)),
            );
        }
    }
    // The OFDM channels a modem can run tests on.
    for record in modem.rxmer.iter().flatten() {
        if let Some(if_index) = IfIndexes::ofdm(profile, &record.channel_id) {
            table.insert(
                oids::ofdm_sc0().child(&[if_index]),
                Value::Integer((record.axis.zero_freq_mhz * 1e6).round() as i64),
            );
        }
    }
}

fn axis_of(axis: schema::observation::SubcarrierAxis) -> Axis {
    Axis {
        subcarrier_zero_hz: (axis.zero_freq_mhz * 1e6).round() as u32,
        first_active: axis.first_active_index as u16,
        spacing_khz: axis.spacing_khz as u8,
    }
}

/// The file a test writes.
pub fn build_file(
    test: Test,
    if_index: u32,
    modem: &ModemObservation,
    profile: &Profile,
    mac: [u8; 6],
) -> Option<Vec<u8>> {
    let capture = modem.t.0.max(0) as u32;
    match test {
        Test::DsRxmer => {
            let record = modem
                .rxmer
                .as_ref()?
                .iter()
                .find(|r| IfIndexes::ofdm(profile, &r.channel_id) == Some(if_index))?;
            let values = record.rxmer_db.as_ref()?;
            Some(pnm::encode_rxmer(
                capture,
                (if_index - 3000 + 1) as u8,
                mac,
                axis_of(record.axis),
                values,
            ))
        }
        Test::ChanEst => {
            let record = modem
                .ds_chan_est
                .as_ref()?
                .iter()
                .find(|r| IfIndexes::ofdm(profile, &r.channel_id) == Some(if_index))?;
            let coefficients: Vec<Option<(f64, f64)>> = record
                .coefficients
                .as_ref()?
                .iter()
                .map(|c| c.map(|c| (c.re, c.im)))
                .collect();
            Some(pnm::encode_chan_est(
                capture,
                (if_index - 3000 + 1) as u8,
                mac,
                axis_of(record.axis),
                &coefficients,
            ))
        }
        Test::UsPreeq => {
            let record = modem
                .preeq
                .ofdma
                .as_ref()?
                .iter()
                .find(|r| IfIndexes::ofdma(profile, &r.channel_id) == Some(if_index))?;
            let coefficients: Vec<(f64, f64)> = record
                .coefficients
                .as_ref()?
                .iter()
                .map(|c| (c.re, c.im))
                .collect();
            Some(pnm::encode_us_preeq(
                capture,
                (if_index - 4000 + 1) as u8,
                mac,
                [0x02, 0, 0, 0, 0, 1],
                axis_of(record.axis),
                &coefficients,
            ))
        }
        Test::Spectrum => {
            let fbc = modem.fbc.as_ref()?;
            let first = (fbc.first_bin_center_mhz * 1e6).round() as u32;
            let width_hz = (fbc.bin_width_khz * 1000.0).round() as u32;
            let last = first + width_hz * (fbc.fbc_dbmv.len().saturating_sub(1)) as u32;
            let enb = (fbc.noise_bandwidth_khz / fbc.bin_width_khz * 100.0).round() as u16;
            Some(pnm::encode_spectrum(
                capture,
                mac,
                first,
                last,
                last - first,
                enb,
                3,
                &fbc.fbc_dbmv,
            ))
        }
    }
}

/// The CMTS's view: upstream channel quality, and per-modem receive power and RxMER.
pub fn cmts_objects(
    step: &StepObservation,
    profile: &Profile,
    directory: &[([u8; 6], SocketAddr)],
    table: &mut BTreeMap<Oid, Value>,
) {
    for channel in &profile.channel_plan.us_sc_qam {
        if let Some(if_index) = IfIndexes::us_sc(profile, &channel.channel_id) {
            table.insert(
                oids::cmts_us_frequency().child(&[if_index]),
                Value::Integer((channel.center_mhz * 1e6).round() as i64),
            );
        }
    }
    for channel in &profile.channel_plan.ofdma {
        if let Some(if_index) = IfIndexes::ofdma(profile, &channel.channel_id) {
            let center = (channel.first_active_mhz + channel.last_active_mhz) / 2.0;
            table.insert(
                oids::cmts_us_frequency().child(&[if_index]),
                Value::Integer((center * 1e6).round() as i64),
            );
        }
    }
    for record in &step.service_group.us_snr {
        if let Some(if_index) = IfIndexes::us(profile, &record.channel_id) {
            table.insert(
                oids::cmts_us_cnir().child(&[if_index]),
                Value::Integer(tenth(record.us_snr_db)),
            );
        }
    }
    for record in &step.service_group.us_codewords {
        if let Some(if_index) = IfIndexes::us(profile, &record.channel_id) {
            table.insert(
                oids::sigq_unerroreds().child(&[if_index]),
                Value::Counter64(record.total - record.corrected - record.uncorrectable),
            );
            table.insert(
                oids::sigq_correcteds().child(&[if_index]),
                Value::Counter64(record.corrected),
            );
            table.insert(
                oids::sigq_uncorrectables().child(&[if_index]),
                Value::Counter64(record.uncorrectable),
            );
        }
    }
    // `docsIf3CmtsCmUsStatusRxPower`, by CM index and channel, and `docsIf31CmtsCmUsOfdmaChannelStatus`'s mean RxMER.
    for modem in &step.modems {
        let mac = mac_of(modem.element_id.as_str());
        let Some(index) = directory.iter().position(|(m, _)| *m == mac) else {
            continue;
        };
        let index = index as u32 + 1;
        for record in &modem.us_rx {
            if let Some(if_index) = IfIndexes::us(profile, &record.channel_id) {
                table.insert(
                    oids::cmts_us_cnir().child(&[0, index, if_index]),
                    Value::Integer(tenth(record.us_rx_dbmv_per_1p6mhz)),
                );
            }
        }
        for record in modem.us_rxmer.iter().flatten() {
            if let (Some(if_index), Some(mean)) = (
                IfIndexes::ofdma(profile, &record.channel_id),
                record.us_rxmer_mean_db,
            ) {
                table.insert(
                    oids::cmts_us_cnir().child(&[1, index, if_index]),
                    Value::Integer((mean * 100.0).round() as i64),
                );
            }
        }
    }
}
