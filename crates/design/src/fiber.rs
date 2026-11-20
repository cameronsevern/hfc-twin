//! Fiber from the headend to each node: a Steiner tree along streets. Reported, never stored, since it's a derivation of the map
//! and the design (invariant 1). The sim has no optical budget, so nothing in the RF design depends on it.

use schema::common::{LonLat, ServiceGroupId};
use schema::region::StreetMap;
use serde::{Deserialize, Serialize};

use crate::route::{StreetGraph, steiner_tree};

/// The route.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FiberRoute {
    /// Its length, feet. Shared runs count once.
    pub route_ft: f64,
    /// Each run, as its two ends.
    pub lines: Vec<(LonLat, LonLat)>,
    /// Nodes on a piece of street the headend's piece doesn't connect to.
    pub unreached: Vec<ServiceGroupId>,
}

/// Routes fiber from the map's headend to each site.
pub fn fiber_route(map: &StreetMap, sites: &[(ServiceGroupId, LonLat)]) -> FiberRoute {
    let graph = StreetGraph::build(&map.streets, &[], 0.0);
    let Some(root) = graph.nearest_vertex(map.headend) else {
        return FiberRoute {
            unreached: sites.iter().map(|(id, _)| id.clone()).collect(),
            ..FiberRoute::default()
        };
    };
    let terminals: Vec<(usize, &ServiceGroupId)> = sites
        .iter()
        .filter_map(|(id, geo)| graph.nearest_vertex(*geo).map(|v| (v, id)))
        .collect();
    let vertices: Vec<usize> = terminals.iter().map(|(v, _)| *v).collect();
    let tree = steiner_tree(&graph, root, &vertices);
    let mut route = FiberRoute::default();
    for (vertex, (parent, len_ft, _)) in &tree.parent {
        route.route_ft += len_ft;
        route
            .lines
            .push((graph.vertices[*parent].geo, graph.vertices[*vertex].geo));
    }
    route.unreached = terminals
        .iter()
        .filter(|(v, _)| tree.unreached.contains(v))
        .map(|(_, id)| (*id).clone())
        .collect();
    route
}
