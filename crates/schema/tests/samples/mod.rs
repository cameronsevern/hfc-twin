//! One small hand-built value per top-level schema, used for round-trip and golden tests.
//! Between them they reach every enum variant and every `Option` as `Some`.

use schema::capability::{AccessPath, CapabilitySet, FbcCapability};
use schema::common::{
    ChannelId, Complex, ComponentId, EdgeId, ElementId, LonLat, Origin, PortId, ProfileId,
    ServiceGroupId, StructureId, Timestamp,
};
use schema::component::*;
use schema::environment::{self, Environment, EnvironmentSample};
use schema::fault::{self, *};
use schema::observation::{self, *};
use schema::plant::{self, *};
use schema::profile::{self, *};
use schema::work_order::{self, *};

fn element_id(id: &str) -> ElementId {
    ElementId::new(id)
}
fn profile_ref() -> ProfileId {
    ProfileId::new("sample-profile")
}
fn channel(id: &str) -> ChannelId {
    ChannelId::new(id)
}
fn loss(a_db: f64, b_db: f64) -> Vec<LossPoint> {
    vec![
        LossPoint {
            f_mhz: 55.0,
            loss_db: a_db,
        },
        LossPoint {
            f_mhz: 1000.0,
            loss_db: b_db,
        },
    ]
}

pub fn capability() -> CapabilitySet {
    CapabilitySet {
        ds_rxmer_file: true,
        ds_rxmer_summary: true,
        ds_chan_est_file: true,
        ds_chan_est_summary: true,
        ofdma_preeq_file: true,
        ofdma_preeq_summary: true,
        sc_qam_preeq: true,
        fbc: Some(FbcCapability {
            file: true,
            snmp: true,
            lo_mhz: 54.0,
            hi_mhz: 1218.0,
        }),
        per_profile_fec: true,
        plc_ncp_counters: true,
        band_power_6mhz: true,
        us_rxmer_file: true,
        us_rxmer_summary: true,
        us_spectrum_capture: true,
        v_in: true,
        i_draw: true,
        temp_c: true,
        status: true,
        out_level: true,
        access: vec![AccessPath::CmSnmp, AccessPath::CmTftp, AccessPath::CmtsSnmp],
    }
}

pub fn profile() -> Profile {
    Profile {
        schema_version: profile::SCHEMA_VERSION.into(),
        id: profile_ref(),
        docsis: Docsis::D31,
        band_plan: vec![
            BandSegment {
                lo_mhz: 5.0,
                hi_mhz: 204.0,
                direction: Direction::Us,
            },
            BandSegment {
                lo_mhz: 258.0,
                hi_mhz: 1218.0,
                direction: Direction::Ds,
            },
        ],
        channel_plan: ChannelPlan {
            ds_sc_qam: vec![DsScQamChannel {
                channel_id: channel("ds-sc-01"),
                center_mhz: 603.0,
                width_mhz: 6.0,
                modulation: Qam::Qam256,
            }],
            ofdm: vec![OfdmChannel {
                channel_id: channel("ofdm-1"),
                first_active_mhz: 835.0,
                last_active_mhz: 1024.95,
                fft_size: 4096,
                subcarrier_spacing_khz: 50.0,
                cp_us: 2.5,
                rolloff_us: 1.25,
                data_profiles: vec![
                    DataProfile {
                        data_profile_id: 0,
                        modulation: Qam::Qam256,
                    },
                    DataProfile {
                        data_profile_id: 1,
                        modulation: Qam::Qam4096,
                    },
                ],
                ncp_modulation: NcpModulation::Qam16,
            }],
            us_sc_qam: vec![UsScQamChannel {
                channel_id: channel("us-sc-1"),
                center_mhz: 16.4,
                width_mhz: 6.4,
                symbol_rate_msym_per_s: 5.12,
                modulation: Qam::Qam64,
            }],
            ofdma: vec![OfdmaChannel {
                channel_id: channel("ofdma-1"),
                first_active_mhz: 108.5,
                last_active_mhz: 203.45,
                fft_size: 2048,
                subcarrier_spacing_khz: 50.0,
                cp_us: 2.5,
                rolloff_us: 1.25,
                data_profiles: vec![DataProfile {
                    data_profile_id: 13,
                    modulation: Qam::Qam256,
                }],
            }],
        },
        max_cascade: 6,
        max_homes_passed: 500,
        design_levels: DesignLevels {
            out_dbmv_at_hi: 52.0,
            tilt_db: 12.1,
            psd_spread_max_db: 10.0,
            modem_in_min_dbmv_by_qam: vec![
                QamFloor {
                    qam: Qam::Qam256,
                    min_dbmv: -15.0,
                },
                QamFloor {
                    qam: Qam::Qam4096,
                    min_dbmv: -6.0,
                },
            ],
            modem_in_max_dbmv: 15.0,
            modem_in_total_max_dbmv: 40.0,
            us_node_in_target_dbmv: 10.0,
        },
        fec: vec![
            FecRow {
                qam: Qam::Qam256,
                thr_db: 24.0,
                req_cnr_db: 27.0,
                req_cnr_above_1002_db: None,
                sc_qam_thr_db: Some(26.0),
            },
            FecRow {
                qam: Qam::Qam4096,
                thr_db: 35.0,
                req_cnr_db: 41.0,
                req_cnr_above_1002_db: Some(41.5),
                sc_qam_thr_db: None,
            },
        ],
        modem_behavior: ModemBehavior {
            tx_saturation: TxSaturation::Clamp,
            ranging_spacing_s: 15.0,
            reregistration_s: 60.0,
        },
        node_kind: NodeKind::Rpd,
        capability: capability(),
        plant_reports: Default::default(),
    }
}

fn active(duplex: Duplex) -> ActiveSpec {
    ActiveSpec {
        max_out_dbmv_at_hi: 56.0,
        max_gain_db: 36.0,
        min_in_dbmv: 15.0,
        nf_db: 7.0,
        return_nf_db: 10.0,
        ctb_ref_db: 70.0,
        cso_ref_db: 72.0,
        cin_ref_db: 58.0,
        ref_out_dbmv: 52.0,
        return_gain_db: 25.0,
        agc_range_db: 3.0,
        return_loss_db: 16.0,
        duplex,
        power_w: 60.0,
        v_min: 60.0,
        outputs: Vec::new(),
        plugin_family: None,
        nf_table: Vec::new(),
        cin_table: Vec::new(),
        gain_flatness_db: None,
        agc_pilots_mhz: Vec::new(),
        return_loss_table: Vec::new(),
        gain_table: Vec::new(),
        ref_composite_dbmv: None,
    }
}

fn group_delay() -> Vec<GroupDelayPoint> {
    vec![
        GroupDelayPoint {
            f_mhz: 5.0,
            delay_ns: 50.0,
        },
        GroupDelayPoint {
            f_mhz: 1218.0,
            delay_ns: 5.0,
        },
    ]
}

fn diplexed() -> Duplex {
    Duplex::Diplexed {
        splits: vec![Split {
            us_hi_mhz: 204.0,
            ds_lo_mhz: 258.0,
            group_delay_ns: group_delay(),
        }],
        remote_switchable: true,
    }
}

fn component(id: &str, spec: ComponentSpec) -> Component {
    Component {
        id: ComponentId::new(id),
        version: 1,
        vendor: "Generic".into(),
        model: id.into(),
        source_url: Some("https://example.com/datasheet".into()),
        spec_confidence: SpecConfidence::Generic,
        rated_lo_mhz: Some(5.0),
        rated_hi_mhz: Some(1218.0),
        tolerance: None,
        spec,
    }
}

pub fn component_library() -> ComponentLibrary {
    let cable = CableSpec {
        atten_a: 0.0598,
        atten_b: 0.00046,
        k_temp: 0.002,
        v_f: 0.87,
        r_loop_ohm_per_kft: 1.7,
        family: None,
        ampacity_a: None,
        r_loop_k_temp: None,
    };
    let mut power_supply = component(
        "power-supply",
        ComponentSpec::PowerSupply(PowerSupplySpec {
            v_out: 90.0,
            i_rated_a: 15.0,
        }),
    );
    power_supply.rated_lo_mhz = None;
    power_supply.rated_hi_mhz = None;
    ComponentLibrary {
        schema_version: schema::component::SCHEMA_VERSION.into(),
        components: vec![
            component(
                "node",
                ComponentSpec::Node(NodeSpec {
                    active: active(diplexed()),
                    optics_cnr_db: 48.0,
                    us_optics_snr_db: 40.0,
                    source_mer: Vec::new(),
                }),
            ),
            component(
                "rpd",
                ComponentSpec::Rpd(RpdSpec {
                    active: active(diplexed()),
                    supported_pnm_tests: vec![
                        RpdPnmTest::Upc,
                        RpdPnmTest::UsRxmer,
                        RpdPnmTest::ImpulseNoise,
                        RpdPnmTest::DsSymbolCapture,
                        RpdPnmTest::Histogram,
                    ],
                    source_mer: Vec::new(),
                    us_rx_ceiling_db: None,
                }),
            ),
            component(
                "amp",
                ComponentSpec::Amp(active(Duplex::FullDuplexEc {
                    group_delay_ns: group_delay(),
                })),
            ),
            component(
                "tap",
                ComponentSpec::Tap(TapSpec {
                    ports: 4,
                    tap_value_db: 17.0,
                    insertion_loss: loss(1.6, 2.44),
                    tap_loss: loss(17.0, 17.0),
                    port_isolation_db: 32.0,
                    return_loss_db: 18.0,
                    power_passing: true,
                    terminating: false,
                    plugin_family: None,
                    return_loss_by_port: Vec::new(),
                }),
            ),
            component(
                "splitter",
                ComponentSpec::Splitter(SplitterSpec {
                    legs: 2,
                    leg_loss: loss(3.5, 4.0),
                    return_loss_db: 18.0,
                    power_passing: true,
                    leg_isolation_db: None,
                    return_loss_table: Vec::new(),
                }),
            ),
            component(
                "coupler",
                ComponentSpec::DirectionalCoupler(DirectionalCouplerSpec {
                    coupled_value_db: 8.0,
                    excess_loss_db: 0.3,
                    return_loss_db: 18.0,
                    power_passing: true,
                    return_loss_table: Vec::new(),
                }),
            ),
            component(
                "inserter",
                ComponentSpec::PowerInserter(PowerInserterSpec {
                    i_rated_a: 15.0,
                    power_passing: true,
                    insertion_loss: Vec::new(),
                    port_i_rated_a: None,
                    return_loss_db: None,
                }),
            ),
            power_supply,
            component(
                "terminator",
                ComponentSpec::Terminator(TerminatorSpec {
                    return_loss_db: 30.0,
                    return_loss_table: Vec::new(),
                }),
            ),
            component("cable", ComponentSpec::Cable(cable.clone())),
            component("drop", ComponentSpec::Drop(cable)),
            component(
                "modem",
                ComponentSpec::Modem(ModemSpec {
                    generation: ModemGeneration::D31,
                    impl_mer_db: 44.0,
                    nf_db: 10.0,
                    return_loss_db: 18.0,
                    tx_max_total_dbmv: 65.0,
                    capability: capability(),
                    return_loss_table: Vec::new(),
                }),
            ),
        ],
    }
}

fn element(id: &str, kind: ElementKind, ports: &[(&str, PortRole)]) -> Element {
    Element {
        id: element_id(id),
        kind,
        component_ref: ComponentId::new(id),
        geo: Some(LonLat {
            lon_deg: -105.1,
            lat_deg: 39.9,
        }),
        ports: ports
            .iter()
            .map(|(port, role)| Port {
                id: PortId::new(*port),
                role: *role,
            })
            .collect(),
        record_status: RecordStatus::Built,
        fitted: None,
        structure: None,
    }
}

fn edge(id: &str, from: (&str, &str), to: (&str, &str), kind: EdgeKind) -> Edge {
    Edge {
        id: EdgeId::new(id),
        from: PortRef {
            element_id: element_id(from.0),
            port: PortId::new(from.1),
        },
        to: PortRef {
            element_id: element_id(to.0),
            port: PortId::new(to.1),
        },
        kind,
        record_status: None,
    }
}

fn span(installation: Installation) -> EdgeKind {
    EdgeKind::Span {
        cable_ref: ComponentId::new("cable"),
        len_ft: 100.0,
        installation,
        power_passing: true,
        fittings: Vec::new(),
        support: None,
        path: Vec::new(),
    }
}

pub fn plant() -> Plant {
    use PortRole::*;
    Plant {
        schema_version: plant::SCHEMA_VERSION.into(),
        service_group: ServiceGroupId::new("sg-1"),
        profile_ref: profile_ref(),
        origin: Origin::Synthetic,
        elements: vec![
            element(
                "rpd",
                ElementKind::Rpd {
                    return_atten_db: 0.0,
                },
                &[("out", Output)],
            ),
            element(
                "inserter",
                ElementKind::PowerInserter,
                &[("in", Input), ("out", Output)],
            ),
            element(
                "amp",
                ElementKind::Amp {
                    return_atten_db: 6.0,
                },
                &[("in", Input), ("out", Output)],
            ),
            element(
                "splitter",
                ElementKind::Splitter,
                &[("in", Input), ("leg-1", Leg), ("leg-2", Leg)],
            ),
            element(
                "coupler",
                ElementKind::DirectionalCoupler,
                &[("in", Input), ("through", Through), ("coupled", Coupled)],
            ),
            element(
                "tap",
                ElementKind::Tap,
                &[
                    ("in", Input),
                    ("through", Through),
                    ("tap-1", TapPort),
                    ("tap-2", TapPort),
                    ("tap-3", TapPort),
                    ("tap-4", TapPort),
                ],
            ),
            element("terminator", ElementKind::Terminator, &[("in", Input)]),
            element("modem", ElementKind::Modem, &[("rf", Input)]),
        ],
        edges: vec![
            edge(
                "span-1",
                ("rpd", "out"),
                ("inserter", "in"),
                span(Installation::Aerial),
            ),
            edge(
                "span-2",
                ("inserter", "out"),
                ("amp", "in"),
                span(Installation::Underground),
            ),
            edge(
                "span-3",
                ("amp", "out"),
                ("splitter", "in"),
                span(Installation::Aerial),
            ),
            edge(
                "span-4",
                ("splitter", "leg-1"),
                ("coupler", "in"),
                span(Installation::Aerial),
            ),
            edge(
                "span-5",
                ("coupler", "through"),
                ("tap", "in"),
                span(Installation::Aerial),
            ),
            edge(
                "direct-1",
                ("tap", "through"),
                ("terminator", "in"),
                EdgeKind::Direct,
            ),
            edge(
                "drop-1",
                ("tap", "tap-1"),
                ("modem", "rf"),
                EdgeKind::Drop {
                    cable_ref: ComponentId::new("drop"),
                    len_ft: 80.0,
                    fittings: Vec::new(),
                    path: Vec::new(),
                    installation: None,
                },
            ),
        ],
        power_supplies: vec![PowerSupply {
            id: element_id("power-supply"),
            component_ref: ComponentId::new("power-supply"),
            feeds: element_id("rpd"),
            geo: Some(LonLat {
                lon_deg: -105.1,
                lat_deg: 39.9,
            }),
            batteries: None,
            structure: None,
            record_status: None,
        }],
        modem_bindings: vec![ModemBinding {
            element_id: element_id("modem"),
            pseudonym: "3f2a9c".into(),
        }],
        structures: Vec::new(),
    }
}

fn hourly() -> Schedule {
    Schedule {
        hourly_multipliers: [1.0; 24],
    }
}

fn at_span() -> Location {
    Location::Span {
        span_id: EdgeId::new("span-1"),
        pos_ft: 50.0,
    }
}

fn at_port() -> Location {
    Location::Port {
        element_id: element_id("tap"),
        port: PortId::new("through"),
    }
}

pub fn fault_set() -> FaultSet {
    let instant = |kind| Fault {
        onset: Onset::Instant,
        kind,
    };
    FaultSet {
        schema_version: fault::SCHEMA_VERSION.into(),
        seed: 7,
        faults: vec![
            Fault {
                onset: Onset::Ramp {
                    start: Timestamp(0),
                    end: Timestamp(3600),
                },
                kind: FaultKind::WaterIngress {
                    span_id: EdgeId::new("span-1"),
                    pos_ft: 10.0,
                    affected_len_ft: 40.0,
                    k_w_db_per_100ft: 2.0,
                    rain_coupling_per_mm_per_hr: 0.05,
                    impedance_variation_pct: 5.0,
                },
            },
            instant(FaultKind::ConnectorReflection {
                location: at_span(),
                return_loss_db: 10.0,
                couplings: None,
            }),
            instant(FaultKind::Suckout {
                location: at_port(),
                f0_mhz: 450.0,
                depth_db: 6.0,
                width_mhz: 30.0,
            }),
            instant(FaultKind::FrequencySelectiveLoss {
                location: at_port(),
                shape: LossShape::BandStop,
                f_lo_mhz: 300.0,
                f_hi_mhz: 400.0,
                depth_db: 20.0,
                stop_return_loss_db: 6.0,
                skirt_mhz: None,
            }),
            instant(FaultKind::FrequencySelectiveLoss {
                location: at_port(),
                shape: LossShape::HighPass,
                f_lo_mhz: 54.0,
                f_hi_mhz: 54.0,
                depth_db: 20.0,
                stop_return_loss_db: 6.0,
                skirt_mhz: None,
            }),
            instant(FaultKind::FrequencySelectiveLoss {
                location: at_port(),
                shape: LossShape::LowPass,
                f_lo_mhz: 1002.0,
                f_hi_mhz: 1002.0,
                depth_db: 20.0,
                stop_return_loss_db: 6.0,
                skirt_mhz: None,
            }),
            instant(FaultKind::AmpDrift {
                amp_id: element_id("amp"),
                state: AmpState::Drift {
                    gain_delta_db: -2.0,
                    tilt_delta_db: 1.0,
                    compression_onset_dbmv: Some(54.0),
                },
            }),
            instant(FaultKind::AmpDrift {
                amp_id: element_id("amp"),
                state: AmpState::Failed,
            }),
            instant(FaultKind::UpstreamIngress {
                entry_location: at_span(),
                f_mhz: 20.0,
                bw_mhz: 2.0,
                power_dbmv: -20.0,
                schedule: hourly(),
                burst: Some(Burst {
                    duration_ms: 10.0,
                    rate_per_s: 5.0,
                }),
            }),
            instant(FaultKind::DownstreamIngress {
                entry_location: at_port(),
                f_mhz: 700.0,
                bw_mhz: 10.0,
                power_dbmv: -30.0,
                schedule: hourly(),
            }),
            instant(FaultKind::PowerSupplySag {
                psu_id: element_id("power-supply"),
                v_delta: 12.0,
                schedule: hourly(),
            }),
            instant(FaultKind::BadDrop {
                modem_id: element_id("modem"),
                extra_loss_db: 6.0,
                return_loss_db: Some(12.0),
            }),
        ],
    }
}

fn axis() -> SubcarrierAxis {
    SubcarrierAxis {
        zero_freq_mhz: 800.0,
        first_active_index: 700,
        spacing_khz: 50.0,
    }
}

fn summary() -> ResponseSummary {
    ResponseSummary {
        mag_mean_db: 0.0,
        mag_slope_db_per_mhz: 0.01,
        mag_ripple_rms_db: 0.1,
        mag_ripple_pp_db: 0.4,
        gd_mean_ns: 120.0,
        gd_slope_ns_per_mhz: 0.02,
        gd_ripple_rms_ns: 1.0,
        gd_ripple_pp_ns: 4.0,
    }
}

fn state(id: &str, usable: ChannelUsability) -> ChannelStateRecord {
    ChannelStateRecord {
        channel_id: channel(id),
        profile_ref: profile_ref(),
        state: usable,
    }
}

pub fn modem_observation() -> ModemObservation {
    let unit = Complex { re: 1.0, im: 0.0 };
    ModemObservation {
        schema_version: observation::SCHEMA_VERSION.into(),
        t: Timestamp(300),
        origin: Origin::Synthetic,
        element_id: element_id("modem"),
        generation: ModemGeneration::D31,
        rxmer: Some(vec![DsRxmerRecord {
            channel_id: channel("ofdm-1"),
            profile_ref: profile_ref(),
            axis: axis(),
            rxmer_db: Some(vec![Some(41.75), None, Some(42.0)]),
            rxmer_mean_db: Some(41.88),
            rxmer_stddev_db: Some(0.2),
            rxmer_p2_db: Some(41.75),
        }]),
        fbc: Some(FbcRecord {
            profile_ref: profile_ref(),
            first_bin_center_mhz: 258.0,
            bin_width_khz: 29.3,
            noise_bandwidth_khz: 43.9,
            fbc_dbmv: vec![-14.0, -13.75],
        }),
        ds_chan_est: Some(vec![DsChanEstRecord {
            channel_id: channel("ofdm-1"),
            profile_ref: profile_ref(),
            axis: axis(),
            coefficients: Some(vec![Some(unit), None]),
            summary: Some(summary()),
        }]),
        ds_rx: vec![
            DsRxRecord {
                channel_id: channel("ds-sc-01"),
                profile_ref: profile_ref(),
                ds_rx_dbmv: DsRxLevels::ScQam { level_dbmv: 7.5 },
            },
            DsRxRecord {
                channel_id: channel("ofdm-1"),
                profile_ref: profile_ref(),
                ds_rx_dbmv: DsRxLevels::Ofdm {
                    bands: vec![DsBandLevel {
                        center_mhz: 837.0,
                        level_dbmv: 7.25,
                    }],
                    plc_band_dbmv: 7.5,
                },
            },
        ],
        codewords: vec![
            CodewordRecord {
                channel_id: channel("ds-sc-01"),
                profile_ref: profile_ref(),
                data_profile_id: None,
                total: 1000,
                corrected: 1,
                uncorrectable: 0,
            },
            CodewordRecord {
                channel_id: channel("ofdm-1"),
                profile_ref: profile_ref(),
                data_profile_id: Some(1),
                total: 2000,
                corrected: 900,
                uncorrectable: 0,
            },
        ],
        plc: Some(vec![PlcRecord {
            channel_id: channel("ofdm-1"),
            profile_ref: profile_ref(),
            total: 100,
            unreliable: 0,
        }]),
        ncp: Some(vec![NcpRecord {
            channel_id: channel("ofdm-1"),
            profile_ref: profile_ref(),
            total: 100,
            crc_failures: 0,
        }]),
        preeq: Preeq {
            sc_qam: Some(vec![ScQamPreeqRecord {
                channel_id: channel("us-sc-1"),
                profile_ref: profile_ref(),
                taps: vec![unit; 24],
                main_tap_index: 8,
                mtc_db: 0.005,
                mtr_db: 29.0,
                nmter_db: -29.0,
            }]),
            ofdma: Some(vec![OfdmaPreeqRecord {
                channel_id: channel("ofdma-1"),
                profile_ref: profile_ref(),
                axis: axis(),
                coefficients: Some(vec![unit]),
                summary: Some(summary()),
            }]),
        },
        us_tx: vec![
            UsTxRecord {
                channel_id: channel("us-sc-1"),
                profile_ref: profile_ref(),
                psd_dbmv_per_1p6mhz: 23.5,
                total_dbmv: Some(29.52),
            },
            UsTxRecord {
                channel_id: channel("ofdma-1"),
                profile_ref: profile_ref(),
                psd_dbmv_per_1p6mhz: 31.99,
                total_dbmv: None,
            },
        ],
        us_rx: vec![UsRxRecord {
            channel_id: channel("us-sc-1"),
            profile_ref: profile_ref(),
            us_rx_dbmv_per_1p6mhz: 3.98,
        }],
        us_rxmer: Some(vec![UsRxmerRecord {
            channel_id: channel("ofdma-1"),
            profile_ref: profile_ref(),
            axis: axis(),
            us_rxmer_db: Some(vec![Some(40.0), None]),
            us_rxmer_mean_db: Some(40.0),
        }]),
        reg_state: RegState::Operational,
        ds_channel_state: vec![state("ds-sc-01", ChannelUsability::Usable)],
        us_channel_state: vec![state("us-sc-1", ChannelUsability::Unusable)],
        partial_channel: Some(vec![PartialChannelRecord {
            channel_id: channel("ofdm-1"),
            profile_ref: profile_ref(),
            failed_data_profiles: vec![2],
            ncp_failed: false,
            plc_failed: false,
        }]),
        t3_timeouts: vec![TimeoutRecord {
            channel_id: channel("us-sc-1"),
            profile_ref: profile_ref(),
            count: 3,
        }],
        t4_timeouts: vec![TimeoutRecord {
            channel_id: channel("us-sc-1"),
            profile_ref: profile_ref(),
            count: 0,
        }],
        ranging_status: vec![RangingStatusRecord {
            channel_id: channel("us-sc-1"),
            profile_ref: profile_ref(),
            status: RangingState::Success,
        }],
    }
}

/// A D3.0 modem: everything a capability mask can remove, removed.
pub fn modem_observation_masked() -> ModemObservation {
    ModemObservation {
        generation: ModemGeneration::D30,
        rxmer: None,
        fbc: None,
        ds_chan_est: None,
        plc: None,
        ncp: None,
        preeq: Preeq {
            sc_qam: None,
            ofdma: None,
        },
        us_rxmer: None,
        partial_channel: None,
        ..modem_observation()
    }
}

pub fn active_observation() -> ActiveObservation {
    ActiveObservation {
        schema_version: observation::SCHEMA_VERSION.into(),
        t: Timestamp(300),
        origin: Origin::Field,
        element_id: element_id("amp"),
        v_in: Some(86.47),
        i_draw: Some(0.69),
        temp_c: Some(31.0),
        status: Some(ActiveStatus::Up),
        out_level: Some(OutLevel {
            out_level_dbmv: 46.1,
            f_ref_mhz: 750.0,
        }),
        out_levels: Vec::new(),
    }
}

pub fn service_group_observation() -> ServiceGroupObservation {
    ServiceGroupObservation {
        schema_version: observation::SCHEMA_VERSION.into(),
        t: Timestamp(300),
        origin: Origin::Synthetic,
        service_group: ServiceGroupId::new("sg-1"),
        us_snr: vec![UsSnrRecord {
            channel_id: channel("us-sc-1"),
            profile_ref: profile_ref(),
            us_snr_db: 39.74,
        }],
        us_spectrum: Some(vec![UsSpectrumRecord {
            port: PortId::new("out"),
            profile_ref: profile_ref(),
            first_bin_center_mhz: 5.0,
            bin_width_khz: 100.0,
            noise_bandwidth_khz: 150.0,
            us_spectrum_dbmv: vec![-40.5, -40.25],
        }]),
        us_codewords: vec![UsCodewordRecord {
            channel_id: channel("us-sc-1"),
            profile_ref: profile_ref(),
            total: 500,
            corrected: 2,
            uncorrectable: 0,
        }],
        optics: None,
    }
}

pub fn environment() -> Environment {
    Environment {
        schema_version: environment::SCHEMA_VERSION.into(),
        samples: vec![
            EnvironmentSample {
                t: Timestamp(0),
                temp_c: 20.0,
                ground_temp_c: 15.0,
                rain_mm_per_hr: 0.0,
                solar_w_per_m2: None,
                wind_m_per_s: None,
            },
            EnvironmentSample {
                t: Timestamp(300),
                temp_c: 19.5,
                ground_temp_c: 15.0,
                rain_mm_per_hr: 2.5,
                solar_w_per_m2: None,
                wind_m_per_s: None,
            },
        ],
        ambient_sources: Vec::new(),
    }
}

/// Version 2's fields: an ambient source, wind and sun (§18, §19).
pub fn environment_v2() -> Environment {
    let mut hourly_multipliers = [1.0; 24];
    hourly_multipliers[19] = 2.0;
    hourly_multipliers[3] = 0.5;
    Environment {
        schema_version: environment::SCHEMA_VERSION.into(),
        samples: vec![EnvironmentSample {
            t: Timestamp(0),
            temp_c: 31.0,
            ground_temp_c: 18.0,
            rain_mm_per_hr: 0.0,
            wind_m_per_s: Some(6.5),
            solar_w_per_m2: Some(850.0),
        }],
        ambient_sources: vec![environment::AmbientSource {
            id: "ambient-hf".into(),
            lo_mhz: 5.0,
            hi_mhz: 30.0,
            level_dbmv_per_mhz: -10.0,
            schedule: fault::Schedule { hourly_multipliers },
        }],
    }
}

/// Version 4's fields: a connector fault's couplings (§18, §19) and a misalignment (§15).
pub fn fault_set_v4() -> FaultSet {
    FaultSet {
        schema_version: "4".into(),
        seed: 7,
        faults: vec![
            Fault {
                onset: Onset::Instant,
                kind: FaultKind::ConnectorReflection {
                    location: at_span(),
                    return_loss_db: 14.0,
                    couplings: Some(fault::ConnectorCouplings {
                        temp_db_per_c: Some(0.2),
                        wind_db_per_m_per_s: Some(-0.8),
                        ambient_coupling_db: Some(25.0),
                    }),
                },
            },
            Fault {
                onset: Onset::Instant,
                kind: FaultKind::Misalignment {
                    active_id: ElementId::new("amp-1"),
                    pad_delta_db: 2.0,
                    eq_delta_db: -4.0,
                    return_pad_delta_db: 0.0,
                },
            },
        ],
    }
}

/// Task 19's damage events, one of each (§20).
pub fn fault_set_v5() -> FaultSet {
    let instant = |kind| Fault {
        onset: Onset::Instant,
        kind,
    };
    FaultSet {
        schema_version: fault::SCHEMA_VERSION.into(),
        seed: 11,
        faults: vec![
            instant(FaultKind::SpanCut {
                span_id: EdgeId::new("span-4"),
                pos_ft: 50.0,
                end: CutEnd::Shorted,
                repair_s: Some(14_400.0),
            }),
            instant(FaultKind::AnalogNodeFailure {
                node_id: ElementId::new("node-1"),
                mode: NodeFailureMode::RfSection,
                repair_s: None,
            }),
            instant(FaultKind::WaterInDevice {
                element_id: ElementId::new("tap-2"),
                loss_db_at_1ghz: 34.64,
                return_loss_db: 7.0,
                rain_coupling_per_mm_per_hr: 0.05,
                failed_after_s: Some(86_400.0),
            }),
            instant(FaultKind::PowerShort {
                span_id: EdgeId::new("span-2"),
                pos_ft: 120.0,
                fault_resistance_ohm: 0.5,
                repair_s: None,
            }),
            instant(FaultKind::DamagedDevice {
                element_id: ElementId::new("tap-1"),
                through_loss_db: 6.0,
                return_loss_db: 8.0,
                ports_lost: vec![PortId::new("tap-2")],
                ingress_power_dbmv: Some(-20.0),
                cpd_second_order_dbmv_per_mhz: Some(-45.0),
            }),
            instant(FaultKind::UtilityOutage {
                center: LonLat {
                    lon_deg: -105.1,
                    lat_deg: 39.9,
                },
                radius_ft: 800.0,
                repair_s: Some(7_200.0),
            }),
        ],
    }
}

/// The fiber fault document, one of each class and location (§16).
pub fn fiber_fault_set() -> schema::fiber_fault::FiberFaultSet {
    use schema::common::{FiberCableId, FiberElementId};
    use schema::fiber_fault::{
        FiberFault, FiberFaultKind, FiberFaultSet, FiberLocation, StrandRef,
    };
    FiberFaultSet {
        schema_version: schema::fiber_fault::SCHEMA_VERSION.into(),
        region: "region-1".into(),
        faults: vec![
            FiberFault {
                onset: Onset::Instant,
                kind: FiberFaultKind::FiberCut {
                    cable: FiberCableId::new("fiber-1"),
                    pos_ft: 850.0,
                    repair_s: Some(14_400.0),
                },
            },
            FiberFault {
                onset: Onset::Ramp {
                    start: Timestamp(0),
                    end: Timestamp(86_400),
                },
                kind: FiberFaultKind::EventLoss {
                    location: FiberLocation::Cable {
                        cable: FiberCableId::new("fiber-2"),
                        pos_ft: 400.0,
                    },
                    loss_db: 3.0,
                },
            },
            FiberFault {
                onset: Onset::Instant,
                kind: FiberFaultKind::EventLoss {
                    location: FiberLocation::Splice {
                        element: FiberElementId::new("closure-1"),
                        strands: [
                            StrandRef {
                                cable: FiberCableId::new("fiber-1"),
                                strand: 2,
                            },
                            StrandRef {
                                cable: FiberCableId::new("fiber-2"),
                                strand: 2,
                            },
                        ],
                    },
                    loss_db: 1.5,
                },
            },
            FiberFault {
                onset: Onset::Instant,
                kind: FiberFaultKind::EventLoss {
                    location: FiberLocation::Element {
                        element: FiberElementId::new("panel-1"),
                    },
                    loss_db: 0.8,
                },
            },
            FiberFault {
                onset: Onset::Instant,
                kind: FiberFaultKind::TransmitterDrift {
                    element: FiberElementId::new("ftx-1"),
                    power_delta_db: -2.0,
                    rin_delta_db: 5.0,
                },
            },
        ],
    }
}

/// Observation version 2's fields: a node's optics and an active's levels per port (§16, §15).
pub fn service_group_observation_v2() -> ServiceGroupObservation {
    ServiceGroupObservation {
        optics: Some(observation::OpticsObservation {
            node_rx_dbm: Some(-2.1),
            hub_rx_dbm: Some(1.7),
            rpd_optic: Some(observation::OpticStatus {
                link_up: true,
                tx_dbm: -3.8,
                rx_dbm: Some(-5.2),
            }),
        }),
        ..service_group_observation()
    }
}

/// Observation version 2's per-port levels.
pub fn active_observation_v2() -> ActiveObservation {
    ActiveObservation {
        out_levels: vec![
            observation::PortOutLevel {
                port: PortId::new("out-1"),
                out_level_dbmv: 50.1,
                f_ref_mhz: 1002.0,
            },
            observation::PortOutLevel {
                port: PortId::new("out-2"),
                out_level_dbmv: 49.8,
                f_ref_mhz: 1002.0,
            },
        ],
        ..active_observation()
    }
}

/// Profile version 2's flags.
pub fn profile_v2() -> Profile {
    Profile {
        plant_reports: profile::PlantReports {
            node_optical_rx: true,
            hub_optical_rx: true,
            rpd_optic_status: true,
            out_level_per_port: true,
        },
        ..profile()
    }
}

pub fn policy_table() -> PolicyTable {
    PolicyTable {
        schema_version: work_order::SCHEMA_VERSION.into(),
        entries: vec![
            PolicyEntry {
                cause: CauseClass::BadDrop,
                component_class: Some(ComponentClass::Drop),
                action: "Replace the drop".into(),
                parts: vec!["drop cable".into()],
                crew: "service".into(),
            },
            PolicyEntry {
                cause: CauseClass::Unknown,
                component_class: None,
                action: "Investigate".into(),
                parts: vec![],
                crew: "maintenance".into(),
            },
        ],
    }
}

pub fn work_order(status: WorkOrderStatus) -> WorkOrder {
    let service_group = ServiceGroupId::new("sg-1");
    let t = Timestamp(300);
    let id = element_id("modem");
    let cause = CauseClass::BadDrop;
    WorkOrder {
        schema_version: work_order::SCHEMA_VERSION.into(),
        id: WorkOrder::derive_id(&service_group, t, &id, cause),
        service_group,
        t,
        element_id: id.clone(),
        geo: Some(LonLat {
            lon_deg: -105.1,
            lat_deg: 39.9,
        }),
        cause,
        parameters: Fault {
            onset: Onset::Instant,
            kind: FaultKind::BadDrop {
                modem_id: id,
                extra_loss_db: 6.0,
                return_loss_db: None,
            },
        },
        confidence: 0.92,
        affected_modems: 1,
        action: "Replace the drop".into(),
        parts: vec!["drop cable".into()],
        explanation: "Extra loss on one drop only.".into(),
        status,
    }
}

pub fn spectrum_row() -> schema::spectrum::SpectrumRow {
    use schema::common::{ChannelId, ElementId, Origin, ProfileId};
    use schema::spectrum::{Shape, ShapeLabel, SpectrumRow};
    use schema::work_order::CauseClass;
    SpectrumRow {
        schema_version: schema::spectrum::SCHEMA_VERSION.to_string(),
        origin: Origin::Synthetic,
        observation_schema: schema::observation::SCHEMA_VERSION.to_string(),
        fault_schema: "4".to_string(),
        map: Some("omaha_ne".into()),
        profile: ProfileId::new("d31_lowsplit_1002_iccap"),
        service_group: "sg-1".into(),
        modem: ElementId::new("modem-2"),
        channel: ChannelId::new("ofdm-1"),
        first_mhz: 811.1,
        seed: 7,
        causes: vec![CauseClass::Suckout],
        shapes: vec![ShapeLabel {
            shape: Shape::Dip,
            worst_fall_db: 18.0,
            bins: 2,
        }],
        worst_fall_db: 18.0,
        min_hex: "a050ff".into(),
        mean_hex: "a058ff".into(),
        fall_hex: "004800".into(),
        shapes_hex: "000004000000".into(),
    }
}

fn rl(f_mhz: f64, return_loss_db: f64) -> ReturnLossPoint {
    ReturnLossPoint {
        f_mhz,
        return_loss_db,
    }
}

fn no_rf(mut component: Component) -> Component {
    component.rated_lo_mhz = None;
    component.rated_hi_mhz = None;
    component
}

/// Version 2: every new class once, and every new optional field on a version 1 class set.
pub fn component_library_as_built() -> ComponentLibrary {
    let mut amp = active(diplexed());
    amp.outputs = vec![
        OutputSpec {
            port: PortId::new("out-1"),
            loss_from_main_db: 0.0,
            return_combining_loss_db: 3.5,
        },
        OutputSpec {
            port: PortId::new("out-2"),
            loss_from_main_db: 4.0,
            return_combining_loss_db: 7.0,
        },
    ];
    amp.plugin_family = Some("family-a".into());
    amp.nf_table = vec![NoiseFigurePoint {
        f_mhz: 1218.0,
        nf_db: 8.0,
    }];
    amp.cin_table = vec![CinPoint {
        f_mhz: 1218.0,
        cin_db: 56.0,
    }];
    amp.ref_composite_dbmv = Some(70.0);
    amp.gain_flatness_db = Some(0.75);
    amp.gain_table = vec![
        GainPoint {
            f_mhz: 258.0,
            gain_db: 26.0,
        },
        GainPoint {
            f_mhz: 1218.0,
            gain_db: 36.0,
        },
    ];
    amp.agc_pilots_mhz = vec![99.0, 1100.0];
    amp.return_loss_table = vec![rl(5.0, 16.0), rl(1218.0, 14.0)];
    let mut amp = component("amp-multi", ComponentSpec::Amp(amp));
    amp.tolerance = Some(Tolerance {
        loss_db: Some(0.5),
        gain_db: Some(1.0),
        r_loop_pct: Some(10.0),
        return_loss_db: Some(2.0),
        nf_db: Some(1.0),
        impl_mer_db: Some(1.5),
    });
    let mer = vec![MerPoint {
        f_mhz: 1218.0,
        mer_db: 48.0,
    }];
    let slope = Some(FreqRange {
        lo_mhz: 258.0,
        hi_mhz: 1218.0,
    });
    let cable = CableSpec {
        atten_a: 0.0598,
        atten_b: 0.00046,
        k_temp: 0.002,
        v_f: 0.87,
        r_loop_ohm_per_kft: 1.7,
        family: Some("500".into()),
        ampacity_a: Some(43.0),
        r_loop_k_temp: Some(0.004),
    };
    ComponentLibrary {
        schema_version: schema::component::SCHEMA_VERSION.into(),
        components: vec![
            amp,
            component(
                "node",
                ComponentSpec::Node(NodeSpec {
                    active: active(diplexed()),
                    optics_cnr_db: 48.0,
                    us_optics_snr_db: 40.0,
                    source_mer: mer.clone(),
                }),
            ),
            component(
                "rpd",
                ComponentSpec::Rpd(RpdSpec {
                    active: active(diplexed()),
                    supported_pnm_tests: vec![RpdPnmTest::UsRxmer],
                    source_mer: mer.clone(),
                    us_rx_ceiling_db: Some(45.0),
                }),
            ),
            component(
                "tap-terminating",
                ComponentSpec::Tap(TapSpec {
                    ports: 2,
                    tap_value_db: 8.0,
                    insertion_loss: loss(0.0, 0.0),
                    tap_loss: loss(8.0, 8.5),
                    port_isolation_db: 25.0,
                    return_loss_db: 18.0,
                    power_passing: false,
                    terminating: true,
                    plugin_family: Some("faceplate-a".into()),
                    return_loss_by_port: vec![PortReturnLoss {
                        port: TapPortClass::TapPort,
                        points: vec![rl(5.0, 20.0), rl(1218.0, 16.0)],
                    }],
                }),
            ),
            component(
                "splitter-home",
                ComponentSpec::Splitter(SplitterSpec {
                    legs: 2,
                    leg_loss: loss(3.5, 4.0),
                    return_loss_db: 18.0,
                    power_passing: false,
                    leg_isolation_db: Some(22.0),
                    return_loss_table: vec![rl(5.0, 20.0)],
                }),
            ),
            component(
                "coupler-12",
                ComponentSpec::DirectionalCoupler(DirectionalCouplerSpec {
                    coupled_value_db: 12.0,
                    excess_loss_db: 0.3,
                    return_loss_db: 18.0,
                    power_passing: true,
                    return_loss_table: vec![rl(5.0, 20.0)],
                }),
            ),
            component(
                "terminator-rl",
                ComponentSpec::Terminator(TerminatorSpec {
                    return_loss_db: 30.0,
                    return_loss_table: vec![rl(1218.0, 25.0)],
                }),
            ),
            component(
                "modem-rl",
                ComponentSpec::Modem(ModemSpec {
                    generation: ModemGeneration::D31,
                    impl_mer_db: 44.0,
                    nf_db: 10.0,
                    return_loss_db: 18.0,
                    tx_max_total_dbmv: 65.0,
                    capability: capability(),
                    return_loss_table: vec![rl(1218.0, 12.0)],
                }),
            ),
            component("cable-500", ComponentSpec::Cable(cable)),
            component(
                "inserter-lossy",
                ComponentSpec::PowerInserter(PowerInserterSpec {
                    i_rated_a: 20.0,
                    power_passing: true,
                    insertion_loss: loss(0.9, 1.6),
                    return_loss_db: Some(16.0),
                    port_i_rated_a: Some(15.0),
                }),
            ),
            component(
                "pad-6",
                ComponentSpec::PlugIn(PlugInSpec {
                    kind: PlugInKind::Pad,
                    family: "family-a".into(),
                    value_db: 6.0,
                    slope_band: None,
                    insertion_loss_db: 0.3,
                    loss_table: Vec::new(),
                    return_loss_db: Some(25.0),
                }),
            ),
            component(
                "eq-10",
                ComponentSpec::PlugIn(PlugInSpec {
                    kind: PlugInKind::Equalizer,
                    family: "family-a".into(),
                    value_db: 10.0,
                    slope_band: slope,
                    insertion_loss_db: 1.0,
                    loss_table: loss(10.5, 1.0),
                    return_loss_db: None,
                }),
            ),
            component(
                "line-eq",
                ComponentSpec::LineEqualizer(LineEqualizerSpec {
                    value_db: 8.0,
                    slope_band: FreqRange {
                        lo_mhz: 54.0,
                        hi_mhz: 1002.0,
                    },
                    insertion_loss_db: 1.0,
                    loss_table: Vec::new(),
                    return_loss_db: 18.0,
                    power_passing: true,
                    duplex: Some(diplexed()),
                }),
            ),
            component(
                "pin",
                ComponentSpec::Connector(ConnectorSpec {
                    kind: ConnectorKind::HardlinePin,
                    cable_family: Some("500".into()),
                    insertion_loss_db: 0.05,
                    return_loss_db: 30.0,
                    return_loss_table: vec![rl(1218.0, 26.0)],
                }),
            ),
            component(
                "ground-block",
                ComponentSpec::GroundBlock(GroundBlockSpec {
                    insertion_loss: loss(0.2, 0.4),
                    return_loss_db: 20.0,
                }),
            ),
            component(
                "drop-amp",
                ComponentSpec::DropAmp(DropAmpSpec {
                    outputs: 4,
                    gain_db: 10.0,
                    return_gain_db: None,
                    return_passive_loss_db: Some(1.5),
                    nf_db: 5.0,
                    max_out_dbmv_at_hi: Some(30.0),
                    return_loss_db: 16.0,
                    duplex: diplexed(),
                    return_nf_db: Some(8.0),
                }),
            ),
            component(
                "filter",
                ComponentSpec::Filter(FilterSpec {
                    shape: FilterShape::BandStop,
                    stop_bands: vec![FreqRange {
                        lo_mhz: 1125.0,
                        hi_mhz: 1675.0,
                    }],
                    stop_depth_db: 40.0,
                    pass_loss_db: 1.0,
                    return_loss_db: 18.0,
                    stop_return_loss_db: 1.0,
                    transition_mhz: Some(123.0),
                }),
            ),
            component(
                "set-top",
                ComponentSpec::Cpe(CpeSpec {
                    device: CpeDevice::SetTop,
                    return_loss_db: 10.0,
                    return_loss_table: Vec::new(),
                }),
            ),
            no_rf(component(
                "battery",
                ComponentSpec::Battery(BatterySpec {
                    v_nominal: 12.0,
                    capacity_ah: 100.0,
                    v_end_discharge: 10.2,
                }),
            )),
            no_rf(component(
                "pole",
                ComponentSpec::Structure(StructureSpec {
                    kind: StructureKind::Pole,
                }),
            )),
            no_rf(component("strand", ComponentSpec::Strand(StrandSpec {}))),
            no_rf(component("conduit", ComponentSpec::Conduit(ConduitSpec {}))),
            no_rf(component(
                "fiber-48",
                ComponentSpec::FiberCable(FiberCableSpec {
                    fiber_count: 48,
                    fibers_per_tube: 12,
                    fiber_type: FiberType::G652D,
                    construction: FiberConstruction::LooseTube,
                    loss: vec![WavelengthLoss {
                        wavelength_nm: 1310.0,
                        loss_db_per_kft: 0.107,
                    }],
                }),
            )),
            no_rf(component(
                "closure",
                ComponentSpec::SpliceClosure(SpliceClosureSpec {
                    splice_capacity: 144,
                    trays: 6,
                    splice_loss_db: Some(0.05),
                }),
            )),
            no_rf(component(
                "panel",
                ComponentSpec::PatchPanel(PatchPanelSpec {
                    ports: 48,
                    splice_loss_db: Some(0.05),
                }),
            )),
            no_rf(component(
                "sc-apc",
                ComponentSpec::FiberConnector(FiberConnectorSpec {
                    style: "SC".into(),
                    polish: FiberPolish::Apc,
                    insertion_loss_db: 0.3,
                    return_loss_db: 60.0,
                }),
            )),
            no_rf(component(
                "mux",
                ComponentSpec::OpticalPassive(OpticalPassiveSpec {
                    kind: OpticalPassiveKind::Mux,
                    ports: 2,
                    insertion_loss_db: 1.0,
                    channels_nm: vec![1310.0, 1550.0],
                    isolation_db: Some(30.0),
                    return_loss_db: Some(45.0),
                }),
            )),
            component(
                "forward-tx",
                ComponentSpec::OpticalTx(OpticalTxSpec {
                    direction: OpticalDirection::Forward,
                    laser: LaserKind::Dfb,
                    wavelength_nm: 1310.0,
                    out_dbm: 10.0,
                    rin_db_per_hz: -160.0,
                    omi_pct: 3.0,
                    ref_rf_in_dbmv: 20.0,
                    clip_total_in_dbmv: Some(40.0),
                }),
            ),
            component(
                "forward-rx",
                ComponentSpec::OpticalRx(OpticalRxSpec {
                    direction: OpticalDirection::Forward,
                    in_min_dbm: -6.0,
                    in_max_dbm: 2.0,
                    responsivity_a_per_w: 0.85,
                    ein_pa_per_rthz: 7.0,
                    rf_out_dbmv: 30.0,
                    ref_in_dbm: 0.0,
                    agc_range_db: Some(6.0),
                }),
            ),
            no_rf(component(
                "edfa",
                ComponentSpec::OpticalAmp(OpticalAmpSpec {
                    out_dbm: 20.0,
                    nf_db: 5.0,
                    in_min_dbm: -5.0,
                    in_max_dbm: 10.0,
                    band_lo_nm: 1530.0,
                    band_hi_nm: 1565.0,
                }),
            )),
            no_rf(component(
                "sfp-lr",
                ComponentSpec::Transceiver(TransceiverSpec {
                    form_factor: "SFP+".into(),
                    rate_gbps: 10.0,
                    wavelength_nm: 1310.0,
                    rx_wavelength_nm: Some(1270.0),
                    tx_min_dbm: -8.2,
                    tx_max_dbm: 0.5,
                    rx_sensitivity_dbm: -14.4,
                    rx_overload_dbm: 0.5,
                }),
            )),
            no_rf(component(
                "switch",
                ComponentSpec::Switch(SwitchSpec {
                    ports: 48,
                    port_rate_gbps: 10.0,
                }),
            )),
            component(
                "cmts",
                ComponentSpec::Cmts(CmtsSpec {
                    kind: CmtsKind::Integrated,
                    ports: 16,
                    source_mer: mer,
                    us_rx_ceiling_db: Some(45.0),
                }),
            ),
            no_rf(component(
                "core",
                ComponentSpec::Cmts(CmtsSpec {
                    kind: CmtsKind::Core,
                    ports: 64,
                    source_mer: Vec::new(),
                    us_rx_ceiling_db: None,
                }),
            )),
            no_rf(component(
                "grandmaster",
                ComponentSpec::TimingSource(TimingSourceSpec { ports: 4 }),
            )),
        ],
    }
}

fn at(lon_deg: f64, lat_deg: f64) -> LonLat {
    LonLat { lon_deg, lat_deg }
}

fn structure(id: &str, kind: StructureKind) -> Structure {
    Structure {
        id: StructureId::new(id),
        kind,
        component_ref: ComponentId::new(id),
        geo: Some(at(-105.1, 39.9)),
        record_status: RecordStatus::Built,
    }
}

/// Version 2: the version 1 sample with every new plant feature once. A shape sample: it isn't validated.
pub fn plant_as_built() -> Plant {
    use PortRole::*;
    let mut plant = plant();
    for element in &mut plant.elements {
        match element.id.as_str() {
            "rpd" => {
                element.fitted = Some(Fitted::Active(ActiveFitted {
                    transceivers: vec![ComponentId::new("sfp-lr")],
                    ..ActiveFitted::default()
                }));
                element.structure = Some(StructureId::new("pole-1"));
            }
            "amp" => {
                element.fitted = Some(Fitted::Active(ActiveFitted {
                    input_pad: Some(ComponentId::new("pad-6")),
                    input_eq: Some(ComponentId::new("eq-10")),
                    outputs: vec![OutputFitted {
                        port: PortId::new("out"),
                        pad: Some(ComponentId::new("pad-6")),
                        eq: Some(ComponentId::new("eq-10")),
                    }],
                    return_pad: Some(ComponentId::new("pad-6")),
                    return_eq: Some(ComponentId::new("eq-10")),
                    set_point: Some(SetPoint {
                        out_dbmv_at_hi: 52.0,
                        tilt_db: 12.0,
                    }),
                    forward_rx: Some(ComponentId::new("forward-rx")),
                    return_tx: Some(ComponentId::new("forward-tx")),
                    transceivers: Vec::new(),
                }));
            }
            "tap" => {
                element.fitted = Some(Fitted::Passive(PassiveFitted {
                    conditioner: Some(ComponentId::new("eq-10")),
                    ports: vec![
                        PortStateEntry {
                            port: PortId::new("tap-2"),
                            state: PortState::Terminated {
                                component_ref: ComponentId::new("terminator"),
                            },
                        },
                        PortStateEntry {
                            port: PortId::new("tap-3"),
                            state: PortState::Open,
                        },
                    ],
                }));
            }
            _ => {}
        }
    }
    plant.elements.extend([
        element(
            "ground-block",
            ElementKind::GroundBlock,
            &[("in", Input), ("out", Through)],
        ),
        element(
            "line-eq",
            ElementKind::LineEqualizer,
            &[("in", Input), ("out", Through)],
        ),
        element(
            "drop-amp",
            ElementKind::DropAmp,
            &[("in", Input), ("out-1", Output)],
        ),
        element(
            "filter",
            ElementKind::Filter,
            &[("in", Input), ("out", Through)],
        ),
        element("set-top", ElementKind::Cpe, &[("rf", Input)]),
    ]);
    if let Some(block) = plant
        .elements
        .iter_mut()
        .find(|e| e.id.as_str() == "ground-block")
    {
        block.fitted = Some(Fitted::GroundBlock(GroundBlockFitted { coupling_db: 30.0 }));
    }
    let fittings = vec![
        Fitting {
            pos_ft: 0.0,
            component_ref: ComponentId::new("pin"),
        },
        Fitting {
            pos_ft: 100.0,
            component_ref: ComponentId::new("pin"),
        },
    ];
    let route = vec![RoutePoint {
        geo: at(-105.1, 39.9),
        structure: Some(StructureId::new("pole-1")),
    }];
    plant.edges[0].kind = EdgeKind::Span {
        cable_ref: ComponentId::new("cable-500"),
        len_ft: 100.0,
        installation: Installation::Aerial,
        power_passing: true,
        fittings: fittings.clone(),
        support: Some(ComponentId::new("strand")),
        path: route.clone(),
    };
    plant.edges[0].record_status = Some(RecordStatus::Designed);
    plant.edges.extend([
        edge(
            "drop-2",
            ("tap", "tap-4"),
            ("ground-block", "in"),
            EdgeKind::Drop {
                cable_ref: ComponentId::new("drop"),
                len_ft: 120.0,
                fittings: vec![Fitting {
                    pos_ft: 120.0,
                    component_ref: ComponentId::new("f-connector"),
                }],
                path: route,
                installation: Some(Installation::Underground),
            },
        ),
        edge(
            "house-1",
            ("ground-block", "out"),
            ("set-top", "rf"),
            EdgeKind::House {
                cable_ref: ComponentId::new("drop"),
                len_ft: 30.0,
                fittings,
            },
        ),
    ]);
    plant.power_supplies[0].batteries = Some(BatteryBank {
        component_ref: ComponentId::new("battery"),
        per_string: 3,
        strings: 1,
    });
    plant.power_supplies[0].structure = Some(StructureId::new("cabinet-1"));
    plant.power_supplies[0].record_status = Some(RecordStatus::Built);
    plant.structures = vec![
        structure("pole-1", StructureKind::Pole),
        structure("pedestal-1", StructureKind::Pedestal),
        structure("vault-1", StructureKind::Vault),
        structure("cabinet-1", StructureKind::Cabinet),
    ];
    plant
}

/// A fiber plant with every element kind, hop kind and direction once. A shape sample: it isn't validated.
pub fn fiber_plant() -> schema::fiber::FiberPlant {
    use schema::common::{FiberCableId, FiberElementId, HubId, LinkId};
    use schema::fiber::*;
    let id = |name: &str| FiberElementId::new(name);
    let fiber_element = |name: &str, kind: FiberElementKind| FiberElement {
        id: id(name),
        kind,
        component_ref: ComponentId::new(name),
        geo: Some(at(-105.11, 39.9)),
        hub: Some(HubId::new("hub-1")),
        structure: None,
        ports: vec![PortId::new("in"), PortId::new("out")],
        record_status: RecordStatus::Built,
    };
    let mut closure = fiber_element("closure", FiberElementKind::SpliceClosure);
    closure.hub = None;
    closure.structure = Some(StructureId::new("pole-1"));
    closure.ports = Vec::new();
    FiberPlant {
        schema_version: schema::fiber::SCHEMA_VERSION.into(),
        id: "fiber-1".into(),
        origin: Origin::Synthetic,
        hubs: vec![Hub {
            id: HubId::new("hub-1"),
            geo: at(-105.11, 39.9),
            record_status: RecordStatus::Built,
        }],
        elements: vec![
            fiber_element("cmts", FiberElementKind::Cmts),
            fiber_element("forward-tx", FiberElementKind::OpticalTx),
            fiber_element("return-rx", FiberElementKind::OpticalRx),
            fiber_element("edfa", FiberElementKind::OpticalAmp),
            fiber_element("mux", FiberElementKind::OpticalPassive),
            fiber_element("switch", FiberElementKind::Switch),
            fiber_element("sfp-lr", FiberElementKind::Transceiver),
            fiber_element("grandmaster", FiberElementKind::TimingSource),
            fiber_element("panel", FiberElementKind::PatchPanel),
            closure,
            fiber_element(
                "node-end",
                FiberElementKind::NodeEnd {
                    service_group: ServiceGroupId::new("sg-1"),
                },
            ),
        ],
        cables: vec![FiberCable {
            id: FiberCableId::new("cable-1"),
            component_ref: ComponentId::new("fiber-48"),
            from: id("panel"),
            to: id("closure"),
            len_ft: 5000.0,
            installation: Installation::Aerial,
            path: vec![RoutePoint {
                geo: at(-105.105, 39.9),
                structure: Some(StructureId::new("pole-1")),
            }],
            support: Some(ComponentId::new("strand")),
            record_status: RecordStatus::Designed,
        }],
        links: vec![OpticalLink {
            id: LinkId::new("link-1"),
            service_group: ServiceGroupId::new("sg-1"),
            direction: OpticalDirection::Both,
            wavelength_nm: 1310.0,
            hops: vec![
                Hop::Connector {
                    at: id("panel"),
                    component_ref: ComponentId::new("sc-apc"),
                },
                Hop::Through {
                    element: id("mux"),
                    port_in: PortId::new("in"),
                    port_out: PortId::new("out"),
                },
                Hop::Fiber {
                    cable: FiberCableId::new("cable-1"),
                    strand: 1,
                },
                Hop::Splice {
                    at: id("closure"),
                    loss_db: Some(0.05),
                },
                Hop::Splice {
                    at: id("closure"),
                    loss_db: None,
                },
            ],
        }],
        bindings: vec![ServiceGroupBinding {
            service_group: ServiceGroupId::new("sg-1"),
            cmts: id("cmts"),
            port: PortId::new("in"),
        }],
        structures: vec![structure("pole-1", StructureKind::Pole)],
    }
}

/// A rules file reaching every variant. A shape sample: its values mean nothing.
pub fn design_rules() -> schema::design_rules::DesignRules {
    use schema::design_rules::*;
    fn sourced<T>(value: T) -> Ruled<T> {
        Ruled {
            value,
            basis: Basis::Sourced {
                note: "design-practice.md §1".into(),
            },
        }
    }
    fn generic<T>(value: T) -> Ruled<T> {
        Ruled {
            value,
            basis: Basis::Generic,
        }
    }
    let id = ComponentId::new;
    DesignRules {
        schema_version: schema::design_rules::SCHEMA_VERSION.into(),
        id: "sample-rules".into(),
        tap_port: vec![TapPortTarget {
            profile_ref: profile_ref(),
            at_hi_dbmv: sourced(14.0),
            at_lo_dbmv: Some(generic(4.0)),
            tolerance_db: generic(2.0),
            tilt_tolerance_db: None,
        }],
        premises_allowance: PremisesAllowance {
            drop_loss_db: generic(7.0),
            house_loss_db: generic(8.0),
        },
        end_of_line: EndOfLine {
            last_tap_terminating: generic(true),
            min_port_dbmv_at_hi: generic(12.0),
        },
        tap_families: vec![TapFamily {
            ports: 4,
            values_db: generic(vec![11.0, 14.0, 17.0]),
            terminating_values_db: generic(vec![8.0, 11.0]),
            parts: None,
        }],
        roles: vec![
            RoleRule {
                role: LineRole::Express,
                cable_ref: generic(id("cable")),
                amp_ref: generic(id("amp")),
            },
            RoleRule {
                role: LineRole::Feeder,
                cable_ref: generic(id("cable")),
                amp_ref: generic(id("amp")),
            },
        ],
        drops: vec![DropRule {
            max_len_ft: generic(150.0),
            cable_ref: generic(id("drop")),
        }],
        node: NodeRules {
            max_homes: generic(500),
            rf_ports: generic(4),
            max_cascade: generic(5),
            max_actives: None,
        },
        powering: PoweringRules {
            supply_ref: generic(id("power-supply")),
            design_load_pct: sourced(80.0),
            port_max_a: generic(15.0),
            battery_ref: generic(id("battery")),
            batteries_per_string: generic(3),
            battery_strings: generic(1),
            inserter_ref: None,
            min_margin_v: None,
        },
        structures: StructureRules {
            pole_spacing_ft: generic(150.0),
            pedestal_spacing_ft: generic(300.0),
            expansion_loop_each_pole: generic(true),
            strand_ref: generic(id("strand")),
            conduit_ref: generic(id("conduit")),
            side: None,
            setback_ft: None,
            pole_ref: None,
            pedestal_ref: None,
        },
        premises: vec![PremisesTemplate {
            id: "bare".into(),
            share: generic(1.0),
            basis: Basis::Generic,
            ground_block_ref: Some(id("ground-block")),
            drop_amp_ref: Some(id("drop-amp")),
            filter_ref: Some(id("filter")),
            splitter_ref: Some(id("splitter")),
            house_cable_ft: 40.0,
            modems: 1,
            cpe: vec![
                CpeDevice::SetTop,
                CpeDevice::Television,
                CpeDevice::MocaAdapter,
                CpeDevice::Other,
            ],
        }],
        fiber: FiberRules {
            fibers_per_analog_node: generic(2),
            fibers_per_rpd: generic(2),
            cable_counts: generic(vec![12, 24, 48]),
            reel_len_ft: generic(20000.0),
            link_margin_db: generic(3.0),
            splice_loss_db: generic(0.1),
            connector_loss_db: generic(0.5),
            cable_refs: None,
            closure_ref: None,
            panel_ref: None,
            connector_ref: None,
            jumper_ref: None,
            duplex_jumper_ref: None,
            jumper_len_ft: None,
            composite_index_max: None,
        },
        costs: CostTable {
            parts: vec![PartCost {
                component_ref: id("amp"),
                usd: generic(1500.0),
            }],
            construction: [
                ConstructionKind::AerialCoax,
                ConstructionKind::UndergroundCoax,
                ConstructionKind::AerialFiber,
                ConstructionKind::UndergroundFiber,
                ConstructionKind::AerialDrop,
                ConstructionKind::BuriedDrop,
            ]
            .into_iter()
            .map(|kind| ConstructionCost {
                kind,
                usd_per_ft: generic(10.0),
            })
            .collect(),
            plug_in_families: None,
            ceiling_usd_per_home: None,
        },
        profiles: None,
        conditioning: None,
        branching: None,
        premises_parts: None,
        fittings: None,
        candidates: None,
        upgrade: None,
    }
}

/// [`design_rules`] with every version 2 field, reaching every new variant. A shape sample: its values mean nothing.
pub fn design_rules_v2_fields() -> schema::design_rules::DesignRules {
    use schema::design_rules::*;
    fn generic<T>(value: T) -> Ruled<T> {
        Ruled {
            value,
            basis: Basis::Generic,
        }
    }
    let id = ComponentId::new;
    let mut rules = design_rules();
    rules.tap_port[0].tilt_tolerance_db = Some(generic(2.0));
    rules.tap_families[0].parts = Some(generic(vec![id("tap"), id("tap-terminating")]));
    rules.node.max_actives = Some(generic(20));
    rules.powering.inserter_ref = Some(generic(id("inserter")));
    rules.powering.min_margin_v = Some(generic(5.0));
    rules.structures.side = Some(generic(StreetSide::Right));
    rules.structures.setback_ft = Some(generic(10.0));
    rules.structures.pole_ref = Some(generic(id("pole")));
    rules.structures.pedestal_ref = Some(generic(id("pedestal")));
    rules.fiber.cable_refs = Some(generic(vec![id("fiber-12")]));
    rules.fiber.closure_ref = Some(generic(id("closure")));
    rules.fiber.panel_ref = Some(generic(id("panel")));
    rules.fiber.connector_ref = Some(generic(id("fiber-connector")));
    rules.fiber.jumper_ref = Some(generic(id("jumper")));
    rules.fiber.duplex_jumper_ref = Some(generic(id("jumper-duplex")));
    rules.fiber.jumper_len_ft = Some(generic(15.0));
    rules.fiber.composite_index_max = Some(generic(0.3));
    rules.costs.plug_in_families = Some(vec![FamilyCost {
        family: "pads".into(),
        usd: generic(8.0),
    }]);
    rules.costs.ceiling_usd_per_home = Some(generic(2500.0));
    let station = StationLevels {
        out_dbmv_at_hi: generic(50.0),
        min_out_dbmv_at_hi: generic(44.0),
        tilt_db: generic(14.0),
        min_tilt_db: generic(8.0),
    };
    rules.profiles = Some(vec![
        ProfileRules {
            profile_ref: profile_ref(),
            root_ref: generic(id("node")),
            station: station.clone(),
            premises: generic(vec!["bare".into()]),
            optics: generic(NodeOptics::Analog {
                cmts_ref: id("cmts"),
                forward_tx: vec![id("forward-tx")],
                hub_couplers: vec![id("coupler")],
                hub_amps: Vec::new(),
                node_rx: vec![id("node-rx")],
                return_tx: vec![id("return-tx")],
                hub_rx: vec![id("hub-rx")],
            }),
        },
        ProfileRules {
            profile_ref: ProfileId::new("another-profile"),
            root_ref: generic(id("rpd")),
            station,
            premises: generic(vec!["bare".into()]),
            optics: generic(NodeOptics::Digital {
                core_ref: id("core"),
                switch_ref: id("switch"),
                transceivers: vec![id("sfp")],
                timing_ref: id("timing"),
            }),
        },
    ]);
    rules.conditioning = Some(ConditioningRules {
        conditioners: generic(vec![id("conditioner")]),
        line_equalizers: generic(vec![id("line-equalizer")]),
    });
    rules.branching = Some(BranchRules {
        splitter_ref: generic(id("splitter")),
        couplers: generic(vec![id("coupler-8"), id("coupler-12")]),
    });
    rules.premises_parts = Some(PremisesParts {
        modem_d30_ref: generic(id("modem-d30")),
        modem_d31_ref: generic(id("modem-d31")),
        cpe_refs: generic(vec![CpeRef {
            device: CpeDevice::SetTop,
            component_ref: id("set-top"),
        }]),
        house_cable_ref: generic(id("drop")),
        unused_port: generic(UnusedPort::Terminated),
        terminator_ref: generic(id("terminator")),
        drop_amp_below_dbmv: generic(-6.0),
    });
    rules.fittings = Some(FittingRules {
        connectors: generic(vec![id("pin"), id("f")]),
        hardline_reel_len_ft: generic(2000.0),
        jumper_len_ft: generic(3.0),
    });
    rules.candidates = Some(CandidateRules {
        node_count_steps: generic(vec![-1, 0, 1]),
        express_cables: generic(vec![id("cable")]),
    });
    rules.upgrade = Some(UpgradeRules {
        moves: [
            UpgradeMove::SwapActiveModules,
            UpgradeMove::SwapTapFaceplates,
            UpgradeMove::SwapPassives,
            UpgradeMove::ChangePlugIns,
            UpgradeMove::RemovePremisesParts,
            UpgradeMove::SwapModems,
        ]
        .into_iter()
        .map(generic)
        .collect(),
    });
    let _ = UnusedPort::Open;
    let _ = StreetSide::Left;
    rules
}

/// Task 26: a world's call centre and field records, one of each.
pub fn ops_records() -> schema::ops::OpsRecords {
    use schema::common::{ElementId, LonLat, Origin, ServiceGroupId};
    use schema::ops::*;
    let sg = ServiceGroupId::new("sg-1");
    let at = LonLat {
        lon_deg: -105.09,
        lat_deg: 39.9,
    };
    OpsRecords {
        schema_version: SCHEMA_VERSION.to_string(),
        origin: Origin::Synthetic,
        calls: vec![Call {
            id: 1,
            t_s: 3720,
            home: ElementId::new("modem-5"),
            service_group: sg.clone(),
            reason: CallReason::NoService,
            answered_t_s: Some(3900),
            abandoned_t_s: None,
            ticket: Some(2),
        }],
        tickets: vec![Ticket {
            id: 2,
            service_group: sg.clone(),
            opened_t_s: 3900,
            first_home: ElementId::new("modem-5"),
            calls: vec![1],
            status: TicketStatus::Dispatched,
            jobs: vec![3],
            closed_t_s: None,
        }],
        jobs: vec![Job {
            id: 3,
            created_t_s: 3600,
            service_group: sg,
            target: JobTarget::Plant {
                element: ElementId::new("amp-2"),
            },
            geo: at,
            source: JobSource::Telemetry,
            crew: Crew::Line,
            truck: Some(4),
            arrived_t_s: Some(4500),
            done_t_s: Some(18_900),
            outcome: Some(Outcome::Fixed {
                what: "a dig-in through buried cable".into(),
            }),
        }],
        trucks: vec![Truck {
            id: 4,
            crew: Crew::Line,
            geo: at,
            state: TruckState::Driving {
                job: 3,
                path: vec![at, at],
                depart_t_s: 3660,
                arrive_t_s: 4500,
            },
            on_shift: true,
        }],
        counters: Counters {
            customer_minutes_out: 240,
            homes_out_now: 4,
            calls: 1,
            calls_answered: 1,
            calls_abandoned: 0,
            calls_told_of_outage: 0,
            tickets: 1,
            truck_rolls: 1,
            repeat_rolls: 0,
            no_trouble_found: 0,
            fixes: 1,
            onset_to_first_call: (2, 1),
            onset_to_dispatch: (0, 1),
            onset_to_fix: (255, 1),
        },
    }
}
