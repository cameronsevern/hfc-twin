//! The feature layer's relative values, and the sweep's determinism.

use std::fs;

use features::sweep::{Row, run_sweep};
use features::{DesignReference, StepFeatures, features_for};
use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{Fault, FaultKind, FaultSet, Location, Onset};
use schema::plant::Plant;
use schema::profile::Profile;
use sim::telemetry::{Scenario, SimOptions, simulate};

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

fn featurize(fixture: &Fixture, kinds: Vec<FaultKind>) -> StepFeatures {
    let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
    let faults = FaultSet {
        schema_version: "1".into(),
        seed: 5,
        faults: kinds
            .into_iter()
            .map(|kind| Fault {
                onset: Onset::Instant,
                kind,
            })
            .collect(),
    };
    let environment = Environment {
        schema_version: "1".into(),
        samples: (0..2)
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
    let steps = simulate(&Scenario {
        graph: &graph,
        environment: &environment,
        faults: &faults,
        seed: 5,
        options: SimOptions::default(),
        link: None,
    })
    .unwrap();
    features_for(&graph, &steps[1], &reference, Some(&steps[0]))
}

#[test]
fn a_healthy_plant_reads_zero_drift() {
    for fixture in [high(), low()] {
        let features = featurize(&fixture, vec![]);
        for modem in &features.modems {
            assert!(
                modem.level_drift_db.unwrap().abs() < 0.11,
                "{}",
                modem.element_id
            );
            assert!(modem.us_tx_drift_db.unwrap().abs() < 0.11);
            assert!(modem.sibling_level_dev_db.unwrap().abs() < 0.11);
            assert_eq!(modem.t3_delta, 0);
            assert!(modem.operational);
        }
        assert!(features.service_group.us_snr_drift_min_db.unwrap().abs() < 0.11);
        for active in &features.actives {
            assert!(active.v_margin.unwrap() > 20.0);
            assert!(active.out_level_drift_db.unwrap().abs() < 0.01);
        }
    }
}

#[test]
fn a_bad_drop_stands_out_from_its_siblings_on_both_profiles() {
    let fault = || FaultKind::BadDrop {
        modem_id: ElementId::new("modem-4"),
        extra_loss_db: 8.0,
        return_loss_db: None,
    };
    for fixture in [high(), low()] {
        let features = featurize(&fixture, vec![fault()]);
        let modem = features
            .modems
            .iter()
            .find(|m| m.element_id.as_str() == "modem-4")
            .unwrap();
        let drift = modem.level_drift_db.unwrap();
        assert!((drift + 8.0).abs() < 0.2, "{drift}");
        assert!((modem.us_tx_drift_db.unwrap() - 8.0).abs() < 0.2);
        let other = features
            .modems
            .iter()
            .find(|m| m.element_id.as_str() == "modem-1")
            .unwrap();
        assert!(other.level_drift_db.unwrap().abs() < 0.11);
    }
}

#[test]
fn the_echo_feature_recovers_the_cavity() {
    let features = featurize(
        &high(),
        vec![FaultKind::ConnectorReflection {
            location: Location::Span {
                span_id: EdgeId::new("span-3"),
                pos_ft: 150.0,
            },
            return_loss_db: 6.0,
            couplings: None,
        }],
    );
    let modem = features
        .modems
        .iter()
        .find(|m| m.element_id.as_str() == "modem-4")
        .unwrap();
    let ft = modem.ds_echo_ft.unwrap();
    assert!((ft - 150.0).abs() < 25.0, "{ft}");
}

#[test]
fn margin_buckets_are_a_fixed_size() {
    let features = featurize(&high(), vec![]);
    let d31 = features.modems.iter().find(|m| m.is_d31).unwrap();
    assert_eq!(d31.rxmer_margin_buckets.as_ref().unwrap().len(), 64);
    let d30 = features.modems.iter().find(|m| !m.is_d31).unwrap();
    assert!(d30.rxmer_margin_buckets.is_none());
    assert!(d30.mer_margin_mean_db.is_none());
}

#[test]
fn sweep_is_deterministic_and_labeled_synthetic() {
    let fixture = high();
    let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let go = |seed| {
        let mut out = Vec::new();
        run_sweep(&graph, 20, seed, &mut out).unwrap();
        out
    };
    let (a, b) = (go(1), go(1));
    assert_eq!(a, b);
    assert_ne!(a, go(2));
    let rows: Vec<Row> = String::from_utf8(a)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 20);
    assert!(
        rows.iter()
            .all(|r| r.origin == schema::common::Origin::Synthetic)
    );
    assert!(rows.iter().filter(|r| r.class.is_none()).count() >= 1);
    let classes: std::collections::BTreeSet<_> = rows
        .iter()
        .filter_map(|r| r.class.map(|c| format!("{c:?}")))
        .collect();
    assert_eq!(classes.len(), 9);
}

/// A transient is read while it lasts, since a row labelled with one that's already over would teach a fault no telemetry shows.
#[test]
fn a_transient_is_read_before_it_ends() {
    use features::sampler::{reading_t, sample_fault};
    use schema::work_order::CauseClass;
    use sim::rng::Rng;
    let f = high();
    let graph = PlantGraph::new(&f.plant, &f.library, &f.profile).unwrap();
    for class in [CauseClass::GcpLoss, CauseClass::TimingLoss] {
        for seed in 0..50 {
            let mut rng = Rng::new(seed);
            let fault = sample_fault(&graph, class, &mut rng).unwrap();
            let lasts_s = match fault.kind {
                schema::fault::FaultKind::GcpLoss { duration_s, .. }
                | schema::fault::FaultKind::TimingLoss { duration_s, .. } => duration_s,
                _ => unreachable!(),
            };
            let t = reading_t(Some(&fault), &mut rng);
            assert!(((t + 300) as f64) < lasts_s, "{t} {lasts_s}");
        }
    }
}
