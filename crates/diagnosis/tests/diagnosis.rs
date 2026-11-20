//! The no-ML floor. All synthetic (invariant 11).

use std::fs;

use diagnosis::switching::{localize_by_switching, with_return_atten};
use diagnosis::{DiagnoserConfig, diagnose, work_order};
use features::DesignReference;
use features::sampler::flat_schedule;
use features::sweep::{observe, random_sample};
use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId, PortId, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{Fault, FaultKind, FaultSet, Location, Onset};
use schema::plant::Plant;
use schema::profile::Profile;
use schema::work_order::{CauseClass, PolicyTable};
use sim::rng::Rng;
use sim::telemetry::{Detail, Scenario, SimOptions, simulate};

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

fn faults_of(kinds: Vec<FaultKind>, seed: u64) -> FaultSet {
    FaultSet {
        schema_version: "1".into(),
        seed,
        faults: kinds
            .into_iter()
            .map(|kind| Fault {
                onset: Onset::Instant,
                kind,
            })
            .collect(),
    }
}

fn calm() -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(0),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

/// The v0 bar: the faulty component ranks first in at least 90 % of seeds and positions.
#[test]
fn connector_bar_on_both_profiles() {
    for fixture in [low(), high()] {
        let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
        let reference = DesignReference::compute(&graph).unwrap();
        let config = DiagnoserConfig::default();
        let spans = ["span-1", "span-2", "span-3", "span-4", "span-5", "span-6"];
        let lens = [1200.0, 400.0, 300.0, 100.0, 400.0, 300.0];
        let (mut hits, mut runs) = (0, 0);
        for seed in 0..30u64 {
            let mut rng = Rng::new(seed + 500);
            let i = (seed as usize) % spans.len();
            let kinds = vec![FaultKind::ConnectorReflection {
                location: Location::Span {
                    span_id: EdgeId::new(spans[i]),
                    pos_ft: rng.range(0.1, 0.9) * lens[i],
                },
                // Stronger than the ordinary parts it sits among, or nothing can tell it from them.
                return_loss_db: rng.range(4.0, 10.0),
                couplings: None,
            }];
            let sample = random_sample(&mut rng, 0);
            let observed = observe(
                &graph,
                &reference,
                &faults_of(kinds, seed),
                sample.clone(),
                seed,
            )
            .unwrap();
            let answer = diagnose(&graph, &reference, &observed, &sample, &config).unwrap();
            runs += 1;
            if answer.hypotheses[0]
                .components
                .iter()
                .any(|c| c == spans[i])
            {
                hits += 1;
            }
        }
        assert!(
            hits * 10 >= runs * 9,
            "{}: {hits}/{runs}",
            fixture.profile.id
        );
    }
}

#[test]
fn healthy_plant_stays_healthy() {
    for fixture in [low(), high()] {
        let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
        let reference = DesignReference::compute(&graph).unwrap();
        let config = DiagnoserConfig::default();
        let policy: PolicyTable = read("policy/default.json");
        for seed in 0..5u64 {
            let observed =
                observe(&graph, &reference, &faults_of(vec![], seed), calm(), seed).unwrap();
            let answer = diagnose(&graph, &reference, &observed, &calm(), &config).unwrap();
            let top = &answer.hypotheses[0];
            assert_eq!(top.class, None, "seed {seed}: {:?}", top.components);
            assert!(work_order(&graph, top, 0, &policy, Timestamp(0)).is_empty());
        }
    }
}

#[test]
fn diagnosis_is_byte_identical() {
    let fixture = high();
    let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
    let kinds = || {
        vec![FaultKind::BadDrop {
            modem_id: ElementId::new("modem-4"),
            extra_loss_db: 9.0,
            return_loss_db: None,
        }]
    };
    let go = || {
        let observed = observe(&graph, &reference, &faults_of(kinds(), 1), calm(), 1).unwrap();
        let answer = diagnose(
            &graph,
            &reference,
            &observed,
            &calm(),
            &DiagnoserConfig::default(),
        )
        .unwrap();
        serde_json::to_vec(&answer).unwrap()
    };
    assert_eq!(go(), go());
}

#[test]
fn two_bad_drops_come_back_as_both() {
    let fixture = high();
    let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
    let kinds = vec![
        FaultKind::BadDrop {
            modem_id: ElementId::new("modem-2"),
            extra_loss_db: 9.0,
            return_loss_db: None,
        },
        FaultKind::BadDrop {
            modem_id: ElementId::new("modem-8"),
            extra_loss_db: 12.0,
            return_loss_db: None,
        },
    ];
    let observed = observe(&graph, &reference, &faults_of(kinds, 3), calm(), 3).unwrap();
    let answer = diagnose(
        &graph,
        &reference,
        &observed,
        &calm(),
        &DiagnoserConfig::default(),
    )
    .unwrap();
    let top = &answer.hypotheses[0];
    assert!(
        top.components.contains(&"modem-2".to_string()),
        "{:?}",
        top.components
    );
    assert!(
        top.components.contains(&"modem-8".to_string()),
        "{:?}",
        top.components
    );
}

#[test]
fn the_work_order_comes_from_the_policy_table() {
    let fixture = high();
    let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
    let kinds = vec![FaultKind::BadDrop {
        modem_id: ElementId::new("modem-4"),
        extra_loss_db: 10.0,
        return_loss_db: None,
    }];
    let observed = observe(&graph, &reference, &faults_of(kinds, 2), calm(), 2).unwrap();
    let answer = diagnose(
        &graph,
        &reference,
        &observed,
        &calm(),
        &DiagnoserConfig::default(),
    )
    .unwrap();
    let top = &answer.hypotheses[0];
    assert_eq!(top.class, Some(CauseClass::BadDrop));

    let mut policy: PolicyTable = read("policy/default.json");
    let order = &work_order(&graph, top, 1, &policy, Timestamp(9))[0];
    assert_eq!(order.element_id.as_str(), "modem-4");
    assert_eq!(order.action, "Replace the drop and its connectors");
    assert!(order.geo.is_some());
    assert!((0.0..=1.0).contains(&order.confidence));

    for entry in &mut policy.entries {
        if entry.cause == CauseClass::BadDrop {
            entry.action = "Send the drop crew".into();
        }
    }
    let edited = &work_order(&graph, top, 1, &policy, Timestamp(9))[0];
    assert_eq!(edited.action, "Send the drop crew");
    assert_eq!(
        edited.id, order.id,
        "same fault, same place, same time, same id"
    );
}

#[test]
fn switching_finds_the_ingress_leg() {
    let fixture = high();
    let graph = PlantGraph::new(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let kinds = || {
        vec![FaultKind::UpstreamIngress {
            entry_location: Location::Port {
                element_id: ElementId::new("modem-7"),
                port: PortId::new("rf"),
            },
            f_mhz: 25.0,
            bw_mhz: 20.0,
            power_dbmv: 15.0,
            schedule: flat_schedule(),
            burst: None,
        }]
    };
    let environment = Environment {
        schema_version: "1".into(),
        samples: vec![calm()],
        ambient_sources: Vec::new(),
    };
    // What a technician would read at the node: upstream SNR on a channel in the band, in reverse.
    let measure = |amp: Option<&ElementId>| {
        let plant = match amp {
            Some(id) => with_return_atten(&fixture.plant, id),
            None => fixture.plant.clone(),
        };
        let graph = PlantGraph::new(&plant, &fixture.library, &fixture.profile).ok()?;
        let steps = simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults: &faults_of(kinds(), 1),
            seed: 1,
            options: SimOptions {
                detail: Detail::Summary { stride: 8 },
                randomize: None,
                measurement_noise_db: 0.0,
                rxmer_scatter_db: 0.0,
            },
            link: None,
        })
        .ok()?;
        steps[0]
            .service_group
            .us_snr
            .iter()
            .find(|r| r.channel_id.as_str() == "us-sc-2")
            .map(|r| -r.us_snr_db)
    };
    let found = localize_by_switching(&graph, measure);
    assert_eq!(found, Some(ElementId::new("amp-2")));
}

/// Every answer the hybrid gives comes from the sim: re-simulating its faults gives what it predicted, to the byte. A model only shapes the
/// prior and adds places to try.
#[test]
fn the_hybrid_never_returns_something_it_did_not_simulate() {
    let f = high();
    let graph = PlantGraph::new(&f.plant, &f.library, &f.profile).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
    let kinds = vec![FaultKind::ConnectorReflection {
        location: Location::Span {
            span_id: EdgeId::new("span-3"),
            pos_ft: 150.0,
        },
        return_loss_db: 6.0,
        couplings: None,
    }];
    let observed = observe(&graph, &reference, &faults_of(kinds, 4), calm(), 4).unwrap();
    // A prior that's confidently wrong, and names something that doesn't exist.
    let prior = vec![
        ("modem-1".to_string(), 0.9),
        ("no-such-part".to_string(), 0.09),
    ];
    let answer = diagnosis::engine::diagnose_with_prior(
        &graph,
        &reference,
        &observed,
        &calm(),
        &DiagnoserConfig::default(),
        &prior,
    )
    .unwrap();
    assert!(answer.simulations >= answer.hypotheses.len());
    let environment = Environment {
        schema_version: "1".into(),
        samples: vec![calm()],
        ambient_sources: Vec::new(),
    };
    for h in &answer.hypotheses {
        assert!(!h.components.iter().any(|c| c == "no-such-part"));
        let set = FaultSet {
            schema_version: "1".into(),
            seed: 0,
            faults: h.faults.clone(),
        };
        let steps = simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults: &set,
            seed: 0,
            options: SimOptions {
                detail: Detail::Summary {
                    stride: features::reference::SUMMARY_STRIDE,
                },
                randomize: None,
                measurement_noise_db: 0.0,
                rxmer_scatter_db: 0.0,
            },
            link: None,
        })
        .unwrap();
        let again = features::features_for(&graph, &steps[0], &reference, None);
        assert_eq!(
            serde_json::to_string(&again).unwrap(),
            serde_json::to_string(&h.predicted).unwrap()
        );
    }
    // The sim outweighs the wrong prior, because the real place was still tried.
    assert!(
        answer.hypotheses[0]
            .components
            .contains(&"span-3".to_string())
    );
}

/// A fault under every threshold is still tried, if a model names the place.
#[test]
fn a_proposed_place_is_tried_even_when_nothing_reads_affected() {
    let f = high();
    let graph = PlantGraph::new(&f.plant, &f.library, &f.profile).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
    let observed = observe(&graph, &reference, &faults_of(vec![], 4), calm(), 4).unwrap();
    let floor = diagnose(
        &graph,
        &reference,
        &observed,
        &calm(),
        &DiagnoserConfig::default(),
    )
    .unwrap();
    assert_eq!(floor.simulations, 1, "quiet, so only healthy is tried");
    let prior = vec![("".to_string(), 0.5), ("span-3".to_string(), 0.5)];
    let answer = diagnosis::engine::diagnose_with_prior(
        &graph,
        &reference,
        &observed,
        &calm(),
        &DiagnoserConfig::default(),
        &prior,
    )
    .unwrap();
    assert!(answer.simulations > 1);
    assert!(
        answer
            .hypotheses
            .iter()
            .any(|h| h.components.contains(&"span-3".to_string()))
    );
}

/// How many threads run the sims changes nothing in the answer (invariant 4).
#[test]
fn threads_do_not_change_the_answer() {
    let f = low();
    let graph = PlantGraph::new(&f.plant, &f.library, &f.profile).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
    let kinds = vec![FaultKind::ConnectorReflection {
        location: Location::Span {
            span_id: EdgeId::new("span-3"),
            pos_ft: 150.0,
        },
        return_loss_db: 6.0,
        couplings: None,
    }];
    let observed = observe(&graph, &reference, &faults_of(kinds, 7), calm(), 7).unwrap();
    let run = |threads: usize| {
        let config = DiagnoserConfig {
            threads,
            ..DiagnoserConfig::default()
        };
        let answer = diagnose(&graph, &reference, &observed, &calm(), &config).unwrap();
        serde_json::to_string(&answer.hypotheses).unwrap()
    };
    assert_eq!(run(1), run(8));
}
