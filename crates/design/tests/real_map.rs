//! Real streets, and the importer's rules on them. The geography in `real_broomfield_co.json` is real; every plant here is invented
//! (invariant 11).

use std::collections::BTreeSet;
use std::fs;

use design::fiber::fiber_route;
use design::geo::offset;
use design::osm::{OsmReport, read_overpass};
use design::{DesignParts, design_region};
use schema::common::{LonLat, Origin, ServiceGroupId};
use schema::component::ComponentLibrary;
use schema::plant::Installation;
use schema::profile::Profile;
use schema::region::{DesignControls, Home, MapSource, SCHEMA_VERSION, Street, StreetMap};

fn fixture(path: &str) -> String {
    fs::read_to_string(format!(
        "{}/../../fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    serde_json::from_str(&fixture(path)).unwrap()
}

fn library() -> ComponentLibrary {
    read("library/generic.json")
}

fn profiles() -> [Profile; 2] {
    [
        read("profiles/d31_lowsplit_1002_iccap.json"),
        read("profiles/d31_highsplit_1218_rpd.json"),
    ]
}

const ORIGIN: LonLat = LonLat {
    lon_deg: -105.0,
    lat_deg: 39.9,
};

fn hand_map(streets: Vec<Vec<LonLat>>, homes: Vec<LonLat>) -> StreetMap {
    StreetMap {
        schema_version: SCHEMA_VERSION.to_string(),
        id: "hand".to_string(),
        source: MapSource::Generated {
            family: schema::region::LayoutFamily::Suburban,
            seed: 0,
        },
        streets: streets
            .into_iter()
            .enumerate()
            .map(|(i, points)| Street {
                id: format!("s{i}"),
                points,
                installation: Installation::Aerial,
            })
            .collect(),
        homes: homes
            .into_iter()
            .enumerate()
            .map(|(i, geo)| Home {
                id: format!("h{i}"),
                geo,
            })
            .collect(),
        headend: ORIGIN,
    }
}

#[test]
fn each_import_rule_on_a_hand_written_extract() {
    let (map, report) = read_overpass(
        "rules",
        &fixture("osm/rules_extract.json"),
        Installation::Underground,
    )
    .unwrap();

    let streets: BTreeSet<&str> = map.streets.iter().map(|s| s.id.as_str()).collect();
    // A link and a parking aisle stay. A driveway, a footway and a motorway don't. Way 106 starts 140 ft from way 100's end, so a
    // crossing joins them, and way 107 is over 1,100 ft from anything, so it stays apart.
    assert_eq!(
        streets,
        BTreeSet::from([
            "way-100",
            "way-101",
            "way-102",
            "way-106",
            "way-107",
            "crossing-0"
        ])
    );
    assert!(
        map.streets
            .iter()
            .all(|s| s.installation == Installation::Underground)
    );

    let homes: BTreeSet<&str> = map.homes.iter().map(|h| h.id.as_str()).collect();
    // The house and its address inside it are one home. A shed and a bakery aren't homes. The cafe's address isn't either, and a
    // building with a unit count is that many.
    assert_eq!(
        homes,
        BTreeSet::from([
            "building-200",
            "building-202-0",
            "building-202-1",
            "building-202-2",
            "building-203",
            "address-41",
        ])
    );
    assert_eq!(
        report,
        OsmReport {
            buildings_not_homes: 2,
            apartments_counted_as_one: 1,
            addresses_in_buildings: 1,
            service_roads_skipped: 1,
            crossings_added: 1,
        }
    );

    // The ring's repeated first corner doesn't pull the centre.
    let house = map.homes.iter().find(|h| h.id == "building-200").unwrap();
    assert!((house.geo.lat_deg - 39.9002).abs() < 1e-9);
    assert!((house.geo.lon_deg + 105.099).abs() < 1e-9);
}

#[test]
fn a_real_extract_designs_cleanly_on_both_profiles() {
    let (map, report) = read_overpass(
        "real",
        &fixture("osm/real_broomfield_co.json"),
        Installation::Aerial,
    )
    .unwrap();
    assert!(matches!(map.source, MapSource::OpenStreetMap { .. }));
    assert_eq!(map.homes.len(), 69);
    assert!(report.apartments_counted_as_one > 0);
    let library = library();
    for profile in profiles() {
        for homes_per_node in [30, 60, 120] {
            let controls = DesignControls {
                homes_per_node,
                ..DesignControls::default()
            };
            let design =
                design_region(&map, &profile, &library, &controls, &DesignParts::default())
                    .unwrap();
            assert_eq!(design.region.origin, Origin::Synthetic);
            let passed: u32 = design.nodes.iter().map(|n| n.homes_passed).sum();
            assert_eq!(passed as usize + design.unserved.len(), map.homes.len());
            for node in &design.nodes {
                assert!(
                    node.topology_issues.is_empty() && node.physics_issues.is_empty(),
                    "{} at {homes_per_node}: {:?} {:?}",
                    profile.id,
                    node.topology_issues,
                    node.physics_issues
                );
                assert_eq!(node.plant.origin, Origin::Synthetic);
            }
        }
    }
}

#[test]
fn the_same_extract_gives_the_same_bytes() {
    let run = || {
        let (map, _) = read_overpass(
            "real",
            &fixture("osm/real_broomfield_co.json"),
            Installation::Aerial,
        )
        .unwrap();
        let [profile, _] = profiles();
        serde_json::to_string(
            &design_region(
                &map,
                &profile,
                &library(),
                &DesignControls::default(),
                &DesignParts::default(),
            )
            .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(run(), run());
}

#[test]
fn a_home_past_drop_reach_is_unserved_rather_than_given_a_long_drop() {
    let end = offset(ORIGIN, 2000.0, 0.0);
    let map = hand_map(
        vec![vec![ORIGIN, end]],
        vec![offset(ORIGIN, 1000.0, 100.0), offset(ORIGIN, 1000.0, 400.0)],
    );
    let [profile, _] = profiles();
    let design = design_region(
        &map,
        &profile,
        &library(),
        &DesignControls::default(),
        &DesignParts::default(),
    )
    .unwrap();
    assert_eq!(design.unserved, vec!["h1".to_string()]);
    assert!(design.nodes.iter().all(|n| n.topology_issues.is_empty()));
}

/// §3.2: no amp's input may fall below what it can take. On this extract, a tap leaves the signal under an amp's threshold just ahead of a
/// split, which generated streets never do, and the amps behind the split then read about 1 dB short.
#[test]
fn amps_go_in_ahead_of_a_split_on_a_low_signal() {
    let (map, _) = read_overpass(
        "real",
        &fixture("osm/real_broomfield_large_co.json"),
        Installation::Aerial,
    )
    .unwrap();
    assert_eq!(map.homes.len(), 313);
    let library = library();
    for profile in profiles() {
        for homes_per_node in [60, 120, 250] {
            let controls = DesignControls {
                homes_per_node,
                ..DesignControls::default()
            };
            let design =
                design_region(&map, &profile, &library, &controls, &DesignParts::default())
                    .unwrap();
            for node in &design.nodes {
                assert!(
                    node.physics_issues.is_empty() && node.topology_issues.is_empty(),
                    "{} at {homes_per_node}: {:?} {:?}",
                    profile.id,
                    node.physics_issues,
                    node.topology_issues,
                );
            }
        }
    }
}

#[test]
fn fiber_follows_streets_from_the_headend() {
    let mid = offset(ORIGIN, 500.0, 0.0);
    let end = offset(ORIGIN, 1000.0, 0.0);
    let branch = offset(mid, 0.0, 300.0);
    let far = offset(ORIGIN, 50_000.0, 0.0);
    let far_end = offset(far, 500.0, 0.0);
    let mut map = hand_map(
        vec![
            vec![ORIGIN, mid, end],
            vec![mid, branch],
            vec![far, far_end],
        ],
        vec![],
    );
    let a = ServiceGroupId::new("a");
    let b = ServiceGroupId::new("b");

    // By hand: 500 ft to the branch point, 500 ft on to the end, 300 ft up the branch. Shared runs count once.
    let route = fiber_route(&map, &[(a.clone(), end), (b.clone(), branch)]);
    assert!((route.route_ft - 1300.0).abs() < 5.0, "{}", route.route_ft);
    assert_eq!(route.lines.len(), 3);
    assert!(route.unreached.is_empty());

    // Moved to the branch point, the first 500 ft isn't needed to reach the far end.
    map.headend = mid;
    let moved = fiber_route(&map, &[(a.clone(), end)]);
    assert!((moved.route_ft - 500.0).abs() < 5.0, "{}", moved.route_ft);

    // A node on a piece of street the headend's piece doesn't touch.
    let apart = fiber_route(&map, &[(a.clone(), end), (b.clone(), far_end)]);
    assert_eq!(apart.unreached, vec![b]);
}
