//! Properties over randomly generated valid trees, from fixed seeds. Lengths are checked against a
//! separate walk that ignores edge direction, so a bug in the path code can't hide in both.

mod common;

use std::collections::BTreeMap;

use common::{id, load};
use plant_graph::{Path, PathDirection, PlantGraph, Segment};
use schema::common::{ComponentId, EdgeId, ElementId, Origin, PortId, ServiceGroupId};
use schema::component::ComponentLibrary;
use schema::fault::Location;
use schema::plant::{
    Edge, EdgeKind, Element, ElementKind, Installation, Plant, Port, PortRef, PortRole,
    RecordStatus,
};
use schema::profile::Profile;

const SEEDS: [u64; 8] = [1, 2, 3, 5, 8, 13, 21, 34];

/// xorshift64, so this needs no new dependency. All randomness is seeded (invariant 4).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn feet(&mut self) -> f64 {
        10.0 * (1 + self.below(50)) as f64
    }
}

fn new_element(
    id: String,
    kind: ElementKind,
    component: &str,
    ports: &[(&str, PortRole)],
) -> Element {
    Element {
        id: ElementId::new(id),
        kind,
        component_ref: ComponentId::new(component),
        geo: None,
        ports: ports
            .iter()
            .map(|(p, role)| Port {
                id: PortId::new(*p),
                role: *role,
            })
            .collect(),
        record_status: RecordStatus::Designed,
        fitted: None,
        structure: None,
    }
}

fn random_plant(seed: u64, profile: &Profile) -> Plant {
    use PortRole::*;
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let root = new_element(
        "root".into(),
        ElementKind::Node {
            return_atten_db: 0.0,
        },
        "node-generic-1002",
        &[("rf-1", Output), ("rf-2", Output)],
    );
    let mut open: Vec<(ElementId, Port)> = root
        .ports
        .iter()
        .map(|p| (root.id.clone(), p.clone()))
        .collect();
    let mut elements = vec![root];
    let mut edges = Vec::new();

    for n in 0..40 {
        if open.is_empty() {
            break;
        }
        let at = rng.below(open.len());
        let (from_element, from_port) = open.swap_remove(at);
        let name = format!("x-{n}");
        let element = if from_port.role == TapPort {
            new_element(
                format!("modem-{n}"),
                ElementKind::Modem,
                "modem-generic-d31",
                &[("rf", Input)],
            )
        } else {
            match rng.below(6) {
                0 => new_element(
                    name,
                    ElementKind::Amp {
                        return_atten_db: 0.0,
                    },
                    "amp-generic-1218",
                    &[("in", Input), ("out", Output)],
                ),
                1 | 2 => new_element(
                    name,
                    ElementKind::Tap,
                    "tap-generic-23db-4p",
                    &[
                        ("in", Input),
                        ("through", Through),
                        ("tap-1", TapPort),
                        ("tap-2", TapPort),
                        ("tap-3", TapPort),
                        ("tap-4", TapPort),
                    ],
                ),
                3 => new_element(
                    name,
                    ElementKind::Splitter,
                    "splitter-generic-2way",
                    &[("in", Input), ("leg-1", Leg), ("leg-2", Leg)],
                ),
                4 => new_element(
                    name,
                    ElementKind::DirectionalCoupler,
                    "dc-generic-8db",
                    &[("in", Input), ("through", Through), ("coupled", Coupled)],
                ),
                _ => new_element(
                    name,
                    ElementKind::Terminator,
                    "term-generic",
                    &[("in", Input)],
                ),
            }
        };
        let kind = match element.kind {
            ElementKind::Modem => EdgeKind::Drop {
                cable_ref: ComponentId::new("drop-rg6"),
                len_ft: rng.feet(),
                fittings: Vec::new(),
                installation: None,
                path: Vec::new(),
            },
            ElementKind::Terminator => EdgeKind::Direct,
            _ => EdgeKind::Span {
                cable_ref: ComponentId::new("cable-500-p3"),
                len_ft: rng.feet(),
                installation: Installation::Aerial,
                power_passing: true,
                fittings: Vec::new(),
                path: Vec::new(),
                support: None,
            },
        };
        edges.push(Edge {
            id: EdgeId::new(format!("edge-{n}")),
            from: PortRef {
                element_id: from_element,
                port: from_port.id,
            },
            to: PortRef {
                element_id: element.id.clone(),
                port: PortId::new(in_port(&element)),
            },
            kind,
            record_status: None,
        });
        for port in element.ports.iter().filter(|p| p.role != Input) {
            open.push((element.id.clone(), port.clone()));
        }
        elements.push(element);
    }
    Plant {
        schema_version: schema::plant::SCHEMA_VERSION.into(),
        service_group: ServiceGroupId::new("sg-random"),
        profile_ref: profile.id.clone(),
        origin: Origin::Synthetic,
        elements,
        edges,
        power_supplies: Vec::new(),
        modem_bindings: Vec::new(),
        structures: Vec::new(),
    }
}

fn in_port(element: &Element) -> &str {
    element
        .ports
        .iter()
        .find(|p| p.role == PortRole::Input)
        .unwrap()
        .id
        .as_str()
}

fn inputs() -> (Profile, ComponentLibrary) {
    (
        load("profiles/d31_lowsplit_1002_iccap.json"),
        load("library/generic.json"),
    )
}

/// Every port, and a point on every cabled edge.
fn locations(plant: &Plant, rng: &mut Rng) -> Vec<Location> {
    let mut locations = Vec::new();
    for element in &plant.elements {
        for port in &element.ports {
            locations.push(Location::Port {
                element_id: element.id.clone(),
                port: port.id.clone(),
            });
        }
    }
    for edge in &plant.edges {
        if let EdgeKind::Span { len_ft, .. } | EdgeKind::Drop { len_ft, .. } = edge.kind {
            let pos_ft = len_ft * rng.below(11) as f64 / 10.0;
            locations.push(Location::Span {
                span_id: edge.id.clone(),
                pos_ft,
            });
        }
    }
    locations
}

fn sample_pairs(locations: &[Location], rng: &mut Rng) -> Vec<(Location, Location)> {
    let pick = |rng: &mut Rng| locations[rng.below(locations.len())].clone();
    (0..300).map(|_| (pick(rng), pick(rng))).collect()
}

/// Cable length from an element to every other, walking the edges as undirected.
fn distances(plant: &Plant, start: &ElementId) -> BTreeMap<ElementId, f64> {
    let mut found = BTreeMap::from([(start.clone(), 0.0)]);
    let mut stack = vec![start.clone()];
    while let Some(here) = stack.pop() {
        let at = found[&here];
        for edge in &plant.edges {
            let len_ft = edge.kind.cable().map_or(0.0, |(_, len_ft)| len_ft);
            for (a, b) in [
                (&edge.from.element_id, &edge.to.element_id),
                (&edge.to.element_id, &edge.from.element_id),
            ] {
                if a == &here && !found.contains_key(b) {
                    found.insert(b.clone(), at + len_ft);
                    stack.push(b.clone());
                }
            }
        }
    }
    found
}

/// The elements a location can step off to, each with the cable to get there.
fn exits(plant: &Plant, location: &Location) -> Vec<(ElementId, f64)> {
    match location {
        Location::Port { element_id, .. } => vec![(element_id.clone(), 0.0)],
        Location::Span { span_id, pos_ft } => {
            let edge = plant.edges.iter().find(|e| &e.id == span_id).unwrap();
            let (EdgeKind::Span { len_ft, .. } | EdgeKind::Drop { len_ft, .. }) = edge.kind else {
                panic!("no cable");
            };
            vec![
                (edge.from.element_id.clone(), *pos_ft),
                (edge.to.element_id.clone(), len_ft - pos_ft),
            ]
        }
    }
}

/// The shortest way between two locations, which in a tree is the only way.
fn expected_len_ft(plant: &Plant, a: &Location, b: &Location) -> f64 {
    if let (
        Location::Span {
            span_id: edge_a,
            pos_ft: pos_a,
        },
        Location::Span {
            span_id: edge_b,
            pos_ft: pos_b,
        },
    ) = (a, b)
        && edge_a == edge_b
    {
        return (pos_a - pos_b).abs();
    }
    let mut best = f64::INFINITY;
    for (from, to_exit) in exits(plant, a) {
        let distance = distances(plant, &from);
        for (to, from_exit) in exits(plant, b) {
            best = best.min(to_exit + distance[&to] + from_exit);
        }
    }
    best
}

type Place = (ElementId, PortId);

/// Where a segment starts and ends, as ports.
fn ends(segment: &Segment<'_>) -> (Place, Place) {
    let place = |end: &PortRef| (end.element_id.clone(), end.port.clone());
    match segment {
        Segment::Cable {
            edge, direction, ..
        }
        | Segment::Direct { edge, direction } => match direction {
            PathDirection::Downstream => (place(&edge.from), place(&edge.to)),
            PathDirection::Upstream => (place(&edge.to), place(&edge.from)),
        },
        Segment::Device {
            element,
            in_port,
            out_port,
        } => (
            (element.id.clone(), in_port.id.clone()),
            (element.id.clone(), out_port.id.clone()),
        ),
    }
}

fn assert_connected(path: &Path<'_>, a: &Location, b: &Location) {
    for pair in path.segments.windows(2) {
        assert_eq!(
            ends(&pair[0]).1,
            ends(&pair[1]).0,
            "{a:?} to {b:?}: {path:?}"
        );
        let both_devices = matches!(pair, [Segment::Device { .. }, Segment::Device { .. }]);
        assert!(!both_devices, "{a:?} to {b:?}: {path:?}");
    }
    let as_place = |location: &Location| match location {
        Location::Port { element_id, port } => Some((element_id.clone(), port.clone())),
        Location::Span { .. } => None,
    };
    if let (Some(first), Some(place)) = (path.segments.first(), as_place(a)) {
        assert_eq!(ends(first).0, place, "{a:?} to {b:?}: {path:?}");
    }
    if let (Some(last), Some(place)) = (path.segments.last(), as_place(b)) {
        assert_eq!(ends(last).1, place, "{a:?} to {b:?}: {path:?}");
    }
}

#[test]
fn generated_plants_are_valid_and_varied() {
    let (profile, library) = inputs();
    for seed in SEEDS {
        let plant = random_plant(seed, &profile);
        let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
        assert!(plant.elements.len() > 10, "seed {seed}");
        assert!(!graph.modems().is_empty(), "seed {seed}");
    }
}

#[test]
fn paths_are_reversible_connected_and_the_right_length() {
    let (profile, library) = inputs();
    for seed in SEEDS {
        let plant = random_plant(seed, &profile);
        let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
        let mut rng = Rng(seed | 1);
        let locations = locations(&plant, &mut rng);
        for (a, b) in sample_pairs(&locations, &mut rng) {
            let forward = graph.path_between(&a, &b).unwrap();
            let backward = graph.path_between(&b, &a).unwrap();
            assert_eq!(forward, backward.reversed(), "{a:?} to {b:?}");
            assert_connected(&forward, &a, &b);
            let expected = expected_len_ft(&plant, &a, &b);
            assert!(
                (forward.cable_len_ft() - expected).abs() < 1e-9,
                "{a:?} to {b:?}: {} ft, expected {expected} ft",
                forward.cable_len_ft()
            );
        }
    }
}

#[test]
fn every_modem_is_below_its_ancestors_and_nothing_else() {
    let (profile, library) = inputs();
    for seed in SEEDS {
        let plant = random_plant(seed, &profile);
        let graph = PlantGraph::new(&plant, &library, &profile).unwrap();

        let mut under_root = Vec::new();
        for port in &graph.root().ports {
            let location = Location::Port {
                element_id: graph.root().id.clone(),
                port: port.id.clone(),
            };
            under_root.extend(graph.modems_below(&location).unwrap());
        }
        under_root.sort_by_key(|e| &e.id);
        assert_eq!(under_root, graph.modems(), "seed {seed}");

        for element in plant.elements.iter().filter(|e| e.id != graph.root().id) {
            let location = Location::Port {
                element_id: element.id.clone(),
                port: PortId::new(in_port(element)),
            };
            let below = graph.modems_below(&location).unwrap();
            for modem in graph.modems() {
                let line = graph.ancestors(&modem.id).unwrap();
                let above = modem.id == element.id || line.iter().any(|e| e.id == element.id);
                assert_eq!(
                    below.iter().any(|m| m.id == modem.id),
                    above,
                    "{} {}",
                    element.id,
                    modem.id
                );
            }
        }
    }
}

#[test]
fn root_port_is_where_the_path_from_root_starts() {
    let (profile, library) = inputs();
    for seed in SEEDS {
        let plant = random_plant(seed, &profile);
        let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
        for element in plant.elements.iter().filter(|e| e.id != graph.root().id) {
            let root_port = graph.root_port(&element.id).unwrap().unwrap();
            let path = graph.path_from_root(&element.id).unwrap();
            let (start, _) = ends(&path.segments[0]);
            assert_eq!(start, (graph.root().id.clone(), root_port.id.clone()));
        }
    }
}

/// Every query's answer, as text.
fn snapshot(graph: &PlantGraph<'_>, pairs: &[(Location, Location)]) -> String {
    let mut text = format!(
        "{:?}\n{:?}\n{:?}\n",
        graph.modems(),
        graph.amps(),
        graph.actives()
    );
    text += &format!("{:?}\n{:?}\n", graph.power(), graph.check_design());
    let mut ids: Vec<&ElementId> = graph.plant().elements.iter().map(|e| &e.id).collect();
    ids.sort();
    for element in ids {
        text += &format!(
            "{:?} {:?} {:?} {:?} {:?}\n",
            graph.parent(element),
            graph.children(element),
            graph.ancestors(element),
            graph.cascade(element),
            graph.root_port(element),
        );
        text += &format!("{:?}\n", graph.path_from_root(element));
    }
    for (a, b) in pairs {
        text += &format!("{:?} {:?}\n", graph.path_between(a, b), graph.below(a));
    }
    text
}

#[test]
fn file_order_changes_nothing() {
    let (profile, library) = inputs();
    for seed in SEEDS {
        let plant = random_plant(seed, &profile);
        let mut rng = Rng(seed | 1);
        let pairs = sample_pairs(&locations(&plant, &mut rng), &mut rng);
        let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
        let answer = snapshot(&graph, &pairs);

        let again = PlantGraph::new(&plant, &library, &profile).unwrap();
        assert_eq!(snapshot(&again, &pairs), answer, "seed {seed}: rebuilt");

        let mut reordered = plant.clone();
        reordered.elements.reverse();
        reordered.edges.reverse();
        let middle = reordered.edges.len() / 2;
        reordered.edges.rotate_left(middle);
        let reordered_graph = PlantGraph::new(&reordered, &library, &profile).unwrap();
        assert_eq!(
            snapshot(&reordered_graph, &pairs),
            answer,
            "seed {seed}: reordered"
        );
    }
}

#[test]
fn file_order_changes_nothing_on_the_fixture() {
    for f in common::fixtures() {
        let mut rng = Rng(7);
        let pairs = sample_pairs(&locations(&f.plant, &mut rng), &mut rng);
        let answer = snapshot(&f.graph(), &pairs);
        let mut reordered = f.plant.clone();
        reordered.elements.reverse();
        reordered.edges.rotate_left(5);
        let graph = PlantGraph::new(&reordered, &f.library, &f.profile).unwrap();
        assert_eq!(snapshot(&graph, &pairs), answer);
        assert_eq!(graph.element(&id("amp-1")).unwrap().id, id("amp-1"));
    }
}
