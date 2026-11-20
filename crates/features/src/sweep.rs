//! The parameter sweep: faults by class, place and severity, each simulated and featurized. The same
//! sweep as the impairment workbench, without the UI (`docs/DESIGN.md`).

use std::io::Write;

use plant_graph::PlantGraph;
use schema::common::{Origin, Timestamp};
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{Fault, FaultSet};
use schema::work_order::CauseClass;
use serde::{Deserialize, Serialize};
use sim::PhysicsError;
use sim::rng::{Rng, hash_str};
use sim::telemetry::{Detail, Scenario, SimOptions, simulate};
use thiserror::Error;

use crate::extract::{StepFeatures, features_for};
use crate::reference::{DesignReference, SUMMARY_STRIDE};
use crate::sampler::{CLASSES, class_of, fault_element, sample_fault};

/// What can stop a sweep.
#[derive(Error, Debug)]
pub enum SweepError {
    /// The sim failed.
    #[error(transparent)]
    Physics(#[from] PhysicsError),
    /// Writing failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Encoding failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// One labeled example. Every row says it's synthetic (invariant 11).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Row {
    /// Always `Synthetic` from here.
    pub origin: Origin,
    /// The observation schema it came from (invariant 3).
    pub observation_schema: String,
    /// The plant's service group.
    pub service_group: String,
    /// The profile.
    pub profile: String,
    /// The seed that drew the fault and the telemetry.
    pub seed: u64,
    /// The cause class, or `None` for healthy plant.
    pub class: Option<CauseClass>,
    /// The plant element the fault is on.
    pub element: Option<String>,
    /// The fault itself.
    pub fault: Option<Fault>,
    /// What came out.
    pub features: StepFeatures,
}

/// One environment sample, drawn so temperature and rain vary across a sweep.
pub fn random_sample(rng: &mut Rng, t: i64) -> EnvironmentSample {
    let temp_c = rng.range(-15.0, 40.0);
    EnvironmentSample {
        t: Timestamp(t),
        temp_c,
        ground_temp_c: 10.0 + (temp_c - 10.0) * 0.3,
        rain_mm_per_hr: if rng.uniform() < 0.3 {
            rng.range(0.5, 12.0)
        } else {
            0.0
        },
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

/// Simulates one fault set the way a sweep does: two steps, so counters have a delta, with domain
/// randomization and measurement noise on.
pub fn observe(
    graph: &PlantGraph<'_>,
    reference: &DesignReference,
    faults: &FaultSet,
    sample: EnvironmentSample,
    seed: u64,
) -> Result<StepFeatures, PhysicsError> {
    observe_linked(graph, reference, faults, sample, seed, None)
}

/// [`observe`], with the node's link state (§16), as the twin has it from its region's fiber plant.
pub fn observe_linked(
    graph: &PlantGraph<'_>,
    reference: &DesignReference,
    faults: &FaultSet,
    sample: EnvironmentSample,
    seed: u64,
    link: Option<&sim::optics::NodeLink>,
) -> Result<StepFeatures, PhysicsError> {
    observe_in(graph, reference, faults, sample, &[], seed, link)
}

/// [`observe_linked`], with radio around the plant that healthy homes' wiring picks up (§18).
pub fn observe_in(
    graph: &PlantGraph<'_>,
    reference: &DesignReference,
    faults: &FaultSet,
    sample: EnvironmentSample,
    ambient: &[schema::environment::AmbientSource],
    seed: u64,
    link: Option<&sim::optics::NodeLink>,
) -> Result<StepFeatures, PhysicsError> {
    let mut later = sample.clone();
    later.t = Timestamp(sample.t.0 + 300);
    let environment = Environment {
        schema_version: schema::environment::SCHEMA_VERSION.to_string(),
        samples: vec![sample, later],
        ambient_sources: ambient.to_vec(),
    };
    let steps = simulate(&Scenario {
        graph,
        environment: &environment,
        faults,
        seed,
        options: SimOptions {
            detail: Detail::Summary {
                stride: SUMMARY_STRIDE,
            },
            randomize: Some(seed),
            measurement_noise_db: 0.25,
            rxmer_scatter_db: 0.0,
        },
        link,
    })?;
    Ok(features_for(graph, &steps[1], reference, Some(&steps[0])))
}

/// Writes `n` rows, drawing classes evenly and one row in ten healthy. Same seed, same bytes.
pub fn run_sweep<W: Write>(
    graph: &PlantGraph<'_>,
    n: usize,
    seed: u64,
    out: &mut W,
) -> Result<usize, SweepError> {
    let reference = DesignReference::compute(graph)?;
    let plant_hash = hash_str(graph.plant().service_group.as_str());
    let mut written = 0;
    for i in 0..n {
        let row_seed = Rng::derive(seed, &[plant_hash, i as u64]).next_u64();
        let mut rng = Rng::new(row_seed);
        let fault = if i % 10 == 9 {
            None
        } else {
            sample_fault(graph, CLASSES[i % CLASSES.len()], &mut rng)
        };
        let faults = FaultSet {
            schema_version: schema::fault::SCHEMA_VERSION.to_string(),
            seed: row_seed,
            faults: fault.iter().cloned().collect(),
        };
        let sample = random_sample(&mut rng, 0);
        let features = observe(graph, &reference, &faults, sample, row_seed)?;
        let row = Row {
            origin: Origin::Synthetic,
            observation_schema: schema::observation::SCHEMA_VERSION.to_string(),
            service_group: graph.plant().service_group.to_string(),
            profile: graph.profile().id.to_string(),
            seed: row_seed,
            class: fault.as_ref().map(|f| class_of(&f.kind)),
            element: fault.as_ref().and_then(|f| fault_element(&f.kind)),
            fault,
            features,
        };
        serde_json::to_writer(&mut *out, &row)?;
        out.write_all(b"\n")?;
        written += 1;
    }
    Ok(written)
}
