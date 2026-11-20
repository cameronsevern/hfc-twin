//! §16: fiber faults on the as-built fixture's fiber plant, and §15's misalignment. All synthetic.

use std::fs;

use plant_graph::PlantGraph;
use schema::common::{ElementId, FiberCableId, FiberElementId, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{Fault, FaultKind, FaultSet, Onset};
use schema::fiber::FiberPlant;
use schema::fiber_fault::{FiberFault, FiberFaultKind, FiberLocation, StrandRef};
use schema::plant::Plant;
use schema::profile::Profile;
use sim::optics::{NodeLink, node_link_at, region_links};
use sim::telemetry::{Scenario, SimOptions, StepObservation, simulate};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

struct Region {
    fiber: FiberPlant,
    library: ComponentLibrary,
    low: (Plant, Profile),
    high: (Plant, Profile),
}

fn region() -> Region {
    Region {
        fiber: read("one_street_as_built/fiber.json"),
        library: read("library/generic.json"),
        low: (
            read("one_street_as_built/plant_lowsplit_iccap.json"),
            read("profiles/d31_lowsplit_1002_iccap.json"),
        ),
        high: (
            read("one_street_as_built/plant_highsplit_rpd.json"),
            read("profiles/d31_highsplit_1218_rpd.json"),
        ),
    }
}

fn links(
    r: &Region,
    faults: &[FiberFault],
    t: i64,
) -> std::collections::BTreeMap<String, NodeLink> {
    region_links(
        &r.fiber,
        &[
            (&r.low.0, &r.low.1.channel_plan),
            (&r.high.0, &r.high.1.channel_plan),
        ],
        &r.library,
        faults,
        Timestamp(t),
    )
    .unwrap()
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

fn cut(cable: &str, repair_s: Option<f64>) -> FiberFault {
    FiberFault {
        onset: Onset::Instant,
        kind: FiberFaultKind::FiberCut {
            cable: FiberCableId::new(cable),
            pos_ft: 10.0,
            repair_s,
        },
    }
}

const LOW: &str = "sg-as-built-lowsplit";
const HIGH: &str = "sg-as-built-highsplit";

fn up(link: &NodeLink) -> (bool, bool) {
    (
        link.forward.as_ref().unwrap().up,
        link.ret.as_ref().unwrap().up,
    )
}

#[test]
fn a_cut_on_a_shared_cable_takes_every_node_that_crosses_it_and_no_other() {
    let r = region();
    let healthy = links(&r, &[], 0);
    assert_eq!(up(&healthy[LOW]), (true, true));
    assert_eq!(up(&healthy[HIGH]), (true, true));

    // `fiber-1` carries both nodes' links.
    let shared = links(&r, &[cut("fiber-1", None)], 0);
    assert_eq!(up(&shared[LOW]), (false, false));
    assert_eq!(up(&shared[HIGH]), (false, false));
    assert!(shared[LOW].forward_dark() && shared[HIGH].digital_down());
    let forward = shared[LOW].forward.as_ref().unwrap();
    assert_eq!(forward.received_dbm, None);
    assert!(forward.hops.iter().any(|h| h.loss_db.is_none()));

    // `fiber-2` only the low-split node's: the RPD's links come out exactly as healthy.
    let own = links(&r, &[cut("fiber-2", None)], 0);
    assert_eq!(up(&own[LOW]), (false, false));
    assert_eq!(own[HIGH], healthy[HIGH]);

    // Spliced after four hours.
    let repaired = links(&r, &[cut("fiber-1", Some(14_400.0))], 14_400);
    assert_eq!(repaired, healthy);
    assert_eq!(
        links(&r, &[cut("fiber-1", Some(14_400.0))], 14_399)[LOW],
        shared[LOW]
    );
}

#[test]
fn event_loss_at_each_kind_of_location() {
    let r = region();
    let healthy = links(&r, &[], 0);
    let loss = |location: FiberLocation| FiberFault {
        onset: Onset::Instant,
        kind: FiberFaultKind::EventLoss {
            location,
            loss_db: 3.0,
        },
    };
    let received = |l: &NodeLink, forward: bool| {
        let one = if forward { &l.forward } else { &l.ret };
        one.as_ref().unwrap().received_dbm.unwrap()
    };
    // A bend on `fiber-2`: every strand, so both of the low node's links, 3 dB each; the RPD's untouched.
    let bent = links(
        &r,
        &[loss(FiberLocation::Cable {
            cable: FiberCableId::new("fiber-2"),
            pos_ft: 400.0,
        })],
        0,
    );
    for forward in [true, false] {
        let d = received(&healthy[LOW], forward) - received(&bent[LOW], forward);
        assert!((d - 3.0).abs() < 1e-9, "{d}");
    }
    assert_eq!(bent[HIGH], healthy[HIGH]);

    // A dirty connector at the coupler: only the low forward link passes it, twice, and it counts once.
    let dirty = links(
        &r,
        &[loss(FiberLocation::Element {
            element: FiberElementId::new("osplit-1"),
        })],
        0,
    );
    let d = received(&healthy[LOW], true) - received(&dirty[LOW], true);
    assert!((d - 3.0).abs() < 1e-9, "{d}");
    assert_eq!(dirty[LOW].ret, healthy[LOW].ret);

    // A bad splice in `closure-1`, named by the strands it joins: only the low return, whichever order they're named in.
    for strands in [
        [("fiber-2", 2), ("fiber-1", 2)],
        [("fiber-1", 2), ("fiber-2", 2)],
    ] {
        let strands = strands.map(|(cable, strand)| StrandRef {
            cable: FiberCableId::new(cable),
            strand,
        });
        let bad = links(
            &r,
            &[loss(FiberLocation::Splice {
                element: FiberElementId::new("closure-1"),
                strands,
            })],
            0,
        );
        let d = received(&healthy[LOW], false) - received(&bad[LOW], false);
        assert!((d - 3.0).abs() < 1e-9, "{d}");
        assert_eq!(bad[LOW].forward, healthy[LOW].forward);
        assert_eq!(bad[HIGH], healthy[HIGH]);
    }

    // Half way up a ramp, half the loss.
    let ramped = FiberFault {
        onset: Onset::Ramp {
            start: Timestamp(0),
            end: Timestamp(1000),
        },
        ..loss(FiberLocation::Cable {
            cable: FiberCableId::new("fiber-2"),
            pos_ft: 400.0,
        })
    };
    let half = links(&r, &[ramped], 500);
    let d = received(&healthy[LOW], true) - received(&half[LOW], true);
    assert!((d - 1.5).abs() < 1e-9, "{d}");
}

#[test]
fn a_transmitter_drifts() {
    let r = region();
    let healthy = links(&r, &[], 0);
    let drift = FiberFault {
        onset: Onset::Instant,
        kind: FiberFaultKind::TransmitterDrift {
            element: FiberElementId::new("ftx-1"),
            power_delta_db: -4.0,
            rin_delta_db: 6.0,
        },
    };
    let drifted = links(&r, &[drift], 0);
    let (a, b) = (
        healthy[LOW].forward.as_ref().unwrap(),
        drifted[LOW].forward.as_ref().unwrap(),
    );
    assert!((a.received_dbm.unwrap() - b.received_dbm.unwrap() - 4.0).abs() < 1e-9);
    let (ca, cb) = (a.cnr.as_ref().unwrap(), b.cnr.as_ref().unwrap());
    assert!((ca.rin_db - cb.rin_db - 6.0).abs() < 1e-9);
    // Shot noise moves 1 dB per dB of light, thermal 2 (§16).
    assert!((ca.shot_db - cb.shot_db - 4.0).abs() < 1e-9);
    assert!((ca.thermal_db - cb.thermal_db - 8.0).abs() < 1e-9);
    assert_eq!(drifted[LOW].ret, healthy[LOW].ret);
    assert_eq!(drifted[HIGH], healthy[HIGH]);
}

fn environment(samples: usize) -> Environment {
    Environment {
        schema_version: schema::environment::SCHEMA_VERSION.into(),
        samples: (0..samples)
            .map(|k| EnvironmentSample {
                t: Timestamp(60 * k as i64),
                temp_c: 20.0,
                ground_temp_c: 20.0,
                rain_mm_per_hr: 0.0,
                wind_m_per_s: None,
                solar_w_per_m2: None,
            })
            .collect(),
        ambient_sources: Vec::new(),
    }
}

fn run(
    plant: &Plant,
    r: &Region,
    profile: &Profile,
    link: &NodeLink,
    faults: &FaultSet,
    samples: usize,
) -> Vec<StepObservation> {
    let graph = PlantGraph::new(plant, &r.library, profile).unwrap();
    simulate(&Scenario {
        graph: &graph,
        environment: &environment(samples),
        faults,
        seed: 5,
        options: SimOptions {
            measurement_noise_db: 0.0,
            ..SimOptions::default()
        },
        link: Some(link),
    })
    .unwrap()
}

fn no_faults() -> FaultSet {
    FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.into(),
        seed: 1,
        faults: Vec::new(),
    }
}

fn ds_mean(step: &StepObservation) -> Vec<Option<f64>> {
    step.modems
        .iter()
        .map(|m| {
            m.rxmer
                .as_ref()
                .and_then(|r| r.first())
                .and_then(|r| r.rxmer_mean_db)
        })
        .collect()
}

#[test]
fn a_return_only_cut_leaves_rxmer_and_takes_ranging() {
    let r = region();
    // `jumper-2` carries only the low node's return, hub side.
    let healthy = links(&r, &[], 0);
    let return_cut = links(&r, &[cut("jumper-2", None)], 0);
    assert_eq!(up(&return_cut[LOW]), (true, false));
    assert_eq!(return_cut[HIGH], healthy[HIGH]);

    let (plant, profile) = &r.low;
    let before = run(plant, &r, profile, &healthy[LOW], &no_faults(), 30);
    let after = run(plant, &r, profile, &return_cut[LOW], &no_faults(), 30);
    // Downstream RxMER reads exactly as it did while the modems stay registered: four ranging opportunities a minute all fail,
    // and §7.4's 17 misses take until the fifth step.
    for (a, b) in before.iter().zip(&after).take(4) {
        assert_eq!(ds_mean(a), ds_mean(b));
        assert!(ds_mean(b).iter().any(Option::is_some));
    }
    // Then ranging is lost and they leave service, which a healthy return never does.
    let registered = |step: &StepObservation| {
        step.modems
            .iter()
            .filter(|m| m.reg_state == schema::observation::RegState::Operational)
            .count()
    };
    assert_eq!(registered(before.last().unwrap()), 9);
    assert!(registered(after.last().unwrap()) < 9);
    // The CMTS hears nothing, and the modems can't range.
    let last = after.last().unwrap();
    assert!(last.service_group.us_snr.is_empty());
    assert!(!before.last().unwrap().service_group.us_snr.is_empty());
    let t3: u64 = last
        .modems
        .iter()
        .flat_map(|m| &m.t3_timeouts)
        .map(|r| r.count)
        .sum();
    let t3_before: u64 = before
        .last()
        .unwrap()
        .modems
        .iter()
        .flat_map(|m| &m.t3_timeouts)
        .map(|r| r.count)
        .sum();
    assert!(t3 > t3_before, "{t3} {t3_before}");
    // And it all serializes: no infinities reach the observation.
    let text = serde_json::to_string(&after).unwrap();
    let back: Vec<StepObservation> = serde_json::from_str(&text).unwrap();
    assert_eq!(back, after);
}

#[test]
fn a_forward_cut_darkens_an_analog_node() {
    let r = region();
    let dark = links(&r, &[cut("jumper-1", None)], 0);
    assert_eq!(up(&dark[LOW]), (false, true));
    let (plant, profile) = &r.low;
    let steps = run(plant, &r, profile, &dark[LOW], &no_faults(), 2);
    assert!(
        steps[0]
            .modems
            .iter()
            .all(|m| ds_mean(&steps[0]).iter().all(Option::is_none) || m.rxmer.is_none())
    );
    let text = serde_json::to_string(&steps).unwrap();
    let _: Vec<StepObservation> = serde_json::from_str(&text).unwrap();
}

#[test]
fn a_misalignment_moves_the_station_by_its_plug_in() {
    let r = region();
    let (plant, profile) = &r.low;
    let graph = PlantGraph::new(plant, &r.library, profile).unwrap();
    let sample = &environment(1).samples[0];
    let walker_out = |faults: &FaultSet, f: f64| -> f64 {
        let overlay = sim::Overlay::build(&graph, faults, sample).unwrap();
        let walker = sim::Walker::new(&graph, sample, &overlay).unwrap();
        let capture = walker
            .capture(&sim::Grid::new(vec![f]), sim::Band::Down)
            .unwrap();
        capture.outputs[&(
            ElementId::new("amp-1"),
            schema::common::PortId::new("out-1"),
        )][0]
    };
    let misaligned = |pad_delta_db: f64, eq_delta_db: f64| FaultSet {
        faults: vec![Fault {
            onset: Onset::Instant,
            kind: FaultKind::Misalignment {
                active_id: ElementId::new("amp-1"),
                pad_delta_db,
                eq_delta_db,
                return_pad_delta_db: 0.0,
            },
        }],
        ..no_faults()
    };
    for f in [54.0, 550.0, 1002.0] {
        let base = walker_out(&no_faults(), f);
        // A pad 2 dB too big: 2 dB down everywhere.
        assert!(
            (base - walker_out(&misaligned(2.0, 0.0), f) - 2.0).abs() < 1e-9,
            "{f}"
        );
    }
    // An equalizer 4 dB too big: 4 dB down at the bottom of its slope band, nothing at the top.
    let (lo, hi): (f64, f64) = (54.0, 1218.0);
    let base_lo = walker_out(&no_faults(), lo);
    let base_hi = walker_out(&no_faults(), hi.min(1002.0));
    let eq_lo = walker_out(&misaligned(0.0, 4.0), lo);
    let eq_hi = walker_out(&misaligned(0.0, 4.0), hi.min(1002.0));
    assert!((base_lo - eq_lo - 4.0).abs() < 1e-9);
    let share = (hi.sqrt() - 1002f64.sqrt()) / (hi.sqrt() - lo.sqrt());
    assert!((base_hi - eq_hi - 4.0 * share).abs() < 1e-9);
    let _ = node_link_at;
}
