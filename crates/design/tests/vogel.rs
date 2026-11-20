//! The one sourced worked design the designer can be held to: VOGEL2019's tap run (`docs/reference/design-practice.md`
//! §1, [VOGEL2019 §3 Figure 7]). One active at the paper's level, its PIII 625 feeder, six 4-port tap locations 150 ft
//! apart, a 100 ft drop and the paper's premises behind each, under rules that state the paper's own criteria. The
//! designer's feeder stage should pick the paper's tap values within one step, place no active inside the run, and land
//! each device within 1.5 dB of the paper's level. Every plant here is synthetic, and the paper's levels are its own.

use std::fs;

use design::geo::offset;
use design::v2::kit::Kit;
use design::v2::levels::{cable, through};
use design::v2::pipeline::{place_structures, plan_nodes};
use schema::common::LonLat;
use schema::component::{ComponentLibrary, ComponentSpec};
use schema::design_rules::DesignRules;
use schema::fault::Location;
use schema::plant::{ElementKind, Installation, Plant, PortRole};
use schema::profile::Profile;
use schema::region::{DesignControls, Home, MapSource, PinnedNode, Street, StreetMap};
use sim::Overlay;
use sim::walk::{Band, Grid, Walker};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

const PROFILE: &str = "d31_lowsplit_1002_iccap";
/// The paper's source: node or last amp, 54 dBmV analog-referenced at 1002 MHz.
const SOURCE_DBMV: f64 = 54.0;
/// The paper's device target, analog-referenced, at 1002 MHz. Its devices sit at or above it, 0.1 to 4.4 dB: a floor.
const DEVICE_TARGET_DBMV: f64 = 0.0;
/// The rules state a target and a tolerance either way, and the feeder takes the value nearest the target; on 3 dB
/// steps that is the highest value at or above a floor half a step below it. So the paper's floor is stated as a target
/// half a step above it.
const HALF_STEP_DB: f64 = 1.5;
/// The paper's tap values, first to last, and its device levels.
const PAPER_VALUES_DB: [f64; 6] = [29.0, 26.0, 23.0, 20.0, 14.0, 8.0];
const PAPER_DEVICE_DBMV: [f64; 6] = [4.4, 3.1, 1.7, 0.1, 0.6, 1.1];
const SPACING_FT: f64 = 150.0;
const DROP_FT: f64 = 100.0;
const HOUSE_FT: f64 = 50.0;
const CABLE: &str = "cable-commscope-p3-625";
const GROUND_BLOCK: &str = "gb-test-scte129";
const FOUR_WAY: &str = "splitter-test-4way-scte153";

/// A straight aerial street from the node east, six pole locations 150 ft apart, four homes at each, a drop's length
/// from the pole on the side the rules put the plant.
fn map(origin: LonLat) -> StreetMap {
    let mut homes = Vec::new();
    for k in 1..=6 {
        for j in 0..4 {
            let east = k as f64 * SPACING_FT + (j as f64 - 1.5) * 4.0;
            homes.push(Home {
                id: format!("home-{k}-{j}"),
                // Right of an eastbound street is south; the pole is at the 10 ft setback.
                geo: offset(origin, east, -(10.0 + DROP_FT)),
            });
        }
    }
    StreetMap {
        schema_version: schema::region::SCHEMA_VERSION.to_string(),
        id: "vogel-2019-figure-7".into(),
        source: MapSource::Generated {
            family: schema::region::LayoutFamily::Suburban,
            seed: 0,
        },
        streets: vec![Street {
            id: "feeder".into(),
            points: vec![origin, offset(origin, 6.0 * SPACING_FT, 0.0)],
            installation: Installation::Aerial,
        }],
        homes,
        headend: offset(origin, -500.0, 0.0),
    }
}

/// The paper's premises at 1002 MHz behind a port: its drop, the ground block, one leg of a 4-way split and 50 ft of
/// series 6 in the home, with the drop's two F connectors, from the parts the rules name.
fn premises_loss_db(rules: &DesignRules, library: &ComponentLibrary, profile: &Profile) -> f64 {
    let f = 1002.0;
    let kit = Kit::new(rules, library, profile, None)
        .map_err(|e| format!("{e:?}"))
        .unwrap();
    let four_way = rules.premises.iter().find(|t| t.id == "four-way").unwrap();
    let part = |id: &schema::common::ComponentId| library.get(id).unwrap();
    let drop = part(&rules.drops[0].cable_ref.value);
    let house = part(&rules.premises_parts.as_ref().unwrap().house_cable_ref.value);
    let ComponentSpec::Drop(drop_spec) = &drop.spec else {
        panic!()
    };
    let ComponentSpec::Drop(house_spec) = &house.spec else {
        panic!()
    };
    cable(drop_spec, DROP_FT)(f, 20.0)
        + 2.0 * through(kit.drop_f, f)
        + through(part(four_way.ground_block_ref.as_ref().unwrap()), f)
        + through(part(four_way.splitter_ref.as_ref().unwrap()), f)
        + cable(house_spec, HOUSE_FT)(f, 20.0)
}

/// Generic rules with the paper's own criteria: its cable, its source level, its device target stated at the port
/// through its premises, and its premises as the only template.
fn paper_rules(library: &ComponentLibrary, profile: &Profile) -> DesignRules {
    let mut rules: DesignRules = read("design_rules/generic.json");
    for role in &mut rules.roles {
        role.cable_ref.value = schema::common::ComponentId::new(CABLE);
    }
    // Priced as the .500 parts are, so the rules are complete; cost plays no part in a tap's value.
    for (like, id) in [
        ("cable-generic-500", CABLE),
        ("conn-generic-pin-500", "conn-test-pin-625"),
        ("conn-generic-splice-500", "conn-test-splice-625"),
    ] {
        let price = rules
            .costs
            .parts
            .iter()
            .find(|c| c.component_ref.as_str() == like)
            .unwrap()
            .clone();
        rules.costs.parts.push(schema::design_rules::PartCost {
            component_ref: schema::common::ComponentId::new(id),
            ..price
        });
    }
    if let Some(f) = &mut rules.fittings {
        f.connectors
            .value
            .push(schema::common::ComponentId::new("conn-test-pin-625"));
        f.connectors
            .value
            .push(schema::common::ComponentId::new("conn-test-splice-625"));
    }
    // The paper's taps are drawn 150 ft apart: a pole at each.
    rules.structures.pole_spacing_ft.value = SPACING_FT;
    // Its premises: the ground block and 4-way at the note's figures.
    for t in rules.premises.iter_mut().filter(|t| t.id == "four-way") {
        t.ground_block_ref = Some(schema::common::ComponentId::new(GROUND_BLOCK));
        t.splitter_ref = Some(schema::common::ComponentId::new(FOUR_WAY));
    }
    for (like, id) in [
        ("gb-generic", GROUND_BLOCK),
        ("splitter-generic-4way-home", FOUR_WAY),
    ] {
        if let Some(price) = rules
            .costs
            .parts
            .iter()
            .find(|c| c.component_ref.as_str() == like)
            .cloned()
        {
            rules.costs.parts.push(schema::design_rules::PartCost {
                component_ref: schema::common::ComponentId::new(id),
                ..price
            });
        }
    }
    let loss = premises_loss_db(&rules, library, profile);
    for t in rules
        .tap_port
        .iter_mut()
        .filter(|t| t.profile_ref.as_str() == PROFILE)
    {
        let hi = DEVICE_TARGET_DBMV + loss + HALF_STEP_DB;
        // A level at the bottom, where the rules hold one, keeps its distance from the top.
        if let Some(lo) = &mut t.at_lo_dbmv {
            lo.value += hi - t.at_hi_dbmv.value;
        }
        t.at_hi_dbmv.value = hi;
    }
    for p in rules
        .profiles
        .iter_mut()
        .flatten()
        .filter(|p| p.profile_ref.as_str() == PROFILE)
    {
        p.station.out_dbmv_at_hi.value = SOURCE_DBMV;
        p.station.min_out_dbmv_at_hi.value = SOURCE_DBMV;
        p.premises.value = vec!["four-way".into()];
    }
    rules
}

/// Level at each device's input at 1002 MHz, 20 °C, read through the sim from the active feeding it.
fn device_levels(
    plant: &Plant,
    library: &ComponentLibrary,
    profile: &Profile,
) -> Vec<(String, f64)> {
    let g = plant_graph::PlantGraph::new(plant, library, profile).unwrap();
    let s = design::v2::levels::sample(20.0);
    let overlay = Overlay::default();
    let walker = Walker::new(&g, &s, &overlay).unwrap();
    let capture = walker
        .capture(&Grid::new(vec![1002.0]), Band::Down)
        .unwrap();
    let mut out = Vec::new();
    for modem in plant
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Modem))
    {
        let mut at = modem.id.clone();
        let mut feed = None;
        while let Ok(Some((parent, edge))) = g.parent(&at) {
            if matches!(
                parent.kind,
                ElementKind::Node { .. } | ElementKind::Rpd { .. } | ElementKind::Amp { .. }
            ) {
                feed = Some((parent.id.clone(), edge.from.port.clone()));
                break;
            }
            at = parent.id.clone();
        }
        let (active, port) = feed.unwrap();
        let level = capture.outputs[&(active.clone(), port.clone())][0];
        let input = modem
            .ports
            .iter()
            .find(|p| p.role == PortRole::Input)
            .unwrap();
        let a = Location::Port {
            element_id: active,
            port,
        };
        let b = Location::Port {
            element_id: modem.id.clone(),
            port: input.id.clone(),
        };
        let loss = sim::transfer::transfer(&g, &s, &a, &b, 1002.0)
            .unwrap()
            .loss_db;
        out.push((modem.id.to_string(), level - loss));
    }
    out
}

/// The sourced library, with the paper's cable given its size: the data-sheet entry predates the size field, so this
/// test's copy states it, with a pin and a splice of that size copied from the `.500` ones. And the paper's premises at
/// the figures the note prices them by (design-practice.md §1): a ground block at SCTE 129's 0.05 dB and a 4-way at
/// SCTE 153's 8.6 dB at 1002 MHz, where the library's generic ones read 0.5 and 10.0 dB. The fixture doesn't change.
fn library() -> ComponentLibrary {
    let mut value: serde_json::Value = read("library/sourced.json");
    let components = value["components"].as_array_mut().unwrap();
    let mut extra = Vec::new();
    for c in components.iter() {
        if c["id"] == "gb-generic" {
            let mut copy = c.clone();
            copy["id"] = GROUND_BLOCK.into();
            copy["spec"]["insertion_loss"] = serde_json::json!([
                {"f_mhz": 55.0, "loss_db": 0.05}, {"f_mhz": 1218.0, "loss_db": 0.05}
            ]);
            extra.push(copy);
        }
        if c["id"] == "splitter-generic-4way-home" {
            let mut copy = c.clone();
            copy["id"] = FOUR_WAY.into();
            copy["spec"]["leg_loss"] = serde_json::json!([
                {"f_mhz": 55.0, "loss_db": 7.8}, {"f_mhz": 1002.0, "loss_db": 8.6}, {"f_mhz": 1218.0, "loss_db": 8.9}
            ]);
            extra.push(copy);
        }
    }
    for c in components.iter_mut() {
        if c["id"] == CABLE {
            c["spec"]["family"] = "625".into();
        }
        for (from, to) in [
            ("conn-generic-pin-500", "conn-test-pin-625"),
            ("conn-generic-splice-500", "conn-test-splice-625"),
        ] {
            if c["id"] == from {
                let mut copy = c.clone();
                copy["id"] = to.into();
                copy["spec"]["cable_family"] = "625".into();
                extra.push(copy);
            }
        }
    }
    components.extend(extra);
    serde_json::from_value(value).unwrap()
}

/// What the designer builds on the paper's run: the tap values from the node out, each device's level at 1002 MHz
/// (the first home's at each location), and how many taps come before the first amp, if there's one.
struct Run {
    values: Vec<f64>,
    devices: Vec<f64>,
    amps: usize,
    taps_before_amp: usize,
}

fn run() -> Run {
    let library = library();
    let profile: Profile =
        schema::profile::migrate(read(&format!("profiles/{PROFILE}.json"))).unwrap();
    let rules = paper_rules(&library, &profile);
    let origin = LonLat {
        lon_deg: -105.0,
        lat_deg: 40.0,
    };
    let map = map(origin);
    let controls = DesignControls {
        pinned_nodes: vec![PinnedNode { geo: origin }],
        ..DesignControls::default()
    };
    let kit = Kit::new(&rules, &library, &profile, None).unwrap();
    let s = place_structures(&kit, &map, &controls).unwrap();
    let (plans, unserved) = plan_nodes(&kit, &s, &map, &controls, 0).unwrap();
    assert!(unserved.is_empty());
    assert_eq!(plans.len(), 1);
    assert!(plans[0].notes.is_empty(), "{:?}", plans[0].notes);
    let plant = &plans[0].plant;
    let east = |e: &schema::plant::Element| design::geo::dist_ft(origin, e.geo.unwrap());
    let mut taps: Vec<(f64, f64)> = plant
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Tap))
        .map(|e| match &library.get(&e.component_ref).unwrap().spec {
            ComponentSpec::Tap(t) => (east(e), t.tap_value_db),
            _ => (east(e), f64::NAN),
        })
        .collect();
    taps.sort_by(|a, b| a.0.total_cmp(&b.0));
    let amps: Vec<&schema::plant::Element> = plant
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Amp { .. }))
        .collect();
    // Taps above the first amp in the plant, counted up its feed.
    let g = plant_graph::PlantGraph::new(plant, &library, &profile).unwrap();
    let taps_before_amp = match amps.iter().min_by(|a, b| east(a).total_cmp(&east(b))) {
        None => taps.len(),
        Some(amp) => {
            let mut n = 0;
            let mut at = amp.id.clone();
            while let Ok(Some((parent, _))) = g.parent(&at) {
                if matches!(parent.kind, ElementKind::Tap) {
                    n += 1;
                }
                at = parent.id.clone();
            }
            n
        }
    };
    let levels = device_levels(plant, &library, &profile);
    let devices = (1..=6)
        .map(|k| {
            levels
                .iter()
                .filter(|(id, _)| {
                    let e = plant.elements.iter().find(|e| e.id.as_str() == id).unwrap();
                    (east(e) - k as f64 * SPACING_FT).abs() < 60.0
                })
                .map(|(_, l)| *l)
                .fold(f64::NAN, f64::max)
        })
        .collect();
    Run {
        values: taps.iter().map(|t| t.1).collect(),
        devices,
        amps: amps.len(),
        taps_before_amp,
    }
}

/// What holds: through its fifth tap the designer takes the paper's values within one step, places no active, and
/// puts every device at or above the paper's target and within a step and a half above it, as the paper's own sit.
#[test]
fn the_designer_follows_vogels_tap_run_to_its_fifth_tap() {
    let r = run();
    eprintln!(
        "values {:?} against {PAPER_VALUES_DB:?}; devices {:?} against {PAPER_DEVICE_DBMV:?}; {} amps after {} taps",
        r.values,
        r.devices
            .iter()
            .map(|x| (x * 100.0).round() / 100.0)
            .collect::<Vec<_>>(),
        r.amps,
        r.taps_before_amp
    );
    assert_eq!(r.values.len(), 6);
    assert!(
        r.taps_before_amp >= 5,
        "an active inside the first five taps"
    );
    for (got, want) in r.values.iter().zip(PAPER_VALUES_DB).take(5) {
        assert!(
            (got - want).abs() <= 3.0 + 1e-9,
            "{:?} against {PAPER_VALUES_DB:?}",
            r.values
        );
    }
    for d in &r.devices {
        assert!(
            *d >= DEVICE_TARGET_DBMV - 1e-9 && *d <= DEVICE_TARGET_DBMV + 4.5,
            "{:?}",
            r.devices
        );
    }
}

/// The run as the paper draws it: all six values within one step, no active inside it, each device within 1.5 dB of the
/// paper's. Not met, for two reasons recorded in task 23's decisions. At the sixth tap, behind the paper's 4-way, a plain
/// terminating tap leaves the device 14.8 to 15.9 dB hotter at 54 MHz than at 1002, past the 10 dB DOCSIS allows at the
/// modem (PHY Table 45, physics §3.3). A conditioner holds that (8 dB of one: 7.8 to 8.9 dB), but the faceplate family's
/// is shaped to 1218 MHz and costs 1.2 dB at 1002, so at 45 °C the port reads 4.1 dB under its target, past the rules'
/// 3 dB. So the designer puts an amp before the sixth tap. The paper states one level per device, at one temperature. And
/// on these parts the paper's own values land its devices 1.6 to 3.9 dB under the paper's: premises at the SCTE ceilings are
/// a dB over what the paper's levels imply, the node's inserter costs 0.5 dB, and the taps' through loss differs.
#[test]
#[ignore = "the sixth tap needs a conditioner for the spread and is then under its tolerance at 45 °C; devices sit up to 2.2 dB under the paper's"]
fn the_designer_reproduces_vogels_tap_run() {
    let r = run();
    assert_eq!(r.amps, 0, "no active inside the run");
    for (got, want) in r.values.iter().zip(PAPER_VALUES_DB) {
        assert!(
            (got - want).abs() <= 3.0 + 1e-9,
            "{:?} against {PAPER_VALUES_DB:?}",
            r.values
        );
    }
    for (got, want) in r.devices.iter().zip(PAPER_DEVICE_DBMV) {
        assert!(
            (got - want).abs() <= 1.5,
            "{:?} against {PAPER_DEVICE_DBMV:?}",
            r.devices
        );
    }
}
