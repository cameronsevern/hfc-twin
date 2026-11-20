//! Stage `nodes`: homes grouped by distance along streets, so whatever has no street across it (a rail line, a river)
//! divides them; groups sized by the rules' homes per node; each node sited at a structure where fiber and coax cost
//! least together; homes balanced across its ports.
//!
//! Groups are cut from the shortest-path tree of the structures from the hub, bottom up, so each is a pocket of streets
//! the fiber reaches through one place. A site's cost is the fiber it adds inside its area, at the fiber cable's price
//! (it's lashed to the area's own strand, SCTE 180 §8.11.6 via design-practice.md §8), plus the actives its farthest home
//! costs: the express amp's price per express reach, times the site's farthest street distance.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use schema::common::LonLat;

use super::kit::Kit;
use super::levels::{HI, TEMPS};
use super::structures::Structures;
use crate::route::{StreetEdge, StreetGraph, Vertex, steiner_tree};

/// One node's area.
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    /// The structure the node is at.
    pub site: usize,
    /// Homes it serves, by index in the map.
    pub homes: Vec<usize>,
    /// The cable tree from the site: per structure, its children, in a stable order.
    pub children: BTreeMap<usize, Vec<usize>>,
    /// Per structure below the site, the run from its parent.
    pub run_to: BTreeMap<usize, usize>,
    /// Per structure, the homes served from it.
    pub homes_at: BTreeMap<usize, Vec<usize>>,
    /// Per RF port, the site's children it feeds. The homes at the site itself hang from `site_port`.
    pub ports: Vec<Vec<usize>>,
    /// The port the site's own homes are on.
    pub site_port: usize,
    /// Whether a pin put it here.
    pub pinned: bool,
}

impl Area {
    /// Homes in the subtree below and at `v`.
    pub fn homes_below(&self, v: usize) -> usize {
        let mut n = self.homes_at.get(&v).map_or(0, Vec::len);
        for c in self.children.get(&v).into_iter().flatten() {
            n += self.homes_below(*c);
        }
        n
    }
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

/// Shortest street distances from `from`, and each structure's parent on the way.
pub fn dijkstra(structures: &Structures, from: usize) -> (Vec<f64>, Vec<Option<usize>>) {
    let n = structures.sites.len();
    let mut dist = vec![f64::INFINITY; n];
    let mut parent = vec![None; n];
    let mut heap = BinaryHeap::new();
    dist[from] = 0.0;
    heap.push(Item(0.0, from));
    while let Some(Item(d, u)) = heap.pop() {
        if d > dist[u] {
            continue;
        }
        for &(v, r) in &structures.adjacent[u] {
            let next = d + structures.runs[r].len_ft;
            if next < dist[v] {
                dist[v] = next;
                parent[v] = Some(u);
                heap.push(Item(next, v));
            }
        }
    }
    (dist, parent)
}

/// The structures as `route`'s street graph, so the Steiner router task 08 built grows each area's tree.
pub fn street_graph(structures: &Structures) -> StreetGraph {
    StreetGraph {
        vertices: structures
            .sites
            .iter()
            .map(|s| Vertex { geo: s.geo })
            .collect(),
        edges: structures
            .adjacent
            .iter()
            .map(|list| {
                list.iter()
                    .map(|&(to, r)| StreetEdge {
                        to,
                        len_ft: structures.runs[r].len_ft,
                        installation: structures.runs[r].installation,
                    })
                    .collect()
            })
            .collect(),
        home_vertex: Vec::new(),
    }
}

/// What `nodes` reads.
#[derive(Debug, Clone)]
pub struct NodeInputs {
    /// Homes per node the controls target, at most the rules' and the profile's maximum.
    pub target_homes: usize,
    /// Steps from the count the target gives.
    pub step: i32,
    /// Pinned node positions.
    pub pins: Vec<LonLat>,
    /// Fiber cable's price per foot, the cheapest count.
    pub fiber_usd_per_ft: f64,
    /// An express amp's price.
    pub amp_usd: f64,
}

/// The express reach: feet of express cable an express amp's gain covers at the top of the band, hot, less a margin of
/// one tap step for the taps the line feeds (§3, §15).
pub fn express_reach_ft(kit: &Kit<'_>) -> f64 {
    let band = super::levels::Band::new(kit.profile);
    let per_ft = sim::cable::cable_loss_db(kit.express.spec, band.f[HI], 1.0, TEMPS[2]);
    let gain = sim::passive::active_gain_db(kit.express_amp.spec, band.f[HI]);
    (gain / per_ft.max(1e-9)).max(1.0)
}

/// Cuts the shortest-path tree into groups of about `per` homes each, bottom up: a vertex carries what its children
/// didn't place, and emits a group once it carries `per` or more.
fn cut_tree(
    order: &[usize],
    parent: &[Option<usize>],
    pool: &BTreeMap<usize, Vec<usize>>,
    per: usize,
) -> Vec<(usize, Vec<usize>)> {
    let mut carried: BTreeMap<usize, Vec<Vec<usize>>> = BTreeMap::new();
    let mut groups = Vec::new();
    for &v in order {
        let mut items: Vec<Vec<usize>> = Vec::new();
        if let Some(own) = pool.get(&v)
            && !own.is_empty()
        {
            items.push(own.clone());
        }
        items.extend(carried.remove(&v).unwrap_or_default());
        let mut bin: Vec<usize> = Vec::new();
        for item in items {
            bin.extend(item);
            if bin.len() >= per {
                groups.push((v, std::mem::take(&mut bin)));
            }
        }
        if !bin.is_empty() {
            match parent[v] {
                Some(p) => carried.entry(p).or_default().push(bin),
                None => groups.push((v, bin)),
            }
        }
    }
    groups
}

/// The areas, and the homes no area serves: no structure within a drop, or no street to the hub.
pub fn areas(
    structures: &Structures,
    hub: usize,
    kit: &Kit<'_>,
    inputs: &NodeInputs,
) -> (Vec<Area>, Vec<usize>) {
    let (dist, parent) = dijkstra(structures, hub);
    let mut unserved = Vec::new();
    let mut pool: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (h, at) in structures.homes.iter().enumerate() {
        match at {
            Some(at) if dist[at.site].is_finite() => pool.entry(at.site).or_default().push(h),
            _ => unserved.push(h),
        }
    }
    let total: usize = pool.values().map(Vec::len).sum();
    let target = inputs.target_homes.max(1);
    let count = ((total as f64 / target as f64).ceil() as i64 + inputs.step as i64).max(1) as usize;

    // Pins first: each claims the homes nearest it along streets, up to the target.
    let mut groups: Vec<(Option<usize>, Vec<usize>)> = Vec::new();
    for pin in &inputs.pins {
        let Some(site) = structures.nearest(*pin) else {
            continue;
        };
        let (from_pin, _) = dijkstra(structures, site);
        let mut near: Vec<usize> = pool
            .keys()
            .copied()
            .filter(|s| from_pin[*s].is_finite())
            .collect();
        near.sort_by(|a, b| from_pin[*a].total_cmp(&from_pin[*b]).then(a.cmp(b)));
        let mut claimed = Vec::new();
        for s in near {
            if claimed.len() >= target {
                break;
            }
            if let Some(homes) = pool.remove(&s) {
                claimed.extend(homes);
            }
        }
        groups.push((Some(site), claimed));
    }
    let left: usize = pool.values().map(Vec::len).sum();
    if left > 0 {
        let rest = count.saturating_sub(groups.len()).max(1);
        let per = left.div_ceil(rest).max(1);
        let mut order: Vec<usize> = (0..structures.sites.len())
            .filter(|v| dist[*v].is_finite())
            .collect();
        order.sort_by(|a, b| dist[*b].total_cmp(&dist[*a]).then(b.cmp(a)));
        for (_, homes) in cut_tree(&order, &parent, &pool, per) {
            groups.push((None, homes));
        }
    }

    let graph = street_graph(structures);
    let reach_ft = express_reach_ft(kit);
    let mut out = Vec::new();
    for (pin, mut homes) in groups {
        if homes.is_empty() {
            continue;
        }
        homes.sort_unstable();
        out.push(area_for(
            structures, &graph, &dist, &parent, kit, inputs, reach_ft, pin, homes,
        ));
    }
    // Pins first, then by each area's first home.
    out.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(a.homes[0].cmp(&b.homes[0])));
    unserved.sort_unstable();
    (out, unserved)
}

/// One area for a set of homes: its site, its tree and its ports.
#[allow(clippy::too_many_arguments)]
pub fn area_for(
    structures: &Structures,
    graph: &StreetGraph,
    dist: &[f64],
    parent: &[Option<usize>],
    kit: &Kit<'_>,
    inputs: &NodeInputs,
    reach_ft: f64,
    pin: Option<usize>,
    homes: Vec<usize>,
) -> Area {
    // A structure's tap ports go to its homes, and to an apartment group once, by its first unit.
    let mut homes_at: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &h in &homes {
        if let Some(at) = structures.homes[h]
            && structures.group_of[h].is_none_or(|first| first == h)
        {
            homes_at.entry(at.site).or_default().push(h);
        }
    }
    let home_sites: Vec<usize> = homes_at.keys().copied().collect();
    let site = pin.unwrap_or_else(|| {
        // Candidates: the home structures and every structure on the hub's tree between them and the area's top.
        let top = home_sites
            .iter()
            .copied()
            .min_by(|a, b| dist[*a].total_cmp(&dist[*b]).then(a.cmp(b)))
            .unwrap_or(0);
        let mut candidates: BTreeSet<usize> = BTreeSet::new();
        for &s in &home_sites {
            let mut at = Some(s);
            while let Some(v) = at {
                if !candidates.insert(v) || dist[v] < dist[top] {
                    break;
                }
                at = parent[v];
            }
        }
        candidates.insert(top);
        let per_ft = inputs.amp_usd / reach_ft;
        let mut best: Option<(f64, usize)> = None;
        for &s in &candidates {
            let (from_s, _) = dijkstra(structures, s);
            let far = home_sites.iter().map(|h| from_s[*h]).fold(0.0, f64::max);
            if !far.is_finite() {
                continue;
            }
            let cost = inputs.fiber_usd_per_ft * (dist[s] - dist[top]).max(0.0) + per_ft * far;
            if best.is_none_or(|b| cost < b.0 - 1e-9) {
                best = Some((cost, s));
            }
        }
        best.map_or(top, |b| b.1)
    });
    let tree = steiner_tree(graph, site, &home_sites);
    let mut children: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut run_to: BTreeMap<usize, usize> = BTreeMap::new();
    for (&v, &(p, len_ft, _)) in &tree.parent {
        children.entry(p).or_default().push(v);
        let run = structures.adjacent[p]
            .iter()
            .filter(|(n, _)| *n == v)
            .min_by(|a, b| {
                (structures.runs[a.1].len_ft - len_ft)
                    .abs()
                    .total_cmp(&(structures.runs[b.1].len_ft - len_ft).abs())
            })
            .map(|(_, r)| *r);
        if let Some(r) = run {
            run_to.insert(v, r);
        }
    }
    for list in children.values_mut() {
        list.sort_unstable();
    }
    // Homes the tree couldn't reach aren't this area's.
    let reached: BTreeSet<usize> = std::iter::once(site)
        .chain(tree.parent.keys().copied())
        .collect();
    homes_at.retain(|s, _| reached.contains(s));
    let homes: Vec<usize> = homes_at
        .values()
        .flatten()
        .flat_map(|h| {
            structures
                .groups
                .get(h)
                .cloned()
                .unwrap_or_else(|| vec![*h])
        })
        .collect();
    let mut area = Area {
        site,
        homes,
        children,
        run_to,
        homes_at,
        ports: vec![Vec::new(); kit.rf_ports],
        site_port: 0,
        pinned: pin.is_some(),
    };
    balance(&mut area);
    area.homes.sort_unstable();
    area
}

/// Longest-first: each of the site's branches to the port with the fewest homes so far; the site's own homes last.
fn balance(area: &mut Area) {
    let mut branches: Vec<(usize, usize)> = area
        .children
        .get(&area.site)
        .into_iter()
        .flatten()
        .map(|c| (area.homes_below(*c), *c))
        .collect();
    branches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut load = vec![0usize; area.ports.len()];
    for (homes, c) in branches {
        let p = (0..load.len()).min_by_key(|p| (load[*p], *p)).unwrap_or(0);
        area.ports[p].push(c);
        load[p] += homes;
    }
    for list in &mut area.ports {
        list.sort_unstable();
    }
    let own = area.homes_at.get(&area.site).map_or(0, Vec::len);
    if own > 0 {
        area.site_port = (0..load.len()).min_by_key(|p| (load[*p], *p)).unwrap_or(0);
    }
}

/// Splits an area's homes in two along the hub's tree, for an area whose plant is over the cascade or active limit.
pub fn split(
    structures: &Structures,
    hub: usize,
    kit: &Kit<'_>,
    inputs: &NodeInputs,
    area: &Area,
) -> Vec<Area> {
    let (dist, parent) = dijkstra(structures, hub);
    // Every home by its structure, apartment units included, as `areas` pools them, so a half is half the homes.
    let mut pool: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &h in &area.homes {
        if let Some(at) = structures.homes[h] {
            pool.entry(at.site).or_default().push(h);
        }
    }
    let per = area.homes.len().div_ceil(2).max(1);
    let mut order: Vec<usize> = pool.keys().copied().collect();
    // Every structure between the homes and the hub, so the cut follows the tree.
    let mut all: BTreeSet<usize> = BTreeSet::new();
    for &s in &order {
        let mut at = Some(s);
        while let Some(v) = at {
            if !all.insert(v) {
                break;
            }
            at = parent[v];
        }
    }
    order = all.into_iter().collect();
    order.sort_by(|a, b| dist[*b].total_cmp(&dist[*a]).then(b.cmp(a)));
    let graph = street_graph(structures);
    let reach_ft = express_reach_ft(kit);
    let parts: Vec<Vec<usize>> = cut_tree(&order, &parent, &pool, per)
        .into_iter()
        .filter(|(_, homes)| !homes.is_empty())
        .map(|(_, mut homes)| {
            homes.sort_unstable();
            homes
        })
        .collect();
    // A pinned area keeps its pin on the part whose homes sit nearest it.
    let pinned_part = area.pinned.then(|| {
        let at = structures.sites[area.site].geo;
        let near = |homes: &Vec<usize>| {
            homes
                .iter()
                .filter_map(|h| structures.homes[*h])
                .map(|h| crate::geo::dist_ft(at, structures.sites[h.site].geo))
                .fold(f64::INFINITY, f64::min)
        };
        (0..parts.len())
            .min_by(|a, b| near(&parts[*a]).total_cmp(&near(&parts[*b])))
            .unwrap_or(0)
    });
    parts
        .into_iter()
        .enumerate()
        .map(|(i, homes)| {
            let pin = (pinned_part == Some(i)).then_some(area.site);
            area_for(
                structures, &graph, &dist, &parent, kit, inputs, reach_ft, pin, homes,
            )
        })
        .collect()
}
