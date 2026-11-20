//! A plant as the graph model reads it: every element, its parent and what it reports. One definition for the rows it trains on and for
//! inference, so the two can't drift apart.

use features::StepFeatures;
use plant_graph::PlantGraph;
use schema::plant::Plant;
use serde::{Deserialize, Serialize};

/// One element in a plant's graph, with what it reports.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GraphNode {
    /// The element.
    pub id: String,
    /// `modem`, `amp`, `root`, `tap` or `other`; and, on version 2 plant, `ground_block`, `drop_amp`, `filter`, `cpe`,
    /// `line_eq` and `fitting` (task 24). [`NODE_KINDS`] lists them.
    pub kind: String,
    /// Index of its parent, or -1 at the root.
    pub parent: i32,
    /// Feet from its parent.
    pub len_ft: f64,
    /// The edge from its parent, which is how a span's candidate finds its place in the graph.
    pub edge: String,
    /// What it reports. `None` where there's no value.
    pub x: Vec<Option<f64>>,
}

/// Every kind of node, in the order a graph model trained by task 24 reads them. A model from before reads the first
/// five, and anything else as `other`.
pub const NODE_KINDS: [&str; 11] = [
    "modem",
    "amp",
    "root",
    "tap",
    "other",
    "ground_block",
    "drop_amp",
    "filter",
    "cpe",
    "line_eq",
    "fitting",
];

/// Every element of a plant, sorted by id, so the order of the plant's lists can't change the answer; then each fitting a
/// cable records, by cable and place along it, between the cable's ends (task 24). A plant that records no fittings gives
/// exactly the nodes it did before.
pub fn graph_nodes(
    graph: &PlantGraph<'_>,
    plant: &Plant,
    features: &StepFeatures,
) -> Vec<GraphNode> {
    use schema::plant::ElementKind;
    let mut ids: Vec<&schema::plant::Element> = plant.elements.iter().collect();
    ids.sort_by(|a, b| a.id.cmp(&b.id));
    let index: std::collections::BTreeMap<_, i32> = ids
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.clone(), i as i32))
        .collect();
    // Fittings, as nodes after the elements: each edge's in order along it, the first hanging from the edge's upstream
    // element and the element at its downstream end hanging from the last.
    let mut edges: Vec<&schema::plant::Edge> = plant.edges.iter().collect();
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    let mut fitting_nodes: Vec<GraphNode> = Vec::new();
    // The node an element hangs from, and how far, where a fitting stands between it and its parent.
    let mut below_fitting: std::collections::BTreeMap<String, (i32, f64)> = Default::default();
    for edge in edges {
        let Ok(fittings) = graph.fittings(&edge.id) else {
            continue;
        };
        if fittings.is_empty() {
            continue;
        }
        let mut at: Vec<f64> = fittings.iter().map(|f| f.pos_ft).collect();
        at.sort_by(f64::total_cmp);
        let len_ft = edge.kind.cable().map_or(0.0, |(_, len_ft)| len_ft);
        let mut parent = index.get(&edge.from.element_id).copied().unwrap_or(-1);
        let mut last_ft = 0.0;
        for (i, pos_ft) in at.iter().enumerate() {
            fitting_nodes.push(GraphNode {
                id: format!("{}@{i}", edge.id),
                kind: "fitting".to_string(),
                parent,
                len_ft: (pos_ft - last_ft).max(0.0),
                edge: String::new(),
                x: Vec::new(),
            });
            parent = (ids.len() + fitting_nodes.len() - 1) as i32;
            last_ft = *pos_ft;
        }
        below_fitting.insert(
            edge.to.element_id.to_string(),
            (parent, (len_ft - last_ft).max(0.0)),
        );
    }
    let mut nodes: Vec<GraphNode> = ids
        .iter()
        .map(|element| {
            let parent = graph.parent(&element.id).ok().flatten();
            let edge_id = parent.map_or_else(String::new, |(_, edge)| edge.id.to_string());
            let (parent_index, len_ft) = parent.map_or((-1, 0.0), |(p, edge)| {
                let len = edge.kind.cable().map_or(0.0, |(_, len_ft)| len_ft);
                (index.get(&p.id).copied().unwrap_or(-1), len)
            });
            let (parent_index, len_ft) = below_fitting
                .get(element.id.as_str())
                .copied()
                .unwrap_or((parent_index, len_ft));
            let (kind, x) = match element.kind {
                ElementKind::Modem => {
                    let m = features.modems.iter().find(|m| m.element_id == element.id);
                    let x = m.map_or_else(Vec::new, |m| {
                        let mut x = vec![
                            m.level_drift_db,
                            m.level_drift_slope_db_per_100mhz,
                            m.us_tx_drift_db,
                            m.mer_drift_db,
                            m.chan_est_ripple_db,
                            m.preeq_nmter_drift_db,
                            m.preeq_echo_rise_db,
                            m.preeq_echo_ft.map(|f| f / 1000.0),
                            m.uncorrectable_rate.map(|r| r.max(1e-9).log10()),
                            Some(if m.operational { 0.0 } else { 1.0 }),
                            Some((m.t3_delta as f64).ln_1p()),
                            Some((m.t4_delta as f64).ln_1p()),
                            Some(if m.is_d31 { 1.0 } else { 0.0 }),
                        ];
                        // 8 coarse level buckets, so the file stays a size a laptop can train on.
                        for chunk in m.level_drift_buckets.chunks(8) {
                            let (mut sum, mut n) = (0.0, 0.0);
                            for v in chunk.iter().flatten() {
                                sum += v;
                                n += 1.0;
                            }
                            x.push((n > 0.0).then(|| sum / n));
                        }
                        x
                    });
                    ("modem", x)
                }
                ElementKind::Amp { .. } | ElementKind::Node { .. } | ElementKind::Rpd { .. } => {
                    let a = features.actives.iter().find(|a| a.element_id == element.id);
                    let x = vec![
                        a.and_then(|a| a.out_level_drift_db),
                        a.and_then(|a| a.v_drift),
                        a.map(|a| if a.silent { 1.0 } else { 0.0 }),
                    ];
                    (
                        if matches!(element.kind, ElementKind::Amp { .. }) {
                            "amp"
                        } else {
                            "root"
                        },
                        x,
                    )
                }
                ElementKind::Tap => ("tap", Vec::new()),
                ElementKind::GroundBlock => ("ground_block", Vec::new()),
                ElementKind::DropAmp => ("drop_amp", Vec::new()),
                ElementKind::Filter => ("filter", Vec::new()),
                ElementKind::Cpe => ("cpe", Vec::new()),
                ElementKind::LineEqualizer => ("line_eq", Vec::new()),
                _ => ("other", Vec::new()),
            };
            GraphNode {
                id: element.id.to_string(),
                kind: kind.to_string(),
                parent: parent_index,
                len_ft,
                edge: edge_id,
                x,
            }
        })
        .collect();
    nodes.extend(fitting_nodes);
    nodes
}
