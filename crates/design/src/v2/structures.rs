//! Stage `structures`: poles along aerial streets and pedestals along buried ones, at the rules' spacing, on the rules'
//! side, deterministic from the map. Every other stage places plant at these, so a structure the coax and fiber plants
//! share is one record with one id and one position. A run is the cable path between two neighbouring structures: its
//! length is its path's length, so no span is a stand-in.

use std::collections::BTreeMap;

use schema::common::{ComponentId, LonLat, StructureId};
use schema::component::StructureKind;
use schema::design_rules::StreetSide;
use schema::plant::{Installation, RecordStatus, Structure};
use schema::region::StreetMap;

use crate::geo::{dist_ft, lerp, nearest_share, offset};

/// Points of separately drawn streets closer than this are one, feet, as `route` merges them.
const MERGE_FT: f64 = 3.0;
/// A chain shorter than this has no structure of its own between its ends, feet.
const SHORTEST_CHAIN_FT: f64 = 1.0;

/// What `structures` reads from the rules.
#[derive(Debug, Clone)]
pub struct StructureRules {
    /// Longest run between poles, feet.
    pub pole_spacing_ft: f64,
    /// Longest run between pedestals, feet.
    pub pedestal_spacing_ft: f64,
    /// The side of the street.
    pub side: StreetSide,
    /// From the centerline, feet.
    pub setback_ft: f64,
    /// The pole part.
    pub pole_ref: ComponentId,
    /// The pedestal part.
    pub pedestal_ref: ComponentId,
    /// Longest drop, feet.
    pub max_drop_ft: f64,
    /// Units at which a building is an apartment block, fed a group at a time: the fewest modems a premises template
    /// for a group holds. 0 where the rules have none, and every unit gets its own drop.
    pub block_units: usize,
    /// Units a group's feed serves at most: the most modems a premises template for a group holds.
    pub group_units: usize,
}

/// A structure.
#[derive(Debug, Clone, PartialEq)]
pub struct Site {
    /// Its id, region-wide.
    pub id: StructureId,
    /// Pole or pedestal.
    pub kind: StructureKind,
    /// Its part.
    pub component_ref: ComponentId,
    /// Where it stands.
    pub geo: LonLat,
}

/// The cable path between two neighbouring structures along a street.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// One end, a site.
    pub a: usize,
    /// The other end.
    pub b: usize,
    /// Length along `path`, feet.
    pub len_ft: f64,
    /// Points strictly between the two ends, in order from `a`.
    pub path: Vec<LonLat>,
    /// How plant runs along it.
    pub installation: Installation,
}

impl Run {
    /// The far end from `site`.
    pub fn other(&self, site: usize) -> usize {
        if self.a == site { self.b } else { self.a }
    }

    /// The path's points between the ends, in order from `site`.
    pub fn path_from(&self, site: usize) -> Vec<LonLat> {
        let mut out = self.path.clone();
        if site != self.a {
            out.reverse();
        }
        out
    }
}

/// Where a home is served from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HomeSite {
    /// The structure its drop leaves from.
    pub site: usize,
    /// The drop's length, feet: the straight line from the structure to the home.
    pub drop_ft: f64,
    /// Aerial or buried, as its street's plant.
    pub installation: Installation,
}

/// The structures of a map, the runs between them, and where each home is served from.
#[derive(Debug, Clone, Default)]
pub struct Structures {
    /// Every structure.
    pub sites: Vec<Site>,
    /// Every run.
    pub runs: Vec<Run>,
    /// Per site, `(neighbour, run)`, in a stable order.
    pub adjacent: Vec<Vec<(usize, usize)>>,
    /// Per home, by index in the map: where it's served from, or `None` if no structure is within a drop.
    pub homes: Vec<Option<HomeSite>>,
    /// Apartment blocks served a group of units at a time (`docs/tasks/25-design-any-map.md`): each group's first unit,
    /// which stands for the group on its tap port, to every unit in the group, the first included.
    pub groups: BTreeMap<usize, Vec<usize>>,
    /// Per home, the first unit of its group, if it's in one.
    pub group_of: Vec<Option<usize>>,
    /// Runs added to join a piece of street with homes to the hub's, which no street in the map does.
    pub joins: Vec<usize>,
}

impl Structures {
    /// The structures as records, every one `Designed`.
    pub fn records(&self, sites: impl IntoIterator<Item = usize>) -> Vec<Structure> {
        let mut ids: Vec<usize> = sites.into_iter().collect();
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter()
            .map(|s| {
                let site = &self.sites[s];
                Structure {
                    id: site.id.clone(),
                    kind: site.kind,
                    component_ref: site.component_ref.clone(),
                    geo: Some(site.geo),
                    record_status: RecordStatus::Designed,
                }
            })
            .collect()
    }

    /// The site nearest a point.
    pub fn nearest(&self, geo: LonLat) -> Option<usize> {
        let mut best: Option<(f64, usize)> = None;
        for (s, site) in self.sites.iter().enumerate() {
            let d = dist_ft(site.geo, geo);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, s));
            }
        }
        best.map(|b| b.1)
    }

    /// The run between two neighbouring sites.
    pub fn run_between(&self, a: usize, b: usize) -> Option<usize> {
        self.adjacent[a]
            .iter()
            .find(|(n, _)| *n == b)
            .map(|(_, r)| *r)
    }
}

/// East and north feet from `a` to `b`.
fn delta_ft(a: LonLat, b: LonLat) -> (f64, f64) {
    let east = offset(a, 1.0, 0.0).lon_deg - a.lon_deg;
    let north = offset(a, 0.0, 1.0).lat_deg - a.lat_deg;
    (
        (b.lon_deg - a.lon_deg) / east,
        (b.lat_deg - a.lat_deg) / north,
    )
}

/// The unit normal to `a`→`b` on `side`.
fn normal(a: LonLat, b: LonLat, side: StreetSide) -> (f64, f64) {
    let (dx, dy) = delta_ft(a, b);
    let len = (dx * dx + dy * dy).sqrt().max(1e-9);
    match side {
        StreetSide::Right => (dy / len, -dx / len),
        StreetSide::Left => (-dy / len, dx / len),
    }
}

/// The centerline as a graph: merged points, and each segment with the way its street draws it.
struct Centerline {
    points: Vec<LonLat>,
    /// `(from, to, installation)` as drawn.
    segments: Vec<(usize, usize, Installation)>,
    /// Per point, segment indices.
    at: Vec<Vec<usize>>,
}

fn centerline(map: &StreetMap) -> Centerline {
    let mut graph = Centerline {
        points: Vec::new(),
        segments: Vec::new(),
        at: Vec::new(),
    };
    let mut cells: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    let key = |geo: LonLat| {
        (
            (geo.lon_deg * 10_000.0).floor() as i64,
            (geo.lat_deg * 10_000.0).floor() as i64,
        )
    };
    let mut point = |graph: &mut Centerline, geo: LonLat| -> usize {
        let (x, y) = key(geo);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &p in cells.get(&(x + dx, y + dy)).into_iter().flatten() {
                    if dist_ft(graph.points[p], geo) < MERGE_FT {
                        return p;
                    }
                }
            }
        }
        let p = graph.points.len();
        graph.points.push(geo);
        graph.at.push(Vec::new());
        cells.entry((x, y)).or_default().push(p);
        p
    };
    let mut seen: BTreeMap<(usize, usize), ()> = BTreeMap::new();
    for street in &map.streets {
        for pair in street.points.windows(2) {
            let a = point(&mut graph, pair[0]);
            let b = point(&mut graph, pair[1]);
            if a == b || seen.insert((a.min(b), a.max(b)), ()).is_some() {
                continue;
            }
            let s = graph.segments.len();
            graph.segments.push((a, b, street.installation));
            graph.at[a].push(s);
            graph.at[b].push(s);
        }
    }
    graph
}

/// One stretch of centerline between break points: its points in order, the segments between them, and which way it
/// faces against its first segment's drawing.
struct Chain {
    points: Vec<usize>,
    segments: Vec<usize>,
    installation: Installation,
}

fn chains(graph: &Centerline) -> Vec<Chain> {
    let breaks: Vec<bool> = (0..graph.points.len())
        .map(|p| {
            let at = &graph.at[p];
            at.len() != 2 || graph.segments[at[0]].2 != graph.segments[at[1]].2
        })
        .collect();
    let mut used = vec![false; graph.segments.len()];
    let mut out = Vec::new();
    let walk = |start: usize, first: usize, used: &mut Vec<bool>| -> Chain {
        let mut points = vec![start];
        let mut segments = Vec::new();
        let mut at = start;
        let mut seg = first;
        loop {
            used[seg] = true;
            segments.push(seg);
            let (a, b, _) = graph.segments[seg];
            at = if a == at { b } else { a };
            points.push(at);
            if breaks[at] || at == start {
                break;
            }
            match graph.at[at].iter().find(|s| !used[**s]) {
                Some(next) => seg = *next,
                None => break,
            }
        }
        Chain {
            points,
            segments,
            installation: graph.segments[first].2,
        }
    };
    for (p, _) in breaks.iter().enumerate().filter(|(_, b)| **b) {
        for &s in &graph.at[p] {
            if !used[s] {
                out.push(walk(p, s, &mut used));
            }
        }
    }
    // Loops with no break point on them start at their lowest point.
    for (p, at) in graph.at.iter().enumerate().take(graph.points.len()) {
        for &s in at {
            if !used[s] {
                out.push(walk(p, s, &mut used));
            }
        }
    }
    out
}

/// Places the structures and runs of `map`, and serves each home from one.
pub fn structures(map: &StreetMap, rules: &StructureRules) -> Structures {
    let graph = centerline(map);
    let mut out = Structures::default();
    let mut at_point: BTreeMap<usize, usize> = BTreeMap::new();
    let (mut poles, mut pedestals) = (0usize, 0usize);
    let mut new_site = |out: &mut Structures, geo: LonLat, installation: Installation| -> usize {
        let (kind, part, id) = match installation {
            Installation::Aerial => {
                poles += 1;
                (
                    StructureKind::Pole,
                    &rules.pole_ref,
                    format!("pole-{poles}"),
                )
            }
            Installation::Underground => {
                pedestals += 1;
                (
                    StructureKind::Pedestal,
                    &rules.pedestal_ref,
                    format!("ped-{pedestals}"),
                )
            }
        };
        out.sites.push(Site {
            id: StructureId::new(id),
            kind,
            component_ref: part.clone(),
            geo,
        });
        out.adjacent.push(Vec::new());
        out.sites.len() - 1
    };

    for chain in chains(&graph) {
        let geo: Vec<LonLat> = chain.points.iter().map(|&p| graph.points[p]).collect();
        let mut along = vec![0.0];
        for pair in geo.windows(2) {
            along.push(along[along.len() - 1] + dist_ft(pair[0], pair[1]));
        }
        let total = along[along.len() - 1];
        // Which way this chain faces against its first segment as drawn: the side holds along the whole chain.
        let (a0, _, _) = graph.segments[chain.segments[0]];
        let flip = a0 != chain.points[0];
        let side = match (rules.side, flip) {
            (side, false) => side,
            (StreetSide::Right, true) => StreetSide::Left,
            (StreetSide::Left, true) => StreetSide::Right,
        };
        let shifted = |i: usize| -> LonLat {
            // A point's offset: the normal of the segment before it, or after it at the chain's start.
            let (p, q) = if i + 1 < geo.len() {
                (geo[i], geo[i + 1])
            } else {
                (geo[i - 1], geo[i])
            };
            let (mut nx, mut ny) = normal(p, q, side);
            if i > 0 && i + 1 < geo.len() {
                let (mx, my) = normal(geo[i - 1], geo[i], side);
                let (sx, sy) = (nx + mx, ny + my);
                let len = (sx * sx + sy * sy).sqrt();
                if len > 1e-6 {
                    (nx, ny) = (sx / len, sy / len);
                }
            }
            offset(geo[i], nx * rules.setback_ft, ny * rules.setback_ft)
        };
        // An end where three or more streets meet stays on the centerline: it's the corner every chain shares.
        let end_site =
            |out: &mut Structures,
             at_point: &mut BTreeMap<usize, usize>,
             new_site: &mut dyn FnMut(&mut Structures, LonLat, Installation) -> usize,
             i: usize|
             -> usize {
                let p = chain.points[i];
                if let Some(&s) = at_point.get(&p) {
                    return s;
                }
                let geo_here = if graph.at[p].len() >= 3 {
                    geo[i]
                } else {
                    shifted(i)
                };
                let installation = if graph.at[p]
                    .iter()
                    .any(|s| graph.segments[*s].2 == Installation::Aerial)
                {
                    Installation::Aerial
                } else {
                    Installation::Underground
                };
                let s = new_site(out, geo_here, installation);
                at_point.insert(p, s);
                s
            };
        let first = end_site(&mut out, &mut at_point, &mut new_site, 0);
        let last_index = geo.len() - 1;
        let loop_chain = chain.points[0] == chain.points[last_index];
        let spacing = match chain.installation {
            Installation::Aerial => rules.pole_spacing_ft,
            Installation::Underground => rules.pedestal_spacing_ft,
        };
        let intervals = if total < SHORTEST_CHAIN_FT {
            1
        } else {
            ((total / spacing) - 1e-9).ceil().max(1.0) as usize
        };
        // Structures at even spacing; between them, the chain's own points, offset the same way.
        let point_at = |x: f64| -> LonLat {
            let j = along
                .windows(2)
                .position(|w| x <= w[1])
                .unwrap_or(along.len() - 2);
            let share = ((x - along[j]) / (along[j + 1] - along[j]).max(1e-9)).clamp(0.0, 1.0);
            let a = shifted(j);
            let b = shifted(j + 1);
            lerp(a, b, share)
        };
        let mut previous = first;
        let mut previous_x = 0.0;
        for k in 1..=intervals {
            let x = total * k as f64 / intervals as f64;
            let site = if k == intervals {
                if loop_chain {
                    first
                } else {
                    end_site(&mut out, &mut at_point, &mut new_site, last_index)
                }
            } else {
                new_site(&mut out, point_at(x), chain.installation)
            };
            // The chain's own points strictly between the two structures.
            let mut path: Vec<LonLat> = Vec::new();
            for (i, &d) in along.iter().enumerate() {
                if d > previous_x + 1e-6 && d < x - 1e-6 {
                    path.push(shifted(i));
                }
            }
            if site != previous {
                let mut len_ft = 0.0;
                let mut last = out.sites[previous].geo;
                for p in path.iter().chain(std::iter::once(&out.sites[site].geo)) {
                    len_ft += dist_ft(last, *p);
                    last = *p;
                }
                let run = out.runs.len();
                out.runs.push(Run {
                    a: previous,
                    b: site,
                    len_ft,
                    path,
                    installation: chain.installation,
                });
                out.adjacent[previous].push((site, run));
                out.adjacent[site].push((previous, run));
            }
            previous = site;
            previous_x = x;
        }
    }

    // Each home from the nearer end of the run it's nearest.
    out.homes = map
        .homes
        .iter()
        .map(|home| {
            let mut best: Option<(f64, usize)> = None;
            for (r, run) in out.runs.iter().enumerate() {
                let mut points = vec![out.sites[run.a].geo];
                points.extend(run.path.iter().copied());
                points.push(out.sites[run.b].geo);
                for pair in points.windows(2) {
                    let share = nearest_share(pair[0], pair[1], home.geo);
                    let d = dist_ft(lerp(pair[0], pair[1], share), home.geo);
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, r));
                    }
                }
            }
            let (_, r) = best?;
            let run = &out.runs[r];
            let mut ends = [run.a, run.b];
            ends.sort_by(|x, y| {
                dist_ft(out.sites[*x].geo, home.geo)
                    .total_cmp(&dist_ft(out.sites[*y].geo, home.geo))
                    .then(x.cmp(y))
            });
            let site = ends[0];
            let drop_ft = dist_ft(out.sites[site].geo, home.geo);
            (drop_ft <= rules.max_drop_ft).then_some(HomeSite {
                site,
                drop_ft,
                installation: run.installation,
            })
        })
        .collect();
    out.joins = join_pieces(&mut out, map.headend);
    (out.groups, out.group_of) = apartment_groups(map, &out.homes, rules);
    out
}

/// Longest crossing that joins a piece of street with homes to the hub's, feet. In OpenStreetMap a neighbourhood whose road
/// in leaves the drawn box, or is of a kind the importer skips, is a piece on its own; in the street it's a road away. A
/// bore or a span across that far stands in for the road (task 25) [inferred]. Past it, the piece's homes are unserved.
pub const LONGEST_JOIN_FT: f64 = 1500.0;

/// Joins each piece of the structure graph that serves a home to the piece the hub is on, nearest piece first, by an
/// underground run between the two closest structures, while the gap is under [`LONGEST_JOIN_FT`]. Returns the runs added.
fn join_pieces(out: &mut Structures, headend: LonLat) -> Vec<usize> {
    let n = out.sites.len();
    let mut joins = Vec::new();
    let Some(hub) = out.nearest(headend) else {
        return joins;
    };
    loop {
        // Pieces, by a walk from each unvisited site.
        let mut piece = vec![usize::MAX; n];
        let mut count = 0;
        for start in 0..n {
            if piece[start] != usize::MAX {
                continue;
            }
            let mut stack = vec![start];
            piece[start] = count;
            while let Some(v) = stack.pop() {
                for &(w, _) in &out.adjacent[v] {
                    if piece[w] == usize::MAX {
                        piece[w] = count;
                        stack.push(w);
                    }
                }
            }
            count += 1;
        }
        let main = piece[hub];
        let mut served = vec![false; count];
        for at in out.homes.iter().flatten() {
            served[piece[at.site]] = true;
        }
        let mains: Vec<usize> = (0..n).filter(|v| piece[*v] == main).collect();
        // The nearest pair from any piece with homes to the hub's, ties by site index.
        let mut best: Option<(f64, usize, usize)> = None;
        for v in (0..n).filter(|v| piece[*v] != main && served[piece[*v]]) {
            for &m in &mains {
                let d = dist_ft(out.sites[v].geo, out.sites[m].geo);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, m, v));
                }
            }
        }
        let Some((len_ft, a, b)) = best.filter(|b| b.0 <= LONGEST_JOIN_FT) else {
            return joins;
        };
        let run = out.runs.len();
        out.runs.push(Run {
            a,
            b,
            len_ft,
            path: Vec::new(),
            installation: Installation::Underground,
        });
        out.adjacent[a].push((b, run));
        out.adjacent[b].push((a, run));
        joins.push(run);
    }
}

/// The building a home is a unit of: the importer names units `building-<id>-<unit>` (`design::osm`).
fn building_of(id: &str) -> Option<&str> {
    let rest = id.strip_prefix("building-")?;
    let (building, unit) = rest.rsplit_once('-')?;
    (!building.is_empty()
        && building.bytes().all(|b| b.is_ascii_digit())
        && unit.bytes().all(|b| b.is_ascii_digit()))
    .then_some(building)
}

/// The apartment groups of a map: units of one building, served from the same structure, at least the rules'
/// `block_units` of them, in runs of at most `group_units` by index. A building of several units is fed from the street to
/// a box in the building and split there, not given a drop a unit [CS-DROP-MANUAL §10] (design-practice.md §9).
fn apartment_groups(
    map: &StreetMap,
    homes: &[Option<HomeSite>],
    rules: &StructureRules,
) -> (BTreeMap<usize, Vec<usize>>, Vec<Option<usize>>) {
    let mut groups = BTreeMap::new();
    let mut group_of = vec![None; map.homes.len()];
    if rules.block_units < 2 || rules.group_units < rules.block_units {
        return (groups, group_of);
    }
    let mut by_building: BTreeMap<(&str, usize), Vec<usize>> = BTreeMap::new();
    for (h, home) in map.homes.iter().enumerate() {
        if let (Some(b), Some(at)) = (building_of(&home.id), homes[h]) {
            by_building.entry((b, at.site)).or_default().push(h);
        }
    }
    for units in by_building.into_values() {
        if units.len() < rules.block_units {
            continue;
        }
        for chunk in units.chunks(rules.group_units) {
            for &u in chunk {
                group_of[u] = Some(chunk[0]);
            }
            groups.insert(chunk[0], chunk.to_vec());
        }
    }
    (groups, group_of)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::generate;
    use schema::region::LayoutFamily;

    fn rules() -> StructureRules {
        StructureRules {
            pole_spacing_ft: 200.0,
            pedestal_spacing_ft: 300.0,
            side: StreetSide::Right,
            setback_ft: 10.0,
            pole_ref: ComponentId::new("structure-generic-pole"),
            pedestal_ref: ComponentId::new("structure-generic-pedestal"),
            max_drop_ft: 250.0,
            block_units: 0,
            group_units: 0,
        }
    }

    #[test]
    fn runs_are_their_paths_and_no_longer_than_the_spacing() {
        for family in [
            LayoutFamily::UrbanGrid,
            LayoutFamily::Suburban,
            LayoutFamily::Rural,
        ] {
            let map = generate(family, 7, 300);
            let s = structures(&map, &rules());
            assert!(!s.runs.is_empty());
            for run in &s.runs {
                let mut len = 0.0;
                let mut last = s.sites[run.a].geo;
                for p in run.path.iter().chain(std::iter::once(&s.sites[run.b].geo)) {
                    len += dist_ft(last, *p);
                    last = *p;
                }
                assert!((len - run.len_ft).abs() < 1e-6);
                let spacing = match run.installation {
                    Installation::Aerial => 200.0,
                    Installation::Underground => 300.0,
                };
                // Offsetting a curve's inside can only shorten it; the outside lengthens it by setback over radius.
                assert!(run.len_ft <= spacing * 1.2 + 25.0, "{} ft", run.len_ft);
            }
            let served = s.homes.iter().filter(|h| h.is_some()).count();
            assert!(served * 10 >= map.homes.len() * 9, "{family:?}: {served}");
            // Deterministic.
            let again = structures(&map, &rules());
            assert_eq!(again.sites, s.sites);
            assert_eq!(again.runs, s.runs);
        }
    }
}
