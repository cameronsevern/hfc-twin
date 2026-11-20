//! Incremental recompute (`docs/DESIGN.md`, "Reactivity"): derived values per service group, keyed by a hash of everything they derive
//! from. Holds nothing that isn't a pure function of its key, so dropping it can't change an answer (invariant 1).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use features::StepFeatures;
use schema::common::{ComponentId, ServiceGroupId};
use schema::component::ComponentLibrary;
use schema::environment::EnvironmentSample;
use schema::fault::Fault;
use schema::plant::{EdgeKind, Plant};
use schema::profile::Profile;
use sha2::{Digest, Sha256};

/// How the cache has been doing.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct CacheStats {
    /// Answers served without simulating.
    pub hits: u64,
    /// Answers that needed a simulation, per service group.
    pub recomputed: BTreeMap<ServiceGroupId, u64>,
}

#[derive(Debug, Default)]
struct Inner {
    entries: BTreeMap<ServiceGroupId, ([u8; 32], StepFeatures)>,
    stats: CacheStats,
}

/// Shared, since read-only tools fill it too.
#[derive(Debug, Clone, Default)]
pub struct Cache {
    inner: Arc<Mutex<Inner>>,
}

/// The plant in canonical order: two files that differ only in list order are the same plant, so they share a key.
fn canonical_plant(plant: &Plant) -> Plant {
    let mut out = plant.clone();
    out.elements.sort_by(|a, b| a.id.cmp(&b.id));
    for element in &mut out.elements {
        element.ports.sort_by(|a, b| a.id.cmp(&b.id));
    }
    out.edges.sort_by(|a, b| a.id.cmp(&b.id));
    out.power_supplies.sort_by(|a, b| a.id.cmp(&b.id));
    out.modem_bindings
        .sort_by(|a, b| a.element_id.cmp(&b.element_id));
    out
}

/// The parts a plant actually uses, so a change to another part doesn't touch it.
fn parts_used(plant: &Plant) -> BTreeSet<&ComponentId> {
    let mut used: BTreeSet<&ComponentId> =
        plant.elements.iter().map(|e| &e.component_ref).collect();
    used.extend(plant.power_supplies.iter().map(|p| &p.component_ref));
    for edge in &plant.edges {
        if let EdgeKind::Span { cable_ref, .. } | EdgeKind::Drop { cable_ref, .. } = &edge.kind {
            used.insert(cable_ref);
        }
    }
    used
}

/// The key: a service group's plant, profile, parts, environment, faults, seed and link state (§16).
pub fn key(
    plant: &Plant,
    profile: &Profile,
    library: &ComponentLibrary,
    sample: &EnvironmentSample,
    faults: &[Fault],
    seed: u64,
    link: Option<&sim::optics::NodeLink>,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    let mut feed = |bytes: Vec<u8>| {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
    };
    feed(serde_json::to_vec(&canonical_plant(plant)).unwrap_or_default());
    feed(serde_json::to_vec(profile).unwrap_or_default());
    for part in parts_used(plant) {
        if let Some(component) = library.get(part) {
            feed(serde_json::to_vec(component).unwrap_or_default());
        }
    }
    feed(serde_json::to_vec(sample).unwrap_or_default());
    feed(serde_json::to_vec(faults).unwrap_or_default());
    feed(seed.to_le_bytes().to_vec());
    feed(serde_json::to_vec(&link).unwrap_or_default());
    hash.finalize().into()
}

impl Cache {
    /// What's cached under this key, if the key still matches.
    pub fn get(&self, id: &ServiceGroupId, wanted: &[u8; 32]) -> Option<StepFeatures> {
        let mut inner = self.inner.lock().ok()?;
        let found = inner
            .entries
            .get(id)
            .filter(|(k, _)| k == wanted)
            .map(|(_, f)| f.clone());
        if found.is_some() {
            inner.stats.hits += 1;
        }
        found
    }

    /// Stores a fresh answer.
    pub fn put(&self, id: &ServiceGroupId, wanted: [u8; 32], features: StepFeatures) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.entries.insert(id.clone(), (wanted, features));
            *inner.stats.recomputed.entry(id.clone()).or_insert(0) += 1;
        }
    }

    /// How it's been doing.
    pub fn stats(&self) -> CacheStats {
        self.inner
            .lock()
            .map(|i| i.stats.clone())
            .unwrap_or_default()
    }

    /// Forgets everything, which can only make the next answer slower.
    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.entries.clear();
        }
    }
}
