//! Telemetry over the real path: sim → emulated devices → SNMP and TFTP → adapter → the same schema. All
//! synthetic, and the adapter has to say so (invariant 11).

use std::fs;
use std::net::UdpSocket;

use adapter::collect::{Budget, Collector};
use adapter::identity::{bind_plant, pseudonym};
use diagnosis::{DiagnoserConfig, diagnose};
use emulator::{Emulator, EmulatorConfig};
use features::{DesignReference, features_for};
use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId, Origin, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::EnvironmentSample;
use schema::fault::{Fault, FaultKind, Location, Onset};
use schema::observation::{DsRxLevels, ModemObservation};
use schema::plant::Plant;
use schema::profile::Profile;
use schema::work_order::CauseClass;

const KEY: &[u8] = b"test key, not a secret";

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn calm(t: i64) -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(t),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

fn free_udp_port() -> u16 {
    UdpSocket::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct Setup {
    plant: Plant,
    library: ComponentLibrary,
    profile: Profile,
    emulator: Emulator,
    tftp_port: u16,
}

fn setup(plant_file: &str, profile_file: &str) -> Setup {
    let plant: Plant = read(plant_file);
    let library: ComponentLibrary = read("library/generic.json");
    let profile: Profile = read(profile_file);
    let tftp_port = free_udp_port();
    let emulator = Emulator::start(
        EmulatorConfig {
            plant: plant.clone(),
            library: library.clone(),
            profile: profile.clone(),
            seed: 42,
            randomize: Some(42),
            measurement_noise_db: 0.25,
            tftp_port,
        },
        calm(0),
    )
    .unwrap();
    // An inventory of which MAC hangs where: the input an operator's records provide.
    let inventory: Vec<(ElementId, [u8; 6])> = emulator
        .modems
        .iter()
        .map(|(id, mac, _)| (ElementId::new(id.as_str()), *mac))
        .collect();
    let mut bound = plant;
    bind_plant(&mut bound, KEY, &inventory);
    Setup {
        plant: bound,
        library,
        profile,
        emulator,
        tftp_port,
    }
}

fn high() -> Setup {
    setup(
        "one_street/plant_highsplit_rpd.json",
        "profiles/d31_highsplit_1218_rpd.json",
    )
}

fn low() -> Setup {
    setup(
        "one_street/plant_lowsplit_iccap.json",
        "profiles/d31_lowsplit_1002_iccap.json",
    )
}

impl Setup {
    fn collector(&self) -> Collector<'_> {
        Collector::connect(
            &self.plant,
            &self.profile,
            &self.library,
            self.emulator.cmts_addr,
            KEY,
            self.tftp_port,
            Budget::default(),
        )
        .unwrap()
    }
}

fn near(a: f64, b: f64, tolerance: f64, what: &str) {
    assert!((a - b).abs() <= tolerance, "{what}: {a} against {b}");
}

/// What crossed the wire equals what the sim emitted, to each field's encoding.
fn assert_same(wire: &ModemObservation, truth: &ModemObservation) {
    assert_eq!(wire.element_id, truth.element_id);
    assert_eq!(wire.generation, truth.generation);
    assert_eq!(wire.reg_state, truth.reg_state);
    assert_eq!(wire.t3_timeouts, truth.t3_timeouts);
    assert_eq!(wire.t4_timeouts, truth.t4_timeouts);

    assert_eq!(wire.ds_rx.len(), truth.ds_rx.len());
    for t in &truth.ds_rx {
        let w = wire
            .ds_rx
            .iter()
            .find(|r| r.channel_id == t.channel_id)
            .unwrap();
        match (&w.ds_rx_dbmv, &t.ds_rx_dbmv) {
            (DsRxLevels::ScQam { level_dbmv: a }, DsRxLevels::ScQam { level_dbmv: b }) => {
                near(*a, *b, 0.051, "SC-QAM level")
            }
            (
                DsRxLevels::Ofdm {
                    bands: a,
                    plc_band_dbmv: pa,
                },
                DsRxLevels::Ofdm {
                    bands: b,
                    plc_band_dbmv: pb,
                },
            ) => {
                near(*pa, *pb, 0.051, "PLC level");
                assert_eq!(a.len(), b.len());
                for (x, y) in a.iter().zip(b) {
                    near(x.level_dbmv, y.level_dbmv, 0.051, "band level");
                }
            }
            _ => panic!("channel changed kind"),
        }
    }

    assert_eq!(wire.codewords.len(), truth.codewords.len());
    for t in &truth.codewords {
        let w = wire
            .codewords
            .iter()
            .find(|c| c.channel_id == t.channel_id && c.data_profile_id == t.data_profile_id)
            .unwrap();
        assert_eq!(
            (w.total, w.corrected, w.uncorrectable),
            (t.total, t.corrected, t.uncorrectable)
        );
    }

    assert_eq!(wire.rxmer.is_some(), truth.rxmer.is_some());
    for t in truth.rxmer.iter().flatten() {
        let w = wire
            .rxmer
            .as_ref()
            .unwrap()
            .iter()
            .find(|r| r.channel_id == t.channel_id)
            .unwrap();
        let (a, b) = (w.rxmer_db.as_ref().unwrap(), t.rxmer_db.as_ref().unwrap());
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            // A quarter dB is the file's resolution [CM-OSSI §D.2.1.3].
            near(x.unwrap(), y.unwrap(), 0.126, "RxMER");
        }
        assert_eq!(w.axis.first_active_index, t.axis.first_active_index);
        near(w.axis.zero_freq_mhz, t.axis.zero_freq_mhz, 1e-6, "SC0");
        near(
            w.rxmer_mean_db.unwrap(),
            t.rxmer_mean_db.unwrap(),
            0.13,
            "mean",
        );
    }

    for t in truth.ds_chan_est.iter().flatten() {
        let w = wire
            .ds_chan_est
            .as_ref()
            .unwrap()
            .iter()
            .find(|r| r.channel_id == t.channel_id)
            .unwrap();
        for (x, y) in w
            .coefficients
            .as_ref()
            .unwrap()
            .iter()
            .zip(t.coefficients.as_ref().unwrap())
        {
            let (x, y) = (x.unwrap(), y.unwrap());
            near(x.re, y.re, 1.0 / 8192.0, "coefficient re");
            near(x.im, y.im, 1.0 / 8192.0, "coefficient im");
        }
    }

    for t in truth.preeq.sc_qam.iter().flatten() {
        let w = wire
            .preeq
            .sc_qam
            .as_ref()
            .unwrap()
            .iter()
            .find(|r| r.channel_id == t.channel_id)
            .unwrap();
        assert_eq!(w.main_tap_index, t.main_tap_index);
        near(w.nmter_db, t.nmter_db, 0.5, "NMTER from taps");
    }
    for t in truth.preeq.ofdma.iter().flatten() {
        let w = wire
            .preeq
            .ofdma
            .as_ref()
            .unwrap()
            .iter()
            .find(|r| r.channel_id == t.channel_id)
            .unwrap();
        for (x, y) in w
            .coefficients
            .as_ref()
            .unwrap()
            .iter()
            .zip(t.coefficients.as_ref().unwrap())
        {
            near(x.re, y.re, 1.0 / 8192.0, "pre-eq re");
        }
    }

    for t in &truth.us_tx {
        let w = wire
            .us_tx
            .iter()
            .find(|r| r.channel_id == t.channel_id)
            .unwrap();
        near(
            w.psd_dbmv_per_1p6mhz,
            t.psd_dbmv_per_1p6mhz,
            0.2,
            "transmit PSD",
        );
    }
    for t in &truth.us_rx {
        let w = wire
            .us_rx
            .iter()
            .find(|r| r.channel_id == t.channel_id)
            .unwrap();
        near(
            w.us_rx_dbmv_per_1p6mhz,
            t.us_rx_dbmv_per_1p6mhz,
            0.051,
            "receive PSD",
        );
    }

    if let (Some(a), Some(b)) = (&wire.fbc, &truth.fbc) {
        assert_eq!(a.fbc_dbmv.len(), b.fbc_dbmv.len());
        near(
            a.first_bin_center_mhz,
            b.first_bin_center_mhz,
            1e-6,
            "FBC start",
        );
        near(a.bin_width_khz, b.bin_width_khz, 1e-3, "FBC bin");
        for (x, y) in a.fbc_dbmv.iter().zip(&b.fbc_dbmv) {
            near(*x, *y, 0.006, "FBC bin level");
        }
    }
}

#[test]
fn every_field_survives_the_wire_on_both_profiles() {
    for setup in [high(), low()] {
        let collector = setup.collector();
        assert_eq!(collector.addresses.len(), 8);
        let step = collector.collect(Timestamp(0)).unwrap();
        assert_eq!(step.modems.len(), 8);
        for wire in &step.modems {
            let truth = setup.emulator.truth(wire.element_id.as_str()).unwrap();
            assert_same(wire, &truth);
            match wire.generation {
                schema::component::ModemGeneration::D30 => assert!(wire.rxmer.is_none()),
                schema::component::ModemGeneration::D31 => assert!(wire.rxmer.is_some()),
            }
        }
        let truth = setup.emulator.truth_step().unwrap();
        for t in &truth.service_group.us_snr {
            let w = step
                .service_group
                .us_snr
                .iter()
                .find(|r| r.channel_id == t.channel_id)
                .unwrap();
            near(w.us_snr_db, t.us_snr_db, 0.051, "upstream SNR");
        }
        assert_eq!(
            step.service_group.us_codewords.len(),
            truth.service_group.us_codewords.len()
        );
    }
}

#[test]
fn it_says_it_is_synthetic_and_keeps_macs_out_of_the_plant() {
    let setup = high();
    let collector = setup.collector();
    assert_eq!(collector.origin().unwrap(), Origin::Synthetic);
    let step = collector.collect(Timestamp(0)).unwrap();
    assert!(step.modems.iter().all(|m| m.origin == Origin::Synthetic));

    // Pseudonymized at ingest: nothing in the plant or the observations carries a MAC.
    let plant_text = serde_json::to_string(&setup.plant).unwrap();
    let step_text = serde_json::to_string(&step.modems[0].element_id).unwrap();
    for (_, mac, _) in &setup.emulator.modems {
        let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
        assert!(!plant_text.contains(&hex));
        assert!(!step_text.contains(&hex));
        assert!(plant_text.contains(&pseudonym(KEY, mac)));
    }
}

#[test]
fn a_modem_refuses_a_second_test_while_one_runs() {
    use pnm_wire::oids::{self, Test};
    use pnm_wire::snmp::Value;
    let setup = high();
    let d31 = setup
        .emulator
        .modems
        .iter()
        .find(|(id, _, _)| id == "modem-2")
        .unwrap();
    let mut client = adapter::SnmpClient::new(d31.2, b"public").unwrap();
    // With nowhere to upload, a trigger is refused outright [CM-OSSI §D.5.3].
    let err = client
        .set(&Test::DsRxmer.enable(3000), Value::Integer(1))
        .unwrap_err();
    assert!(matches!(err, adapter::WireError::Status(12)), "{err:?}");
    // Read-only objects stay read-only.
    let err = client
        .set(&oids::sys_descr(), Value::Integer(1))
        .unwrap_err();
    assert!(matches!(err, adapter::WireError::Status(17)), "{err:?}");
    // And a malformed datagram isn't a crash: the agent still answers afterward.
    let socket = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    socket.send_to(&[0xff, 0x00, 0x13], d31.2).unwrap();
    assert!(client.get(&oids::sys_descr()).unwrap().is_some());
}

#[test]
fn diagnosis_reads_the_same_through_the_wire() {
    let setup = high();
    let collector = setup.collector();
    let graph = PlantGraph::new(&setup.plant, &setup.library, &setup.profile).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
    let kind = FaultKind::ConnectorReflection {
        location: Location::Span {
            span_id: EdgeId::new("span-3"),
            pos_ft: 150.0,
        },
        return_loss_db: 6.0,
        couplings: None,
    };
    let previous = collector.collect(Timestamp(0)).unwrap();
    setup.emulator.set_faults(vec![Fault {
        onset: Onset::Instant,
        kind,
    }]);
    setup.emulator.push_sample(calm(300));
    let step = collector.collect(Timestamp(300)).unwrap();
    let observed = features_for(&graph, &step, &reference, Some(&previous));
    let answer = diagnose(
        &graph,
        &reference,
        &observed,
        &calm(300),
        &DiagnoserConfig::default(),
    )
    .unwrap();
    let top = &answer.hypotheses[0];
    assert_eq!(top.class, Some(CauseClass::ConnectorReflection));
    assert_eq!(top.components, vec!["span-3".to_string()]);
}

#[test]
fn a_silent_modem_is_a_timeout_not_a_hang() {
    let mut client = adapter::SnmpClient::new("127.0.0.1:9".parse().unwrap(), b"public").unwrap();
    let err = client.get(&pnm_wire::oids::sys_descr()).unwrap_err();
    assert!(matches!(err, adapter::WireError::Timeout), "{err:?}");
}
