//! Hand-computed answers on the one-street fixture, from `docs/tasks/02-plant-graph.md`. Each runs
//! on both plants.

mod common;

use common::*;
use plant_graph::{DesignIssue, FaultRefProblem, PlantGraph, QueryError};
use schema::common::{ComponentId, EdgeId, ElementId, PortId};
use schema::component::ComponentSpec;
use schema::fault::{AmpState, Fault, FaultKind, FaultSet, Location, Onset, Schedule};
use schema::plant::{EdgeKind, PowerSupply};
use schema::profile::Profile;

fn set_span_power(fixture: &mut Fixture, edge: &str, passing: bool) {
    let edge = fixture
        .plant
        .edges
        .iter_mut()
        .find(|e| e.id == EdgeId::new(edge))
        .unwrap();
    let EdgeKind::Span { power_passing, .. } = &mut edge.kind else {
        panic!("not a span")
    };
    *power_passing = passing;
}

fn feeds(fixture: &mut Fixture, element: &str) {
    fixture.plant.power_supplies[0].feeds = id(element);
}

fn second_supply(fixture: &mut Fixture, element: &str) {
    let first = &fixture.plant.power_supplies[0];
    let second = PowerSupply {
        id: id("psu-2"),
        component_ref: first.component_ref.clone(),
        feeds: id(element),
        geo: None,
        batteries: None,
        record_status: None,
        structure: None,
    };
    fixture.plant.power_supplies.push(second);
}

/// The segments as (edge, actives beyond), for one supply.
fn segments(graph: &PlantGraph<'_>, feed: usize) -> Vec<(String, Vec<String>)> {
    let power = graph.power();
    let lines = power.feeds[feed].segments.iter().map(|s| {
        let actives = s.actives_beyond.iter().map(ToString::to_string).collect();
        (s.edge.id.to_string(), actives)
    });
    lines.collect()
}

fn pair(edge: &str, actives: &[&str]) -> (String, Vec<String>) {
    (
        edge.to_string(),
        actives.iter().map(ToString::to_string).collect(),
    )
}

#[test]
fn path_amp_to_modem() {
    for f in fixtures() {
        let path = f
            .graph()
            .path_between(&port("amp-1", "out"), &port("modem-4", "rf"))
            .unwrap();
        assert_eq!(
            show(&path),
            [
                "span-2 down 400",
                "tap-1 in>through",
                "span-3 down 300",
                "tap-2 in>tap-2",
                "drop-4 down 120"
            ]
        );
        assert_eq!(path.cable_len_ft(), 820.0);
    }
}

#[test]
fn path_root_to_far_modem() {
    for f in fixtures() {
        let path = f
            .graph()
            .path_between(&port(f.root, "rf-1"), &port("modem-8", "rf"))
            .unwrap();
        assert_eq!(
            show(&path),
            [
                "span-1 down 1200",
                "amp-1 in>out",
                "span-2 down 400",
                "tap-1 in>through",
                "span-3 down 300",
                "tap-2 in>through",
                "span-4 down 100",
                "amp-2 in>out",
                "span-5 down 400",
                "tap-3 in>through",
                "span-6 down 300",
                "tap-4 in>tap-2",
                "drop-8 down 120"
            ]
        );
        assert_eq!(path.cable_len_ft(), 2820.0);
    }
}

#[test]
fn path_between_modems_on_one_tap() {
    for f in fixtures() {
        let path = f
            .graph()
            .path_between(&port("modem-1", "rf"), &port("modem-2", "rf"))
            .unwrap();
        assert_eq!(
            show(&path),
            ["drop-1 up 80", "tap-1 tap-1>tap-2", "drop-2 down 150"]
        );
        assert_eq!(path.cable_len_ft(), 230.0);
    }
}

#[test]
fn path_between_modems_on_two_taps() {
    for f in fixtures() {
        let path = f
            .graph()
            .path_between(&port("modem-3", "rf"), &port("modem-1", "rf"))
            .unwrap();
        assert_eq!(
            show(&path),
            [
                "drop-3 up 80",
                "tap-2 tap-1>in",
                "span-3 up 300",
                "tap-1 through>tap-1",
                "drop-1 down 80"
            ]
        );
        assert_eq!(path.cable_len_ft(), 460.0);
    }
}

#[test]
fn path_to_a_reflector_off_the_modems_path() {
    // §5.1: tap-2's input lies beyond modem-2's tap, reached through tap-1's isolation.
    for f in fixtures() {
        let path = f
            .graph()
            .path_between(&port("modem-2", "rf"), &port("tap-2", "in"))
            .unwrap();
        assert_eq!(
            show(&path),
            ["drop-2 up 150", "tap-1 tap-2>through", "span-3 down 300"]
        );
        assert_eq!(path.cable_len_ft(), 450.0);
    }
}

#[test]
fn path_from_a_point_along_a_span() {
    // §5.1's oracle cavity: 150 ft from a fault mid-span to the next device.
    for f in fixtures() {
        let g = f.graph();
        let fault = point("span-3", 150.0);
        let path = g.path_between(&fault, &port("tap-2", "in")).unwrap();
        assert_eq!(show(&path), ["span-3 down 150"]);
        assert_eq!(path.cable_len_ft(), 150.0);
        let path = g.path_between(&fault, &port("tap-1", "through")).unwrap();
        assert_eq!(show(&path), ["span-3 up 150"]);
        let path = g
            .path_between(&point("span-3", 100.0), &point("span-3", 250.0))
            .unwrap();
        assert_eq!(show(&path), ["span-3 down 150"]);
    }
}

#[test]
fn path_across_a_direct_edge() {
    for f in fixtures() {
        let path = f
            .graph()
            .path_between(&port("tap-4", "through"), &port("term-1", "in"))
            .unwrap();
        assert_eq!(show(&path), ["direct-1 down"]);
        assert_eq!(path.cable_len_ft(), 0.0);
    }
}

#[test]
fn path_across_one_device() {
    for f in fixtures() {
        let path = f
            .graph()
            .path_between(&port("tap-1", "tap-1"), &port("tap-1", "through"))
            .unwrap();
        assert_eq!(show(&path), ["tap-1 tap-1>through"]);
        assert_eq!(path.cable_len_ft(), 0.0);
    }
}

#[test]
fn path_to_an_open_port() {
    for f in fixtures() {
        let path = f
            .graph()
            .path_between(&port("modem-2", "rf"), &port("tap-1", "tap-3"))
            .unwrap();
        assert_eq!(show(&path), ["drop-2 up 150", "tap-1 tap-2>tap-3"]);
        assert_eq!(path.cable_len_ft(), 150.0);
    }
}

#[test]
fn zero_length_cable_is_left_out() {
    for f in fixtures() {
        let g = f.graph();
        let here = point("span-3", 150.0);
        assert!(g.path_between(&here, &here).unwrap().segments.is_empty());
        let at_end = point("span-3", 300.0);
        assert!(
            g.path_between(&at_end, &port("tap-2", "in"))
                .unwrap()
                .segments
                .is_empty()
        );
        let at_start = point("span-3", 0.0);
        assert!(
            g.path_between(&at_start, &port("tap-1", "through"))
                .unwrap()
                .segments
                .is_empty()
        );
        let same = port("tap-1", "in");
        assert!(g.path_between(&same, &same).unwrap().segments.is_empty());
    }
}

#[test]
fn path_from_root_reaches_the_input_port() {
    // 1,200 ft to the amp, plus §7.2's 700 ft of hardline, plus the 120 ft drop.
    for f in fixtures() {
        let g = f.graph();
        let path = g.path_from_root(&id("modem-4")).unwrap();
        assert_eq!(path.cable_len_ft(), 2020.0);
        assert_eq!(show(&path)[0], "span-1 down 1200");
        assert!(g.path_from_root(&id(f.root)).unwrap().segments.is_empty());
    }
}

#[test]
fn tree_neighbours() {
    for f in fixtures() {
        let g = f.graph();
        assert_eq!(g.root().id, id(f.root));
        assert_eq!(
            ids(&g.children(&id("tap-1")).unwrap()),
            ["modem-1", "modem-2", "tap-2"]
        );
        assert_eq!(
            ids(&g.ancestors(&id("modem-4")).unwrap()),
            ["tap-2", "tap-1", "amp-1", f.root]
        );
        assert!(g.ancestors(&id(f.root)).unwrap().is_empty());
        let (parent, edge) = g.parent(&id("modem-4")).unwrap().unwrap();
        assert_eq!((parent.id.as_str(), edge.id.as_str()), ("tap-2", "drop-4"));
        assert!(g.parent(&id(f.root)).unwrap().is_none());
        assert_eq!(ids(&g.modems()), modems(1..=8));
        assert_eq!(ids(&g.amps()), ["amp-1", "amp-2"]);
    }
}

#[test]
fn cascade_counts_amps_only() {
    for f in fixtures() {
        let g = f.graph();
        assert_eq!(ids(&g.cascade(&id("modem-4")).unwrap()), ["amp-1"]);
        assert_eq!(ids(&g.cascade(&id("modem-8")).unwrap()), ["amp-1", "amp-2"]);
        assert!(g.cascade(&id("amp-1")).unwrap().is_empty());
    }
}

#[test]
fn root_port_is_the_one_rf_port() {
    for f in fixtures() {
        let g = f.graph();
        assert_eq!(
            g.root_port(&id("modem-8")).unwrap().unwrap().id,
            PortId::new("rf-1")
        );
        assert!(g.root_port(&id(f.root)).unwrap().is_none());
    }
}

#[test]
fn modems_below_each_kind_of_location() {
    for f in fixtures() {
        let g = f.graph();
        let below = |location: Location| ids(&g.modems_below(&location).unwrap());
        assert_eq!(below(port(f.root, "rf-1")), modems(1..=8));
        assert_eq!(below(point("span-3", 150.0)), modems(3..=8));
        assert_eq!(below(port("tap-2", "in")), modems(3..=8));
        assert_eq!(below(port("tap-2", "through")), modems(5..=8));
        assert_eq!(below(port("tap-1", "tap-1")), ["modem-1"]);
        assert_eq!(below(point("drop-4", 60.0)), ["modem-4"]);
        assert_eq!(below(port("amp-2", "in")), modems(5..=8));
        assert_eq!(below(port("modem-4", "rf")), ["modem-4"]);
        assert!(below(port("tap-1", "tap-3")).is_empty());
    }
}

#[test]
fn amps_below_gives_the_funneling_count() {
    // §7.3's oracle has M = 2.
    for f in fixtures() {
        let amps = f.graph().amps_below(&port(f.root, "rf-1")).unwrap();
        assert_eq!(ids(&amps), ["amp-1", "amp-2"]);
    }
}

#[test]
fn common_ancestors() {
    for f in fixtures() {
        let g = f.graph();
        let common = |a: &str, b: &str| g.common_ancestor(&[id(a), id(b)]).unwrap().id.to_string();
        assert_eq!(common("modem-1", "modem-2"), "tap-1");
        assert_eq!(common("modem-3", "modem-4"), "tap-2");
        assert_eq!(common("modem-1", "modem-8"), "tap-1");
        assert_eq!(common("modem-5", "modem-8"), "tap-3");
        assert_eq!(common("amp-1", "modem-8"), "amp-1");
        assert_eq!(
            g.common_ancestor(&[id("modem-1")]).unwrap().id,
            id("modem-1")
        );
    }
}

#[test]
fn power_on_the_fixture() {
    for f in fixtures() {
        let g = f.graph();
        let power = g.power();
        assert_eq!(power.feeds.len(), 1);
        let actives: Vec<_> = power.feeds[0]
            .actives
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(actives, ["amp-1", "amp-2", f.root]);
        assert_eq!(
            segments(&g, 0),
            [
                pair("span-1", &["amp-1", "amp-2"]),
                pair("span-2", &["amp-2"]),
                pair("span-3", &["amp-2"]),
                pair("span-4", &["amp-2"]),
            ]
        );
        assert!(power.unpowered.is_empty() && power.contested.is_empty());
    }
}

#[test]
fn power_walks_both_ways_from_mid_cascade() {
    for mut f in fixtures() {
        feeds(&mut f, "amp-1");
        let g = f.graph();
        assert_eq!(
            segments(&g, 0),
            [
                pair("span-1", &[f.root]),
                pair("span-2", &["amp-2"]),
                pair("span-3", &["amp-2"]),
                pair("span-4", &["amp-2"]),
            ]
        );
        assert!(g.power().unpowered.is_empty());
    }
}

#[test]
fn a_non_passing_tap_cuts_power() {
    for mut f in fixtures() {
        let tap = ComponentId::new("tap-generic-23db-4p");
        let entry = f
            .library
            .components
            .iter_mut()
            .find(|c| c.id == tap)
            .unwrap();
        let ComponentSpec::Tap(spec) = &mut entry.spec else {
            panic!("not a tap")
        };
        spec.power_passing = false;
        let g = f.graph();
        assert_eq!(segments(&g, 0), [pair("span-1", &["amp-1"])]);
        assert_eq!(g.power().unpowered, [id("amp-2")]);
    }
}

#[test]
fn a_non_passing_span_cuts_power() {
    for mut f in fixtures() {
        set_span_power(&mut f, "span-1", false);
        let g = f.graph();
        assert!(segments(&g, 0).is_empty());
        assert_eq!(g.power().unpowered, [id("amp-1"), id("amp-2")]);
    }
}

#[test]
fn two_supplies_contest_until_something_separates_them() {
    for mut f in fixtures() {
        second_supply(&mut f, "amp-2");
        assert_eq!(
            f.graph().power().contested,
            [id("amp-1"), id("amp-2"), id(f.root)]
        );
        set_span_power(&mut f, "span-4", false);
        let power = f.graph().power();
        assert!(power.contested.is_empty() && power.unpowered.is_empty());
        assert_eq!(power.feeds[1].actives, [id("amp-2")]);
    }
}

#[test]
fn design_checks_pass_on_the_fixture() {
    for f in fixtures() {
        assert_eq!(f.graph().check_design(), []);
    }
}

#[test]
fn design_checks_find_a_long_cascade() {
    for mut f in fixtures() {
        f.profile.max_cascade = 1;
        let expected: Vec<_> = (5..=8)
            .map(|n| DesignIssue::CascadeExceeded {
                modem: id(&format!("modem-{n}")),
                depth: 2,
                max: 1,
            })
            .collect();
        assert_eq!(f.graph().check_design(), expected);
    }
}

#[test]
fn design_checks_find_an_unqualified_part() {
    let mut f = high();
    let cable = ComponentId::new("cable-500-p3");
    let entry = f
        .library
        .components
        .iter_mut()
        .find(|c| c.id == cable)
        .unwrap();
    entry.rated_hi_mhz = Some(1002.0);
    assert_eq!(
        f.graph().check_design(),
        [DesignIssue::PartNotQualified { component: cable }]
    );
}

#[test]
fn design_checks_find_power_problems() {
    let mut f = low();
    set_span_power(&mut f, "span-4", false);
    assert_eq!(
        f.graph().check_design(),
        [DesignIssue::Unpowered {
            element: id("amp-2")
        }]
    );
    let mut f = low();
    second_supply(&mut f, "amp-2");
    let issues = f.graph().check_design();
    assert!(
        issues.contains(&DesignIssue::ContestedPower {
            element: id("amp-2")
        }),
        "{issues:?}"
    );
}

#[test]
fn shipped_fault_files_fit_both_plants() {
    let examples: FaultSet = load("one_street/fault_examples.json");
    let empty: FaultSet = load("one_street/faults.json");
    for f in fixtures() {
        let g = f.graph();
        g.check_faults(&examples).unwrap();
        g.check_faults(&empty).unwrap();
    }
}

/// The one problem found in a fault set holding just this fault.
fn problem(kind: FaultKind) -> FaultRefProblem {
    let f = low();
    let set = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.into(),
        seed: 1,
        faults: vec![Fault {
            onset: Onset::Instant,
            kind,
        }],
    };
    let mut errors = f.graph().check_faults(&set).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].fault_index, 0);
    errors.remove(0).problem
}

fn water(span: &str, pos_ft: f64, affected_len_ft: f64) -> FaultKind {
    FaultKind::WaterIngress {
        span_id: EdgeId::new(span),
        pos_ft,
        affected_len_ft,
        k_w_db_per_100ft: 2.0,
        rain_coupling_per_mm_per_hr: 0.05,
        impedance_variation_pct: 5.0,
    }
}

fn reflection(location: Location) -> FaultKind {
    FaultKind::ConnectorReflection {
        location,
        return_loss_db: 10.0,
        couplings: None,
    }
}

#[test]
fn fault_position_past_the_span() {
    assert!(matches!(
        problem(reflection(point("span-2", 500.0))),
        FaultRefProblem::Location(QueryError::PositionOffEdge { .. })
    ));
    assert!(matches!(
        problem(water("span-2", 500.0, 10.0)),
        FaultRefProblem::Location(QueryError::PositionOffEdge { .. })
    ));
}

#[test]
fn fault_water_overrunning_its_span() {
    assert!(matches!(
        problem(water("span-2", 380.0, 50.0)),
        FaultRefProblem::WaterOverruns { .. }
    ));
}

#[test]
fn fault_water_on_a_drop() {
    assert_eq!(
        problem(water("drop-4", 10.0, 10.0)),
        FaultRefProblem::NotASpan(EdgeId::new("drop-4"))
    );
}

#[test]
fn fault_on_a_direct_edge() {
    assert_eq!(
        problem(reflection(point("direct-1", 0.0))),
        FaultRefProblem::Location(QueryError::NoCable(EdgeId::new("direct-1")))
    );
}

#[test]
fn fault_naming_the_wrong_kind_of_element() {
    let amp = FaultKind::AmpDrift {
        amp_id: id("tap-1"),
        state: AmpState::Failed,
    };
    assert_eq!(problem(amp), FaultRefProblem::NotAnAmp(id("tap-1")));
    let modem = FaultKind::BadDrop {
        modem_id: id("amp-1"),
        extra_loss_db: 6.0,
        return_loss_db: None,
    };
    assert_eq!(problem(modem), FaultRefProblem::NotAModem(id("amp-1")));
    let supply = FaultKind::PowerSupplySag {
        psu_id: id("psu-9"),
        v_delta: 12.0,
        schedule: Schedule {
            hourly_multipliers: [1.0; 24],
        },
    };
    assert_eq!(problem(supply), FaultRefProblem::UnknownSupply(id("psu-9")));
}

#[test]
fn fault_at_an_unknown_port() {
    assert!(matches!(
        problem(reflection(port("tap-1", "no-such-port"))),
        FaultRefProblem::Location(QueryError::UnknownPort { .. })
    ));
}

#[test]
fn new_returns_the_errors_of_all_three_inputs() {
    let mut f = low();
    f.plant.edges[3].to.element_id = id("amp-1");
    f.library.components.push(f.library.components[0].clone());
    f.profile.channel_plan.ofdm[0].subcarrier_spacing_khz = 25.0;
    let error = PlantGraph::new(&f.plant, &f.library, &f.profile).unwrap_err();
    assert!(!error.library.is_empty());
    assert!(!error.profile.is_empty());
    assert!(
        error
            .plant
            .iter()
            .any(|e| matches!(e, schema::plant::PlantError::Cycle(_)))
    );
}

#[test]
fn new_rejects_the_wrong_profile() {
    let f = low();
    let other: Profile = load("profiles/d31_highsplit_1218_rpd.json");
    let error = PlantGraph::new(&f.plant, &f.library, &other).unwrap_err();
    assert!(!error.plant.is_empty());
}

#[test]
fn bad_queries_return_errors() {
    let f = low();
    let g = f.graph();
    let ghost = ElementId::new("ghost");
    assert_eq!(
        g.element(&ghost).unwrap_err(),
        QueryError::UnknownElement(ghost.clone())
    );
    assert!(g.parent(&ghost).is_err());
    assert!(g.children(&ghost).is_err());
    assert!(g.ancestors(&ghost).is_err());
    assert!(g.cascade(&ghost).is_err());
    assert!(g.root_port(&ghost).is_err());
    assert!(g.path_from_root(&ghost).is_err());
    assert!(g.common_ancestor(std::slice::from_ref(&ghost)).is_err());
    assert_eq!(g.common_ancestor(&[]).unwrap_err(), QueryError::NoElements);
    assert!(g.edge(&EdgeId::new("ghost")).is_err());

    let here = port("tap-1", "in");
    assert!(matches!(
        g.path_between(&here, &port("tap-1", "ghost")).unwrap_err(),
        QueryError::UnknownPort { .. }
    ));
    assert!(matches!(
        g.path_between(&here, &point("ghost", 1.0)).unwrap_err(),
        QueryError::UnknownEdge(_)
    ));
    for pos_ft in [-1.0, 301.0, f64::NAN] {
        assert!(matches!(
            g.below(&point("span-3", pos_ft)).unwrap_err(),
            QueryError::PositionOffEdge { .. }
        ));
    }
    assert!(matches!(
        g.below(&point("direct-1", 0.0)).unwrap_err(),
        QueryError::NoCable(_)
    ));
}
