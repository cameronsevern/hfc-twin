//! Steiner tree over the street graph, by Takahashi–Matsuyama: from the tree so far, reach the nearest home not yet on it.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};

use schema::common::LonLat;
use schema::plant::Installation;
use schema::region::{Home, Street};

use crate::geo::{dist_ft, lerp, nearest_share};

/// A vertex of the street graph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    /// Where it is.
    pub geo: LonLat,
}

/// An edge of the street graph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreetEdge {
    /// The other end.
    pub to: usize,
    /// Length, feet.
    pub len_ft: f64,
    /// How plant runs along it.
    pub installation: Installation,
}

/// The streets as a graph, with every home's nearest point on a street as a vertex.
#[derive(Debug, Clone, Default)]
pub struct StreetGraph {
    /// Vertices.
    pub vertices: Vec<Vertex>,
    /// Adjacency, in a stable order.
    pub edges: Vec<Vec<StreetEdge>>,
    /// Per home index: the vertex it snaps to.
    pub home_vertex: Vec<Option<usize>>,
}

/// Vertices closer than this are one, feet. Streets drawn separately meet only by coordinates.
const MERGE_FT: f64 = 3.0;

struct Index {
    cells: BTreeMap<(i64, i64), Vec<usize>>,
}

impl Index {
    fn key(geo: LonLat) -> (i64, i64) {
        (
            (geo.lon_deg * 10_000.0).floor() as i64,
            (geo.lat_deg * 10_000.0).floor() as i64,
        )
    }

    fn find(&self, vertices: &[Vertex], geo: LonLat) -> Option<usize> {
        let (x, y) = Self::key(geo);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = self.cells.get(&(x + dx, y + dy)) {
                    for &v in list {
                        if dist_ft(vertices[v].geo, geo) < MERGE_FT {
                            return Some(v);
                        }
                    }
                }
            }
        }
        None
    }

    fn insert(&mut self, geo: LonLat, v: usize) {
        self.cells.entry(Self::key(geo)).or_default().push(v);
    }
}

impl StreetGraph {
    /// Builds the graph, snapping each home to its nearest street within `max_snap_ft`.
    pub fn build(streets: &[Street], homes: &[Home], max_snap_ft: f64) -> StreetGraph {
        // Each street segment, with the homes that snap to it along it.
        let mut segments: Vec<(LonLat, LonLat, Installation)> = Vec::new();
        for street in streets {
            for pair in street.points.windows(2) {
                segments.push((pair[0], pair[1], street.installation));
            }
        }
        let mut stops: Vec<Vec<(f64, usize)>> = vec![Vec::new(); segments.len()];
        for (h, home) in homes.iter().enumerate() {
            let mut best: Option<(f64, usize, f64)> = None;
            for (s, (a, b, _)) in segments.iter().enumerate() {
                let share = nearest_share(*a, *b, home.geo);
                let d = dist_ft(lerp(*a, *b, share), home.geo);
                if best.is_none_or(|x| d < x.0) {
                    best = Some((d, s, share));
                }
            }
            if let Some((d, s, share)) = best
                && d <= max_snap_ft
            {
                stops[s].push((share, h));
            }
        }

        let mut graph = StreetGraph {
            vertices: Vec::new(),
            edges: Vec::new(),
            home_vertex: vec![None; homes.len()],
        };
        let mut index = Index {
            cells: BTreeMap::new(),
        };
        let mut vertex = |graph: &mut StreetGraph, geo: LonLat| -> usize {
            if let Some(v) = index.find(&graph.vertices, geo) {
                return v;
            }
            let v = graph.vertices.len();
            graph.vertices.push(Vertex { geo });
            graph.edges.push(Vec::new());
            index.insert(geo, v);
            v
        };
        for (s, (a, b, installation)) in segments.iter().enumerate() {
            let mut along = std::mem::take(&mut stops[s]);
            along.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
            let mut previous = vertex(&mut graph, *a);
            for (share, home) in along {
                let at = vertex(&mut graph, lerp(*a, *b, share));
                graph.home_vertex[home] = Some(at);
                connect(&mut graph, previous, at, *installation);
                previous = at;
            }
            let end = vertex(&mut graph, *b);
            connect(&mut graph, previous, end, *installation);
        }
        graph
    }

    /// The vertex nearest a point.
    pub fn nearest_vertex(&self, geo: LonLat) -> Option<usize> {
        let mut best: Option<(f64, usize)> = None;
        for (v, vertex) in self.vertices.iter().enumerate() {
            let d = dist_ft(vertex.geo, geo);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, v));
            }
        }
        best.map(|b| b.1)
    }
}

fn connect(graph: &mut StreetGraph, a: usize, b: usize, installation: Installation) {
    if a == b {
        return;
    }
    let len_ft = dist_ft(graph.vertices[a].geo, graph.vertices[b].geo);
    graph.edges[a].push(StreetEdge {
        to: b,
        len_ft,
        installation,
    });
    graph.edges[b].push(StreetEdge {
        to: a,
        len_ft,
        installation,
    });
}

#[derive(PartialEq)]
struct Item(f64, usize);
impl Eq for Item {}
impl PartialOrd for Item {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Item {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0).then(other.1.cmp(&self.1))
    }
}

/// A routed tree: each vertex on it, with its parent and the edge's facts.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    /// `vertex → (parent, feet, installation)`. The root has no entry.
    pub parent: BTreeMap<usize, (usize, f64, Installation)>,
    /// `vertex → children`, in a stable order.
    pub children: BTreeMap<usize, Vec<usize>>,
    /// Terminals it couldn't reach, because the street map isn't connected there.
    pub unreached: Vec<usize>,
}

/// Grows a tree from `root` to every terminal.
pub fn steiner_tree(graph: &StreetGraph, root: usize, terminals: &[usize]) -> Tree {
    let n = graph.vertices.len();
    let mut on_tree = vec![false; n];
    on_tree[root] = true;
    let mut tree = Tree::default();
    let mut remaining: Vec<usize> = terminals.iter().copied().filter(|t| *t != root).collect();
    remaining.sort_unstable();
    remaining.dedup();
    let mut is_terminal = vec![false; n];
    for &t in &remaining {
        is_terminal[t] = true;
    }

    // Dijkstra from the whole tree, stopping at the nearest terminal, then again.
    let mut dist = vec![f64::INFINITY; n];
    let mut from: Vec<Option<(usize, f64, Installation)>> = vec![None; n];
    let mut heap = BinaryHeap::new();
    dist[root] = 0.0;
    heap.push(Item(0.0, root));
    let mut left = remaining.len();
    while left > 0 {
        let mut reached = None;
        while let Some(Item(d, u)) = heap.pop() {
            if d > dist[u] {
                continue;
            }
            if is_terminal[u] && !on_tree[u] {
                reached = Some(u);
                break;
            }
            for edge in &graph.edges[u] {
                let next = d + edge.len_ft;
                if next < dist[edge.to] {
                    dist[edge.to] = next;
                    from[edge.to] = Some((u, edge.len_ft, edge.installation));
                    heap.push(Item(next, edge.to));
                }
            }
        }
        let Some(terminal) = reached else {
            for &t in &remaining {
                if !on_tree[t] {
                    tree.unreached.push(t);
                }
            }
            break;
        };
        // Add the path, and every vertex on it becomes a source at distance 0.
        let mut path = Vec::new();
        let mut at = terminal;
        while !on_tree[at] {
            path.push(at);
            let Some((parent, len_ft, installation)) = from[at] else {
                break;
            };
            tree.parent.insert(at, (parent, len_ft, installation));
            tree.children.entry(parent).or_default().push(at);
            at = parent;
        }
        for &v in &path {
            on_tree[v] = true;
            if is_terminal[v] {
                is_terminal[v] = false;
                left -= 1;
            }
        }
        dist.iter_mut().for_each(|d| *d = f64::INFINITY);
        heap.clear();
        for (v, on) in on_tree.iter().enumerate() {
            if *on {
                dist[v] = 0.0;
                heap.push(Item(0.0, v));
            }
        }
    }
    for list in tree.children.values_mut() {
        list.sort_unstable();
    }
    tree
}
