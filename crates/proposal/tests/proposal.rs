//! Rows, and the exported model against Python's own numbers. All synthetic.

use std::fs;

use features::{DesignReference, features_for};
use plant_graph::PlantGraph;
use proposal::{ComponentRow, FEATURE_NAMES, Model, ModelError, Proposer, component_rows};
use schema::common::{EdgeId, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{Fault, FaultKind, FaultSet, Location, Onset};
use schema::plant::Plant;
use schema::profile::Profile;
use serde::Deserialize;
use sim::telemetry::{Detail, Scenario, SimOptions, simulate};

fn root(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
}

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    serde_json::from_str(&fs::read_to_string(root(path)).unwrap()).unwrap()
}

fn rows_for(plant: &Plant, faults: Vec<FaultKind>) -> Vec<ComponentRow> {
    let library: ComponentLibrary = read("fixtures/library/generic.json");
    let profile: Profile = read("fixtures/profiles/d31_highsplit_1218_rpd.json");
    let graph = PlantGraph::new(plant, &library, &profile).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
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
    let set = FaultSet {
        schema_version: "2".into(),
        seed: 1,
        faults: faults
            .into_iter()
            .map(|kind| Fault {
                onset: Onset::Instant,
                kind,
            })
            .collect(),
    };
    let steps = simulate(&Scenario {
        graph: &graph,
        environment: &environment,
        faults: &set,
        seed: 1,
        options: SimOptions {
            detail: Detail::Summary { stride: 8 },
            randomize: None,
            measurement_noise_db: 0.0,
            rxmer_scatter_db: 0.0,
        },
        link: None,
    })
    .unwrap();
    component_rows(
        &graph,
        &features_for(&graph, &steps[1], &reference, Some(&steps[0])),
    )
}

fn fixture() -> Plant {
    read("fixtures/one_street/plant_highsplit_rpd.json")
}

#[test]
fn one_row_per_place_a_fault_could_be() {
    let rows = rows_for(&fixture(), vec![]);
    // Healthy, the root, 6 spans, 2 amps, 8 drops, 1 supply.
    assert_eq!(rows.len(), 19);
    assert_eq!(rows[0].component, "");
    for row in &rows {
        assert_eq!(row.x.len(), FEATURE_NAMES.len(), "{}", row.component);
    }
}

#[test]
fn rows_are_deterministic_and_order_free() {
    let plant = fixture();
    let mut shuffled = fixture();
    shuffled.elements.reverse();
    shuffled.edges.reverse();
    assert_eq!(rows_for(&plant, vec![]), rows_for(&shuffled, vec![]));
}

#[test]
fn a_fault_shows_in_the_row_of_the_span_above_it() {
    let kind = FaultKind::Suckout {
        location: Location::Span {
            span_id: EdgeId::new("span-5"),
            pos_ft: 100.0,
        },
        f0_mhz: 700.0,
        depth_db: 25.0,
        width_mhz: 15.0,
    };
    let rows = rows_for(&fixture(), vec![kind]);
    let index = FEATURE_NAMES
        .iter()
        .position(|n| n == "level_min_delta")
        .unwrap();
    let span = |id: &str| rows.iter().find(|r| r.component == id).unwrap();
    let at_fault = span("span-5").x[index].unwrap();
    let elsewhere = span("span-3").x[index].unwrap();
    assert!(at_fault < elsewhere - 2.0, "{at_fault} against {elsewhere}");
}

#[derive(Deserialize)]
struct ParityRow {
    x: Vec<Vec<Option<f64>>>,
    component: Vec<f64>,
    class: Vec<Vec<f64>>,
}

/// What Rust computes is what Python computed. Not skipped: without the artifact, this fails.
#[test]
fn the_evaluator_matches_python() {
    let model =
        Model::from_json(&fs::read_to_string(root("ml/artifacts/proposal_gbdt.json")).unwrap())
            .unwrap();
    let parity: Vec<ParityRow> = read("ml/artifacts/parity.json");
    assert!(!parity.is_empty());
    let mut checked = 0;
    for row in &parity {
        for (i, x) in row.x.iter().enumerate() {
            let component = model.component.raw(x)[0];
            assert!(
                (component - row.component[i]).abs() < 1e-9,
                "{component} against {}",
                row.component[i]
            );
            let class = model.class.raw(x);
            for (a, b) in class.iter().zip(&row.class[i]) {
                assert!((a - b).abs() < 1e-9, "{a} against {b}");
            }
            checked += 1;
        }
    }
    assert!(checked > 20);
    assert_eq!(model.trained_on, schema::common::Origin::Synthetic);
}

#[test]
fn a_model_from_another_schema_is_refused() {
    let text = fs::read_to_string(root("ml/artifacts/proposal_gbdt.json")).unwrap();
    let good: Model = serde_json::from_str(&text).unwrap();
    good.check().unwrap();
    let mut old = good.clone();
    old.observation_schema = "0".into();
    assert!(matches!(
        old.check(),
        Err(ModelError::ObservationSchema { .. })
    ));
    let mut old = good.clone();
    old.fault_schema = "1".into();
    assert!(matches!(old.check(), Err(ModelError::FaultSchema { .. })));
    // A model reads the features it names, so fewer is fine, and one this build doesn't compute isn't (task 24).
    let mut fewer = good.clone();
    fewer.features.pop();
    fewer.check().unwrap();
    let mut different = good;
    different.features.push("no_such_feature".into());
    assert!(matches!(
        different.check(),
        Err(ModelError::UnknownFeature(_))
    ));
}

#[test]
fn proposals_are_a_distribution() {
    let model =
        Model::from_json(&fs::read_to_string(root("ml/artifacts/proposal_gbdt.json")).unwrap())
            .unwrap();
    let rows = rows_for(&fixture(), vec![]);
    let proposals = model.propose(&rows);
    assert_eq!(proposals.len(), rows.len());
    let total: f64 = proposals.iter().map(|p| p.probability).sum();
    assert!((total - 1.0).abs() < 1e-9);
    assert!(
        proposals
            .windows(2)
            .all(|w| w[0].probability >= w[1].probability)
    );
}

#[derive(Deserialize)]
struct GraphParityRow {
    nodes: Vec<proposal::GraphNode>,
    rows: Vec<ComponentRow>,
    scores: Vec<f64>,
}

/// What Rust computes is what Python computed in double precision. Not skipped: without the artifact, this fails.
#[test]
fn the_graph_network_matches_python() {
    let text = fs::read_to_string(root("ml/artifacts/gnn_maps.json")).unwrap();
    let Proposer::Graph(model) = Proposer::from_json(&text).unwrap() else {
        panic!("this file is a graph model");
    };
    let parity: Vec<GraphParityRow> = read("ml/artifacts/gnn_parity.json");
    assert!(!parity.is_empty());
    let mut checked = 0;
    for row in &parity {
        let scores = model.scores(&row.nodes, &row.rows).unwrap();
        assert_eq!(scores.len(), row.scores.len());
        for (a, b) in scores.iter().zip(&row.scores) {
            assert!((a - b).abs() < 1e-9, "{a} against {b}");
            checked += 1;
        }
    }
    assert!(checked > 20);
    assert_eq!(model.trained_on, schema::common::Origin::Synthetic);
}

#[test]
fn a_graph_model_on_another_schema_is_refused() {
    let text = fs::read_to_string(root("ml/artifacts/gnn_maps.json")).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["observation_schema"] = "0".into();
    let error = Proposer::from_json(&value.to_string()).unwrap_err();
    assert!(error.to_string().contains("observation schema"), "{error}");
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["fault_schema"] = "0".into();
    let error = Proposer::from_json(&value.to_string()).unwrap_err();
    assert!(error.to_string().contains("fault schema"), "{error}");
}

#[test]
fn a_graph_model_with_the_wrong_shape_is_refused() {
    let text = fs::read_to_string(root("ml/artifacts/gnn_maps.json")).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["hidden"] = 3.into();
    let error = Proposer::from_json(&value.to_string()).unwrap_err();
    assert!(error.to_string().contains("shape"), "{error}");
}

/// The answer doesn't depend on the order a plant's lists are in, and two runs give the same bytes (invariant 4).
#[test]
fn the_graph_network_is_deterministic() {
    let text = fs::read_to_string(root("ml/artifacts/gnn_maps.json")).unwrap();
    let model = Proposer::from_json(&text).unwrap();
    let plant: Plant = read("fixtures/one_street/plant_highsplit_rpd.json");
    let library: ComponentLibrary = read("fixtures/library/generic.json");
    let profile: Profile = read("fixtures/profiles/d31_highsplit_1218_rpd.json");
    let answer = |plant: &Plant| {
        let graph = PlantGraph::new(plant, &library, &profile).unwrap();
        let reference = DesignReference::compute(&graph).unwrap();
        let faults = FaultSet {
            schema_version: schema::fault::SCHEMA_VERSION.to_string(),
            seed: 1,
            faults: Vec::new(),
        };
        let sample = EnvironmentSample {
            t: Timestamp(0),
            temp_c: 20.0,
            ground_temp_c: 15.0,
            rain_mm_per_hr: 0.0,
            solar_w_per_m2: None,
            wind_m_per_s: None,
        };
        let observed = features::sweep::observe(&graph, &reference, &faults, sample, 1).unwrap();
        let rows = component_rows(&graph, &observed);
        serde_json::to_string(&model.propose(&graph, plant, &observed, &rows).unwrap()).unwrap()
    };
    let first = answer(&plant);
    assert_eq!(first, answer(&plant));
    let mut reordered = plant.clone();
    reordered.elements.reverse();
    reordered.edges.reverse();
    assert_eq!(first, answer(&reordered));
}

/// Task 24: a model from before reads the features it names, which are this build's first, in the order it was trained on,
/// so on a version 1 plant it gives the same answers to the byte as on rows cut to what it knew. The columns added after
/// them are empty there, and nothing it reads moved.
#[test]
fn a_model_from_before_gives_the_same_answers_on_version_1_plant() {
    let text = fs::read_to_string(root("ml/artifacts/proposal_gbdt.json")).unwrap();
    let model = Model::from_json(&text).unwrap();
    let trained: Vec<String> = read("ml/artifacts/feature_names.json");
    assert_eq!(model.features, trained);
    assert_eq!(&FEATURE_NAMES[..trained.len()], trained.as_slice());
    for faults in [
        vec![],
        vec![FaultKind::ConnectorReflection {
            location: Location::Span {
                span_id: EdgeId::new("span-3"),
                pos_ft: 150.0,
            },
            return_loss_db: 6.0,
            couplings: None,
        }],
    ] {
        let rows = rows_for(&fixture(), faults);
        // No version 2 kinds on a version 1 plant, and nothing in the new columns.
        assert!(rows.iter().all(|r| !matches!(
            r.kind,
            proposal::rows::ComponentKind::Premises | proposal::rows::ComponentKind::DropCable
        )));
        for r in &rows {
            let extra = &r.x[trained.len()..];
            // The two kind flags are 0; no fittings, optics, per-port levels or second modem in a home to read.
            assert!(
                extra[0] == Some(0.0) && extra[1] == Some(0.0),
                "{:?}",
                extra
            );
            assert!(extra[2..7].iter().all(Option::is_none), "{:?}", extra);
        }
        let cut: Vec<ComponentRow> = rows
            .iter()
            .map(|r| ComponentRow {
                x: r.x[..trained.len()].to_vec(),
                ..r.clone()
            })
            .collect();
        assert_eq!(
            serde_json::to_string(&model.propose(&rows)).unwrap(),
            serde_json::to_string(&model.propose(&cut)).unwrap()
        );
    }
}

/// Task 24's models, from `ml/artifacts/as_built/` (or `HFC_AS_BUILT_ARTIFACTS`).
fn as_built(file: &str) -> String {
    let dir =
        std::env::var("HFC_AS_BUILT_ARTIFACTS").unwrap_or_else(|_| root("ml/artifacts/as_built"));
    format!("{dir}/{file}")
}

/// Task 24: the GBDT trained on version 2 plant matches Python on its parity rows, records the plant it trained on, and
/// reads only features this build computes. Not skipped: without the artifact, this fails.
#[test]
fn the_as_built_trees_match_python() {
    let model = Model::from_json(&fs::read_to_string(as_built("gbdt_maps.json")).unwrap()).unwrap();
    assert_eq!(model.plant_schema.as_deref(), Some("2"));
    assert_eq!(model.trained_on, schema::common::Origin::Synthetic);
    let parity: Vec<ParityRow> =
        serde_json::from_str(&fs::read_to_string(as_built("parity.json")).unwrap()).unwrap();
    let mut checked = 0;
    for row in &parity {
        for (i, x) in row.x.iter().enumerate() {
            assert!((model.component.raw(x)[0] - row.component[i]).abs() < 1e-9);
            for (a, b) in model.class.raw(x).iter().zip(&row.class[i]) {
                assert!((a - b).abs() < 1e-9, "{a} against {b}");
            }
            checked += 1;
        }
    }
    assert!(checked > 20);
}

/// Task 24: the graph model trained on version 2 plant matches Python in double precision on its parity rows, with the
/// version 2 node kinds and the two new candidate kinds. Not skipped: without the artifact, this fails.
#[test]
fn the_as_built_graph_network_matches_python() {
    let Proposer::Graph(model) =
        Proposer::from_json(&fs::read_to_string(as_built("gnn_maps.json")).unwrap()).unwrap()
    else {
        panic!("this file is a graph model");
    };
    assert_eq!(model.plant_schema.as_deref(), Some("2"));
    assert_eq!(model.node_kinds, proposal::NODE_KINDS.to_vec());
    let parity: Vec<GraphParityRow> =
        serde_json::from_str(&fs::read_to_string(as_built("gnn_parity.json")).unwrap()).unwrap();
    let mut checked = 0;
    for row in &parity {
        let scores = model.scores(&row.nodes, &row.rows).unwrap();
        assert_eq!(scores.len(), row.scores.len());
        for (a, b) in scores.iter().zip(&row.scores) {
            assert!((a - b).abs() < 1e-9, "{a} against {b}");
            checked += 1;
        }
    }
    assert!(checked > 20);
}
