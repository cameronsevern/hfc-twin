//! `diagnosis::outage` (task 19): worked trees on `one_street`, and events run through the sim on the hand-built as-built
//! region, whose analog node has its fiber. All synthetic (invariant 11).
//!
//! `one_street`'s tree: node-1, amp-1, tap-1 (modems 1, 2), tap-2 (3, 4), then span-4 to amp-2, tap-3 (5, 6), tap-4 (7, 8).

use std::fs;

use diagnosis::fiber::LinkReading;
use diagnosis::outage::{LostWay, OutageVerdict, hops, hops_to_span, read_outage};
use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId, FiberElementId, Timestamp};
use schema::component::ComponentLibrary;
use schema::component::OpticalDirection;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{CutEnd, Fault, FaultKind, FaultSet, NodeFailureMode, Onset};
use schema::fiber::{FiberPlant, Hop};
use schema::fiber_fault::{FiberFault, FiberFaultKind, FiberLocation, StrandRef};
use schema::observation::{ModemObservation, RegState};
use schema::plant::Plant;
use schema::profile::{ChannelPlan, PlantReports, Profile};
use sim::optics::region_links;
use sim::telemetry::{Detail, Scenario, SimOptions, StepObservation, simulate};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn at(t: i64) -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(t),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

fn run(
    graph: &PlantGraph<'_>,
    kinds: Vec<FaultKind>,
    link: Option<&sim::optics::NodeLink>,
    times: &[i64],
) -> Vec<StepObservation> {
    let environment = Environment {
        schema_version: "1".into(),
        samples: times.iter().map(|t| at(*t)).collect(),
        ambient_sources: Vec::new(),
    };
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.into(),
        seed: 5,
        faults: kinds
            .into_iter()
            .map(|kind| Fault {
                onset: Onset::Instant,
                kind,
            })
            .collect(),
    };
    simulate(&Scenario {
        graph,
        environment: &environment,
        faults: &faults,
        seed: 5,
        options: SimOptions {
            detail: Detail::Summary { stride: 8 },
            ..SimOptions::default()
        },
        link,
    })
    .unwrap()
}

struct Street {
    plant: Plant,
    library: ComponentLibrary,
    profile: Profile,
}

fn street() -> Street {
    Street {
        plant: read("one_street/plant_lowsplit_iccap.json"),
        library: read("library/generic.json"),
        profile: read("profiles/d31_lowsplit_1002_iccap.json"),
    }
}

/// One healthy step's modems, with the ones named set to a state.
fn with_states(graph: &PlantGraph<'_>, states: &[(u32, RegState)]) -> Vec<ModemObservation> {
    let mut modems = run(graph, Vec::new(), None, &[0]).remove(0).modems;
    for m in &mut modems {
        assert_eq!(m.reg_state, RegState::Operational);
        for (n, state) in states {
            if m.element_id.as_str() == format!("modem-{n}") {
                m.reg_state = *state;
            }
        }
    }
    modems
}

fn dark(ns: &[u32]) -> Vec<(u32, RegState)> {
    ns.iter().map(|n| (*n, RegState::NotSynchronized)).collect()
}

fn id(s: &str) -> ElementId {
    ElementId::new(s)
}

#[test]
fn all_up_is_normal() {
    let s = street();
    let graph = PlantGraph::new(&s.plant, &s.library, &s.profile).unwrap();
    let r = read_outage(&graph, &with_states(&graph, &[]), None, None);
    assert_eq!(r.verdict, OutageVerdict::Normal);
    assert_eq!((r.modems, r.vanished), (8, 0));
}

#[test]
fn the_back_half_dark_names_the_longer_span_that_explains_it() {
    let s = street();
    let graph = PlantGraph::new(&s.plant, &s.library, &s.profile).unwrap();
    let r = read_outage(
        &graph,
        &with_states(&graph, &dark(&[5, 6, 7, 8])),
        None,
        None,
    );
    // amp-2 and tap-3 hold the same four modems, so the break is on span-4 or span-5, and cable is cut by the foot: span-5
    // is the longer, so it's named, and amp-2 is within one element of either.
    assert_eq!(
        r.verdict,
        OutageVerdict::PlantBreak {
            element: id("tap-3"),
            span: Some(EdgeId::new("span-5")),
            lost: LostWay::Downstream,
        }
    );
    assert_eq!(
        hops_to_span(&graph, &id("tap-3"), &EdgeId::new("span-4")),
        Some(3)
    );
    assert!(r.contradicting.is_empty());
}

#[test]
fn modems_that_hear_but_cant_range_read_as_the_return_lost() {
    let s = street();
    let graph = PlantGraph::new(&s.plant, &s.library, &s.profile).unwrap();
    let states: Vec<_> = [5, 6, 7, 8]
        .iter()
        .map(|n| (*n, RegState::RangingInProgress))
        .collect();
    let r = read_outage(&graph, &with_states(&graph, &states), None, None);
    assert!(matches!(
        r.verdict,
        OutageVerdict::PlantBreak {
            lost: LostWay::ReturnOnly,
            ..
        }
    ));
}

#[test]
fn everything_dark_names_the_node() {
    let s = street();
    let graph = PlantGraph::new(&s.plant, &s.library, &s.profile).unwrap();
    let r = read_outage(
        &graph,
        &with_states(&graph, &dark(&[1, 2, 3, 4, 5, 6, 7, 8])),
        None,
        None,
    );
    assert_eq!(
        r.verdict,
        OutageVerdict::PlantBreak {
            element: id("node-1"),
            span: None,
            lost: LostWay::Downstream,
        }
    );
}

#[test]
fn a_modem_up_below_rules_a_point_out() {
    let s = street();
    let graph = PlantGraph::new(&s.plant, &s.library, &s.profile).unwrap();
    // Three of the four past amp-2 gone and modem-8 up: nothing below a break can be up, so no one point is it.
    let r = read_outage(&graph, &with_states(&graph, &dark(&[5, 6, 7])), None, None);
    assert!(
        matches!(r.verdict, OutageVerdict::NoOnePoint { .. }),
        "{:?}",
        r.verdict
    );
}

#[test]
fn a_modem_gone_outside_the_point_is_named_against_it() {
    let s = street();
    let graph = PlantGraph::new(&s.plant, &s.library, &s.profile).unwrap();
    // The back half gone and modem-3, on the tap above: within one modem's tolerance, and listed.
    let r = read_outage(
        &graph,
        &with_states(&graph, &dark(&[3, 5, 6, 7, 8])),
        None,
        None,
    );
    let OutageVerdict::PlantBreak { element, .. } = &r.verdict else {
        panic!("{:?}", r.verdict);
    };
    assert_eq!(element, &id("tap-3"));
    assert_eq!(r.contradicting, vec![id("modem-3")]);
}

#[test]
fn scattered_losses_fit_no_one_point() {
    let s = street();
    let graph = PlantGraph::new(&s.plant, &s.library, &s.profile).unwrap();
    let r = read_outage(
        &graph,
        &with_states(&graph, &dark(&[1, 4, 6, 8])),
        None,
        None,
    );
    assert_eq!(r.verdict, OutageVerdict::NoOnePoint { groups: 4 });
    assert!(!r.is_plant_break());
}

#[test]
fn one_modem_gone_is_its_home() {
    let s = street();
    let graph = PlantGraph::new(&s.plant, &s.library, &s.profile).unwrap();
    let r = read_outage(&graph, &with_states(&graph, &dark(&[3])), None, None);
    assert_eq!(
        r.verdict,
        OutageVerdict::Premises {
            element: id("modem-3")
        }
    );
}

#[test]
fn hops_count_elements_and_spans_alike() {
    let s = street();
    let graph = PlantGraph::new(&s.plant, &s.library, &s.profile).unwrap();
    assert_eq!(hops(&graph, &id("amp-2"), &id("amp-2")), Some(0));
    assert_eq!(hops(&graph, &id("tap-3"), &id("amp-2")), Some(2));
    assert_eq!(hops(&graph, &id("tap-3"), &id("tap-2")), Some(4));
    assert_eq!(
        hops_to_span(&graph, &id("amp-2"), &EdgeId::new("span-4")),
        Some(1)
    );
    assert_eq!(
        hops_to_span(&graph, &id("tap-2"), &EdgeId::new("span-4")),
        Some(1)
    );
    assert_eq!(
        hops_to_span(&graph, &id("tap-3"), &EdgeId::new("span-4")),
        Some(3)
    );
}

#[test]
fn a_cut_through_the_sim_is_named_within_one_element() {
    let s = street();
    let graph = PlantGraph::new(&s.plant, &s.library, &s.profile).unwrap();
    for end in [CutEnd::Open, CutEnd::Shorted] {
        let steps = run(
            &graph,
            vec![FaultKind::SpanCut {
                span_id: EdgeId::new("span-5"),
                pos_ft: 50.0,
                end,
                repair_s: None,
            }],
            None,
            &[0, 300],
        );
        let r = read_outage(&graph, &steps[1].modems, None, None);
        let OutageVerdict::PlantBreak { element, .. } = &r.verdict else {
            panic!("{end:?}: {:?}", r.verdict);
        };
        match end {
            CutEnd::Open => assert!(
                hops_to_span(&graph, element, &EdgeId::new("span-5")).unwrap() <= 1,
                "{element}"
            ),
            // §20.4: the shorted end trips the street's one supply, so everything it feeds goes and the top of that is named.
            CutEnd::Shorted => {
                assert_eq!(element, &id("node-1"));
                assert_eq!(r.vanished, 8);
            }
        }
    }
}

#[test]
fn a_node_failure_through_the_sim_names_the_node() {
    let s = street();
    let graph = PlantGraph::new(&s.plant, &s.library, &s.profile).unwrap();
    for mode in [NodeFailureMode::Power, NodeFailureMode::RfSection] {
        let steps = run(
            &graph,
            vec![FaultKind::AnalogNodeFailure {
                node_id: id("node-1"),
                mode,
                repair_s: None,
            }],
            None,
            &[0, 300],
        );
        let r = read_outage(&graph, &steps[1].modems, None, None);
        assert!(
            matches!(&r.verdict, OutageVerdict::PlantBreak { element, .. } if element == &id("node-1")),
            "{mode:?}: {:?}",
            r.verdict
        );
    }
}

/// The as-built region's analog node, with its fiber, on a profile that reports the node's optics.
struct Region {
    fiber: FiberPlant,
    library: ComponentLibrary,
    plant: Plant,
    profile: Profile,
}

fn region(optics: bool) -> Region {
    let mut profile: Profile = read("profiles/d31_lowsplit_1002_iccap.json");
    if optics {
        profile.plant_reports = PlantReports {
            node_optical_rx: true,
            hub_optical_rx: true,
            rpd_optic_status: true,
            out_level_per_port: false,
        };
    }
    Region {
        fiber: read("one_street_as_built/fiber.json"),
        library: read("library/generic.json"),
        plant: read("one_street_as_built/plant_lowsplit_iccap.json"),
        profile,
    }
}

/// A loss past any link's margin at the splice one direction's strand passes through, and nothing on the other.
fn one_way(fiber: &FiberPlant, direction: OpticalDirection) -> FiberFault {
    let link = fiber
        .links
        .iter()
        .find(|l| l.service_group.as_str() == "sg-as-built-lowsplit" && l.direction == direction)
        .unwrap();
    let at = link
        .hops
        .iter()
        .position(|h| matches!(h, Hop::Splice { .. }))
        .unwrap();
    let strand = |hop: &Hop| match hop {
        Hop::Fiber { cable, strand } => StrandRef {
            cable: cable.clone(),
            strand: *strand,
        },
        other => panic!("{other:?}"),
    };
    let Hop::Splice { at: closure, .. } = &link.hops[at] else {
        unreachable!()
    };
    FiberFault {
        onset: Onset::Instant,
        kind: FiberFaultKind::EventLoss {
            location: FiberLocation::Splice {
                element: FiberElementId::new(closure.as_str()),
                strands: [strand(&link.hops[at - 1]), strand(&link.hops[at + 1])],
            },
            loss_db: 45.0,
        },
    }
}

/// Steps from before to the read at 600 s: a return lost takes 17 missed ranging opportunities, 15 s apart, to reinit (§7.4).
const STEPS: [i64; 7] = [-60, 0, 60, 120, 300, 450, 600];

fn read_region(r: &Region, faults: &[FiberFault]) -> (OutageVerdict, Option<RegState>) {
    let graph = PlantGraph::new(&r.plant, &r.library, &r.profile).unwrap();
    let pairs: Vec<(&Plant, &ChannelPlan)> = vec![(&r.plant, &r.profile.channel_plan)];
    let sg = &r.plant.service_group;
    let healthy = region_links(&r.fiber, &pairs, &r.library, &[], Timestamp(0))
        .unwrap()
        .remove(sg)
        .unwrap();
    let now = region_links(&r.fiber, &pairs, &r.library, faults, Timestamp(0))
        .unwrap()
        .remove(sg)
        .unwrap();
    let steps = run(&graph, Vec::new(), Some(&now), &STEPS);
    let step = steps.last().unwrap();
    let reading = read_outage(
        &graph,
        &step.modems,
        step.service_group.optics.as_ref(),
        Some(&LinkReading::of(&healthy)),
    );
    (reading.verdict, step.modems.first().map(|m| m.reg_state))
}

#[test]
fn one_direction_of_the_fiber_lost_is_told_apart() {
    for optics in [true, false] {
        let r = region(optics);
        let (forward, fstate) = read_region(&r, &[one_way(&r.fiber, OpticalDirection::Forward)]);
        let (ret, rstate) = read_region(&r, &[one_way(&r.fiber, OpticalDirection::Return)]);
        // Forward lost: no downstream, so no RxMER either. Return lost: the modems hear, and can't range.
        assert_eq!(fstate, Some(RegState::NotSynchronized));
        assert!(matches!(
            rstate,
            Some(RegState::PhySynchronized | RegState::RangingInProgress)
        ));
        let graph = PlantGraph::new(&r.plant, &r.library, &r.profile).unwrap();
        let lost = |v: &OutageVerdict| match v {
            OutageVerdict::PlantBreak { lost, element, .. } => {
                assert_eq!(element, &graph.root().id);
                *lost
            }
            other => panic!("{other:?}"),
        };
        if optics {
            assert_eq!(lost(&forward), LostWay::ForwardOnly);
        } else {
            assert_eq!(lost(&forward), LostWay::Downstream);
        }
        assert_eq!(lost(&ret), LostWay::ReturnOnly, "optics {optics}");
    }
}
