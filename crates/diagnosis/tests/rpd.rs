//! RPD faults, on the high-split fixture. All synthetic.

use std::fs;

use diagnosis::{DiagnoserConfig, diagnose, work_order};
use features::{DesignReference, features_for};
use plant_graph::PlantGraph;
use schema::common::{ElementId, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{Fault, FaultKind, FaultSet, Onset, RpdFailureMode};
use schema::observation::RegState;
use schema::plant::Plant;
use schema::profile::Profile;
use schema::rpd::{GcpStatus, PtpResult, RpdState};
use schema::work_order::{CauseClass, PolicyTable};
use sim::rpd::{GCP_GRACE_S, HOLDOVER_IN_SPEC_S, HOLDOVER_OUT_OF_SPEC_S};
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

fn rpd() -> ElementId {
    ElementId::new("rpd-1")
}

fn at(t: i64) -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(t),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

fn run(fixture: &Fixture, kind: FaultKind, times: &[i64]) -> Vec<StepObservation> {
    let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let environment = Environment {
        schema_version: "1".into(),
        samples: times.iter().map(|t| at(*t)).collect(),
        ambient_sources: Vec::new(),
    };
    let faults = FaultSet {
        schema_version: "2".into(),
        seed: 3,
        faults: vec![Fault {
            onset: Onset::Instant,
            kind,
        }],
    };
    simulate(&Scenario {
        graph: &graph,
        environment: &environment,
        faults: &faults,
        seed: 3,
        options: SimOptions {
            detail: Detail::Summary { stride: 8 },
            randomize: None,
            measurement_noise_db: 0.0,
            rxmer_scatter_db: 0.0,
        },
        link: None,
    })
    .unwrap()
}

#[test]
fn gcp_loss_leaves_rf_alone_until_retries_run_out() {
    let fixture = high();
    let early = GCP_GRACE_S as i64 - 60;
    let late = GCP_GRACE_S as i64 + 300;
    let steps = run(
        &fixture,
        FaultKind::GcpLoss {
            rpd_id: rpd(),
            duration_s: 7200.0,
        },
        &[0, early, late, late + 300],
    );
    // The trap: control plane gone, and every modem still fine.
    let state = steps[1].rpd.as_ref().unwrap();
    assert_eq!(state.gcp_status, GcpStatus::Reconnecting);
    assert!(state.rf_output_up);
    assert!(
        steps[1]
            .modems
            .iter()
            .all(|m| m.reg_state == RegState::Operational)
    );
    // Then it soft-resets, and the modems go with it.
    let state = steps[3].rpd.as_ref().unwrap();
    assert!(!state.rf_output_up);
    assert!(
        steps[3]
            .modems
            .iter()
            .all(|m| m.reg_state != RegState::Operational)
    );
}

#[test]
fn a_cut_fiber_and_a_dead_rpd_differ_only_in_what_it_draws() {
    let fixture = high();
    let cut = run(
        &fixture,
        FaultKind::CinLinkCut {
            rpd_id: rpd(),
            repair_s: None,
        },
        &[0, 300],
    );
    let dead = run(
        &fixture,
        FaultKind::RpdFailure {
            rpd_id: rpd(),
            mode: RpdFailureMode::Dead,
        },
        &[0, 300],
    );
    let (c, d) = (cut[1].rpd.as_ref().unwrap(), dead[1].rpd.as_ref().unwrap());
    assert!(!c.rf_output_up && !d.rf_output_up);
    assert!(c.drawing_power);
    assert!(!d.drawing_power);
    assert_eq!(c.top_level_state, RpdState::Offline);
    for step in [&cut[1], &dead[1]] {
        assert!(
            step.modems
                .iter()
                .all(|m| m.reg_state != RegState::Operational)
        );
    }
}

#[test]
fn a_reset_comes_back() {
    let fixture = high();
    let steps = run(
        &fixture,
        FaultKind::RpdFailure {
            rpd_id: rpd(),
            mode: RpdFailureMode::HardReset { recovery_s: 400.0 },
        },
        &[0, 300, 600, 900],
    );
    assert_eq!(
        steps[1].rpd.as_ref().unwrap().top_level_state,
        RpdState::Initializing
    );
    assert!(!steps[1].rpd.as_ref().unwrap().rf_output_up);
    assert_eq!(
        steps[2].rpd.as_ref().unwrap().top_level_state,
        RpdState::Operational
    );
    assert!(
        steps[3]
            .modems
            .iter()
            .all(|m| m.reg_state == RegState::Operational)
    );
}

#[test]
fn timing_loss_runs_through_holdover_before_rf_goes() {
    let fixture = high();
    let in_spec = HOLDOVER_IN_SPEC_S as i64 - 10;
    let out_of_spec = HOLDOVER_IN_SPEC_S as i64 + 10;
    let gone = (HOLDOVER_IN_SPEC_S + HOLDOVER_OUT_OF_SPEC_S) as i64 + 10;
    let steps = run(
        &fixture,
        FaultKind::TimingLoss {
            rpd_id: rpd(),
            duration_s: 9000.0,
        },
        &[0, in_spec, out_of_spec, gone],
    );
    assert_eq!(
        steps[1].rpd.as_ref().unwrap().ptp_result,
        PtpResult::HoldoverWithinSpec
    );
    assert!(steps[1].rpd.as_ref().unwrap().rf_output_up);
    assert_eq!(
        steps[2].rpd.as_ref().unwrap().ptp_result,
        PtpResult::HoldoverOutOfSpec
    );
    assert!(steps[2].rpd.as_ref().unwrap().rf_output_up);
    assert!(!steps[3].rpd.as_ref().unwrap().rf_output_up);
}

#[test]
fn a_misaligned_rfm_moves_every_modem() {
    let fixture = high();
    let steps = run(
        &fixture,
        FaultKind::RfmMisalignment {
            rpd_id: rpd(),
            ds_gain_delta_db: -2.0,
            tilt_delta_db: 0.0,
        },
        &[0],
    );
    let active = steps[0]
        .actives
        .iter()
        .find(|a| a.element_id == rpd())
        .unwrap();
    assert!((active.out_level.unwrap().out_level_dbmv - 50.0).abs() < 0.01);
    assert!(steps[0].rpd.as_ref().unwrap().rf_output_up);
}

fn diagnosed(
    fixture: &Fixture,
    kind: FaultKind,
    t: i64,
) -> (
    Vec<Option<CauseClass>>,
    Vec<Vec<String>>,
    diagnosis::Hypothesis,
) {
    let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
    let steps = run(fixture, kind, &[t - 300, t]);
    let observed = features_for(&graph, &steps[1], &reference, Some(&steps[0]));
    let answer = diagnose(
        &graph,
        &reference,
        &observed,
        &at(t),
        &DiagnoserConfig::default(),
    )
    .unwrap();
    (
        answer.hypotheses.iter().map(|h| h.class).collect(),
        answer
            .hypotheses
            .iter()
            .map(|h| h.components.clone())
            .collect(),
        answer.hypotheses[0].clone(),
    )
}

#[test]
fn the_diagnoser_tells_them_apart() {
    let fixture = high();
    for (kind, t, expected) in [
        (
            FaultKind::RpdFailure {
                rpd_id: rpd(),
                mode: RpdFailureMode::Dead,
            },
            900,
            CauseClass::RpdFailure,
        ),
        (
            FaultKind::CinLinkCut {
                rpd_id: rpd(),
                repair_s: None,
            },
            900,
            CauseClass::CinLinkCut,
        ),
        // Healthy RF, and the control plane says otherwise.
        (
            FaultKind::GcpLoss {
                rpd_id: rpd(),
                duration_s: 7200.0,
            },
            120,
            CauseClass::GcpLoss,
        ),
        (
            FaultKind::TimingLoss {
                rpd_id: rpd(),
                duration_s: 7200.0,
            },
            120,
            CauseClass::TimingLoss,
        ),
        (
            FaultKind::RfmMisalignment {
                rpd_id: rpd(),
                ds_gain_delta_db: -4.0,
                tilt_delta_db: 0.0,
            },
            900,
            CauseClass::RfmMisalignment,
        ),
    ] {
        let (classes, components, _) = diagnosed(&fixture, kind, t);
        assert_eq!(classes[0], Some(expected), "{classes:?}");
        assert_eq!(components[0], vec!["rpd-1".to_string()]);
    }
}

#[test]
fn work_orders_come_from_the_new_policy_rows() {
    let fixture = high();
    let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let policy: PolicyTable = read("policy/default.json");
    let (_, _, top) = diagnosed(
        &fixture,
        FaultKind::GcpLoss {
            rpd_id: rpd(),
            duration_s: 7200.0,
        },
        120,
    );
    let orders = work_order(&graph, &top, 0, &policy, Timestamp(120));
    assert_eq!(orders[0].cause, CauseClass::GcpLoss);
    assert!(orders[0].action.contains("no plant work"));
    assert_eq!(orders[0].element_id, rpd());
}
