//! Trucks on the region's streets: a route is the shortest way along the street graph the design was built on, and a truck
//! moves along it at the rates file's speed.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use design::route::StreetGraph;
use schema::common::LonLat;
use sim::faults::dist_ft;

const FT_PER_MILE: f64 = 5280.0;

#[derive(PartialEq)]
struct Item(f64, usize);
impl Eq for Item {}
impl Ord for Item {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0).then(other.1.cmp(&self.1))
    }
}
impl PartialOrd for Item {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The way from one point to another along the streets, as points from `from` to `to`, and its length in feet. Off the
/// street graph, or where the streets don't join, it's the straight line.
pub fn route(streets: &StreetGraph, from: LonLat, to: LonLat) -> (Vec<LonLat>, f64) {
    let straight = || (vec![from, to], dist_ft(from, to));
    let (Some(a), Some(b)) = (streets.nearest_vertex(from), streets.nearest_vertex(to)) else {
        return straight();
    };
    let n = streets.vertices.len();
    let mut dist = vec![f64::INFINITY; n];
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut heap = BinaryHeap::new();
    dist[a] = 0.0;
    heap.push(Item(0.0, a));
    while let Some(Item(d, u)) = heap.pop() {
        if u == b {
            break;
        }
        if d > dist[u] {
            continue;
        }
        for edge in &streets.edges[u] {
            let next = d + edge.len_ft;
            if next < dist[edge.to] {
                dist[edge.to] = next;
                parent[edge.to] = Some(u);
                heap.push(Item(next, edge.to));
            }
        }
    }
    if !dist[b].is_finite() {
        return straight();
    }
    let mut chain = vec![b];
    let mut at = b;
    while let Some(p) = parent[at] {
        chain.push(p);
        at = p;
    }
    chain.reverse();
    let mut path = vec![from];
    path.extend(chain.iter().map(|v| streets.vertices[*v].geo));
    path.push(to);
    let len: f64 = path.windows(2).map(|w| dist_ft(w[0], w[1])).sum();
    (path, len)
}

/// Seconds to drive `len_ft` at `mph`.
pub fn drive_s(len_ft: f64, mph: f64) -> i64 {
    (len_ft / FT_PER_MILE / mph.max(1.0) * 3600.0).ceil() as i64
}

/// Where along `path` a truck is at `t_s`, having left at `depart_t_s` and arriving at `arrive_t_s`, at an even pace.
pub fn position(path: &[LonLat], depart_t_s: i64, arrive_t_s: i64, t_s: i64) -> LonLat {
    let Some(first) = path.first() else {
        return LonLat {
            lon_deg: 0.0,
            lat_deg: 0.0,
        };
    };
    if t_s <= depart_t_s || path.len() == 1 {
        return *first;
    }
    if t_s >= arrive_t_s {
        return *path.last().unwrap_or(first);
    }
    let lens: Vec<f64> = path.windows(2).map(|w| dist_ft(w[0], w[1])).collect();
    let total: f64 = lens.iter().sum();
    let mut along = total * (t_s - depart_t_s) as f64 / (arrive_t_s - depart_t_s).max(1) as f64;
    for (i, len) in lens.iter().enumerate() {
        if along <= *len {
            let s = if *len > 0.0 { along / len } else { 0.0 };
            let (p, q) = (path[i], path[i + 1]);
            return LonLat {
                lon_deg: p.lon_deg + (q.lon_deg - p.lon_deg) * s,
                lat_deg: p.lat_deg + (q.lat_deg - p.lat_deg) * s,
            };
        }
        along -= len;
    }
    *path.last().unwrap_or(first)
}
