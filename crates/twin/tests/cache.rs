//! Incremental recompute: one node changed, one node recomputed. All synthetic.

use std::path::Path;

use schema::common::{ComponentId, ElementId, ServiceGroupId};
use schema::component::ComponentSpec;
use schema::fault::{Fault, FaultKind, Onset};
use twin::{Twin, load_one_street};

const LOW: &str = "sg-one-street-lowsplit";
const HIGH: &str = "sg-one-street-highsplit";

fn twin() -> Twin {
    load_one_street(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"),
        1,
    )
    .unwrap()
}

fn id(name: &str) -> ServiceGroupId {
    ServiceGroupId::new(name)
}

fn count(t: &Twin, name: &str) -> u64 {
    t.cache
        .stats()
        .recomputed
        .get(&id(name))
        .copied()
        .unwrap_or(0)
}

fn bad_drop() -> Fault {
    Fault {
        onset: Onset::Instant,
        kind: FaultKind::BadDrop {
            modem_id: ElementId::new("modem-4"),
            extra_loss_db: 9.0,
            return_loss_db: None,
        },
    }
}

#[test]
fn only_what_changed_recomputes() {
    let mut t = twin();
    let ids = [id(LOW), id(HIGH)];
    t.features_many(&ids).unwrap();
    assert_eq!((count(&t, LOW), count(&t, HIGH)), (1, 1));

    // Nothing changed.
    t.features_many(&ids).unwrap();
    assert_eq!((count(&t, LOW), count(&t, HIGH)), (1, 1));
    assert!(t.cache.stats().hits >= 2);

    // A fault on one node.
    t.node_mut(&id(HIGH)).unwrap().faults.push(bad_drop());
    t.features_many(&ids).unwrap();
    assert_eq!((count(&t, LOW), count(&t, HIGH)), (1, 2));

    // The environment is everyone's.
    t.sample.temp_c = 31.0;
    t.features_many(&ids).unwrap();
    assert_eq!((count(&t, LOW), count(&t, HIGH)), (2, 3));
}

#[test]
fn list_order_recomputes_nothing() {
    let mut t = twin();
    let ids = [id(HIGH)];
    let before = t.features_many(&ids).unwrap();
    let node = t.node_mut(&id(HIGH)).unwrap();
    node.plant.elements.reverse();
    node.plant.edges.reverse();
    let after = t.features_many(&ids).unwrap();
    assert_eq!(count(&t, HIGH), 1);
    assert_eq!(before, after);
}

#[test]
fn a_part_recomputes_only_nodes_that_use_it() {
    let mut t = twin();
    let ids = [id(LOW), id(HIGH)];
    t.features_many(&ids).unwrap();
    // Only the high-split node has an RPD, and only the low-split node has an analog node.
    for part in &mut t.library.components {
        if part.id == ComponentId::new("rpd-generic-1218")
            && let ComponentSpec::Rpd(rpd) = &mut part.spec
        {
            rpd.active.return_loss_db = 15.0;
        }
    }
    t.features_many(&ids).unwrap();
    assert_eq!((count(&t, LOW), count(&t, HIGH)), (1, 2));
    // And a part neither uses changes nothing.
    for part in &mut t.library.components {
        if part.id == ComponentId::new("dc-generic-8db")
            && let ComponentSpec::DirectionalCoupler(dc) = &mut part.spec
        {
            dc.excess_loss_db = 0.5;
        }
    }
    t.features_many(&ids).unwrap();
    assert_eq!((count(&t, LOW), count(&t, HIGH)), (1, 2));
}

#[test]
fn a_cached_answer_is_a_fresh_answer() {
    let mut t = twin();
    t.node_mut(&id(HIGH)).unwrap().faults.push(bad_drop());
    let first = serde_json::to_vec(&t.features(&id(HIGH)).unwrap()).unwrap();
    let cached = serde_json::to_vec(&t.features(&id(HIGH)).unwrap()).unwrap();
    t.cache.clear();
    let fresh = serde_json::to_vec(&t.features(&id(HIGH)).unwrap()).unwrap();
    assert_eq!(first, cached);
    assert_eq!(first, fresh);
}

#[test]
fn seeds_stay_with_their_node() {
    // A change on one node keeps the other's telemetry, draw for draw.
    let mut t = twin();
    let before = serde_json::to_vec(&t.features(&id(LOW)).unwrap()).unwrap();
    t.node_mut(&id(HIGH)).unwrap().faults.push(bad_drop());
    t.cache.clear();
    t.features(&id(HIGH)).unwrap();
    let after = serde_json::to_vec(&t.features(&id(LOW)).unwrap()).unwrap();
    assert_eq!(before, after);
}

/// "Affected" is off the node's own normal, so a healthy node reads nothing and a repair can read clean.
#[test]
fn a_healthy_node_reads_no_one_affected() {
    let twin = twin();
    for id in twin.nodes.keys() {
        let features = twin.features(id).unwrap();
        assert_eq!(
            twin.who(id, &features).unwrap().affected,
            Vec::<schema::common::ElementId>::new()
        );
    }
}
