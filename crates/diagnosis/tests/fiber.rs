//! `diagnosis::fiber` on the hand-built region (task 24): `fixtures/one_street_as_built/`, two nodes whose links share
//! `fiber-1`, with `fiber-2` the low-split node's alone. A cut on the shared cable is named, a loss on one node's own cable
//! is named, and readings no one fault on the fiber explains are not fiber. All synthetic (invariant 11).

use std::collections::BTreeMap;
use std::fs;

use diagnosis::fiber::{FiberVerdict, LinkReading, diagnose_fiber};
use schema::common::{FiberCableId, ServiceGroupId, Timestamp};
use schema::component::ComponentLibrary;
use schema::fault::Onset;
use schema::fiber::FiberPlant;
use schema::fiber_fault::{FiberFault, FiberFaultKind, FiberLocation};
use schema::plant::Plant;
use schema::profile::{ChannelPlan, Profile};
use schema::work_order::CauseClass;
use sim::optics::region_links;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

struct Region {
    fiber: FiberPlant,
    library: ComponentLibrary,
    nodes: Vec<(Plant, Profile)>,
}

fn region() -> Region {
    Region {
        fiber: read("one_street_as_built/fiber.json"),
        library: read("library/generic.json"),
        nodes: vec![
            (
                read("one_street_as_built/plant_lowsplit_iccap.json"),
                read("profiles/d31_lowsplit_1002_iccap.json"),
            ),
            (
                read("one_street_as_built/plant_highsplit_rpd.json"),
                read("profiles/d31_highsplit_1218_rpd.json"),
            ),
        ],
    }
}

const LOW: &str = "sg-as-built-lowsplit";
const HIGH: &str = "sg-as-built-highsplit";

impl Region {
    fn pairs(&self) -> Vec<(&Plant, &ChannelPlan)> {
        self.nodes
            .iter()
            .map(|(p, pr)| (p, &pr.channel_plan))
            .collect()
    }

    fn readings(&self, faults: &[FiberFault]) -> BTreeMap<ServiceGroupId, LinkReading> {
        region_links(
            &self.fiber,
            &self.pairs(),
            &self.library,
            faults,
            Timestamp(0),
        )
        .unwrap()
        .iter()
        .map(|(sg, l)| (sg.clone(), LinkReading::of(l)))
        .collect()
    }
}

fn on_cable(kind: impl Fn(FiberCableId) -> FiberFaultKind, cable: &str) -> FiberFault {
    FiberFault {
        onset: Onset::Instant,
        kind: kind(FiberCableId::new(cable)),
    }
}

#[test]
fn a_cut_on_the_shared_cable_is_named() {
    let r = region();
    let cut = on_cable(
        |cable| FiberFaultKind::FiberCut {
            cable,
            pos_ft: 10.0,
            repair_s: None,
        },
        "fiber-1",
    );
    let answer = diagnose_fiber(&r.fiber, &r.pairs(), &r.library, &r.readings(&[cut])).unwrap();
    match answer.verdict {
        FiberVerdict::Fiber {
            place,
            class,
            mut affected,
            ..
        } => {
            assert_eq!(place, "fiber-1");
            assert_eq!(class, CauseClass::FiberCut);
            affected.sort();
            assert_eq!(
                affected,
                vec![ServiceGroupId::new(HIGH), ServiceGroupId::new(LOW)]
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_loss_on_one_nodes_own_cable_is_named() {
    let r = region();
    let loss = on_cable(
        |cable| FiberFaultKind::EventLoss {
            location: FiberLocation::Cable {
                cable,
                pos_ft: 400.0,
            },
            loss_db: 4.0,
        },
        "fiber-2",
    );
    let answer = diagnose_fiber(&r.fiber, &r.pairs(), &r.library, &r.readings(&[loss])).unwrap();
    let FiberVerdict::Fiber {
        class, affected, ..
    } = &answer.verdict
    else {
        panic!("{:?}", answer.verdict);
    };
    assert_eq!(*class, CauseClass::FiberEventLoss);
    assert_eq!(affected, &vec![ServiceGroupId::new(LOW)]);
    // A loss anywhere on the low-split node's own path reads the same from the node; the cable is first or tied first, and
    // the shared cable, which would have moved the RPD too, is not.
    let best = answer.ranked[0].2;
    let fit = |place: &str| {
        answer
            .ranked
            .iter()
            .find(|(p, c, _)| p == place && *c == CauseClass::FiberEventLoss)
            .map(|r| r.2)
    };
    assert!(
        fit("fiber-2").is_some_and(|c| (c - best).abs() < 1e-6),
        "{:?}",
        answer.ranked
    );
    assert!(
        fit("fiber-1").is_none_or(|c| c > best + 1.0),
        "{:?}",
        answer.ranked
    );
}

#[test]
fn readings_no_fiber_fault_explains_are_not_fiber() {
    let r = region();
    // The low-split node's light up 6 dB and the RPD's down 6 dB: no loss raises light, and the two have different
    // transmitters, so no one drift does both.
    let mut seen = r.readings(&[]);
    for (sg, delta) in [(LOW, 6.0), (HIGH, -6.0)] {
        let reading = seen.get_mut(&ServiceGroupId::new(sg)).unwrap();
        reading.forward_dbm = reading.forward_dbm.map(|x| x + delta);
    }
    let answer = diagnose_fiber(&r.fiber, &r.pairs(), &r.library, &seen).unwrap();
    assert!(
        matches!(answer.verdict, FiberVerdict::NotFiber { .. }),
        "{:?}",
        answer.verdict
    );
    let answer = diagnose_fiber(&r.fiber, &r.pairs(), &r.library, &r.readings(&[])).unwrap();
    assert_eq!(answer.verdict, FiberVerdict::Healthy);
}
