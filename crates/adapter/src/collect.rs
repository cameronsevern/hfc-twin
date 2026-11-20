//! Collection: discover modems through the CMTS, run their tests, take their files, and map it all into the
//! observation schema. Adapters map vendor data *into* the sim's schema (invariant 2), so a field this path can't fill
//! is `None`, as it would be from a real device.

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};

use pnm_wire::oids;
use pnm_wire::oids::{Test, ctl_status, meas_status};
use pnm_wire::pnm;
use pnm_wire::snmp::{Oid, Value};
use pnm_wire::tftp::Server;
use schema::capability::effective;
use schema::common::{ChannelId, Complex, ElementId, Origin, Timestamp};
use schema::component::{ComponentLibrary, ComponentSpec, ModemGeneration};
use schema::observation as obs;
use schema::plant::Plant;
use schema::profile::Profile;
use sim::preeq;
use sim::telemetry::StepObservation;
use sim::units::C64;

use crate::client::{SnmpClient, WireError};
use crate::identity::{AddressStore, pseudonym};

/// Limits on how hard collection may lean on devices (`docs/DESIGN.md`, "Polling is twin-driven"). One test and one
/// upload per modem is the device's own rule; the rest protects the CMTS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    /// Modems collected at once per CMTS.
    pub max_concurrent_modems: usize,
    /// How long to wait for a test.
    pub test_timeout: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            max_concurrent_modems: 4,
            test_timeout: Duration::from_secs(10),
        }
    }
}

/// One plant's collector: an adapter for one CMTS, with its TFTP endpoint.
pub struct Collector<'a> {
    plant: &'a Plant,
    profile: &'a Profile,
    library: &'a ComponentLibrary,
    key: Vec<u8>,
    community: Vec<u8>,
    budget: Budget,
    /// Where uploads land.
    pub tftp: Server,
    /// What it takes to reach each modem. Stays in this crate (invariant 10).
    pub addresses: AddressStore,
    cmts: SocketAddr,
    by_pseudonym: BTreeMap<String, ElementId>,
}

fn as_int(value: Option<Value>) -> Option<i64> {
    match value? {
        Value::Integer(v) => Some(v),
        Value::Counter32(v) | Value::Gauge32(v) => Some(i64::from(v)),
        Value::Counter64(v) => Some(v as i64),
        _ => None,
    }
}

fn last_arc(oid: &Oid) -> Option<u32> {
    oid.0.last().copied()
}

fn reg_state(code: i64) -> obs::RegState {
    // [CM-OSSI §F.2.2] collapsed to what §7.4 moves through: the adapter's job (invariant 2).
    match code {
        12 => obs::RegState::Operational,
        18 | 11 | 7 | 8 | 9 | 10 | 14 | 15 | 16 | 17 => obs::RegState::RegistrationInProgress,
        6 => obs::RegState::RangingComplete,
        5 | 21 | 22 => obs::RegState::RangingInProgress,
        4 => obs::RegState::PhySynchronized,
        _ => obs::RegState::NotSynchronized,
    }
}

impl<'a> Collector<'a> {
    /// Starts a TFTP endpoint and reads the CMTS's table of modems. MACs are hashed here, on arrival.
    pub fn connect(
        plant: &'a Plant,
        profile: &'a Profile,
        library: &'a ComponentLibrary,
        cmts: SocketAddr,
        key: &[u8],
        tftp_port: u16,
        budget: Budget,
    ) -> Result<Collector<'a>, WireError> {
        let tftp = Server::bind(&format!("127.0.0.1:{tftp_port}"))?;
        let mut collector = Collector {
            plant,
            profile,
            library,
            key: key.to_vec(),
            community: b"public".to_vec(),
            budget,
            tftp,
            addresses: AddressStore::default(),
            cmts,
            by_pseudonym: plant
                .modem_bindings
                .iter()
                .map(|b| (b.pseudonym.clone(), b.element_id.clone()))
                .collect(),
        };
        collector.discover()?;
        Ok(collector)
    }

    fn discover(&mut self) -> Result<(), WireError> {
        let mut client = SnmpClient::new(self.cmts, &self.community)?;
        let macs = client.walk(&oids::cmts_cm_mac())?;
        for (oid, value) in macs {
            let (Some(index), Value::Octets(mac)) = (last_arc(&oid), value) else {
                continue;
            };
            let mac: [u8; 6] = mac
                .try_into()
                .map_err(|_| WireError::Unexpected("MAC length".into()))?;
            let port = as_int(client.get(&oids::emulator_cm_port().child(&[index]))?);
            let ip = client.get(&oids::cmts_cm_ip().child(&[index]))?;
            let octets = match ip {
                Some(Value::IpAddress(octets)) => octets,
                _ => return Err(WireError::Unexpected("no IP for a modem".into())),
            };
            // A real CM is on UDP 161. Only the emulator has a port column.
            let addr = SocketAddr::from((octets, port.unwrap_or(161) as u16));
            self.addresses.insert(pseudonym(&self.key, &mac), addr, mac);
        }
        Ok(())
    }

    /// The data's origin, from what the devices say they are. Never asserted, and never `Field` for the emulator.
    pub fn origin(&self) -> Result<Origin, WireError> {
        let mut client = SnmpClient::new(self.cmts, &self.community)?;
        let descr = client.get(&oids::sys_descr())?;
        Ok(match descr {
            Some(Value::Octets(text)) if String::from_utf8_lossy(&text).contains("simulated") => {
                Origin::Synthetic
            }
            _ if self.plant.origin == Origin::Synthetic => Origin::Synthetic,
            _ => Origin::Field,
        })
    }

    fn run_test(
        &self,
        client: &mut SnmpClient,
        test: Test,
        if_index: u32,
    ) -> Result<Vec<u8>, WireError> {
        let deadline = Instant::now() + self.budget.test_timeout;
        // One covered test at a time [CM-OSSI §D.2.3]: wait for ready.
        loop {
            match as_int(client.get(&oids::cm_ctl_status())?) {
                Some(ctl_status::READY) => break,
                _ if Instant::now() > deadline => {
                    return Err(WireError::Test("modem never ready".into()));
                }
                _ => thread::sleep(Duration::from_millis(5)),
            }
        }
        client.set(&test.enable(if_index), Value::Integer(1))?;
        loop {
            match as_int(client.get(&test.status(if_index))?) {
                Some(meas_status::SAMPLE_READY) => break,
                Some(meas_status::ERROR) => {
                    return Err(WireError::Test(format!("{test:?} reports error")));
                }
                _ if Instant::now() > deadline => {
                    return Err(WireError::Test(format!("{test:?} timed out")));
                }
                _ => thread::sleep(Duration::from_millis(5)),
            }
        }
        let Some(Value::Octets(name)) = client.get(&test.file_name(if_index))? else {
            return Err(WireError::Test("no file name".into()));
        };
        let name = String::from_utf8_lossy(&name).to_string();
        // The application reruns a test whose file never arrives [PNM-HFC §8.4.3].
        loop {
            if let Some(bytes) = self.tftp.take(&name) {
                return Ok(bytes);
            }
            if Instant::now() > deadline {
                return Err(WireError::Test(format!("{name} never arrived")));
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn channel_by_ds_frequency(&self, hz: i64) -> Option<ChannelId> {
        let mhz = hz as f64 / 1e6;
        self.profile
            .channel_plan
            .ds_sc_qam
            .iter()
            .find(|c| (c.center_mhz - mhz).abs() < 0.01)
            .map(|c| c.channel_id.clone())
    }

    /// Matches an OFDM channel by subcarrier zero: `first active = SC0 + index × spacing`.
    fn channel_by_axis(&self, axis: pnm::Axis) -> Option<ChannelId> {
        let first = f64::from(axis.subcarrier_zero_hz) / 1e6
            + f64::from(axis.first_active) * f64::from(axis.spacing_khz) / 1000.0;
        self.profile
            .channel_plan
            .ofdm
            .iter()
            .find(|c| (c.first_active_mhz - first).abs() < 0.01)
            .map(|c| c.channel_id.clone())
    }

    fn ofdma_by_axis(&self, axis: pnm::Axis) -> Option<ChannelId> {
        let first = f64::from(axis.subcarrier_zero_hz) / 1e6
            + f64::from(axis.first_active) * f64::from(axis.spacing_khz) / 1000.0;
        self.profile
            .channel_plan
            .ofdma
            .iter()
            .find(|c| (c.first_active_mhz - first).abs() < 0.01)
            .map(|c| c.channel_id.clone())
    }

    /// One modem, as the schema has it. `addr` comes from the store; the MAC never leaves it.
    pub fn collect_modem(
        &self,
        element_id: &ElementId,
        addr: SocketAddr,
        t: Timestamp,
        origin: Origin,
    ) -> Result<obs::ModemObservation, WireError> {
        let element = self
            .plant
            .element(element_id)
            .ok_or_else(|| WireError::Unexpected(format!("no {element_id}")))?;
        let component = self
            .library
            .get(&element.component_ref)
            .ok_or_else(|| WireError::Unexpected("no component".into()))?;
        let ComponentSpec::Modem(spec) = &component.spec else {
            return Err(WireError::Unexpected("not a modem".into()));
        };
        let capability = effective(&self.profile.capability, &spec.capability);
        let profile_ref = self.profile.id.clone();
        let mut client = SnmpClient::new(addr, &self.community)?;

        let reg_code = as_int(client.get(&oids::cm_status_value().child(&[1]))?)
            .ok_or_else(|| WireError::Unexpected("no registration state".into()))?;
        let reg_state = reg_state(reg_code);
        let operational = reg_state == obs::RegState::Operational;

        // Where uploads go.
        let tftp_ip = match self.tftp.addr {
            SocketAddr::V4(v4) => v4.ip().octets().to_vec(),
            SocketAddr::V6(_) => return Err(WireError::Unexpected("IPv6 endpoint".into())),
        };
        client.set(&oids::bulk_dest_ip_type(), Value::Integer(1))?;
        client.set(&oids::bulk_dest_ip_addr(), Value::Octets(tftp_ip))?;
        client.set(&oids::bulk_dest_path(), Value::Octets(Vec::new()))?;

        let mut rxmer = Vec::new();
        let mut chan_est = Vec::new();
        let mut preeq_ofdma = Vec::new();
        let mut us_rxmer_out = Vec::new();
        let mut fbc = None;
        let ofdm_ifs: Vec<u32> = client
            .walk(&oids::ofdm_sc0())?
            .iter()
            .filter_map(|(oid, _)| last_arc(oid))
            .collect();

        if operational && spec.generation == ModemGeneration::D31 {
            for &if_index in &ofdm_ifs {
                if capability.ds_rxmer_file {
                    let file =
                        pnm::decode_rxmer(&self.run_test(&mut client, Test::DsRxmer, if_index)?)?;
                    let channel_id = self
                        .channel_by_axis(file.axis)
                        .ok_or_else(|| WireError::Unexpected("OFDM channel not in plan".into()))?;
                    let values: Vec<f64> = file.data.iter().flatten().copied().collect();
                    let (mean, std, p2) = preeq::rxmer_summary(&values).unwrap_or((0.0, 0.0, 0.0));
                    rxmer.push(obs::DsRxmerRecord {
                        channel_id,
                        profile_ref: profile_ref.clone(),
                        axis: axis_back(file.axis),
                        rxmer_db: Some(file.data),
                        rxmer_mean_db: Some(mean),
                        rxmer_stddev_db: Some(std),
                        rxmer_p2_db: Some(p2),
                    });
                }
                if capability.ds_chan_est_file {
                    let file = pnm::decode_chan_est(&self.run_test(
                        &mut client,
                        Test::ChanEst,
                        if_index,
                    )?)?;
                    let channel_id = self
                        .channel_by_axis(file.axis)
                        .ok_or_else(|| WireError::Unexpected("OFDM channel not in plan".into()))?;
                    let axis = axis_back(file.axis);
                    let response: Vec<C64> = file
                        .data
                        .iter()
                        .map(|c| {
                            let (re, im) = c.unwrap_or((0.0, 0.0));
                            C64 { re, im }
                        })
                        .collect();
                    let f: Vec<f64> = (0..response.len())
                        .map(|i| {
                            axis.zero_freq_mhz
                                + (axis.first_active_index as usize + i) as f64 * axis.spacing_khz
                                    / 1000.0
                        })
                        .collect();
                    let summary = preeq::response_summary(&response, &f, axis.spacing_khz * 1000.0);
                    chan_est.push(obs::DsChanEstRecord {
                        channel_id,
                        profile_ref: profile_ref.clone(),
                        axis,
                        coefficients: Some(
                            file.data
                                .iter()
                                .map(|c| c.map(|(re, im)| Complex { re, im }))
                                .collect(),
                        ),
                        summary: Some(summary),
                    });
                }
            }
            for k in 0..self.profile.channel_plan.ofdma.len() {
                let if_index = 4000 + k as u32;
                if capability.ofdma_preeq_file {
                    let file = pnm::decode_us_preeq(&self.run_test(
                        &mut client,
                        Test::UsPreeq,
                        if_index,
                    )?)?;
                    let channel_id = self
                        .ofdma_by_axis(file.axis)
                        .ok_or_else(|| WireError::Unexpected("OFDMA channel not in plan".into()))?;
                    let axis = axis_back(file.axis);
                    let response: Vec<C64> = file
                        .data
                        .iter()
                        .map(|(re, im)| C64 { re: *re, im: *im })
                        .collect();
                    let f: Vec<f64> = (0..response.len())
                        .map(|i| {
                            axis.zero_freq_mhz
                                + (axis.first_active_index as usize + i) as f64 * axis.spacing_khz
                                    / 1000.0
                        })
                        .collect();
                    let summary = preeq::response_summary(&response, &f, axis.spacing_khz * 1000.0);
                    preeq_ofdma.push(obs::OfdmaPreeqRecord {
                        channel_id,
                        profile_ref: profile_ref.clone(),
                        axis,
                        coefficients: Some(
                            file.data
                                .iter()
                                .map(|(re, im)| Complex { re: *re, im: *im })
                                .collect(),
                        ),
                        summary: Some(summary),
                    });
                }
            }
            if capability.fbc.is_some_and(|f| f.file) {
                let file = pnm::decode_spectrum(&self.run_test(&mut client, Test::Spectrum, 0)?)?;
                let bins = file.bins_dbmv.len().max(2) as f64;
                let width_khz =
                    f64::from(file.last_center_hz - file.first_center_hz) / (bins - 1.0) / 1000.0;
                fbc = Some(obs::FbcRecord {
                    profile_ref: profile_ref.clone(),
                    first_bin_center_mhz: f64::from(file.first_center_hz) / 1e6,
                    bin_width_khz: width_khz,
                    noise_bandwidth_khz: width_khz * f64::from(file.enb) / 100.0,
                    fbc_dbmv: file.bins_dbmv,
                });
            }
        }

        // Status objects, by walking.
        let mut ds_rx = Vec::new();
        let frequency_of: BTreeMap<u32, i64> = client
            .walk(&oids::down_channel_frequency())?
            .into_iter()
            .filter_map(|(oid, value)| Some((last_arc(&oid)?, as_int(Some(value))?)))
            .collect();
        let mut sc_id_of: BTreeMap<u32, ChannelId> = BTreeMap::new();
        for (if_index, hz) in &frequency_of {
            if let Some(id) = self.channel_by_ds_frequency(*hz) {
                sc_id_of.insert(*if_index, id);
            }
        }
        for (oid, value) in client.walk(&oids::down_channel_power())? {
            let (Some(if_index), Some(tenths)) = (last_arc(&oid), as_int(Some(value))) else {
                continue;
            };
            if let Some(channel_id) = sc_id_of.get(&if_index) {
                ds_rx.push(obs::DsRxRecord {
                    channel_id: channel_id.clone(),
                    profile_ref: profile_ref.clone(),
                    ds_rx_dbmv: obs::DsRxLevels::ScQam {
                        level_dbmv: tenths as f64 / 10.0,
                    },
                });
            }
        }
        // OFDM band power, grouped by channel.
        let mut band_power: BTreeMap<(u32, u32), f64> = BTreeMap::new();
        let mut band_center: BTreeMap<(u32, u32), f64> = BTreeMap::new();
        for (oid, value) in client.walk(&oids::ofdm_band_power())? {
            let arcs = &oid.0;
            if let (Some(band), Some(if_index), Some(tenths)) = (
                arcs.last(),
                arcs.get(arcs.len().wrapping_sub(2)),
                as_int(Some(value)),
            ) {
                band_power.insert((*if_index, *band), tenths as f64 / 10.0);
            }
        }
        for (oid, value) in client.walk(&oids::ofdm_band_center())? {
            let arcs = &oid.0;
            if let (Some(band), Some(if_index), Some(hz)) = (
                arcs.last(),
                arcs.get(arcs.len().wrapping_sub(2)),
                as_int(Some(value)),
            ) {
                band_center.insert((*if_index, *band), hz as f64 / 1e6);
            }
        }
        let plan_ofdm = &self.profile.channel_plan.ofdm;
        for (k, channel) in plan_ofdm.iter().enumerate() {
            let if_index = 3000 + k as u32;
            if let Some(plc) = band_power.get(&(if_index, 0)) {
                let bands = band_power
                    .iter()
                    .filter(|((i, b), _)| *i == if_index && *b > 0)
                    .filter_map(|((i, b), level)| {
                        Some(obs::DsBandLevel {
                            center_mhz: *band_center.get(&(*i, *b))?,
                            level_dbmv: *level,
                        })
                    })
                    .collect();
                ds_rx.push(obs::DsRxRecord {
                    channel_id: channel.channel_id.clone(),
                    profile_ref: profile_ref.clone(),
                    ds_rx_dbmv: obs::DsRxLevels::Ofdm {
                        bands,
                        plc_band_dbmv: *plc,
                    },
                });
            }
        }

        let mut codewords = Vec::new();
        let corrected: BTreeMap<u32, u64> = client
            .walk(&oids::sigq_correcteds())?
            .into_iter()
            .filter_map(|(oid, v)| Some((last_arc(&oid)?, as_int(Some(v))? as u64)))
            .collect();
        let uncorrectable: BTreeMap<u32, u64> = client
            .walk(&oids::sigq_uncorrectables())?
            .into_iter()
            .filter_map(|(oid, v)| Some((last_arc(&oid)?, as_int(Some(v))? as u64)))
            .collect();
        for (oid, value) in client.walk(&oids::sigq_unerroreds())? {
            let (Some(if_index), Some(unerrored)) = (last_arc(&oid), as_int(Some(value))) else {
                continue;
            };
            if let Some(channel_id) = sc_id_of.get(&if_index) {
                let c = corrected.get(&if_index).copied().unwrap_or(0);
                let u = uncorrectable.get(&if_index).copied().unwrap_or(0);
                codewords.push(obs::CodewordRecord {
                    channel_id: channel_id.clone(),
                    profile_ref: profile_ref.clone(),
                    data_profile_id: None,
                    total: unerrored as u64 + c + u,
                    corrected: c,
                    uncorrectable: u,
                });
            }
        }
        let profile_key = |oid: &Oid| -> Option<(u32, u32)> {
            let arcs = &oid.0;
            Some((*arcs.get(arcs.len().wrapping_sub(2))?, *arcs.last()?))
        };
        let c_ofdm: BTreeMap<(u32, u32), u64> = client
            .walk(&oids::ofdm_profile_corrected())?
            .into_iter()
            .filter_map(|(oid, v)| Some((profile_key(&oid)?, as_int(Some(v))? as u64)))
            .collect();
        let u_ofdm: BTreeMap<(u32, u32), u64> = client
            .walk(&oids::ofdm_profile_uncorrectable())?
            .into_iter()
            .filter_map(|(oid, v)| Some((profile_key(&oid)?, as_int(Some(v))? as u64)))
            .collect();
        for (oid, value) in client.walk(&oids::ofdm_profile_total())? {
            let (Some(key), Some(total)) = (profile_key(&oid), as_int(Some(value))) else {
                continue;
            };
            if let Some(channel) = plan_ofdm.get(key.0.wrapping_sub(3000) as usize) {
                codewords.push(obs::CodewordRecord {
                    channel_id: channel.channel_id.clone(),
                    profile_ref: profile_ref.clone(),
                    data_profile_id: Some(key.1 as u8),
                    total: total as u64,
                    corrected: c_ofdm.get(&key).copied().unwrap_or(0),
                    uncorrectable: u_ofdm.get(&key).copied().unwrap_or(0),
                });
            }
        }

        // Upstream.
        let us_id = |if_index: u32| -> Option<ChannelId> {
            let plan = &self.profile.channel_plan;
            if (2000..3000).contains(&if_index) {
                plan.us_sc_qam
                    .get(if_index as usize - 2000)
                    .map(|c| c.channel_id.clone())
            } else {
                plan.ofdma
                    .get(if_index as usize - 4000)
                    .map(|c| c.channel_id.clone())
            }
        };
        let per_1p6 = sim::upstream::per_1p6_from_per_6p4_db();
        let mut us_tx = Vec::new();
        let sc_psd: BTreeMap<u32, f64> = client
            .walk(&oids::us_sc_tx_psd())?
            .into_iter()
            .filter_map(|(oid, v)| Some((last_arc(&oid)?, as_int(Some(v))? as f64 / 4.0)))
            .collect();
        for (oid, value) in client.walk(&oids::us_tx_power())? {
            if let (Some(if_index), Some(tenths)) = (last_arc(&oid), as_int(Some(value)))
                && let Some(channel_id) = us_id(if_index)
            {
                let total = tenths as f64 / 10.0;
                us_tx.push(obs::UsTxRecord {
                    channel_id,
                    profile_ref: profile_ref.clone(),
                    psd_dbmv_per_1p6mhz: sc_psd.get(&if_index).copied().unwrap_or(total - per_1p6),
                    total_dbmv: Some(total),
                });
            }
        }
        for (oid, value) in client.walk(&oids::ofdma_tx_power())? {
            if let (Some(if_index), Some(quarters)) = (last_arc(&oid), as_int(Some(value)))
                && let Some(channel_id) = us_id(if_index)
            {
                us_tx.push(obs::UsTxRecord {
                    channel_id,
                    profile_ref: profile_ref.clone(),
                    psd_dbmv_per_1p6mhz: quarters as f64 / 4.0,
                    total_dbmv: None,
                });
            }
        }
        let counts =
            |client: &mut SnmpClient, root: Oid| -> Result<Vec<obs::TimeoutRecord>, WireError> {
                Ok(client
                    .walk(&root)?
                    .into_iter()
                    .filter_map(|(oid, v)| {
                        Some(obs::TimeoutRecord {
                            channel_id: us_id(last_arc(&oid)?)?,
                            profile_ref: profile_ref.clone(),
                            count: as_int(Some(v))? as u64,
                        })
                    })
                    .collect())
            };
        // Canonical order is by channel id, not by whatever ifIndex a device gave it.
        let mut t3_timeouts = counts(&mut client, oids::us_t3())?;
        t3_timeouts.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
        let mut t4_timeouts = counts(&mut client, oids::us_t4())?;
        t4_timeouts.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
        let mut ranging_status = Vec::new();
        let mut us_channel_state = Vec::new();
        for (oid, value) in client.walk(&oids::us_ranging_status())? {
            if let (Some(if_index), Some(code)) = (last_arc(&oid), as_int(Some(value)))
                && let Some(channel_id) = us_id(if_index)
            {
                let status = match code {
                    2 => obs::RangingState::Aborted,
                    3 => obs::RangingState::RetriesExceeded,
                    5 => obs::RangingState::Continue,
                    _ => obs::RangingState::Success,
                };
                us_channel_state.push(obs::ChannelStateRecord {
                    channel_id: channel_id.clone(),
                    profile_ref: profile_ref.clone(),
                    state: if status == obs::RangingState::RetriesExceeded {
                        obs::ChannelUsability::Unusable
                    } else {
                        obs::ChannelUsability::Usable
                    },
                });
                ranging_status.push(obs::RangingStatusRecord {
                    channel_id,
                    profile_ref: profile_ref.clone(),
                    status,
                });
            }
        }
        ranging_status.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
        us_channel_state.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
        let mut sc_preeq = Vec::new();
        for (oid, value) in client.walk(&oids::us_eq_data())? {
            let (Some(if_index), Value::Octets(bytes)) = (last_arc(&oid), value) else {
                continue;
            };
            if let Some(channel_id) = us_id(if_index) {
                let (main, taps) = pnm::decode_equalizer(&bytes)?;
                let complex: Vec<C64> = taps
                    .iter()
                    .map(|(re, im)| C64 { re: *re, im: *im })
                    .collect();
                // MTC, MTR and NMTER aren't MIB objects: the application computes them [PNM-HFC §6.2.6.4.2].
                let (mtc_db, mtr_db, nmter_db) = preeq::tap_metrics(&complex, main as usize);
                sc_preeq.push(obs::ScQamPreeqRecord {
                    channel_id,
                    profile_ref: profile_ref.clone(),
                    taps: taps
                        .iter()
                        .map(|(re, im)| Complex { re: *re, im: *im })
                        .collect(),
                    main_tap_index: main,
                    mtc_db,
                    mtr_db,
                    nmter_db,
                });
            }
        }

        // CMTS's side of this modem.
        let mut cmts = SnmpClient::new(self.cmts, &self.community)?;
        let index = {
            let mut found = None;
            let macs = cmts.walk(&oids::cmts_cm_mac())?;
            for (oid, value) in macs {
                if let (Some(i), Value::Octets(mac)) = (last_arc(&oid), value) {
                    let mac: [u8; 6] = mac
                        .try_into()
                        .map_err(|_| WireError::Unexpected("MAC length".into()))?;
                    let this = pseudonym(&self.key, &mac);
                    if self.by_pseudonym.get(&this) == Some(element_id) {
                        found = Some(i);
                    }
                }
            }
            found
        };
        let mut us_rx = Vec::new();
        if let Some(index) = index {
            for (oid, value) in cmts.walk(&oids::cmts_us_cnir().child(&[0, index]))? {
                if let (Some(if_index), Some(tenths)) = (last_arc(&oid), as_int(Some(value)))
                    && let Some(channel_id) = us_id(if_index)
                {
                    us_rx.push(obs::UsRxRecord {
                        channel_id,
                        profile_ref: profile_ref.clone(),
                        us_rx_dbmv_per_1p6mhz: tenths as f64 / 10.0,
                    });
                }
            }
            for (oid, value) in cmts.walk(&oids::cmts_us_cnir().child(&[1, index]))? {
                if let (Some(if_index), Some(hundredths)) = (last_arc(&oid), as_int(Some(value)))
                    && let Some(channel) = self
                        .profile
                        .channel_plan
                        .ofdma
                        .get(if_index as usize - 4000)
                {
                    us_rxmer_out.push(obs::UsRxmerRecord {
                        channel_id: channel.channel_id.clone(),
                        profile_ref: profile_ref.clone(),
                        axis: obs::SubcarrierAxis {
                            zero_freq_mhz: channel.first_active_mhz,
                            first_active_index: 0,
                            spacing_khz: channel.subcarrier_spacing_khz,
                        },
                        // Per-subcarrier needs a CCAP test; the channel's mean is a status object.
                        us_rxmer_db: None,
                        us_rxmer_mean_db: Some(hundredths as f64 / 100.0),
                    });
                }
            }
        }

        // Downstream channel state.
        let mut ds_channel_state = Vec::new();
        for (oid, value) in client.walk(&oids::cm_status_value().child(&[2]))? {
            let (Some(if_index), Some(code)) = (last_arc(&oid), as_int(Some(value))) else {
                continue;
            };
            let channel_id = if (3000..4000).contains(&if_index) {
                plan_ofdm
                    .get(if_index as usize - 3000)
                    .map(|c| c.channel_id.clone())
            } else {
                sc_id_of.get(&if_index).cloned()
            };
            if let Some(channel_id) = channel_id {
                ds_channel_state.push(obs::ChannelStateRecord {
                    channel_id,
                    profile_ref: profile_ref.clone(),
                    state: if code == 1 {
                        obs::ChannelUsability::Usable
                    } else {
                        obs::ChannelUsability::Unusable
                    },
                });
            }
        }

        let on = |present: bool| operational && present;
        Ok(obs::ModemObservation {
            schema_version: obs::SCHEMA_VERSION.to_string(),
            t,
            origin,
            element_id: element_id.clone(),
            generation: spec.generation,
            rxmer: on(capability.ds_rxmer_file).then_some(rxmer),
            fbc,
            ds_chan_est: on(capability.ds_chan_est_file).then_some(chan_est),
            ds_rx,
            codewords,
            // PLC and NCP counters and partial channel state are status objects the adapter doesn't read yet.
            plc: None,
            ncp: None,
            preeq: obs::Preeq {
                sc_qam: on(capability.sc_qam_preeq).then_some(sc_preeq),
                ofdma: on(capability.ofdma_preeq_file).then_some(preeq_ofdma),
            },
            us_tx,
            us_rx,
            us_rxmer: on(capability.us_rxmer_summary || capability.us_rxmer_file)
                .then_some(us_rxmer_out),
            reg_state,
            ds_channel_state,
            us_channel_state,
            partial_channel: None,
            t3_timeouts,
            t4_timeouts,
            ranging_status,
        })
    }

    /// The whole service group, no more than `max_concurrent_modems` at a time.
    pub fn collect(&self, t: Timestamp) -> Result<StepObservation, WireError> {
        let origin = self.origin()?;
        let mut targets: Vec<(ElementId, SocketAddr)> = Vec::new();
        for binding in &self.plant.modem_bindings {
            let addr = self.addresses.address(&binding.pseudonym).ok_or_else(|| {
                WireError::Unexpected(format!("the CMTS doesn't list {}", binding.element_id))
            })?;
            targets.push((binding.element_id.clone(), addr));
        }
        let mut modems = Vec::new();
        for chunk in targets.chunks(self.budget.max_concurrent_modems.max(1)) {
            let results: Vec<Result<obs::ModemObservation, WireError>> = thread::scope(|scope| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|(id, addr)| scope.spawn(move || self.collect_modem(id, *addr, t, origin)))
                    .collect();
                handles
                    .into_iter()
                    .map(|h| {
                        h.join().unwrap_or_else(|_| {
                            Err(WireError::Unexpected("a collection panicked".into()))
                        })
                    })
                    .collect()
            });
            for result in results {
                modems.push(result?);
            }
        }
        modems.sort_by(|a, b| a.element_id.cmp(&b.element_id));

        let mut cmts = SnmpClient::new(self.cmts, &self.community)?;
        let profile_ref = self.profile.id.clone();
        let us_id = |if_index: u32| -> Option<ChannelId> {
            let plan = &self.profile.channel_plan;
            if (2000..3000).contains(&if_index) {
                plan.us_sc_qam
                    .get(if_index as usize - 2000)
                    .map(|c| c.channel_id.clone())
            } else {
                plan.ofdma
                    .get(if_index as usize - 4000)
                    .map(|c| c.channel_id.clone())
            }
        };
        let mut us_snr = Vec::new();
        let mut seen = BTreeSet::new();
        for (oid, value) in cmts.walk(&oids::cmts_us_cnir())? {
            // Per-channel rows have one arc after the column; the per-CM rows above have three.
            if oid.0.len() != oids::cmts_us_cnir().0.len() + 1 {
                continue;
            }
            if let (Some(if_index), Some(tenths)) = (last_arc(&oid), as_int(Some(value)))
                && let Some(channel_id) = us_id(if_index)
                && seen.insert(channel_id.clone())
            {
                us_snr.push(obs::UsSnrRecord {
                    channel_id,
                    profile_ref: profile_ref.clone(),
                    us_snr_db: tenths as f64 / 10.0,
                });
            }
        }
        let corrected: BTreeMap<u32, u64> = cmts
            .walk(&oids::sigq_correcteds())?
            .into_iter()
            .filter_map(|(oid, v)| Some((last_arc(&oid)?, as_int(Some(v))? as u64)))
            .collect();
        let uncorrectable: BTreeMap<u32, u64> = cmts
            .walk(&oids::sigq_uncorrectables())?
            .into_iter()
            .filter_map(|(oid, v)| Some((last_arc(&oid)?, as_int(Some(v))? as u64)))
            .collect();
        let mut us_codewords = Vec::new();
        for (oid, value) in cmts.walk(&oids::sigq_unerroreds())? {
            if let (Some(if_index), Some(unerrored)) = (last_arc(&oid), as_int(Some(value)))
                && let Some(channel_id) = us_id(if_index)
            {
                let c = corrected.get(&if_index).copied().unwrap_or(0);
                let u = uncorrectable.get(&if_index).copied().unwrap_or(0);
                us_codewords.push(obs::UsCodewordRecord {
                    channel_id,
                    profile_ref: profile_ref.clone(),
                    total: unerrored as u64 + c + u,
                    corrected: c,
                    uncorrectable: u,
                });
            }
        }

        Ok(StepObservation {
            t,
            modems,
            // Plant-active transponders are v2 (`docs/DESIGN.md`).
            actives: Vec::new(),
            service_group: obs::ServiceGroupObservation {
                schema_version: obs::SCHEMA_VERSION.to_string(),
                t,
                origin,
                service_group: self.plant.service_group.clone(),
                us_snr,
                // UTSC needs a vendor's help on every known deployment [PNM-HFC §8.4.6].
                us_spectrum: None,
                us_codewords,
                optics: None,
            },
            // RPD state comes from the Core, and that's a vCore adapter's job (v2).
            rpd: None,
        })
    }
}

fn axis_back(axis: pnm::Axis) -> obs::SubcarrierAxis {
    obs::SubcarrierAxis {
        zero_freq_mhz: f64::from(axis.subcarrier_zero_hz) / 1e6,
        first_active_index: u32::from(axis.first_active),
        spacing_khz: f64::from(axis.spacing_khz),
    }
}
