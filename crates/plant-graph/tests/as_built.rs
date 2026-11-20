//! Version 2's queries on `fixtures/one_street_as_built/`, against answers listed by hand from the fixture
//! (`docs/tasks/21-as-built-schemas.md`, "Readers"): the home a device is in, an edge's fittings, a link's hops, the
//! links that cross a cable, and whether every node has a link each way. Each holds under shuffled lists. All synthetic.

use std::fs;

use plant_graph::{FiberGraph, PlantGraph, RegionIssue, check_region};
use schema::common::{EdgeId, ElementId, FiberCableId, LinkId};
use schema::component::{ComponentLibrary, OpticalDirection};
use schema::fiber::{FiberPlant, Hop};
use schema::plant::Plant;
use schema::profile::Profile;
use schema::region::Region;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn plants() -> Vec<(Plant, Profile)> {
    vec![
        (
            read("one_street_as_built/plant_lowsplit_iccap.json"),
            read("profiles/d31_lowsplit_1002_iccap.json"),
        ),
        (
            read("one_street_as_built/plant_highsplit_rpd.json"),
            read("profiles/d31_highsplit_1218_rpd.json"),
        ),
    ]
}

fn library() -> ComponentLibrary {
    read("library/generic.json")
}

fn ids<T>(items: &[&T], id: impl Fn(&T) -> String) -> Vec<String> {
    items.iter().map(|t| id(t)).collect()
}

/// Every answer this file checks, as one string, so a shuffled copy can be compared whole.
fn answers(graph: &PlantGraph<'_>) -> String {
    let mut out = String::new();
    for home in graph.premises().unwrap() {
        out += &format!(
            "{} {} {:?} {:?} {:?} {:?}\n",
            home.drop.id,
            home.tap.id,
            home.ground_block.map(|g| g.id.to_string()),
            ids(&home.elements, |e| e.id.to_string()),
            ids(&home.devices, |e| e.id.to_string()),
            ids(&home.house_edges, |e| e.id.to_string()),
        );
    }
    let mut edges: Vec<_> = graph.plant().edges.iter().collect();
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    for edge in edges {
        let fittings: Vec<String> = graph
            .fittings(&edge.id)
            .unwrap()
            .iter()
            .map(|f| format!("{}@{}", f.component_ref, f.pos_ft))
            .collect();
        out += &format!("{} {fittings:?}\n", edge.id);
    }
    out += &format!("{:?}\n", graph.check_design());
    out
}

#[test]
fn the_home_a_device_is_in() {
    let library = library();
    for (plant, profile) in plants() {
        let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
        let home = graph
            .premises_of(&ElementId::new("stb-2"))
            .unwrap()
            .unwrap();
        assert_eq!(home.drop.id.as_str(), "drop-2");
        assert_eq!(home.tap.id.as_str(), "tap-1");
        assert_eq!(home.ground_block.unwrap().id.as_str(), "gb-2");
        assert_eq!(
            ids(&home.elements, |e| e.id.to_string()),
            ["gb-2", "hsp-2", "modem-2", "stb-2"]
        );
        assert_eq!(
            ids(&home.devices, |e| e.id.to_string()),
            ["modem-2", "stb-2"]
        );
        assert_eq!(
            ids(&home.house_edges, |e| e.id.to_string()),
            ["house-2a", "house-2b", "house-2c"]
        );

        // The same home from any part of it, and none for outside plant.
        let from_splitter = graph
            .premises_of(&ElementId::new("hsp-2"))
            .unwrap()
            .unwrap();
        assert_eq!(from_splitter.drop.id.as_str(), "drop-2");
        assert!(
            graph
                .premises_of(&ElementId::new("tap-1"))
                .unwrap()
                .is_none()
        );
        assert!(graph.premises_of(&graph.root().id).unwrap().is_none());

        let bare = graph
            .premises_of(&ElementId::new("modem-1"))
            .unwrap()
            .unwrap();
        assert!(bare.ground_block.is_none());
        assert_eq!(ids(&bare.elements, |e| e.id.to_string()), ["modem-1"]);

        let filtered = graph
            .premises_of(&ElementId::new("modem-4"))
            .unwrap()
            .unwrap();
        assert_eq!(
            ids(&filtered.elements, |e| e.id.to_string()),
            ["filter-4", "gb-4", "modem-4"]
        );
        assert_eq!(
            ids(&filtered.house_edges, |e| e.id.to_string()),
            ["house-4"]
        );

        let homes = graph.premises().unwrap();
        assert_eq!(
            homes
                .iter()
                .map(|h| h.drop.id.to_string())
                .collect::<Vec<_>>(),
            [
                "drop-1", "drop-2", "drop-3", "drop-4", "drop-5", "drop-6", "drop-7", "drop-8"
            ]
        );
        let devices: usize = homes.iter().map(|h| h.devices.len()).sum();
        assert_eq!(devices, 10, "nine modems and a set-top");
        let pair = graph
            .premises_of(&ElementId::new("modem-8b"))
            .unwrap()
            .unwrap();
        assert_eq!(
            ids(&pair.devices, |e| e.id.to_string()),
            ["modem-8a", "modem-8b"]
        );
        assert!(graph.premises_of(&ElementId::new("nothing")).is_err());
    }
}

#[test]
fn an_edges_fittings() {
    let library = library();
    for (plant, profile) in plants() {
        let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
        let at = |edge: &str| -> Vec<String> {
            graph
                .fittings(&EdgeId::new(edge))
                .unwrap()
                .iter()
                .map(|f| format!("{}@{}", f.component_ref, f.pos_ft))
                .collect()
        };
        assert_eq!(
            at("span-2"),
            [
                "conn-generic-pin-750@0",
                "conn-generic-splice-750@300",
                "conn-generic-pin-750@600"
            ]
        );
        assert_eq!(at("drop-5"), ["conn-generic-f@0", "conn-generic-f@300"]);
        assert_eq!(at("house-3b"), ["conn-generic-f@0", "conn-generic-f@20"]);
        assert!(at("direct-1").is_empty());
        assert!(graph.fittings(&EdgeId::new("nothing")).is_err());
        // Nothing here is wrong as a design either: every part qualifies, power reaches every active.
        assert!(
            graph.check_design().is_empty(),
            "{:?}",
            graph.check_design()
        );
        // Power enters at the inserter, and reaches the root and both amps.
        let power = graph.power();
        assert_eq!(power.feeds.len(), 1);
        let actives: Vec<String> = power.feeds[0]
            .actives
            .iter()
            .map(ToString::to_string)
            .collect();
        let mut expected = vec!["amp-1", "amp-2", graph.root().id.as_str()];
        expected.sort();
        assert_eq!(actives, expected);
    }
}

#[test]
fn links_hops_and_what_a_cut_reaches() {
    let library = library();
    let fiber: FiberPlant = read("one_street_as_built/fiber.json");
    let graph = FiberGraph::new(&fiber, &library).unwrap();
    let names = |links: Vec<&schema::fiber::OpticalLink>| -> Vec<String> {
        links.iter().map(|l| l.id.to_string()).collect()
    };
    let cut = |cable: &str| names(graph.links_on_cable(&FiberCableId::new(cable)).unwrap());
    assert_eq!(
        cut("fiber-1"),
        [
            "link-high-fwd",
            "link-high-ret",
            "link-low-fwd",
            "link-low-ret"
        ]
    );
    assert_eq!(cut("fiber-2"), ["link-low-fwd", "link-low-ret"]);
    assert_eq!(cut("fiber-3"), ["link-high-fwd", "link-high-ret"]);
    assert_eq!(cut("jumper-3"), ["link-high-fwd", "link-high-ret"]);
    assert!(graph.links_on_cable(&FiberCableId::new("nothing")).is_err());

    let hops = graph.hops(&LinkId::new("link-low-ret")).unwrap();
    let text: Vec<String> = hops
        .iter()
        .map(|h| match h {
            Hop::Fiber { cable, strand } => format!("fiber {cable}/{strand}"),
            Hop::Splice { at, .. } => format!("splice {at}"),
            Hop::Connector { at, .. } => format!("connector {at}"),
            Hop::Through { element, .. } => format!("through {element}"),
        })
        .collect();
    assert_eq!(
        text,
        [
            "connector node-end-low",
            "fiber fiber-2/2",
            "splice closure-1",
            "fiber fiber-1/2",
            "connector panel-1",
            "fiber jumper-2/1",
            "connector rrx-1"
        ]
    );
    let low = schema::common::ServiceGroupId::new("sg-as-built-lowsplit");
    let directions: Vec<OpticalDirection> =
        graph.links_for(&low).iter().map(|l| l.direction).collect();
    assert_eq!(
        directions,
        [OpticalDirection::Forward, OpticalDirection::Return]
    );
}

#[test]
fn every_node_has_a_link_each_way() {
    let library = library();
    let fiber: FiberPlant = read("one_street_as_built/fiber.json");
    let region: Region = read("one_street_as_built/region.json");
    let all = plants();
    let plants: Vec<&Plant> = all.iter().map(|(p, _)| p).collect();
    let graph = FiberGraph::new(&fiber, &library).unwrap();
    assert!(check_region(&region, &graph, &plants).is_empty());

    // Without the high-split return link, that node has none; without its binding, no port; a region naming nothing.
    let mut broken = fiber.clone();
    broken.links.retain(|l| l.id.as_str() != "link-high-ret");
    broken
        .bindings
        .retain(|b| b.service_group.as_str() != "sg-as-built-highsplit");
    let graph = FiberGraph::new(&broken, &library).unwrap();
    let high = schema::common::ServiceGroupId::new("sg-as-built-highsplit");
    let mut unnamed = region.clone();
    unnamed.fiber_ref = None;
    assert_eq!(
        check_region(&unnamed, &graph, &plants),
        [
            RegionIssue::WrongFiberPlant {
                region: "region-as-built".into(),
                named: None,
                checked: "fiber-as-built".into()
            },
            RegionIssue::NoBinding(high.clone()),
            RegionIssue::NoReturnLink(high.clone()),
        ]
    );

    // A node end whose part isn't the plant's root.
    let mut swapped = fiber.clone();
    for element in &mut swapped.elements {
        if element.id.as_str() == "node-end-high" {
            element.component_ref = schema::common::ComponentId::new("rpd-generic-1218");
        }
    }
    let graph = FiberGraph::new(&swapped, &library).unwrap();
    assert_eq!(
        check_region(&region, &graph, &plants),
        [RegionIssue::NodeEndPart(high)]
    );
}

/// Canonical order: reversing and rotating every list leaves every answer the same (invariant 4).
#[test]
fn order_does_not_matter() {
    let library = library();
    for (plant, profile) in plants() {
        let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
        let expected = answers(&graph);
        let mut shuffled = plant.clone();
        shuffled.elements.reverse();
        shuffled.edges.rotate_left(7);
        shuffled.structures.reverse();
        for edge in &mut shuffled.edges {
            if let schema::plant::EdgeKind::Span { fittings, .. } = &mut edge.kind {
                fittings.reverse();
            }
        }
        let shuffled_graph = PlantGraph::new(&shuffled, &library, &profile).unwrap();
        assert_eq!(answers(&shuffled_graph), expected);
    }

    let fiber: FiberPlant = read("one_street_as_built/fiber.json");
    let summary = |f: &FiberPlant| {
        let graph = FiberGraph::new(f, &library).unwrap();
        let mut out = String::new();
        for cable in [
            "fiber-1", "fiber-2", "fiber-3", "jumper-1", "jumper-2", "jumper-3",
        ] {
            let links = graph.links_on_cable(&FiberCableId::new(cable)).unwrap();
            out += &format!(
                "{cable}: {:?}\n",
                links.iter().map(|l| l.id.to_string()).collect::<Vec<_>>()
            );
        }
        out += &format!("{:?} {}\n", f.parts_list(), f.splices());
        out
    };
    let expected = summary(&fiber);
    let mut shuffled = fiber.clone();
    shuffled.elements.reverse();
    shuffled.cables.rotate_left(2);
    shuffled.links.reverse();
    shuffled.structures.reverse();
    assert_eq!(summary(&shuffled), expected);
}
