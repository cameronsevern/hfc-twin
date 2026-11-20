//! Loading the one-street fixture, and short ways to write locations and paths.
#![allow(dead_code)]

use std::fs;
use std::path::Path as FilePath;

use plant_graph::{Path, PathDirection, PlantGraph, Segment};
use schema::common::{EdgeId, ElementId, PortId};
use schema::component::ComponentLibrary;
use schema::fault::Location;
use schema::plant::{Element, Plant};
use schema::profile::Profile;
use serde::de::DeserializeOwned;

pub fn load<T: DeserializeOwned>(file: &str) -> T {
    let path = FilePath::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(file);
    let text = fs::read_to_string(&path).unwrap();
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{file}: {e}"))
}

pub struct Fixture {
    pub plant: Plant,
    pub profile: Profile,
    pub library: ComponentLibrary,
    pub root: &'static str,
}

impl Fixture {
    pub fn graph(&self) -> PlantGraph<'_> {
        PlantGraph::new(&self.plant, &self.library, &self.profile).unwrap()
    }
}

pub fn low() -> Fixture {
    Fixture {
        plant: load("one_street/plant_lowsplit_iccap.json"),
        profile: load("profiles/d31_lowsplit_1002_iccap.json"),
        library: load("library/generic.json"),
        root: "node-1",
    }
}

pub fn high() -> Fixture {
    Fixture {
        plant: load("one_street/plant_highsplit_rpd.json"),
        profile: load("profiles/d31_highsplit_1218_rpd.json"),
        library: load("library/generic.json"),
        root: "rpd-1",
    }
}

pub fn fixtures() -> [Fixture; 2] {
    [low(), high()]
}

pub fn id(element: &str) -> ElementId {
    ElementId::new(element)
}

pub fn port(element: &str, port: &str) -> Location {
    Location::Port {
        element_id: id(element),
        port: PortId::new(port),
    }
}

pub fn point(edge: &str, pos_ft: f64) -> Location {
    Location::Span {
        span_id: EdgeId::new(edge),
        pos_ft,
    }
}

pub fn ids(elements: &[&Element]) -> Vec<String> {
    elements.iter().map(|e| e.id.to_string()).collect()
}

pub fn modems(range: std::ops::RangeInclusive<u32>) -> Vec<String> {
    range.map(|n| format!("modem-{n}")).collect()
}

/// One line per segment: `span-3 up 150`, `tap-1 in>through`, `direct-1 down`.
pub fn show(path: &Path<'_>) -> Vec<String> {
    let way = |direction| match direction {
        PathDirection::Downstream => "down",
        PathDirection::Upstream => "up",
    };
    let lines = path.segments.iter().map(|segment| match segment {
        Segment::Cable {
            edge,
            len_ft,
            direction,
        } => {
            format!("{} {} {len_ft}", edge.id, way(*direction))
        }
        Segment::Direct { edge, direction } => format!("{} {}", edge.id, way(*direction)),
        Segment::Device {
            element,
            in_port,
            out_port,
        } => {
            format!("{} {}>{}", element.id, in_port.id, out_port.id)
        }
    });
    lines.collect()
}
