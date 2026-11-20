//! What the sim can't price yet. Version 2 of the plant and component schemas holds parts whose physics is task 22's
//! (`docs/tasks/21-as-built-schemas.md`, "Readers"). Until then the sim refuses a plant with any of them that has an
//! RF or power effect, naming each, and never treats one as transparent. A plant with none of them simulates exactly
//! as before, since nothing here changes a number (`docs/as-built-plan.md`, rule 2).
//!
//! Structures, strand, conduit, paths, record statuses, size families, ampacity and batteries carry no RF and change
//! no voltage the sim computes today, so they're ignored. Task 22 prices the rest a section at a time, and this list
//! shrinks with it: §13 took the home's parts, house cable and buried drops, §14 fittings, port states, return loss
//! tables and an inserter's loss, §15 plug-ins, set points and gain tables, §16 a root's fitted optics, §17 source and
//! receiver tables and loading.

use std::collections::BTreeSet;

use plant_graph::PlantGraph;
use schema::plant::ElementKind;

use crate::error::PhysicsError;

/// Version 2 fields on version 1 classes that a section of `docs/physics.md` now prices.
const PRICED_FIELDS: &[&str] = &[
    // §13: two devices behind one splitter.
    "leg_isolation_db",
    // §14: return loss by frequency, and an inserter's loss and mismatch.
    "return_loss_table",
    "return_loss_by_port",
    "insertion_loss",
    "return_loss_db",
    // §15: an amp's outputs, and its gain by frequency. A terminating tap is a tap with no through port, which §2.1
    // already prices.
    "outputs",
    "gain_table",
    "terminating",
    "agc_pilots_mhz",
    // §17: source MER, a receiver's ceiling, noise figure and CIN by frequency, and CIN against the composite.
    "source_mer",
    "us_rx_ceiling_db",
    "nf_table",
    "cin_table",
    "ref_composite_dbmv",
    // §19: loop resistance with temperature.
    "r_loop_k_temp",
];

/// Whether a section prices this kind yet: §13 the home's parts. Everything a plant can have fitted is priced: a ground
/// block's coupling acts only on ambient sources (§18), plug-ins and port states are §14's and §15's, and a root's
/// optics are §16's, acting only with a link (`sim::optics`).
fn priced_kind(kind: &ElementKind) -> bool {
    kind.is_version_1()
        || matches!(
            kind,
            ElementKind::GroundBlock
                | ElementKind::DropAmp
                | ElementKind::Filter
                | ElementKind::Cpe
                | ElementKind::LineEqualizer
        )
}

/// Everything in this plant the sim has no physics for yet, as `what: where`, in canonical order. Empty for a plant
/// with no version 2 part that changes RF or power.
pub fn unsupported(graph: &PlantGraph<'_>) -> Vec<String> {
    let plant = graph.plant();
    let mut found = BTreeSet::new();
    let mut parts = BTreeSet::new();
    for element in &plant.elements {
        if !priced_kind(&element.kind) {
            found.insert(format!("{:?}: {}", element.kind, element.id));
        }
        parts.insert(&element.component_ref);
    }
    for edge in &plant.edges {
        // §14 prices fittings.
        if let Some((cable_ref, _)) = edge.kind.cable() {
            parts.insert(cable_ref);
        }
    }
    for supply in &plant.power_supplies {
        parts.insert(&supply.component_ref);
    }
    for id in parts {
        if let Some(part) = graph.component(id) {
            for field in part.spec.version_2_rf_fields() {
                if !PRICED_FIELDS.contains(&field) {
                    found.insert(format!("{field}: {id}"));
                }
            }
        }
    }
    found.into_iter().collect()
}

/// `Err(Unsupported)` naming everything [`unsupported`] finds, or `Ok` if it finds nothing.
pub fn check_supported(graph: &PlantGraph<'_>) -> Result<(), PhysicsError> {
    let found = unsupported(graph);
    if found.is_empty() {
        Ok(())
    } else {
        Err(PhysicsError::Unsupported(found))
    }
}
