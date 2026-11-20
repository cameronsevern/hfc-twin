//! Task 22's additions to `check_physics`, and §16's optical margin, on the as-built fixture. All synthetic.

use std::fs;

use plant_graph::PlantGraph;
use schema::common::{ComponentId, ElementId, Timestamp};
use schema::component::{ComponentLibrary, ComponentSpec};
use schema::environment::EnvironmentSample;
use schema::plant::Plant;
use schema::profile::Profile;
use sim::budget::{PhysicsIssue, check_physics};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn sample() -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(0),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        wind_m_per_s: None,
        solar_w_per_m2: None,
    }
}

fn issues(plant: &Plant, library: &ComponentLibrary, profile: &Profile) -> Vec<PhysicsIssue> {
    let graph = PlantGraph::new(plant, library, profile).unwrap();
    check_physics(&graph, &[sample()], &sim::Overlay::default()).unwrap()
}

fn new_kinds(found: &[PhysicsIssue]) -> Vec<&PhysicsIssue> {
    found
        .iter()
        .filter(|i| {
            matches!(
                i,
                PhysicsIssue::InserterPortOverloaded { .. }
                    | PhysicsIssue::PlugInOutsideFamily { .. }
                    | PhysicsIssue::FilterBlocksChannel { .. }
            )
        })
        .collect()
}

fn low() -> (Plant, Profile) {
    (
        read("one_street_as_built/plant_lowsplit_iccap.json"),
        read("profiles/d31_lowsplit_1002_iccap.json"),
    )
}

fn high() -> (Plant, Profile) {
    (
        read("one_street_as_built/plant_highsplit_rpd.json"),
        read("profiles/d31_highsplit_1218_rpd.json"),
    )
}

#[test]
fn the_fixture_as_built_raises_none_of_them() {
    let library: ComponentLibrary = read("library/generic.json");
    for (plant, profile) in [low(), high()] {
        assert_eq!(
            new_kinds(&issues(&plant, &library, &profile)),
            Vec::<&PhysicsIssue>::new()
        );
    }
    // `one_street` neither.
    let plant: Plant = read("one_street/plant_lowsplit_iccap.json");
    let profile: Profile = read("profiles/d31_lowsplit_1002_iccap.json");
    assert!(new_kinds(&issues(&plant, &library, &profile)).is_empty());
}

#[test]
fn a_misalignment_beyond_what_the_family_has() {
    use schema::fault::{Fault, FaultKind, FaultSet, Onset};
    let library: ComponentLibrary = read("library/generic.json");
    let (plant, profile) = low();
    let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
    let at = |pad_delta_db: f64| {
        let faults = FaultSet {
            schema_version: schema::fault::SCHEMA_VERSION.into(),
            seed: 1,
            faults: vec![Fault {
                onset: Onset::Instant,
                kind: FaultKind::Misalignment {
                    active_id: ElementId::new("amp-1"),
                    pad_delta_db,
                    eq_delta_db: 0.0,
                    return_pad_delta_db: 0.0,
                },
            }],
        };
        let overlay = sim::Overlay::build(&graph, &faults, &sample()).unwrap();
        let found = check_physics(&graph, &[sample()], &overlay).unwrap();
        new_kinds(&found).into_iter().cloned().collect::<Vec<_>>()
    };
    // `amp-1` has `pad-generic-10db`; the family's pads run 0 to 20 dB.
    assert!(at(8.0).is_empty());
    assert_eq!(
        at(12.0),
        [PhysicsIssue::PlugInOutsideFamily {
            element: ElementId::new("amp-1"),
            part: ComponentId::new("pad-generic-10db"),
            value_db: 22.0,
            lo_db: 0.0,
            hi_db: 20.0,
        }]
    );
    assert_eq!(at(-11.0).len(), 1);
}

#[test]
fn a_1002_mhz_filter_under_a_1218_mhz_plan() {
    // The sourced point-of-entry filter stops from 1125 MHz, which `qualifies` can't see from its rated band (task 21).
    let library: ComponentLibrary = read("library/sourced.json");
    let (mut plant, profile) = high();
    for element in &mut plant.elements {
        if element.id.as_str() == "filter-4" {
            element.component_ref = ComponentId::new("filter-antronix-glf1002");
        }
    }
    let found = issues(&plant, &library, &profile);
    let blocked: Vec<_> = new_kinds(&found)
        .into_iter()
        .filter_map(|i| match i {
            PhysicsIssue::FilterBlocksChannel {
                filter,
                device,
                lo_mhz,
                ..
            } => Some((filter.to_string(), device.to_string(), *lo_mhz)),
            _ => None,
        })
        .collect();
    assert_eq!(
        blocked,
        [("filter-4".to_string(), "modem-4".to_string(), 1027.0)]
    );
    // On the low-split plan it's harmless.
    let (mut plant, profile) = low();
    for element in &mut plant.elements {
        if element.id.as_str() == "filter-4" {
            element.component_ref = ComponentId::new("filter-antronix-glf1002");
        }
    }
    assert!(new_kinds(&issues(&plant, &library, &profile)).is_empty());
}

#[test]
fn an_inserter_port_over_its_rating() {
    let mut library: ComponentLibrary = read("library/generic.json");
    for part in &mut library.components {
        if let ComponentSpec::PowerInserter(spec) = &mut part.spec
            && part.id.as_str() == "pi-generic-1218"
        {
            spec.port_i_rated_a = Some(0.2);
        }
    }
    let (plant, profile) = low();
    let found = issues(&plant, &library, &profile);
    let over: Vec<_> = new_kinds(&found)
        .into_iter()
        .filter(|i| matches!(i, PhysicsIssue::InserterPortOverloaded { inserter, .. } if inserter.as_str() == "pi-1"))
        .collect();
    assert!(!over.is_empty());
    for issue in over {
        let PhysicsIssue::InserterPortOverloaded { i_a, rated_a, .. } = issue else {
            unreachable!();
        };
        assert!(*i_a > *rated_a);
    }
}

#[test]
fn transmit_power_through_the_premises() {
    // Home 2's modem and set-top share a 2-way splitter with 22 dB of isolation. Low split stays below the tuner band; high split
    // reaches it, so the modem's transmit at the set-top is read against TR-MIDSPLIT's 21 dBmV.
    let library: ComponentLibrary = read("library/generic.json");
    let tuner = |found: &[PhysicsIssue]| -> Vec<(String, String, f64)> {
        found
            .iter()
            .filter_map(|i| match i {
                PhysicsIssue::TransmitIntoTuner {
                    modem,
                    device,
                    level_dbmv,
                } => Some((modem.to_string(), device.to_string(), *level_dbmv)),
                _ => None,
            })
            .collect()
    };
    let (plant, profile) = low();
    assert!(tuner(&issues(&plant, &library, &profile)).is_empty());
    let (plant, profile) = high();
    let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
    let budget = sim::budget::level_budget(&graph, &sample(), &sim::Overlay::default()).unwrap();
    let tx = budget.modems[&ElementId::new("modem-2")].tx_used_max_dbmv_per_1p6mhz + 6.0206;
    let found = tuner(&issues(&plant, &library, &profile));
    // `modem-2` transmits up to 48.6 dBmV per 6.4 MHz on high split (its highest, at the band's top: approx, since at 54 MHz it
    // sends less). Less the splitter's 22 dB and two short house cables, about 25.5 dBmV reaches `stb-2`, over the 21.
    assert!((tx - 48.63).abs() < 0.01, "{tx}");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        (found[0].0.as_str(), found[0].1.as_str()),
        ("modem-2", "stb-2")
    );
    assert!(
        found[0].2 < tx - 22.0 && found[0].2 > tx - 23.5,
        "{}",
        found[0].2
    );
    // A splitter with more isolation keeps it out.
    let mut quiet = library.clone();
    for part in &mut quiet.components {
        if let ComponentSpec::Splitter(spec) = &mut part.spec
            && spec.leg_isolation_db.is_some()
        {
            spec.leg_isolation_db = Some(60.0);
        }
    }
    assert!(tuner(&issues(&plant, &quiet, &profile)).is_empty());
}

#[test]
fn optical_margin() {
    use sim::optics::{link_issues, node_link};
    let library: ComponentLibrary = read("library/generic.json");
    let fiber: schema::fiber::FiberPlant = read("one_street_as_built/fiber.json");
    for (plant, profile) in [low(), high()] {
        let link = node_link(&fiber, &plant, &library, &profile.channel_plan)
            .unwrap()
            .unwrap();
        assert!(link_issues(&link, &library, &profile.channel_plan).is_empty());
    }
    // Without the hub coupler, 10 dBm overloads the node's receiver, rated to +1 (§16).
    let mut direct = fiber.clone();
    for link in &mut direct.links {
        if link.id.as_str() == "link-low-fwd" {
            link.hops.retain(|h| {
                !matches!(h, schema::fiber::Hop::Through { .. })
                    && !matches!(h, schema::fiber::Hop::Connector { at, .. } if at.as_str() == "osplit-1")
            });
        }
    }
    let (plant, profile) = low();
    // The coupler's two jumpers stay in the path as if spliced through; only the budget matters here.
    let link = node_link(&direct, &plant, &library, &profile.channel_plan);
    if let Ok(Some(link)) = link {
        let found = link_issues(&link, &library, &profile.channel_plan);
        assert!(
            found
                .iter()
                .any(|i| matches!(i, PhysicsIssue::OpticalInputOutOfRange { lo_dbm, hi_dbm, .. } if *lo_dbm == -8.0 && *hi_dbm == 1.0)),
            "{found:?}"
        );
    }
    // A return laser told its reference is far hotter than its clip.
    let mut hot = library.clone();
    for part in &mut hot.components {
        if let ComponentSpec::OpticalTx(tx) = &mut part.spec
            && part.id.as_str() == "rtx-generic-1310"
        {
            tx.ref_rf_in_dbmv = 20.0;
        }
    }
    let link = node_link(&fiber, &plant, &hot, &profile.channel_plan)
        .unwrap()
        .unwrap();
    let found = link_issues(&link, &hot, &profile.channel_plan);
    assert!(
        found.iter().any(
            |i| matches!(i, PhysicsIssue::LaserOverdriven { clip_dbmv, .. } if *clip_dbmv == 22.6)
        ),
        "{found:?}"
    );
}
