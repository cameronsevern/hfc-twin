//! Faults go in as physical parameters and these signatures must come out, with nothing in the sim
//! that says what a fault looks like (invariant 5). Observed through the §9 schema only.

use std::collections::BTreeMap;
use std::fs;

use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId, PortId, Timestamp};
use schema::component::{ComponentLibrary, ModemGeneration};
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{AmpState, Fault, FaultKind, FaultSet, Location, LossShape, Onset, Schedule};
use schema::observation::{DsRxLevels, ModemObservation, RegState};
use schema::plant::Plant;
use schema::profile::{Profile, Qam};
use sim::rng::Rng;
use sim::state::{ModemState, StepInput, exhaustion_probability, t3_cap};
use sim::telemetry::{Detail, Scenario, SimOptions, StepObservation, simulate};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

struct Fixture {
    plant: Plant,
    library: ComponentLibrary,
    profile: Profile,
}

fn high() -> Fixture {
    Fixture {
        plant: read("one_street/plant_highsplit_rpd.json"),
        library: read("library/generic.json"),
        profile: read("profiles/d31_highsplit_1218_rpd.json"),
    }
}

fn low() -> Fixture {
    Fixture {
        plant: read("one_street/plant_lowsplit_iccap.json"),
        library: read("library/generic.json"),
        profile: read("profiles/d31_lowsplit_1002_iccap.json"),
    }
}

fn env(temp_c: f64, rain: f64) -> Environment {
    Environment {
        schema_version: "1".into(),
        samples: vec![EnvironmentSample {
            t: Timestamp(0),
            temp_c,
            ground_temp_c: temp_c,
            rain_mm_per_hr: rain,
            solar_w_per_m2: None,
            wind_m_per_s: None,
        }],
        ambient_sources: Vec::new(),
    }
}

fn run(
    fixture: &Fixture,
    faults: Vec<FaultKind>,
    environment: &Environment,
) -> Vec<StepObservation> {
    let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let faults = FaultSet {
        schema_version: "1".into(),
        seed: 7,
        faults: faults
            .into_iter()
            .map(|kind| Fault {
                onset: Onset::Instant,
                kind,
            })
            .collect(),
    };
    simulate(&Scenario {
        graph: &graph,
        environment,
        faults: &faults,
        seed: 11,
        options: SimOptions::default(),
        link: None,
    })
    .unwrap()
}

fn by_id(step: &StepObservation) -> BTreeMap<String, &ModemObservation> {
    step.modems
        .iter()
        .map(|m| (m.element_id.to_string(), m))
        .collect()
}

fn mer_mean(m: &ModemObservation) -> f64 {
    m.rxmer.as_ref().unwrap()[0].rxmer_mean_db.unwrap()
}

/// Mean level over the SC-QAM channels, dBmV.
fn sc_level(m: &ModemObservation) -> f64 {
    let levels: Vec<f64> = m
        .ds_rx
        .iter()
        .filter_map(|r| match &r.ds_rx_dbmv {
            DsRxLevels::ScQam { level_dbmv } => Some(*level_dbmv),
            _ => None,
        })
        .collect();
    levels.iter().sum::<f64>() / levels.len() as f64
}

fn uncorrectable(m: &ModemObservation) -> u64 {
    m.codewords.iter().map(|c| c.uncorrectable).sum()
}

fn on_span(span: &str, pos_ft: f64) -> Location {
    Location::Span {
        span_id: EdgeId::new(span),
        pos_ft,
    }
}

fn flat_schedule() -> Schedule {
    Schedule {
        hourly_multipliers: [1.0; 24],
    }
}

const D31: [&str; 4] = ["modem-2", "modem-4", "modem-6", "modem-8"];

#[test]
fn state_oracles() {
    let p = exhaustion_probability(0.5);
    assert!((p - 7.6e-6).abs() < 0.05e-6, "{p}");
    assert_eq!(t3_cap(300.0), 1500);

    // Total downstream loss reinitializes the modem at about 30 s.
    let channel = schema::common::ChannelId::new("us-1");
    let mut state = ModemState::default();
    let mut rng = Rng::new(1);
    let input = StepInput {
        step_s: 10.0,
        p_down: 1.0,
        p_loss: [(channel.clone(), 1.0)].into_iter().collect(),
        ranging_spacing_s: 15.0,
        reregistration_s: 60.0,
    };
    state.advance(&input, &mut rng);
    state.advance(&input, &mut rng);
    assert_eq!(state.reg_state, RegState::Operational);
    state.advance(&input, &mut rng);
    assert_eq!(state.reg_state, RegState::NotSynchronized);
    assert_eq!(state.t4[&channel], 1);
    assert_eq!(state.t3[&channel], 0, "no opportunity, no T3");

    // It comes back, after reregistration_s.
    let back = StepInput {
        p_down: 0.0,
        p_loss: [(channel.clone(), 0.0)].into_iter().collect(),
        step_s: 30.0,
        ..input.clone()
    };
    state.advance(&back, &mut rng);
    assert_ne!(state.reg_state, RegState::Operational);
    state.advance(&back, &mut rng);
    assert_eq!(state.reg_state, RegState::Operational);

    // Upstream-only loss raises T3 and leaves T4 flat, and retries run out at 17.
    let mut state = ModemState::default();
    let up_only = StepInput {
        step_s: 15.0,
        p_down: 0.0,
        p_loss: [(channel.clone(), 1.0)].into_iter().collect(),
        ranging_spacing_s: 15.0,
        reregistration_s: 60.0,
    };
    for _ in 0..16 {
        state.advance(&up_only, &mut rng);
    }
    assert_eq!(state.t3[&channel], 16);
    assert_eq!(state.reg_state, RegState::Operational);
    state.advance(&up_only, &mut rng);
    assert_eq!(state.t4[&channel], 0);
    assert_ne!(state.reg_state, RegState::Operational);
}

/// §7.4's table: a flat curve on a 256-QAM boot profile, at spacing 15 s.
#[test]
fn modem_state_near_the_edge() {
    let m = |rxmer_db: f64| 1.0 / (1.0 + (15.0f64 * (rxmer_db - 24.0 + 0.55)).exp());
    assert!((m(24.0) - 2.6e-4).abs() < 0.05e-4);
    assert!((m(23.5) - 0.3208).abs() < 0.0001);
    assert!((m(23.0) - 0.99883).abs() < 0.00001);

    let channels = ["us-1", "us-2", "us-3", "us-4"].map(schema::common::ChannelId::new);
    let downtime = |p_down: f64| -> (f64, u64) {
        let mut state = ModemState::default();
        let mut rng = Rng::new(7);
        let input = StepInput {
            step_s: 300.0,
            p_down,
            p_loss: channels.iter().map(|c| (c.clone(), p_down)).collect(),
            ranging_spacing_s: 15.0,
            reregistration_s: 60.0,
        };
        let mut down = 0;
        const STEPS: usize = 2000;
        for _ in 0..STEPS {
            state.advance(&input, &mut rng);
            if state.reg_state != RegState::Operational {
                down += 1;
            }
        }
        (down as f64 / STEPS as f64, state.t4[&channels[0]])
    };

    // Stays up: nothing near T4.
    let (share, t4) = downtime(m(24.0));
    assert_eq!(share, 0.0);
    assert_eq!(t4, 0);

    // Goes up and down: T4 fires, and it's neither always up nor always down.
    let (share, t4) = downtime(m(23.5));
    assert!(t4 > 0);
    assert!(share > 0.0 && share < 0.9, "{share}");

    // Mostly down: an attempt almost never succeeds.
    let (share, _) = downtime(m(23.0));
    assert!(share > 0.9, "{share}");
}

/// §7.4: FEC failing with pilots held doesn't take the modem down, if another way in still decodes.
#[test]
fn losing_profiles_is_not_going_offline() {
    let step_on = |boot: Qam| {
        let mut fixture = low();
        for channel in &mut fixture.profile.channel_plan.ofdm {
            for data_profile in &mut channel.data_profiles {
                if data_profile.data_profile_id == 0 {
                    data_profile.modulation = boot;
                }
            }
        }
        let faults = vec![FaultKind::ForwardOptics { cnr_loss_db: 30.0 }];
        run(&fixture, faults, &env(20.0, 0.0)).remove(0)
    };

    // A 16-QAM boot profile rides out a fall that takes 256-QAM boot modems off.
    let robust = step_on(Qam::Qam16);
    let fragile = step_on(Qam::Qam256);
    let up = |step: &StepObservation| {
        step.modems
            .iter()
            .filter(|m| m.reg_state == RegState::Operational)
            .count()
    };
    assert!(
        up(&robust) > up(&fragile),
        "{} {}",
        up(&robust),
        up(&fragile)
    );

    // Those modems are in partial service: registered, with a curve, and with channels gone.
    let mut seen = 0;
    for m in robust
        .modems
        .iter()
        .filter(|m| m.reg_state == RegState::Operational)
    {
        let failed = m
            .partial_channel
            .iter()
            .flatten()
            .any(|p| !p.failed_data_profiles.is_empty());
        if !failed {
            continue;
        }
        seen += 1;
        let mean = m
            .rxmer
            .iter()
            .flatten()
            .next()
            .unwrap()
            .rxmer_mean_db
            .unwrap();
        assert!(mean < 25.0, "{} {mean}", m.element_id.as_str());
    }
    assert!(seen > 0);
}

#[test]
fn healthy_fixture() {
    for fixture in [high(), low()] {
        let steps = run(&fixture, vec![], &env(20.0, 0.0));
        let step = &steps[0];
        assert_eq!(step.modems.len(), 8);
        for m in &step.modems {
            assert_eq!(m.reg_state, RegState::Operational);
            // §4.6: 4096-QAM at 1.25 dB of capped margin still fails at about 1.5e−8.
            let total: u64 = m.codewords.iter().map(|c| c.total).sum();
            assert!(
                uncorrectable(m) as f64 / (total as f64) < 1e-6,
                "{}",
                m.element_id
            );
            assert!(m.t3_timeouts.iter().all(|t| t.count == 0));
            match m.generation {
                ModemGeneration::D30 => {
                    assert!(m.rxmer.is_none());
                    assert!(m.ds_chan_est.is_none());
                    assert!(m.plc.is_none());
                    assert!(m.preeq.ofdma.is_none());
                    assert!(m.preeq.sc_qam.is_some());
                }
                ModemGeneration::D31 => {
                    let mean = mer_mean(m);
                    assert!((38.0..44.0).contains(&mean), "{} {mean}", m.element_id);
                    let rec = &m.rxmer.as_ref().unwrap()[0];
                    assert!(rec.rxmer_stddev_db.unwrap() < 1.0);
                    assert_eq!(rec.rxmer_db.as_ref().unwrap().len(), 3800);
                }
            }
            for us in &m.us_tx {
                assert!(us.psd_dbmv_per_1p6mhz > 17.0 && us.psd_dbmv_per_1p6mhz < 53.0);
            }
        }
        assert!(step.service_group.us_snr.iter().all(|s| s.us_snr_db > 35.0));
    }
}

#[test]
fn no_non_finite_values_reach_the_wire() {
    let fixture = high();
    let steps = run(
        &fixture,
        vec![FaultKind::AmpDrift {
            amp_id: ElementId::new("amp-2"),
            state: AmpState::Failed,
        }],
        &env(20.0, 0.0),
    );
    let text = serde_json::to_string(&steps).unwrap();
    assert!(!text.contains("null,null,null") || !text.contains("NaN"));
    let back: Vec<StepObservation> = serde_json::from_str(&text).unwrap();
    assert_eq!(back, steps);
}

#[test]
fn determinism() {
    let fault = || {
        vec![FaultKind::WaterIngress {
            span_id: EdgeId::new("span-3"),
            pos_ft: 100.0,
            affected_len_ft: 100.0,
            k_w_db_per_100ft: 1.0,
            rain_coupling_per_mm_per_hr: 0.1,
            impedance_variation_pct: 10.0,
        }]
    };
    let fixture = high();
    let environment = env(25.0, 2.0);
    let a = serde_json::to_vec(&run(&fixture, fault(), &environment)).unwrap();
    let b = serde_json::to_vec(&run(&fixture, fault(), &environment)).unwrap();
    assert_eq!(a, b);

    // Order of the plant's lists changes nothing.
    let mut shuffled = high();
    shuffled.plant.elements.reverse();
    shuffled.plant.edges.reverse();
    let c = serde_json::to_vec(&run(&shuffled, fault(), &environment)).unwrap();
    assert_eq!(a, c);
}

#[test]
fn measurement_noise_only_moves_readings() {
    let fixture = high();
    let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let faults = FaultSet {
        schema_version: "1".into(),
        seed: 1,
        faults: vec![],
    };
    let environment = env(20.0, 0.0);
    let go = |seed: u64| {
        simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults: &faults,
            seed,
            options: SimOptions {
                detail: Detail::Summary { stride: 8 },
                randomize: None,
                measurement_noise_db: 0.25,
                rxmer_scatter_db: 0.0,
            },
            link: None,
        })
        .unwrap()
    };
    let (a, b) = (go(1), go(2));
    assert_ne!(a, b);
    let (ma, mb) = (by_id(&a[0]), by_id(&b[0]));
    assert!((mer_mean(ma["modem-4"]) - mer_mean(mb["modem-4"])).abs() < 0.2);
}

#[test]
fn connector_on_a_span() {
    let fixture = high();
    let clean = run(&fixture, vec![], &env(20.0, 0.0));
    let faulty = run(
        &fixture,
        vec![FaultKind::ConnectorReflection {
            location: on_span("span-3", 150.0),
            return_loss_db: 10.0,
            couplings: None,
        }],
        &env(20.0, 0.0),
    );
    let (c, f) = (by_id(&clean[0]), by_id(&faulty[0]));
    let ripple = |m: &ModemObservation| {
        m.ds_chan_est.as_ref().unwrap()[0]
            .summary
            .unwrap()
            .mag_ripple_pp_db
    };
    // The cavity it forms with tap-2 is above modem-4, so ripple grows there. The upstream reflector
    // for modems above it is reached only by leaking through isolation.
    assert!(
        ripple(f["modem-4"]) > ripple(c["modem-4"]) + 0.05,
        "{} {}",
        ripple(f["modem-4"]),
        ripple(c["modem-4"])
    );
    // Modem-2 hangs off tap-1, above the fault, and still sees it: through tap isolation (§5.1).
    assert!(ripple(f["modem-2"]) > ripple(c["modem-2"]));
    assert!(ripple(f["modem-2"]) < ripple(c["modem-2"]) + 0.6);
    // RxMER barely moves: the equalizer undoes the channel (§5.2).
    assert!((mer_mean(f["modem-4"]) - mer_mean(c["modem-4"])).abs() < 0.3);

    // Pre-eq: more energy off the main tap at modems below it (§5.3).
    let nmter = |m: &ModemObservation| m.preeq.sc_qam.as_ref().unwrap()[0].nmter_db;
    assert!(nmter(f["modem-3"]) > nmter(c["modem-3"]) + 3.0);
    // Modem-1 is off-path, by §5.1 about (38 − 23) + 10 + 2·A·1.5 = 25.9 dB down at 16.4 MHz.
    assert!(
        (nmter(f["modem-1"]) - -25.9).abs() < 1.5,
        "{}",
        nmter(f["modem-1"])
    );

    // Levels don't change: a mismatch isn't a loss.
    assert!((sc_level(f["modem-4"]) - sc_level(c["modem-4"])).abs() < 0.2);
}

#[test]
fn connector_cavity_recovered_from_ripple() {
    // §5.4: 150 ft to tap-2, from ripple spacing in the channel estimate of a modem below it.
    let fixture = high();
    let steps = run(
        &fixture,
        vec![FaultKind::ConnectorReflection {
            location: on_span("span-3", 150.0),
            return_loss_db: 6.0,
            couplings: None,
        }],
        &env(20.0, 0.0),
    );
    let m = by_id(&steps[0])["modem-4"];
    let rec = &m.ds_chan_est.as_ref().unwrap()[0];
    let mags: Vec<f64> = rec
        .coefficients
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| {
            let c = c.unwrap();
            10.0 * (c.re * c.re + c.im * c.im).log10()
        })
        .collect();
    // Detrend, then look for the strongest period across candidate delays.
    let n = mags.len();
    let mean = mags.iter().sum::<f64>() / n as f64;
    let spacing_mhz = rec.axis.spacing_khz / 1000.0;
    let mut best = (0.0, 0.0);
    let mut tau_us = 0.05;
    while tau_us < 1.5 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, y) in mags.iter().enumerate() {
            let phase = 2.0 * std::f64::consts::PI * tau_us * i as f64 * spacing_mhz;
            re += (y - mean) * phase.cos();
            im += (y - mean) * phase.sin();
        }
        let power = re * re + im * im;
        if power > best.1 {
            best = (tau_us, power);
        }
        tau_us += 0.002;
    }
    let d_ft = sim::echo::cavity_len_from_ripple_ft(0.87, 1.0 / best.0);
    assert!((d_ft - 150.0).abs() < 20.0, "{d_ft}");
}

#[test]
fn suckout_on_a_span() {
    let fixture = high();
    let clean = run(&fixture, vec![], &env(20.0, 0.0));
    let f0 = 900.0;
    let faulty = run(
        &fixture,
        vec![FaultKind::Suckout {
            location: on_span("span-2", 100.0),
            f0_mhz: f0,
            depth_db: 25.0,
            width_mhz: 6.0,
        }],
        &env(20.0, 0.0),
    );
    let mer_at = |m: &ModemObservation, f: f64| {
        let rec = &m.rxmer.as_ref().unwrap()[0];
        let i = ((f
            - (rec.axis.zero_freq_mhz
                + rec.axis.first_active_index as f64 * rec.axis.spacing_khz / 1000.0))
            / (rec.axis.spacing_khz / 1000.0))
            .round() as usize;
        rec.rxmer_db.as_ref().unwrap()[i].unwrap()
    };
    let (c, f) = (by_id(&clean[0]), by_id(&faulty[0]));
    for modem in D31 {
        assert!(
            mer_at(c[modem], f0) - mer_at(f[modem], f0) > 3.0,
            "{modem} {} {}",
            mer_at(c[modem], f0),
            mer_at(f[modem], f0)
        );
        // The dip is local: far from f0 nothing moved.
        assert!((mer_at(c[modem], 1000.0) - mer_at(f[modem], 1000.0)).abs() < 0.1);
    }
    // Same subcarriers across neighbours.
    let dip_at = |m: &ModemObservation| {
        let rec = &m.rxmer.as_ref().unwrap()[0];
        let values = rec.rxmer_db.as_ref().unwrap();
        let (i, _) = values
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.unwrap().total_cmp(&b.1.unwrap()))
            .unwrap();
        rec.axis.zero_freq_mhz
            + (rec.axis.first_active_index as usize + i) as f64 * rec.axis.spacing_khz / 1000.0
    };
    assert!((dip_at(f["modem-2"]) - f0).abs() < 2.0);
    assert!((dip_at(f["modem-8"]) - f0).abs() < 2.0);
}

#[test]
fn amp_drift() {
    let fixture = high();
    let clean = run(&fixture, vec![], &env(20.0, 0.0));
    let faulty = run(
        &fixture,
        vec![FaultKind::AmpDrift {
            amp_id: ElementId::new("amp-1"),
            state: AmpState::Drift {
                gain_delta_db: -2.0,
                tilt_delta_db: 0.0,
                compression_onset_dbmv: None,
            },
        }],
        &env(20.0, 0.0),
    );
    let (c, f) = (by_id(&clean[0]), by_id(&faulty[0]));
    for modem in ["modem-1", "modem-2", "modem-3", "modem-4"] {
        assert!(
            (sc_level(c[modem]) - sc_level(f[modem]) - 2.0).abs() < 0.15,
            "{modem}"
        );
    }
    // Below amp-2, its AGC takes up a 2 dB shortfall at the pilot. Cable-shaped, so not exactly flat.
    for modem in ["modem-5", "modem-6", "modem-7", "modem-8"] {
        assert!(
            (sc_level(c[modem]) - sc_level(f[modem])).abs() < 1.2,
            "{modem}"
        );
    }
    assert!(
        faulty[0]
            .actives
            .iter()
            .any(|a| a.element_id.as_str() == "amp-1"
                && (a.out_level.unwrap().out_level_dbmv - 50.0).abs() < 0.01)
    );
}

#[test]
fn amp_failure_takes_down_what_is_below() {
    let fixture = high();
    let faulty = run(
        &fixture,
        vec![FaultKind::AmpDrift {
            amp_id: ElementId::new("amp-2"),
            state: AmpState::Failed,
        }],
        &Environment {
            schema_version: "1".into(),
            samples: (0..4)
                .map(|i| EnvironmentSample {
                    t: Timestamp(i * 300),
                    temp_c: 20.0,
                    ground_temp_c: 20.0,
                    rain_mm_per_hr: 0.0,
                    solar_w_per_m2: None,
                    wind_m_per_s: None,
                })
                .collect(),
            ambient_sources: Vec::new(),
        },
    );
    let last = by_id(faulty.last().unwrap());
    for modem in ["modem-5", "modem-6", "modem-7", "modem-8"] {
        assert_eq!(last[modem].reg_state, RegState::NotSynchronized, "{modem}");
        assert!(last[modem].ds_rx.is_empty());
        assert!(last[modem].t4_timeouts.iter().any(|t| t.count >= 1));
    }
    for modem in ["modem-1", "modem-2", "modem-3", "modem-4"] {
        assert_eq!(last[modem].reg_state, RegState::Operational, "{modem}");
        assert!(last[modem].ds_rx.len() >= 32);
    }
}

#[test]
fn water_grows_with_rain() {
    let fixture = high();
    let water = || FaultKind::WaterIngress {
        span_id: EdgeId::new("span-3"),
        pos_ft: 100.0,
        affected_len_ft: 150.0,
        k_w_db_per_100ft: 4.0,
        rain_coupling_per_mm_per_hr: 0.3,
        impedance_variation_pct: 10.0,
    };
    let clean = run(&fixture, vec![], &env(20.0, 0.0));
    let dry = run(&fixture, vec![water()], &env(20.0, 0.0));
    let wet = run(&fixture, vec![water()], &env(20.0, 10.0));
    let (c, d, w) = (by_id(&clean[0]), by_id(&dry[0]), by_id(&wet[0]));
    let loss = |a: &ModemObservation, b: &ModemObservation| sc_level(a) - sc_level(b);
    assert!(loss(c["modem-3"], d["modem-3"]) > 1.0);
    assert!(loss(c["modem-3"], w["modem-3"]) > loss(c["modem-3"], d["modem-3"]) + 1.0);
    // Below amp-2, whose AGC can't fully hold: still some of it.
    assert!(loss(c["modem-5"], w["modem-5"]).abs() > 0.0);
    // Above the wet span, nothing.
    assert!(loss(c["modem-1"], w["modem-1"]).abs() < 0.05);
    // Non-periodic ripple in the channel estimate.
    let ripple = |m: &ModemObservation| {
        m.ds_chan_est.as_ref().unwrap()[0]
            .summary
            .unwrap()
            .mag_ripple_rms_db
    };
    assert!(
        ripple(w["modem-4"]) > ripple(c["modem-4"]) + 0.02,
        "{} {}",
        ripple(w["modem-4"]),
        ripple(c["modem-4"])
    );
}

#[test]
fn upstream_ingress_funnels_across_the_group() {
    let fixture = high();
    let clean = run(&fixture, vec![], &env(20.0, 0.0));
    let faulty = run(
        &fixture,
        vec![FaultKind::UpstreamIngress {
            entry_location: Location::Port {
                element_id: ElementId::new("modem-4"),
                port: PortId::new("rf"),
            },
            f_mhz: 25.0,
            bw_mhz: 30.0,
            power_dbmv: 5.0,
            schedule: flat_schedule(),
            burst: None,
        }],
        &env(20.0, 0.0),
    );
    let snr = |s: &StepObservation, channel: &str| {
        s.service_group
            .us_snr
            .iter()
            .find(|r| r.channel_id.as_str() == channel)
            .unwrap()
            .us_snr_db
    };
    assert!(snr(&clean[0], "us-sc-2") - snr(&faulty[0], "us-sc-2") > 5.0);
    // Modems' downstream levels don't care.
    let (c, f) = (by_id(&clean[0]), by_id(&faulty[0]));
    for modem in ["modem-1", "modem-8"] {
        assert!((sc_level(c[modem]) - sc_level(f[modem])).abs() < 0.01);
    }
    // Outside its band, nothing changed. OFDMA is well above it.
    assert!((snr(&clean[0], "ofdma-1") - snr(&faulty[0], "ofdma-1")).abs() < 0.1);
}

#[test]
fn upstream_only_fault_raises_t3_not_t4() {
    let fixture = high();
    let environment = Environment {
        schema_version: "1".into(),
        samples: (0..3)
            .map(|i| EnvironmentSample {
                t: Timestamp(i * 300),
                temp_c: 20.0,
                ground_temp_c: 20.0,
                rain_mm_per_hr: 0.0,
                solar_w_per_m2: None,
                wind_m_per_s: None,
            })
            .collect(),
        ambient_sources: Vec::new(),
    };
    let faulty = run(
        &fixture,
        vec![FaultKind::UpstreamIngress {
            entry_location: Location::Port {
                element_id: ElementId::new("modem-4"),
                port: PortId::new("rf"),
            },
            f_mhz: 25.0,
            bw_mhz: 30.0,
            power_dbmv: 35.0,
            schedule: flat_schedule(),
            burst: None,
        }],
        &environment,
    );
    let last = by_id(faulty.last().unwrap());
    let t3: u64 = last["modem-1"].t3_timeouts.iter().map(|t| t.count).sum();
    let t4: u64 = last["modem-1"].t4_timeouts.iter().map(|t| t.count).sum();
    assert!(t3 > 0);
    assert_eq!(t4, 0);
}

#[test]
fn downstream_ingress_is_only_below_its_entry() {
    let fixture = high();
    let clean = run(&fixture, vec![], &env(20.0, 0.0));
    let faulty = run(
        &fixture,
        vec![FaultKind::DownstreamIngress {
            entry_location: on_span("span-5", 200.0),
            f_mhz: 900.0,
            bw_mhz: 10.0,
            power_dbmv: 25.0,
            schedule: flat_schedule(),
        }],
        &env(20.0, 0.0),
    );
    let p2 = |m: &ModemObservation| m.rxmer.as_ref().unwrap()[0].rxmer_p2_db.unwrap();
    let (c, f) = (by_id(&clean[0]), by_id(&faulty[0]));
    assert!(p2(c["modem-6"]) - p2(f["modem-6"]) > 3.0);
    assert!(p2(c["modem-8"]) - p2(f["modem-8"]) > 3.0);
    assert!((p2(c["modem-2"]) - p2(f["modem-2"])).abs() < 0.11);
    assert!((p2(c["modem-4"]) - p2(f["modem-4"])).abs() < 0.11);
}

#[test]
fn schedule_follows_the_hour() {
    let fixture = high();
    let mut hours = [0.0; 24];
    hours[6] = 1.0;
    let environment = Environment {
        schema_version: "1".into(),
        samples: [0i64, 6 * 3600]
            .iter()
            .map(|t| EnvironmentSample {
                t: Timestamp(*t),
                temp_c: 20.0,
                ground_temp_c: 20.0,
                rain_mm_per_hr: 0.0,
                solar_w_per_m2: None,
                wind_m_per_s: None,
            })
            .collect(),
        ambient_sources: Vec::new(),
    };
    let steps = run(
        &fixture,
        vec![FaultKind::UpstreamIngress {
            entry_location: Location::Port {
                element_id: ElementId::new("modem-4"),
                port: PortId::new("rf"),
            },
            f_mhz: 25.0,
            bw_mhz: 30.0,
            power_dbmv: 5.0,
            schedule: Schedule {
                hourly_multipliers: hours,
            },
            burst: None,
        }],
        &environment,
    );
    let snr = |s: &StepObservation| s.service_group.us_snr[1].us_snr_db;
    assert!(snr(&steps[0]) - snr(&steps[1]) > 5.0);
}

#[test]
fn power_sag_browns_out_below_vmin() {
    let fixture = high();
    let faulty = run(
        &fixture,
        vec![FaultKind::PowerSupplySag {
            psu_id: ElementId::new("psu-1"),
            v_delta: -27.0,
            schedule: flat_schedule(),
        }],
        &Environment {
            schema_version: "1".into(),
            samples: (0..3)
                .map(|i| EnvironmentSample {
                    t: Timestamp(i * 300),
                    temp_c: 20.0,
                    ground_temp_c: 20.0,
                    rain_mm_per_hr: 0.0,
                    solar_w_per_m2: None,
                    wind_m_per_s: None,
                })
                .collect(),
            ambient_sources: Vec::new(),
        },
    );
    let step = faulty.last().unwrap();
    let amp2 = step
        .actives
        .iter()
        .find(|a| a.element_id.as_str() == "amp-2")
        .unwrap();
    assert!(amp2.v_in.unwrap() < 60.0);
    let m = by_id(step);
    assert_ne!(m["modem-8"].reg_state, RegState::Operational);
}

#[test]
fn bad_drop_is_one_modem() {
    let fixture = high();
    let clean = run(&fixture, vec![], &env(20.0, 0.0));
    let faulty = run(
        &fixture,
        vec![FaultKind::BadDrop {
            modem_id: ElementId::new("modem-4"),
            extra_loss_db: 6.0,
            return_loss_db: None,
        }],
        &env(20.0, 0.0),
    );
    let (c, f) = (by_id(&clean[0]), by_id(&faulty[0]));
    assert!((sc_level(c["modem-4"]) - sc_level(f["modem-4"]) - 6.0).abs() < 0.1);
    assert!(
        f["modem-4"].us_tx[0].psd_dbmv_per_1p6mhz - c["modem-4"].us_tx[0].psd_dbmv_per_1p6mhz > 5.5
    );
    for modem in ["modem-1", "modem-2", "modem-3", "modem-5", "modem-8"] {
        assert!(
            (sc_level(c[modem]) - sc_level(f[modem])).abs() < 0.01,
            "{modem}"
        );
    }
}

#[test]
fn frequency_selective_loss_takes_a_band_away() {
    let fixture = high();
    let clean = run(&fixture, vec![], &env(20.0, 0.0));
    let faulty = run(
        &fixture,
        vec![FaultKind::FrequencySelectiveLoss {
            location: on_span("span-3", 50.0),
            shape: LossShape::LowPass,
            f_lo_mhz: 0.0,
            f_hi_mhz: 900.0,
            depth_db: 40.0,
            stop_return_loss_db: 3.0,
            skirt_mhz: None,
        }],
        &env(20.0, 0.0),
    );
    let (c, f) = (by_id(&clean[0]), by_id(&faulty[0]));
    let rec = |m: &ModemObservation| m.rxmer.as_ref().unwrap()[0].clone();
    // ofdm-1 is 835–1025: above 900 it's gone at modems below, and intact above the fault.
    assert!(rec(f["modem-4"]).rxmer_p2_db.unwrap() < rec(c["modem-4"]).rxmer_p2_db.unwrap() - 10.0);
    assert!(
        (rec(f["modem-2"]).rxmer_mean_db.unwrap() - rec(c["modem-2"]).rxmer_mean_db.unwrap()).abs()
            < 0.2
    );
    // SC-QAM at 603–789 is below the stop band.
    assert!((sc_level(f["modem-3"]) - sc_level(c["modem-3"])).abs() < 0.2);
    // FBC shows the missing band directly.
    let fbc = f["modem-4"].fbc.as_ref().unwrap();
    let bin =
        |f_mhz: f64| ((f_mhz - fbc.first_bin_center_mhz) / (fbc.bin_width_khz / 1000.0)) as usize;
    assert!(fbc.fbc_dbmv[bin(1000.0)] < fbc.fbc_dbmv[bin(850.0)] - 10.0);
}

#[test]
fn same_fault_gives_comparable_margin_on_both_profiles() {
    let fault = || FaultKind::BadDrop {
        modem_id: ElementId::new("modem-4"),
        extra_loss_db: 12.0,
        return_loss_db: None,
    };
    for fixture in [high(), low()] {
        let clean = run(&fixture, vec![], &env(20.0, 0.0));
        let faulty = run(&fixture, vec![fault()], &env(20.0, 0.0));
        let (c, f) = (by_id(&clean[0]), by_id(&faulty[0]));
        assert!((sc_level(c["modem-4"]) - sc_level(f["modem-4"]) - 12.0).abs() < 0.1);
        assert!(mer_mean(f["modem-4"]) < mer_mean(c["modem-4"]));
    }
}

#[test]
fn compound_faults_all_apply() {
    let fixture = high();
    let clean = run(&fixture, vec![], &env(20.0, 0.0));
    let faulty = run(
        &fixture,
        vec![
            FaultKind::BadDrop {
                modem_id: ElementId::new("modem-2"),
                extra_loss_db: 5.0,
                return_loss_db: None,
            },
            FaultKind::BadDrop {
                modem_id: ElementId::new("modem-8"),
                extra_loss_db: 7.0,
                return_loss_db: None,
            },
        ],
        &env(20.0, 0.0),
    );
    let (c, f) = (by_id(&clean[0]), by_id(&faulty[0]));
    assert!((sc_level(c["modem-2"]) - sc_level(f["modem-2"]) - 5.0).abs() < 0.1);
    assert!((sc_level(c["modem-8"]) - sc_level(f["modem-8"]) - 7.0).abs() < 0.1);
}

#[test]
fn domain_randomization_changes_plant_not_structure() {
    let fixture = high();
    let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let faults = FaultSet {
        schema_version: "1".into(),
        seed: 1,
        faults: vec![],
    };
    let environment = env(20.0, 0.0);
    let go = |randomize| {
        simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults: &faults,
            seed: 3,
            options: SimOptions {
                detail: Detail::Summary { stride: 16 },
                randomize,
                measurement_noise_db: 0.0,
                rxmer_scatter_db: 0.0,
            },
            link: None,
        })
        .unwrap()
    };
    let base = go(None);
    let (a, b, a2) = (go(Some(1)), go(Some(2)), go(Some(1)));
    assert_ne!(a, b);
    assert_eq!(a, a2);
    let (m0, m1) = (by_id(&base[0]), by_id(&a[0]));
    let moved = (sc_level(m0["modem-4"]) - sc_level(m1["modem-4"])).abs();
    assert!(moved > 0.0 && moved < 4.0, "{moved}");
    assert!(
        a[0].modems
            .iter()
            .all(|m| m.reg_state == RegState::Operational)
    );
}
