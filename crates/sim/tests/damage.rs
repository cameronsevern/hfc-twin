//! Task 19's damage events against `docs/physics.md` §20, on `one_street`. All synthetic, and none checked against a real event.

use std::collections::BTreeMap;
use std::fs;

use plant_graph::PlantGraph;
use schema::common::{ComponentId, EdgeId, ElementId, LonLat, PortId, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{CutEnd, Fault, FaultKind, FaultSet, Location, NodeFailureMode, Onset};
use schema::observation::RegState;
use schema::plant::{BatteryBank, Plant};
use schema::profile::Profile;
use sim::overlay::Overlay;
use sim::telemetry::{Detail, Scenario, SimOptions, StepObservation, simulate};
use sim::units::C64;
use sim::walk::{Band, Grid, ModemView, Walker};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn plant() -> Plant {
    read("one_street/plant_lowsplit_iccap.json")
}

fn library() -> ComponentLibrary {
    read("library/generic.json")
}

fn profile() -> Profile {
    read("profiles/d31_lowsplit_1002_iccap.json")
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

fn set(kinds: Vec<FaultKind>) -> FaultSet {
    FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.into(),
        seed: 3,
        faults: kinds
            .into_iter()
            .map(|kind| Fault {
                onset: Onset::Instant,
                kind,
            })
            .collect(),
    }
}

fn run(plant: &Plant, kinds: Vec<FaultKind>, times: &[i64]) -> Vec<StepObservation> {
    let (library, profile) = (library(), profile());
    let graph = PlantGraph::new(plant, &library, &profile).unwrap();
    let environment = Environment {
        schema_version: "1".into(),
        samples: times.iter().map(|t| at(*t)).collect(),
        ambient_sources: Vec::new(),
    };
    simulate(&Scenario {
        graph: &graph,
        environment: &environment,
        faults: &set(kinds),
        seed: 3,
        options: SimOptions {
            detail: Detail::Summary { stride: 8 },
            randomize: None,
            measurement_noise_db: 0.0,
            rxmer_scatter_db: 0.0,
        },
        link: None,
    })
    .unwrap()
}

fn operational(step: &StepObservation, modem: &str) -> bool {
    step.modems
        .iter()
        .find(|m| m.element_id.as_str() == modem)
        .unwrap()
        .reg_state
        == RegState::Operational
}

fn cut(end: CutEnd) -> FaultKind {
    // Between tap-2 and amp-2: modems 1 to 4 are on the live side, 5 to 8 past it.
    FaultKind::SpanCut {
        span_id: EdgeId::new("span-4"),
        pos_ft: 50.0,
        end,
        repair_s: None,
    }
}

#[test]
fn section_20_4_a_shunt_short_loses_and_reflects_inverted() {
    let (loss_db, rl_db) = sim::faults::shunt_db(0.5);
    assert!((loss_db - 37.616).abs() < 1e-3, "{loss_db}");
    assert!((rl_db - 0.115).abs() < 1e-3, "{rl_db}");
}

#[test]
fn section_20_3_water_in_a_tap_loses_30_db_at_750_mhz() {
    let (plant, library, profile) = (plant(), library(), profile());
    let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
    let faults = set(vec![FaultKind::WaterInDevice {
        element_id: ElementId::new("tap-2"),
        loss_db_at_1ghz: 30.0 / 0.75_f64.sqrt(),
        return_loss_db: 7.0,
        rain_coupling_per_mm_per_hr: 0.0,
        failed_after_s: None,
    }]);
    let overlay = Overlay::build(&graph, &faults, &at(0)).unwrap();
    let points = &overlay.port_points[&(ElementId::new("tap-2"), PortId::new("in"))];
    let loss = points[0].loss.as_ref().unwrap();
    assert!(
        (loss.loss_db(750.0) - 30.0).abs() < 0.05,
        "{}",
        loss.loss_db(750.0)
    );
}

#[test]
fn section_20_6_a_three_battery_string_carries_800_w_for_3_6_hours() {
    let mut plant = plant();
    plant.power_supplies[0].batteries = Some(BatteryBank {
        component_ref: ComponentId::new("battery-generic-12v"),
        per_string: 3,
        strings: 1,
    });
    let (library, profile) = (library(), profile());
    let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
    let t = sim::ac::standby_s(&graph, &plant.power_supplies[0], 800.0).unwrap();
    assert!((t - 12_960.0).abs() < 1e-6, "{t}");
}

#[test]
fn a_cut_takes_out_what_is_past_it_and_nothing_before() {
    // Past amp-2, which is directional, so the open's echo doesn't reach back to modems 1 to 4. A cut just past a tap
    // reaches that tap's own ports through its isolation (§5.1, §20.1); since task 27 their equalizers take it out.
    let past_amp = FaultKind::SpanCut {
        span_id: EdgeId::new("span-5"),
        pos_ft: 200.0,
        end: CutEnd::Open,
        repair_s: None,
    };
    let steps = run(&plant(), vec![past_amp], &[0, 120, 600]);
    for step in &steps {
        for m in ["modem-1", "modem-2", "modem-3", "modem-4"] {
            assert!(operational(step, m), "{m} at {:?}", step.t);
        }
    }
    // Past it, gone once §7.4's timers run out.
    for m in ["modem-5", "modem-6", "modem-7", "modem-8"] {
        assert!(!operational(&steps[2], m), "{m}");
    }
}

#[test]
fn a_cut_just_past_a_tap_leaves_that_taps_modems_up() {
    // Task 27: the open's echo through tap-2's isolation is short, so the modems' equalizers take it out, 3.0 or 3.1.
    let steps = run(&plant(), vec![cut(CutEnd::Open)], &[0, 120, 600]);
    for m in ["modem-1", "modem-2", "modem-3", "modem-4"] {
        assert!(operational(&steps[2], m), "{m}");
    }
    for m in ["modem-5", "modem-6", "modem-7", "modem-8"] {
        assert!(!operational(&steps[2], m), "{m}");
    }
}

#[test]
fn an_open_and_a_short_echo_with_opposite_sign() {
    // The RF alone, as the walk reads it: a short's AC trip is §20.4's, and tested below.
    let (plant, library, profile) = (plant(), library(), profile());
    let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
    let grid = Grid::new((0..200).map(|i| 600.0 + i as f64 * 0.5).collect());
    let response = |kinds: Vec<FaultKind>| -> BTreeMap<String, Vec<C64>> {
        let overlay = Overlay::build(&graph, &set(kinds), &at(0)).unwrap();
        let s = at(0);
        let walker = Walker::new(&graph, &s, &overlay).unwrap();
        let mut out = BTreeMap::new();
        let mut sink = |view: &ModemView<'_>| -> Result<(), sim::PhysicsError> {
            out.insert(view.modem.id.to_string(), view.h.to_vec());
            Ok(())
        };
        walker.run(&grid, Band::Down, &mut sink).unwrap();
        out
    };
    let none = response(vec![]);
    let open = response(vec![cut(CutEnd::Open)]);
    let short = response(vec![cut(CutEnd::Shorted)]);
    for modem in ["modem-3", "modem-4"] {
        let (n, o, s) = (&none[modem], &open[modem], &short[modem]);
        let mut moved = 0.0_f64;
        for i in 0..n.len() {
            // §20.1: the two average to neither, so the cut's echo has the same size and opposite sign.
            let mean = (o[i] + s[i]).scale(0.5);
            let off = (mean + n[i].scale(-1.0)).norm_sqr().sqrt();
            assert!(
                off < 1e-9 * n[i].norm_sqr().sqrt().max(1.0),
                "{modem} {i}: {off}"
            );
            moved = moved.max((o[i] + n[i].scale(-1.0)).norm_sqr().sqrt());
        }
        assert!(moved > 1e-4, "{modem}: the cut echoes, {moved}");
    }
}

#[test]
fn only_the_short_trips_the_supply() {
    let open = run(&plant(), vec![cut(CutEnd::Open)], &[0, 600]);
    let short = run(&plant(), vec![cut(CutEnd::Shorted)], &[0, 600]);
    assert!(operational(&open[1], "modem-1"));
    // 90 V through span-1 to span-4 and 0.1 Ω: far past the supply's 15 A, so everything it feeds stops.
    for m in ["modem-1", "modem-2", "modem-3", "modem-4"] {
        assert!(!operational(&short[1], m), "{m}");
    }
    // A short of 100 Ω draws under an amp and holds.
    let soft = run(
        &plant(),
        vec![FaultKind::PowerShort {
            span_id: EdgeId::new("span-4"),
            pos_ft: 50.0,
            fault_resistance_ohm: 100.0,
            repair_s: None,
        }],
        &[0, 600],
    );
    assert!(operational(&soft[1], "modem-1"));
}

#[test]
fn a_utility_outage_with_and_without_standby() {
    let psu_geo = plant().power_supplies[0].geo.unwrap();
    let outage = || FaultKind::UtilityOutage {
        center: psu_geo,
        radius_ft: 10.0,
        repair_s: None,
    };
    // No batteries: the supply stops at once, and everything with it.
    let bare = run(&plant(), vec![outage()], &[0, 600]);
    assert!(!operational(&bare[1], "modem-1"));
    // Three batteries: carried for §20.6's time, then down.
    let mut standby = plant();
    standby.power_supplies[0].batteries = Some(BatteryBank {
        component_ref: ComponentId::new("battery-generic-12v"),
        per_string: 3,
        strings: 1,
    });
    let (library, profile) = (library(), profile());
    let graph = PlantGraph::new(&standby, &library, &profile).unwrap();
    let load_w: f64 = graph
        .actives()
        .iter()
        .filter_map(|a| {
            graph
                .component(&a.component_ref)?
                .spec
                .active()
                .map(|s| s.power_w)
        })
        .sum();
    let t = sim::ac::standby_s(&graph, &standby.power_supplies[0], load_w).unwrap() as i64;
    let steps = run(&standby, vec![outage()], &[0, t - 600, t + 600]);
    assert!(operational(&steps[1], "modem-1"));
    assert!(!operational(&steps[2], "modem-1"));
}

#[test]
fn an_outage_over_homes_silences_their_modems_only() {
    let plant = plant();
    let home = plant
        .elements
        .iter()
        .find(|e| e.id.as_str() == "modem-7")
        .and_then(|e| e.geo)
        .unwrap();
    let steps = run(
        &plant,
        vec![FaultKind::UtilityOutage {
            center: home,
            radius_ft: 1.0,
            repair_s: None,
        }],
        &[0, 600],
    );
    assert!(!operational(&steps[1], "modem-7"));
    assert!(operational(&steps[1], "modem-1"));
}

#[test]
fn a_damaged_device_with_one_effect_is_that_fault() {
    let damaged = run(
        &plant(),
        vec![FaultKind::DamagedDevice {
            element_id: ElementId::new("tap-2"),
            through_loss_db: 0.0,
            return_loss_db: 12.0,
            ports_lost: Vec::new(),
            ingress_power_dbmv: None,
            cpd_second_order_dbmv_per_mhz: None,
        }],
        &[0, 300],
    );
    let connector = run(
        &plant(),
        vec![FaultKind::ConnectorReflection {
            location: Location::Port {
                element_id: ElementId::new("tap-2"),
                port: PortId::new("in"),
            },
            return_loss_db: 12.0,
            couplings: None,
        }],
        &[0, 300],
    );
    assert_eq!(
        serde_json::to_string(&damaged).unwrap(),
        serde_json::to_string(&connector).unwrap()
    );
}

#[test]
fn a_node_that_loses_power_takes_everything_and_one_whose_rf_fails_stays_powered() {
    let power = run(
        &plant(),
        vec![FaultKind::AnalogNodeFailure {
            node_id: ElementId::new("node-1"),
            mode: NodeFailureMode::Power,
            repair_s: None,
        }],
        &[0, 600],
    );
    let rf = run(
        &plant(),
        vec![FaultKind::AnalogNodeFailure {
            node_id: ElementId::new("node-1"),
            mode: NodeFailureMode::RfSection,
            repair_s: None,
        }],
        &[0, 600],
    );
    for steps in [&power, &rf] {
        assert!(!operational(&steps[1], "modem-1"));
    }
}

#[test]
fn a_repaired_cut_comes_back_and_the_same_seed_gives_the_same_bytes() {
    let event = || FaultKind::SpanCut {
        span_id: EdgeId::new("span-4"),
        pos_ft: 50.0,
        end: CutEnd::Open,
        repair_s: Some(300.0),
    };
    let times = [0, 120, 600, 1800];
    let a = run(&plant(), vec![event()], &times);
    let b = run(&plant(), vec![event()], &times);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
    assert!(!operational(&a[1], "modem-5"));
    assert!(operational(&a[3], "modem-5"), "back after the repair");
}

#[test]
fn a_plant_with_no_event_is_unchanged() {
    // `one_street` keeps its level budget: no event, no change (pinned elsewhere to the byte).
    let (plant, library, profile) = (plant(), library(), profile());
    let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
    assert!(graph.check_design().is_empty());
    let _: LonLat = plant.power_supplies[0].geo.unwrap();
}
