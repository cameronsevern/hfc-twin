//! What the twin holds. Inputs only: plant, profile, library, policy, environment, faults and seed.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use diagnosis::{DiagnoserConfig, Diagnosis, diagnose};
use features::sweep::observe_linked;
use features::{DesignReference, StepFeatures};
use plant_graph::PlantGraph;
use schema::common::{ProfileId, ServiceGroupId, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::EnvironmentSample;
use schema::fault::{Fault, FaultSet};
use schema::plant::Plant;
use schema::profile::Profile;
use schema::work_order::{PolicyTable, WorkOrder};
use sim::PhysicsError;
use sim::rng::hash_str;
use thiserror::Error;

/// What can go wrong in an operation.
#[derive(Error, Debug)]
pub enum TwinError {
    /// No such service group.
    #[error("no service group {0}")]
    NoServiceGroup(String),
    /// The plant doesn't reference a known profile.
    #[error("no profile {0}")]
    NoProfile(String),
    /// The plant failed validation.
    #[error("invalid plant: {0}")]
    InvalidPlant(String),
    /// A fault names something the plant doesn't have.
    #[error("invalid fault: {0}")]
    InvalidFault(String),
    /// The sim failed.
    #[error(transparent)]
    Physics(#[from] PhysicsError),
    /// A file couldn't be read.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A file couldn't be parsed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// An operation needs something the twin wasn't given.
    #[error("{0}")]
    Refused(String),
    /// A stored document is on a schema version nothing here can bring forward.
    #[error("can't migrate: {0}")]
    Migration(String),
    /// The proposal model couldn't read this plant.
    #[error("proposal model: {0}")]
    Model(String),
}

/// One node's records.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Its plant.
    pub plant: Plant,
    /// Faults injected into the sim.
    pub faults: Vec<Fault>,
    /// Work orders issued against it.
    pub work_orders: Vec<WorkOrder>,
    /// What each order's telemetry looked like when it was issued, and what became of it.
    pub evidence: BTreeMap<uuid::Uuid, crate::lifecycle::Evidence>,
}

/// What a map was designed with.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DesignInput {
    /// The profile every node runs.
    pub profile: schema::common::ProfileId,
    /// The controls, pins among them.
    pub controls: schema::region::DesignControls,
}

/// What a version 2 design weighed and left out, beyond its records.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DesignReport {
    /// The rules.
    pub rules_id: String,
    /// Every candidate's cost and issues, and which was kept.
    pub candidates: Vec<design::v2::Candidate>,
    /// Homes coax would cost too much for.
    pub over_ceiling: Vec<design::v2::OverCeiling>,
    /// Homes no street within a drop reaches, or no street joins to the hub.
    pub unserved: Vec<String>,
    /// In the second mode, what the upgrade did.
    pub upgrade: Option<design::v2::upgrade::UpgradeReport>,
}

/// The twin.
#[derive(Debug, Clone)]
pub struct Twin {
    /// The component library.
    pub library: ComponentLibrary,
    /// The architecture profiles.
    pub profiles: BTreeMap<ProfileId, Profile>,
    /// What to do about each cause.
    pub policy: PolicyTable,
    /// Each node, by service group.
    pub nodes: BTreeMap<ServiceGroupId, Node>,
    /// The environment now.
    pub sample: EnvironmentSample,
    /// The seed this twin's draws derive from.
    pub seed: u64,
    /// Whether `get_modem_identity` may answer.
    pub identity_allowed: bool,
    /// The maps designs were drawn from, by id.
    pub maps: BTreeMap<String, schema::region::StreetMap>,
    /// The regions designed, by id.
    pub regions: BTreeMap<String, schema::region::Region>,
    /// Fiber plants, by id. A region's `fiber_ref` names one. Version 2.
    pub fiber_plants: BTreeMap<String, schema::fiber::FiberPlant>,
    /// Fiber faults injected into the sim, by region id: one fault can reach several nodes (§16).
    pub fiber_faults: BTreeMap<String, Vec<schema::fiber_fault::FiberFault>>,
    /// What each map was last designed with, by map id. A re-solve starts from these, and pins live here.
    pub design_inputs: BTreeMap<String, DesignInput>,
    /// What each OpenStreetMap import left out, by map id.
    pub osm_reports: BTreeMap<String, design::osm::OsmReport>,
    /// Design rules the version 2 designer can design by, by id (task 23).
    pub design_rules: BTreeMap<String, schema::design_rules::DesignRules>,
    /// What each map's last version 2 design weighed: its candidates, the homes over the cost ceiling, an upgrade's report.
    /// Derived, like `cache`: a re-solve makes it again, and it isn't kept.
    pub design_reports: BTreeMap<String, DesignReport>,
    /// Derived values, per service group.
    pub cache: crate::cache::Cache,
    /// Each node's telemetry with nothing wrong with it. Derived, like `cache`.
    pub baselines: crate::cache::Cache,
    /// The proposal model, if one's loaded. Without it, diagnosis is the no-ML floor. Trained on version 1 plant, so it's
    /// asked only about a plant without version 2 parts.
    pub model: Option<proposal::Model>,
    /// The proposal model for a plant with version 2 parts (task 24), if one's loaded and it met task 15's rule on that
    /// plant. Without it, such a plant gets the floor.
    pub as_built_model: Option<proposal::Proposer>,
    /// The region run day to day, as a pair of worlds (task 26), once one is started.
    pub ops: Option<crate::operate::OpsRun>,
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, TwinError> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

/// Brings a stored plant to the current schema version (invariant 3). Version 1 plants, `one_street`'s among them, mean
/// the same at version 2.
pub fn migrate_plant(plant: Plant) -> Result<Plant, TwinError> {
    schema::plant::migrate(plant).map_err(TwinError::Migration)
}

/// Brings a stored library to the current schema version.
pub fn migrate_library(library: ComponentLibrary) -> Result<ComponentLibrary, TwinError> {
    schema::component::migrate(library).map_err(TwinError::Migration)
}

/// The design rules files and the profiles only the version 2 designer uses: the two fully loaded ones and the legacy one
/// its second mode designs under first (task 23).
pub fn load_design_rules(twin: &mut Twin, fixtures: &Path) -> Result<(), TwinError> {
    for name in [
        "d31_lowsplit_1002_iccap_full",
        "d31_highsplit_1218_rpd_full",
        "d30_lowsplit_750_legacy",
    ] {
        let profile =
            schema::profile::migrate(read(&fixtures.join(format!("profiles/{name}.json")))?)
                .map_err(TwinError::Migration)?;
        twin.profiles.entry(profile.id.clone()).or_insert(profile);
    }
    for file in ["generic.json", "legacy-750.json"] {
        let rules: schema::design_rules::DesignRules =
            schema::design_rules::migrate(read(&fixtures.join("design_rules").join(file))?)
                .map_err(TwinError::Migration)?;
        twin.design_rules.insert(rules.id.clone(), rules);
    }
    Ok(())
}

/// Both first profiles' one-street plants, as two service groups: the v0 setup.
pub fn load_one_street(fixtures: &Path, seed: u64) -> Result<Twin, TwinError> {
    let library = migrate_library(read(&fixtures.join("library/generic.json"))?)?;
    let policy = schema::work_order::migrate_policy(read(&fixtures.join("policy/default.json"))?)
        .map_err(TwinError::Migration)?;
    let mut twin = Twin::new(library, policy, seed);
    for (profile_file, plant_file, name) in [
        (
            "profiles/d31_lowsplit_1002_iccap.json",
            "one_street/plant_lowsplit_iccap.json",
            "sg-one-street-lowsplit",
        ),
        (
            "profiles/d31_highsplit_1218_rpd.json",
            "one_street/plant_highsplit_rpd.json",
            "sg-one-street-highsplit",
        ),
    ] {
        let profile: Profile = read(&fixtures.join(profile_file))?;
        twin.profiles.insert(profile.id.clone(), profile);
        let mut plant = migrate_plant(read(&fixtures.join(plant_file))?)?;
        // Both fixture files name one street; here they're two nodes side by side.
        plant.service_group = ServiceGroupId::new(name);
        twin.add_plant(plant)?;
    }
    load_design_rules(&mut twin, fixtures)?;
    Ok(twin)
}

/// `fixtures/one_street_as_built/`: the same street with every version 2 part, on both profiles, with its fiber plant and region
/// (task 22). Every plant and the fiber plant are synthetic.
pub fn load_as_built(fixtures: &Path, seed: u64) -> Result<Twin, TwinError> {
    let library = migrate_library(read(&fixtures.join("library/generic.json"))?)?;
    let policy = schema::work_order::migrate_policy(read(&fixtures.join("policy/default.json"))?)
        .map_err(TwinError::Migration)?;
    let mut twin = Twin::new(library, policy, seed);
    for (profile_file, plant_file) in [
        (
            "profiles/d31_lowsplit_1002_iccap.json",
            "one_street_as_built/plant_lowsplit_iccap.json",
        ),
        (
            "profiles/d31_highsplit_1218_rpd.json",
            "one_street_as_built/plant_highsplit_rpd.json",
        ),
    ] {
        let profile = schema::profile::migrate(read(&fixtures.join(profile_file))?)
            .map_err(TwinError::Migration)?;
        twin.profiles.insert(profile.id.clone(), profile);
        twin.add_plant(migrate_plant(read(&fixtures.join(plant_file))?)?)?;
    }
    for name in [
        "d31_lowsplit_1002_iccap_full",
        "d31_highsplit_1218_rpd_full",
    ] {
        let profile =
            schema::profile::migrate(read(&fixtures.join(format!("profiles/{name}.json")))?)
                .map_err(TwinError::Migration)?;
        twin.profiles.insert(profile.id.clone(), profile);
    }
    twin.add_fiber_plant(read(&fixtures.join("one_street_as_built/fiber.json"))?)?;
    let region: schema::region::Region =
        schema::region::migrate(read(&fixtures.join("one_street_as_built/region.json"))?)
            .map_err(TwinError::Migration)?;
    twin.regions.insert(region.id.clone(), region);
    load_design_rules(&mut twin, fixtures)?;
    Ok(twin)
}

/// The twin answers one person at a time, so a diagnosis gets every core.
fn parallel_config() -> DiagnoserConfig {
    DiagnoserConfig {
        threads: std::thread::available_parallelism().map_or(4, |n| n.get()),
        ..DiagnoserConfig::default()
    }
}

/// The top answer's probability from the model's calibration, measured on held-out runs, in place of the raw posterior. The rest keep their
/// proportions, so the posterior still sums to one.
fn calibrated(
    calibration: Option<&proposal::Calibration>,
    proposals: &[proposal::Proposal],
    mut answer: Diagnosis,
    modems: usize,
) -> Diagnosis {
    let (Some(calibration), Some(top)) = (
        calibration,
        diagnosis::engine::top_answer_summary(&answer, modems),
    ) else {
        return answer;
    };
    let x = proposal::calibration_features(
        top.posterior,
        top.chi2_per_dof,
        proposals
            .iter()
            .find(|p| p.component == top.component)
            .map_or(0.0, |p| p.probability),
        top.component.is_empty(),
        top.affected_share,
    );
    let confidence = calibration.confidence(&x);
    let rest_was = (1.0 - top.posterior).max(1e-12);
    for (i, h) in answer.hypotheses.iter_mut().enumerate() {
        h.posterior = if i == 0 {
            confidence
        } else {
            h.posterior / rest_was * (1.0 - confidence)
        };
    }
    answer
}

impl Twin {
    /// An empty twin at 20 °C.
    pub fn new(library: ComponentLibrary, policy: PolicyTable, seed: u64) -> Twin {
        Twin {
            library,
            profiles: BTreeMap::new(),
            policy,
            nodes: BTreeMap::new(),
            sample: EnvironmentSample {
                t: Timestamp(0),
                temp_c: 20.0,
                ground_temp_c: 20.0,
                rain_mm_per_hr: 0.0,
                solar_w_per_m2: None,
                wind_m_per_s: None,
            },
            seed,
            identity_allowed: false,
            maps: BTreeMap::new(),
            regions: BTreeMap::new(),
            fiber_plants: BTreeMap::new(),
            fiber_faults: BTreeMap::new(),
            design_inputs: BTreeMap::new(),
            osm_reports: BTreeMap::new(),
            design_rules: BTreeMap::new(),
            design_reports: BTreeMap::new(),
            cache: crate::cache::Cache::default(),
            baselines: crate::cache::Cache::default(),
            model: None,
            as_built_model: None,
            ops: None,
        }
    }

    /// Adds a node, refusing a plant that doesn't validate.
    pub fn add_plant(&mut self, plant: Plant) -> Result<(), TwinError> {
        let profile = self
            .profiles
            .get(&plant.profile_ref)
            .ok_or_else(|| TwinError::NoProfile(plant.profile_ref.to_string()))?;
        PlantGraph::new(&plant, &self.library, profile)
            .map_err(|e| TwinError::InvalidPlant(format!("{e:?}")))?;
        self.nodes.insert(
            plant.service_group.clone(),
            Node {
                plant,
                faults: Vec::new(),
                work_orders: Vec::new(),
                evidence: BTreeMap::new(),
            },
        );
        Ok(())
    }

    /// Adds a fiber plant, refusing one that doesn't validate against the library.
    pub fn add_fiber_plant(&mut self, fiber: schema::fiber::FiberPlant) -> Result<(), TwinError> {
        fiber
            .validate(&self.library)
            .map_err(|e| TwinError::InvalidPlant(format!("fiber plant {}: {e:?}", fiber.id)))?;
        self.fiber_plants.insert(fiber.id.clone(), fiber);
        Ok(())
    }

    /// Why the proposal model won't be asked about this node, if it won't. The shipped model was trained on version 1
    /// plants; a plant with any version 2 part is asked task 24's model if one is loaded, and gets the no-ML floor if
    /// not (`docs/as-built-plan.md`, "Risks").
    pub fn model_withheld(&self, id: &ServiceGroupId) -> Result<Option<String>, TwinError> {
        let parts = self.node(id)?.plant.version_2_parts(&self.library);
        // Task 24: a model trained on version 2 plant answers for one, if it's loaded.
        if !parts.is_empty()
            && self
                .as_built_model
                .as_ref()
                .is_some_and(|m| proposal::trained_for(m.plant_schema(), true))
        {
            return Ok(None);
        }
        Ok((!parts.is_empty()).then(|| {
            format!(
                "the proposal model was trained on version 1 plants only, and this plant has version 2 parts ({}), so this is the no-ML answer",
                parts.join("; ")
            )
        }))
    }

    /// Whether the model that would answer for this node has a fitted confidence.
    pub fn model_calibrated(&self, id: &ServiceGroupId) -> Result<bool, TwinError> {
        let as_built = !self
            .node(id)?
            .plant
            .version_2_parts(&self.library)
            .is_empty();
        Ok(if as_built {
            self.as_built_model
                .as_ref()
                .is_some_and(|m| m.calibration().is_some())
        } else {
            self.model.as_ref().is_some_and(|m| m.calibration.is_some())
        })
    }

    /// A node.
    pub fn node(&self, id: &ServiceGroupId) -> Result<&Node, TwinError> {
        self.nodes
            .get(id)
            .ok_or_else(|| TwinError::NoServiceGroup(id.to_string()))
    }

    /// A node, to change.
    pub fn node_mut(&mut self, id: &ServiceGroupId) -> Result<&mut Node, TwinError> {
        self.nodes
            .get_mut(id)
            .ok_or_else(|| TwinError::NoServiceGroup(id.to_string()))
    }

    /// A node's graph.
    pub fn graph<'a>(&'a self, node: &'a Node) -> Result<PlantGraph<'a>, TwinError> {
        let profile = self
            .profiles
            .get(&node.plant.profile_ref)
            .ok_or_else(|| TwinError::NoProfile(node.plant.profile_ref.to_string()))?;
        PlantGraph::new(&node.plant, &self.library, profile)
            .map_err(|e| TwinError::InvalidPlant(format!("{e:?}")))
    }

    /// The region a node is in, if one names it.
    pub fn region_of(&self, id: &ServiceGroupId) -> Option<&schema::region::Region> {
        self.regions
            .values()
            .find(|r| r.nodes.iter().any(|n| &n.service_group == id))
    }

    /// §16: a node's link state, from its region's fiber plant and fiber faults at the twin's time, with the node's own draws
    /// within each part's tolerance. `None` where no fiber plant serves it, so the sim keeps its constants. Pure: derived from
    /// stored plant, faults and the sample (invariant 1).
    pub fn link(&self, id: &ServiceGroupId) -> Result<Option<sim::optics::NodeLink>, TwinError> {
        self.link_under(id, true)
    }

    /// [`Twin::link`] with the region's fiber faults or without them.
    pub fn link_under(
        &self,
        id: &ServiceGroupId,
        with_faults: bool,
    ) -> Result<Option<sim::optics::NodeLink>, TwinError> {
        let Some(region) = self.region_of(id) else {
            return Ok(None);
        };
        let Some(fiber) = region
            .fiber_ref
            .as_ref()
            .and_then(|f| self.fiber_plants.get(f))
        else {
            return Ok(None);
        };
        let node = self.node(id)?;
        let profile = self
            .profiles
            .get(&node.plant.profile_ref)
            .ok_or_else(|| TwinError::NoProfile(node.plant.profile_ref.to_string()))?;
        let faults = if with_faults {
            self.fiber_faults
                .get(&region.id)
                .map_or(&[][..], |f| f.as_slice())
        } else {
            &[]
        };
        Ok(sim::optics::node_link_drawn(
            fiber,
            &node.plant,
            &self.library,
            &profile.channel_plan,
            faults,
            self.sample.t,
            Some(self.group_seed(id)),
        )?)
    }

    /// The seed for one service group: telemetry and faults stay put when another node changes.
    pub fn group_seed(&self, id: &ServiceGroupId) -> u64 {
        self.seed ^ hash_str(id.as_str())
    }

    /// A fault set as the sim takes it.
    pub fn fault_set(&self, id: &ServiceGroupId, faults: &[Fault]) -> FaultSet {
        FaultSet {
            schema_version: schema::fault::SCHEMA_VERSION.to_string(),
            seed: self.group_seed(id),
            faults: faults.to_vec(),
        }
    }

    /// What the node's telemetry says now, under `faults`, as features.
    pub fn features_under(
        &self,
        id: &ServiceGroupId,
        faults: &[Fault],
    ) -> Result<StepFeatures, TwinError> {
        let node = self.node(id)?;
        let graph = self.graph(node)?;
        let reference = DesignReference::compute(&graph)?;
        let set = self.fault_set(id, faults);
        let link = self.link(id)?;
        Ok(observe_linked(
            &graph,
            &reference,
            &set,
            self.sample.clone(),
            self.group_seed(id),
            link.as_ref(),
        )?)
    }

    /// The node on a mild dry day with no faults: its own normal, as history gives a real one. Even healthy plant has modems reading off
    /// design, so "affected" has to mean off this, or it never comes back to nothing after a repair.
    pub fn baseline(&self, id: &ServiceGroupId) -> Result<StepFeatures, TwinError> {
        let node = self.node(id)?;
        let profile = self
            .profiles
            .get(&node.plant.profile_ref)
            .ok_or_else(|| TwinError::NoProfile(node.plant.profile_ref.to_string()))?;
        let mut normal = self.sample.clone();
        normal.temp_c = 20.0;
        normal.ground_temp_c = 20.0;
        normal.rain_mm_per_hr = 0.0;
        // Its normal has no fiber fault either.
        let link = self.link_under(id, false)?;
        let key = crate::cache::key(
            &node.plant,
            profile,
            &self.library,
            &normal,
            &[],
            self.group_seed(id),
            link.as_ref(),
        );
        if let Some(found) = self.baselines.get(id, &key) {
            return Ok(found);
        }
        let graph = self.graph(node)?;
        let reference = DesignReference::compute(&graph)?;
        let set = self.fault_set(id, &[]);
        let fresh = observe_linked(
            &graph,
            &reference,
            &set,
            normal,
            self.group_seed(id),
            link.as_ref(),
        )?;
        self.baselines.put(id, key, fresh.clone());
        Ok(fresh)
    }

    /// Who's affected: past a threshold now, and not already there in the node's normal.
    pub fn who(
        &self,
        id: &ServiceGroupId,
        features: &StepFeatures,
    ) -> Result<diagnosis::who::Who, TwinError> {
        let thresholds = diagnosis::who::WhoThresholds::default();
        let normal = diagnosis::who_is_affected(&self.baseline(id)?, &thresholds);
        let mut who = diagnosis::who_is_affected(features, &thresholds);
        who.affected.retain(|m| !normal.affected.contains(m));
        Ok(who)
    }

    fn cache_key(&self, id: &ServiceGroupId) -> Result<[u8; 32], TwinError> {
        let node = self.node(id)?;
        let profile = self
            .profiles
            .get(&node.plant.profile_ref)
            .ok_or_else(|| TwinError::NoProfile(node.plant.profile_ref.to_string()))?;
        // §16: its link state, so a change on a cable recomputes the nodes whose links cross it and no others.
        let link = self.link(id)?;
        Ok(crate::cache::key(
            &node.plant,
            profile,
            &self.library,
            &self.sample,
            &node.faults,
            self.group_seed(id),
            link.as_ref(),
        ))
    }

    /// What the node's telemetry says now. Recomputed only if something it derives from has changed.
    pub fn features(&self, id: &ServiceGroupId) -> Result<StepFeatures, TwinError> {
        let key = self.cache_key(id)?;
        if let Some(found) = self.cache.get(id, &key) {
            return Ok(found);
        }
        let node = self.node(id)?;
        let fresh = self.features_under(id, &node.faults)?;
        self.cache.put(id, key, fresh.clone());
        Ok(fresh)
    }

    /// As `features`, for many nodes at once, using every core on what needs computing.
    pub fn features_many(&self, ids: &[ServiceGroupId]) -> Result<Vec<StepFeatures>, TwinError> {
        let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
        let next = std::sync::atomic::AtomicUsize::new(0);
        let mut out: Vec<(usize, StepFeatures)> = Vec::with_capacity(ids.len());
        std::thread::scope(|scope| -> Result<(), TwinError> {
            let handles: Vec<_> = (0..workers.min(ids.len()).max(1))
                .map(|_| {
                    scope.spawn(|| -> Result<Vec<(usize, StepFeatures)>, TwinError> {
                        let mut local = Vec::new();
                        loop {
                            let at = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(id) = ids.get(at) else {
                                return Ok(local);
                            };
                            self.baseline(id)?;
                            local.push((at, self.features(id)?));
                        }
                    })
                })
                .collect();
            for handle in handles {
                let part = handle
                    .join()
                    .map_err(|_| TwinError::Refused("a worker panicked".into()))??;
                out.extend(part);
            }
            Ok(())
        })?;
        out.sort_by_key(|r| r.0);
        Ok(out.into_iter().map(|r| r.1).collect())
    }

    /// The model's proposals and its verified answer, if a model's loaded (`docs/tasks/13-reactivity-and-lifecycle-demo.md`).
    pub fn diagnose_with_model(
        &self,
        id: &ServiceGroupId,
    ) -> Result<Option<(Vec<proposal::Proposal>, Diagnosis)>, TwinError> {
        if self.model_withheld(id)?.is_some() {
            return Ok(None);
        }
        let node = self.node(id)?;
        let as_built = !node.plant.version_2_parts(&self.library).is_empty();
        let graph = self.graph(node)?;
        let reference = DesignReference::compute(&graph)?;
        let observed = self.features(id)?;
        let mut at_reading = self.sample.clone();
        at_reading.t = Timestamp(self.sample.t.0 + 300);
        let rows = proposal::component_rows(&graph, &observed);
        // The shipped model for a plant without version 2 parts, and task 24's for one with them.
        let (proposals, calibration) = if as_built {
            let Some(model) = &self.as_built_model else {
                return Ok(None);
            };
            (
                model
                    .propose(&graph, &node.plant, &observed, &rows)
                    .map_err(|e| TwinError::Model(e.to_string()))?,
                model.calibration(),
            )
        } else {
            let Some(model) = &self.model else {
                return Ok(None);
            };
            (model.propose(&rows), model.calibration.as_ref())
        };
        let top: Vec<(String, f64)> = proposals
            .iter()
            .take(5)
            .map(|p| (p.component.clone(), p.probability))
            .collect();
        let answer = diagnosis::engine::diagnose_with_prior(
            &graph,
            &reference,
            &observed,
            &at_reading,
            &self.diagnoser_config(id)?,
            &top,
        )?;
        let answer = calibrated(calibration, &proposals, answer, observed.modems.len());
        Ok(Some((proposals, answer)))
    }

    /// The whole inference on what the node shows now.
    pub fn diagnose(&self, id: &ServiceGroupId) -> Result<Diagnosis, TwinError> {
        let node = self.node(id)?;
        let graph = self.graph(node)?;
        let reference = DesignReference::compute(&graph)?;
        let observed = self.features(id)?;
        // `observe` reads a step after `sample`, so that's when hypotheses have to be tried: a fault with a timeline isn't
        // the same fault 5 minutes earlier.
        let mut at_reading = self.sample.clone();
        at_reading.t = Timestamp(self.sample.t.0 + 300);
        Ok(diagnose(
            &graph,
            &reference,
            &observed,
            &at_reading,
            &self.diagnoser_config(id)?,
        )?)
    }

    /// The diagnoser's settings for a node, with its fiber link as designed where a region's fiber serves it, so every
    /// hypothesis goes through the optics the node really has (task 24).
    pub fn diagnoser_config(&self, id: &ServiceGroupId) -> Result<DiagnoserConfig, TwinError> {
        Ok(DiagnoserConfig {
            link: self.link_under(id, false)?,
            ..parallel_config()
        })
    }
}
