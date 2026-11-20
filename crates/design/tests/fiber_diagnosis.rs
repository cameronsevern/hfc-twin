//! `diagnosis::fiber` on a designed region (task 24): a cut on a cable two nodes share is named, a loss on one node's own
//! cable is named, and readings no one fault on the fiber explains come back as not fiber. The region is the designer's
//! own, version 2, on a generated urban grid with shared sheaths: every plant here is synthetic.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use design::v2::design;
use diagnosis::fiber::{FiberVerdict, LinkReading, diagnose_fiber};
use schema::common::{FiberCableId, ServiceGroupId, Timestamp};
use schema::component::ComponentLibrary;
use schema::design_rules::DesignRules;
use schema::fault::Onset;
use schema::fiber::Hop;
use schema::fiber_fault::{FiberFault, FiberFaultKind, FiberLocation};
use schema::plant::Plant;
use schema::profile::{ChannelPlan, Profile};
use schema::region::{DesignControls, LayoutFamily};
use sim::optics::region_links;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

struct Region {
    library: ComponentLibrary,
    profile: Profile,
    design: design::v2::DesignV2,
}

fn region(profile: &str) -> Region {
    let library: ComponentLibrary = read("library/generic.json");
    let rules: DesignRules = read("design_rules/generic.json");
    let profile: Profile =
        schema::profile::migrate(read(&format!("profiles/{profile}.json"))).unwrap();
    let map = design::generate(LayoutFamily::UrbanGrid, 6, 800);
    let design = design(&map, &profile, &library, &rules, &DesignControls::default()).unwrap();
    assert!(design.nodes.len() >= 4, "a region with cables to share");
    Region {
        library,
        profile,
        design,
    }
}

impl Region {
    fn pairs(&self) -> Vec<(&Plant, &ChannelPlan)> {
        self.design
            .nodes
            .iter()
            .map(|n| (&n.plant, &self.profile.channel_plan))
            .collect()
    }

    /// Every node's link as the sim reads it under these faults.
    fn readings(&self, faults: &[FiberFault]) -> BTreeMap<ServiceGroupId, LinkReading> {
        region_links(
            &self.design.fiber,
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

    /// The outside cables, by the nodes whose links pass them.
    fn cables(&self) -> BTreeMap<String, BTreeSet<ServiceGroupId>> {
        let mut out: BTreeMap<String, BTreeSet<ServiceGroupId>> = BTreeMap::new();
        for link in &self.design.fiber.links {
            for hop in &link.hops {
                if let Hop::Fiber { cable, .. } = hop {
                    out.entry(cable.to_string())
                        .or_default()
                        .insert(link.service_group.clone());
                }
            }
        }
        out
    }
}

const PROFILES: [&str; 2] = ["d31_lowsplit_1002_iccap", "d31_highsplit_1218_rpd"];

#[test]
fn a_cut_on_a_shared_cable_is_named() {
    for id in PROFILES {
        let r = region(id);
        let (cable, behind) = r
            .cables()
            .into_iter()
            .filter(|(c, g)| {
                c.starts_with("fiber-") && g.len() >= 2 && g.len() < r.design.nodes.len()
            })
            .max_by_key(|(_, g)| g.len())
            .expect("a sheath two nodes share");
        let cut = FiberFault {
            onset: Onset::Instant,
            kind: FiberFaultKind::FiberCut {
                cable: FiberCableId::new(&cable),
                pos_ft: 10.0,
                repair_s: None,
            },
        };
        let answer =
            diagnose_fiber(&r.design.fiber, &r.pairs(), &r.library, &r.readings(&[cut])).unwrap();
        match answer.verdict {
            FiberVerdict::Fiber {
                place, affected, ..
            } => {
                assert_eq!(place, cable, "{id}");
                assert_eq!(
                    affected.into_iter().collect::<BTreeSet<_>>(),
                    behind,
                    "{id}"
                );
            }
            other => panic!("{id}: {other:?}"),
        }
    }
}

#[test]
fn a_loss_on_one_nodes_own_cable_is_named() {
    for id in PROFILES {
        let r = region(id);
        let (cable, _) = r
            .cables()
            .into_iter()
            .find(|(c, g)| c.starts_with("fiber-") && g.len() == 1)
            .expect("a sheath one node has to itself");
        let len = r
            .design
            .fiber
            .cables
            .iter()
            .find(|c| c.id.as_str() == cable)
            .unwrap()
            .len_ft;
        let loss = FiberFault {
            onset: Onset::Instant,
            kind: FiberFaultKind::EventLoss {
                location: FiberLocation::Cable {
                    cable: FiberCableId::new(&cable),
                    pos_ft: len * 0.3,
                },
                loss_db: 4.0,
            },
        };
        let answer = diagnose_fiber(
            &r.design.fiber,
            &r.pairs(),
            &r.library,
            &r.readings(&[loss]),
        )
        .unwrap();
        match &answer.verdict {
            FiberVerdict::Fiber {
                place, affected, ..
            } => {
                assert_eq!(affected.len(), 1, "{id}");
                // A loss anywhere on that node's own path is the same reading; the cable is first or tied with it.
                let best = answer.ranked[0].2;
                assert!(
                    answer
                        .ranked
                        .iter()
                        .any(|(p, _, c)| p == &cable && (c - best).abs() < 1e-6),
                    "{id}: {place} against {cable}: {:?}",
                    &answer.ranked[..answer.ranked.len().min(4)]
                );
            }
            other => panic!("{id}: {other:?}"),
        }
    }
}

#[test]
fn readings_no_fiber_fault_explains_are_not_fiber() {
    for id in PROFILES {
        let r = region(id);
        let mut seen = r.readings(&[]);
        // One node's light up 3 dB, another's down 3 dB: no loss, cut or drift does both.
        let ids: Vec<ServiceGroupId> = seen.keys().cloned().collect();
        for (sg, delta) in [(&ids[0], 3.0), (&ids[ids.len() - 1], -3.0)] {
            let reading = seen.get_mut(sg).unwrap();
            reading.forward_dbm = reading.forward_dbm.map(|x| x + delta);
        }
        let answer = diagnose_fiber(&r.design.fiber, &r.pairs(), &r.library, &seen).unwrap();
        assert!(
            matches!(answer.verdict, FiberVerdict::NotFiber { .. }),
            "{id}: {:?}",
            answer.verdict
        );
        // And with nothing wrong, nothing's said.
        let answer =
            diagnose_fiber(&r.design.fiber, &r.pairs(), &r.library, &r.readings(&[])).unwrap();
        assert_eq!(answer.verdict, FiberVerdict::Healthy, "{id}");
    }
}
