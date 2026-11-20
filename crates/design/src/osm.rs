//! OpenStreetMap as Overpass JSON: ways with a `highway` tag are streets; residential buildings, and address nodes
//! outside any building, are homes. OSM's data is ODbL, so a map from it keeps the attribution.

use std::collections::BTreeMap;

use schema::common::LonLat;
use schema::plant::Installation;
use schema::region::{Home, MapSource, SCHEMA_VERSION, Street, StreetMap};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::geo::dist_ft;
use crate::route::StreetGraph;

/// What can go wrong reading an extract.
#[derive(Error, Debug)]
pub enum OsmError {
    /// Not JSON.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// JSON, but not an Overpass result.
    #[error("no elements array")]
    NoElements,
    /// Nothing usable in it.
    #[error("no streets")]
    NoStreets,
}

/// Highway values a cable would follow. Motorways and footpaths aren't plant routes. Links stay, or the roads they join
/// come apart.
const ROADS: [&str; 15] = [
    "primary",
    "primary_link",
    "secondary",
    "secondary_link",
    "tertiary",
    "tertiary_link",
    "residential",
    "unclassified",
    "living_street",
    "service",
    "road",
    "trunk",
    "trunk_link",
    "track",
    "busway",
];

/// `service=` values plant doesn't follow. A house's driveway is a drop, not a hardline run. Parking aisles stay: they're
/// how plant reaches an apartment block, and a route only follows one if a home needs it.
const NOT_PLANT_ROUTES: [&str; 3] = ["driveway", "drive-through", "emergency_access"];

/// `building=` values someone lives in. `yes` is OSM's "a building, kind unknown", and most of the map in many places.
const HOMES: [&str; 13] = [
    "yes",
    "house",
    "detached",
    "semidetached_house",
    "terrace",
    "residential",
    "apartments",
    "bungalow",
    "cabin",
    "farm",
    "static_caravan",
    "dormitory",
    "houseboat",
];

/// Tags that make a building or an address a business or institution, whatever `building=` says.
const NOT_A_HOME: [&str; 5] = ["shop", "amenity", "office", "tourism", "craft"];

/// Longest gap a plant crossing spans between pieces of street, feet. Enough for a divided road; a real crossing of
/// anything wider is a permit and an engineer, not a default.
const MAX_CROSSING_FT: f64 = 200.0;

/// What an import read and what it left out, so nobody takes the home count for more than it is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OsmReport {
    /// Buildings left out because they aren't homes: sheds, garages, shops.
    pub buildings_not_homes: u32,
    /// Apartment buildings with no `building:flats` tag. Each counts as one home, so this undercounts them.
    pub apartments_counted_as_one: u32,
    /// Address nodes left out because they sit inside a building already considered.
    pub addresses_in_buildings: u32,
    /// Service roads left out: driveways and the like.
    pub service_roads_skipped: u32,
    /// Plant crossings added between pieces of street, none of them in OSM.
    pub crossings_added: u32,
}

/// Reads an Overpass JSON extract. See [`read_overpass`].
pub fn from_overpass(
    id: &str,
    text: &str,
    installation: Installation,
) -> Result<StreetMap, OsmError> {
    read_overpass(id, text, installation).map(|(map, _)| map)
}

/// Is a point inside a closed ring? Ray casting on lon/lat, which is fine at a building's scale.
fn inside(ring: &[LonLat], p: LonLat) -> bool {
    let mut odd = false;
    for pair in ring.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if (a.lat_deg > p.lat_deg) != (b.lat_deg > p.lat_deg) {
            let at = a.lon_deg
                + (p.lat_deg - a.lat_deg) / (b.lat_deg - a.lat_deg) * (b.lon_deg - a.lon_deg);
            if p.lon_deg < at {
                odd = !odd;
            }
        }
    }
    odd
}

/// Joins pieces of street that nearly meet. OSM only shares a node where roads connect, so a road's two carriageways, or
/// a street that ends at a divided road, come apart though cable would cross. Nearest pairs first, each joining two
/// different pieces. Returns the crossings as streets of their own.
fn crossings(streets: &[Street], installation: Installation) -> Vec<Street> {
    let graph = StreetGraph::build(streets, &[], 0.0);
    let n = graph.vertices.len();
    let mut piece: Vec<usize> = (0..n).collect();
    fn find(piece: &mut [usize], mut v: usize) -> usize {
        while piece[v] != v {
            piece[v] = piece[piece[v]];
            v = piece[v];
        }
        v
    }
    for (v, edges) in graph.edges.iter().enumerate() {
        for edge in edges {
            let (a, b) = (find(&mut piece, v), find(&mut piece, edge.to));
            piece[a] = b;
        }
    }
    // A grid of about the crossing's length, so only neighbours are compared.
    const CELL_DEG: f64 = 0.001;
    let key = |geo: LonLat| {
        (
            (geo.lon_deg / CELL_DEG).floor() as i64,
            (geo.lat_deg / CELL_DEG).floor() as i64,
        )
    };
    let mut cells: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (v, vertex) in graph.vertices.iter().enumerate() {
        cells.entry(key(vertex.geo)).or_default().push(v);
    }
    let mut candidates: Vec<(f64, usize, usize)> = Vec::new();
    for (v, vertex) in graph.vertices.iter().enumerate() {
        let (x, y) = key(vertex.geo);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &w in cells.get(&(x + dx, y + dy)).into_iter().flatten() {
                    if w > v && find(&mut piece, v) != find(&mut piece, w) {
                        let d = dist_ft(vertex.geo, graph.vertices[w].geo);
                        if d <= MAX_CROSSING_FT {
                            candidates.push((d, v, w));
                        }
                    }
                }
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut added = Vec::new();
    for (_, v, w) in candidates {
        let (a, b) = (find(&mut piece, v), find(&mut piece, w));
        if a != b {
            piece[a] = b;
            added.push(Street {
                id: format!("crossing-{}", added.len()),
                points: vec![graph.vertices[v].geo, graph.vertices[w].geo],
                installation,
            });
        }
    }
    added
}

/// Reads an Overpass JSON extract, and says what it left out. OSM doesn't record plant, so `installation` applies to
/// every street until a user says otherwise (`docs/DESIGN.md`, "Reactivity").
pub fn read_overpass(
    id: &str,
    text: &str,
    installation: Installation,
) -> Result<(StreetMap, OsmReport), OsmError> {
    let root: Value = serde_json::from_str(text)?;
    let elements = root
        .get("elements")
        .and_then(Value::as_array)
        .ok_or(OsmError::NoElements)?;

    let mut nodes: BTreeMap<i64, LonLat> = BTreeMap::new();
    for element in elements {
        if element.get("type").and_then(Value::as_str) == Some("node")
            && let (Some(id), Some(lon_deg), Some(lat_deg)) = (
                element.get("id").and_then(Value::as_i64),
                element.get("lon").and_then(Value::as_f64),
                element.get("lat").and_then(Value::as_f64),
            )
        {
            nodes.insert(id, LonLat { lon_deg, lat_deg });
        }
    }

    let tag = |element: &Value, key: &str| -> Option<String> {
        element.get("tags")?.get(key)?.as_str().map(str::to_string)
    };
    let mut report = OsmReport::default();
    let mut streets = Vec::new();
    let mut homes = Vec::new();
    let mut footprints: Vec<Vec<LonLat>> = Vec::new();
    for element in elements {
        if element.get("type").and_then(Value::as_str) != Some("way") {
            continue;
        }
        let osm_id = element.get("id").and_then(Value::as_i64).unwrap_or(0);
        let points: Vec<LonLat> = element
            .get("nodes")
            .and_then(Value::as_array)
            .map(|refs| {
                refs.iter()
                    .filter_map(|r| r.as_i64().and_then(|n| nodes.get(&n).copied()))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(highway) = tag(element, "highway") {
            if tag(element, "service").is_some_and(|s| NOT_PLANT_ROUTES.contains(&s.as_str())) {
                report.service_roads_skipped += 1;
            } else if ROADS.contains(&highway.as_str()) && points.len() >= 2 {
                streets.push(Street {
                    id: format!("way-{osm_id}"),
                    points,
                    installation,
                });
            }
        } else if let Some(building) = tag(element, "building")
            && !points.is_empty()
        {
            let is_home = HOMES.contains(&building.as_str())
                && !NOT_A_HOME.iter().any(|t| tag(element, t).is_some());
            if is_home {
                // A closed ring repeats its first point, which would pull the centre toward it.
                let corners = match points.split_last() {
                    Some((last, rest)) if !rest.is_empty() && *last == points[0] => rest,
                    _ => &points[..],
                };
                let n = corners.len() as f64;
                let geo = LonLat {
                    lon_deg: corners.iter().map(|p| p.lon_deg).sum::<f64>() / n,
                    lat_deg: corners.iter().map(|p| p.lat_deg).sum::<f64>() / n,
                };
                let flats = tag(element, "building:flats")
                    .and_then(|f| f.parse::<u32>().ok())
                    .filter(|f| *f > 1);
                match flats {
                    Some(flats) => homes.extend((0..flats).map(|unit| Home {
                        id: format!("building-{osm_id}-{unit}"),
                        geo,
                    })),
                    None => {
                        if building == "apartments" {
                            report.apartments_counted_as_one += 1;
                        }
                        homes.push(Home {
                            id: format!("building-{osm_id}"),
                            geo,
                        });
                    }
                }
            } else {
                report.buildings_not_homes += 1;
            }
            footprints.push(points);
        }
    }
    for element in elements {
        if element.get("type").and_then(Value::as_str) == Some("node")
            && tag(element, "addr:housenumber").is_some()
            && !NOT_A_HOME.iter().any(|t| tag(element, t).is_some())
            && let Some(osm_id) = element.get("id").and_then(Value::as_i64)
            && let Some(geo) = nodes.get(&osm_id)
        {
            if footprints.iter().any(|ring| inside(ring, *geo)) {
                report.addresses_in_buildings += 1;
            } else {
                homes.push(Home {
                    id: format!("address-{osm_id}"),
                    geo: *geo,
                });
            }
        }
    }
    let Some(first) = streets.first().and_then(|s| s.points.first().copied()) else {
        return Err(OsmError::NoStreets);
    };
    let joins = crossings(&streets, installation);
    report.crossings_added = joins.len() as u32;
    streets.extend(joins);
    Ok((
        StreetMap {
            schema_version: SCHEMA_VERSION.to_string(),
            id: id.to_string(),
            source: MapSource::OpenStreetMap {
                attribution: "© OpenStreetMap contributors".to_string(),
            },
            streets,
            homes,
            headend: first,
        },
        report,
    ))
}
