//! The design reference: what the healthy plant reads at 20 °C. Derived from the plant by the sim, so
//! it's never stored as an input (invariant 1).

use std::collections::BTreeMap;

use plant_graph::PlantGraph;
use schema::common::{ChannelId, ElementId, PortId, Timestamp};
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::FaultSet;
use schema::observation::DsRxLevels;
use sim::PhysicsError;
use sim::telemetry::{Detail, Scenario, SimOptions, simulate};

/// Stride for hypothesis checks and the reference: 400 kHz, fine enough for echoes up to 1.25 µs.
pub const SUMMARY_STRIDE: usize = 8;

/// What one modem reads when nothing is wrong.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModemReference {
    /// Downstream level per (channel, band centre), dBmV per 6 MHz.
    pub ds_level_dbmv: BTreeMap<(ChannelId, u32), f64>,
    /// Transmit power per upstream channel, dBmV per 1.6 MHz.
    pub us_tx_dbmv: BTreeMap<ChannelId, f64>,
    /// RxMER mean per OFDM channel, dB.
    pub rxmer_mean_db: BTreeMap<ChannelId, f64>,
    /// NMTER per SC-QAM upstream channel, dB.
    pub nmter_db: BTreeMap<ChannelId, f64>,
    /// Pre-eq tap energy, averaged over SC-QAM channels, dB relative to the main tap.
    pub preeq_taps_db: Option<Vec<f64>>,
    /// Channel-estimate magnitude ripple, RMS, dB: what a healthy home's own splitters and cable leave.
    pub chan_est_ripple_db: Option<f64>,
}

/// The healthy plant's readings.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignReference {
    /// Per modem.
    pub modems: BTreeMap<ElementId, ModemReference>,
    /// Upstream SNR per channel, dB.
    pub us_snr_db: BTreeMap<ChannelId, f64>,
    /// Input voltage per active, volts.
    pub v_in: BTreeMap<ElementId, f64>,
    /// Light into the node's forward receiver or RPD optic as designed, dBm, where the plant reports it (§16).
    pub node_rx_dbm: Option<f64>,
    /// Light into the hub's return receiver or optic as designed, dBm, where the plant reports it (§16).
    pub hub_rx_dbm: Option<f64>,
    /// Each active's output level at each port as designed, its set point, dBmV, where the plant reports one per port
    /// (§15).
    pub port_out_dbmv: BTreeMap<(ElementId, PortId), f64>,
}

/// A sample standing for the reference conditions.
pub fn reference_sample() -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(0),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

/// A band's key: its centre in tenths of a MHz, so it can be compared exactly.
pub fn band_key(center_mhz: f64) -> u32 {
    (center_mhz * 10.0).round() as u32
}

impl DesignReference {
    /// Computes the reference from the plant.
    pub fn compute(graph: &PlantGraph<'_>) -> Result<DesignReference, PhysicsError> {
        DesignReference::compute_with_link(graph, None)
    }

    /// The same, with the node's link as its fiber was designed (§16), so optical readings have a design value too.
    pub fn compute_with_link(
        graph: &PlantGraph<'_>,
        link: Option<&sim::optics::NodeLink>,
    ) -> Result<DesignReference, PhysicsError> {
        let environment = Environment {
            schema_version: schema::environment::SCHEMA_VERSION.to_string(),
            samples: vec![reference_sample()],
            ambient_sources: Vec::new(),
        };
        let faults = FaultSet {
            schema_version: schema::fault::SCHEMA_VERSION.to_string(),
            seed: 0,
            faults: vec![],
        };
        let steps = simulate(&Scenario {
            graph,
            environment: &environment,
            faults: &faults,
            seed: 0,
            options: SimOptions {
                detail: Detail::Summary {
                    stride: SUMMARY_STRIDE,
                },
                randomize: None,
                measurement_noise_db: 0.0,
                rxmer_scatter_db: 0.0,
            },
            link,
        })?;
        let mut modems = BTreeMap::new();
        let mut us_snr_db = BTreeMap::new();
        let mut v_in = BTreeMap::new();
        let (mut node_rx_dbm, mut hub_rx_dbm) = (None, None);
        let mut port_out_dbmv = BTreeMap::new();
        if let Some(step) = steps.first() {
            if let Some(optics) = &step.service_group.optics {
                node_rx_dbm = optics.node_rx_dbm;
                hub_rx_dbm = optics.hub_rx_dbm;
            }
            for modem in &step.modems {
                let mut reference = ModemReference::default();
                for record in &modem.ds_rx {
                    match &record.ds_rx_dbmv {
                        DsRxLevels::ScQam { level_dbmv } => {
                            reference
                                .ds_level_dbmv
                                .insert((record.channel_id.clone(), 0), *level_dbmv);
                        }
                        DsRxLevels::Ofdm { bands, .. } => {
                            for band in bands {
                                reference.ds_level_dbmv.insert(
                                    (record.channel_id.clone(), band_key(band.center_mhz)),
                                    band.level_dbmv,
                                );
                            }
                        }
                    }
                }
                for record in &modem.us_tx {
                    reference
                        .us_tx_dbmv
                        .insert(record.channel_id.clone(), record.psd_dbmv_per_1p6mhz);
                }
                for record in modem.rxmer.iter().flatten() {
                    if let Some(mean) = record.rxmer_mean_db {
                        reference
                            .rxmer_mean_db
                            .insert(record.channel_id.clone(), mean);
                    }
                }
                for record in modem.preeq.sc_qam.iter().flatten() {
                    reference
                        .nmter_db
                        .insert(record.channel_id.clone(), record.nmter_db);
                }
                reference.preeq_taps_db = crate::extract::mean_taps_db(modem);
                reference.chan_est_ripple_db = crate::extract::chan_est_ripple(modem);
                modems.insert(modem.element_id.clone(), reference);
            }
            for record in &step.service_group.us_snr {
                us_snr_db.insert(record.channel_id.clone(), record.us_snr_db);
            }
            for active in &step.actives {
                if let Some(v) = active.v_in {
                    v_in.insert(active.element_id.clone(), v);
                }
                for port in &active.out_levels {
                    port_out_dbmv.insert(
                        (active.element_id.clone(), port.port.clone()),
                        port.out_level_dbmv,
                    );
                }
            }
        }
        Ok(DesignReference {
            modems,
            us_snr_db,
            v_in,
            node_rx_dbm,
            hub_rx_dbm,
            port_out_dbmv,
        })
    }
}
