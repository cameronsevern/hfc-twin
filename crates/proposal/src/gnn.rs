//! The graph model as a data file, evaluated in-process: the same network as `ml/train_gnn.py`. Modems encode what they report, messages
//! run along the tree, and each candidate reads what's below it and what isn't, which is the question a fault there asks. Plain loops in
//! a fixed order, so the same inputs give the same bytes (invariant 4). See `docs/tasks/15-held-out-maps-selection.md`.

use std::collections::BTreeMap;

use features::StepFeatures;
use plant_graph::PlantGraph;
use schema::plant::Plant;
use serde::{Deserialize, Serialize};

use crate::graph::{GraphNode, graph_nodes};
use crate::model::{Calibration, Model, ModelError, Proposal};
use crate::rows::{ComponentKind, ComponentRow};

/// 13 modem measures and 8 coarse level buckets, as `graph_nodes` writes them.
const WIDTH: usize = 21;
/// The service group's own features, the first of every candidate's row.
const GLOBAL: usize = 11;
/// The node kinds a model from before task 24 reads, which a file that names none has.
const V1_NODE_KINDS: [&str; 5] = ["modem", "amp", "root", "tap", "other"];
/// The candidate kinds a model from before task 24 reads: healthy, span, amp, drop, supply, root.
const V1_CANDIDATE_KINDS: usize = 6;
const KIND_EMBEDDING: usize = 16;
const ROUNDS: usize = 3;

/// One linear layer. `w[out][in]`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Linear {
    /// Weights.
    pub w: Vec<Vec<f64>>,
    /// Bias.
    pub b: Vec<f64>,
}

impl Linear {
    fn check(&self, inputs: usize, outputs: usize, name: &str) -> Result<(), ModelError> {
        if self.w.len() != outputs
            || self.b.len() != outputs
            || self.w.iter().any(|row| row.len() != inputs)
        {
            return Err(ModelError::Shape(format!(
                "{name} should take {inputs} to {outputs}"
            )));
        }
        Ok(())
    }

    fn apply(&self, x: &[f64]) -> Vec<f64> {
        self.w
            .iter()
            .zip(&self.b)
            .map(|(row, bias)| row.iter().zip(x).map(|(w, v)| w * v).sum::<f64>() + bias)
            .collect()
    }

    fn apply_relu(&self, x: &[f64]) -> Vec<f64> {
        let mut out = self.apply(x);
        for v in &mut out {
            *v = v.max(0.0);
        }
        out
    }
}

/// An exported graph model.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GraphModel {
    /// Which candidate this is.
    pub name: String,
    /// Always `tree_gnn`, which is how a file says it's this kind of model.
    pub architecture: String,
    /// The observation schema it trained on (invariant 3).
    pub observation_schema: String,
    /// The fault schema it trained on.
    pub fault_schema: String,
    /// Always `Synthetic` until field data exists (invariant 11).
    pub trained_on: schema::common::Origin,
    /// Layer width.
    pub hidden: usize,
    /// Encodes a node's input, ReLU between the two layers.
    pub enc: [Linear; 2],
    /// One per round of messages along the tree.
    pub rounds: Vec<Linear>,
    /// Mixes a node with below, outside and everywhere. ReLU after each.
    pub mix: [Linear; 2],
    /// One row per [`ComponentKind`].
    pub kind_embedding: Vec<Vec<f64>>,
    /// Scores a candidate, ReLU between the two layers.
    pub score: [Linear; 2],
    /// Softens scores across candidates, fitted on data it didn't train on.
    pub temperature: f64,
    /// How often a verified top answer is right.
    #[serde(default)]
    pub calibration: Option<Calibration>,
    /// The node kinds it reads, in its input's order; a node of any other kind reads as `other`. A file from before task
    /// 24 names none and reads [`V1_NODE_KINDS`].
    #[serde(default = "v1_node_kinds")]
    pub node_kinds: Vec<String>,
    /// The plant it trained on, as [`crate::Model`]'s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plant_schema: Option<String>,
}

fn v1_node_kinds() -> Vec<String> {
    V1_NODE_KINDS.iter().map(|k| k.to_string()).collect()
}

fn scaled(value: f64) -> f64 {
    value.clamp(-50.0, 50.0) / 10.0
}

fn add(sum: &mut [f64], x: &[f64]) {
    for (s, v) in sum.iter_mut().zip(x) {
        *s += v;
    }
}

fn kind_index(kind: ComponentKind) -> usize {
    match kind {
        ComponentKind::Healthy => 0,
        ComponentKind::Span => 1,
        ComponentKind::Amp => 2,
        ComponentKind::Drop => 3,
        ComponentKind::Supply => 4,
        ComponentKind::Root => 5,
        ComponentKind::Premises => 6,
        ComponentKind::DropCable => 7,
    }
}

impl GraphModel {
    /// Whether this build can run it.
    pub fn check(&self) -> Result<(), ModelError> {
        // A model trained on an earlier observation version still reads this one's (task 22).
        if !schema::observation::model_can_read(&self.observation_schema) {
            return Err(ModelError::ObservationSchema {
                found: self.observation_schema.clone(),
                expected: schema::observation::SCHEMA_VERSION.to_string(),
            });
        }
        if !schema::fault::model_can_read(&self.fault_schema) {
            return Err(ModelError::FaultSchema {
                found: self.fault_schema.clone(),
                expected: schema::fault::SCHEMA_VERSION.to_string(),
            });
        }
        let h = self.hidden;
        if !self.node_kinds.iter().any(|k| k == "other") {
            return Err(ModelError::Shape("node kinds without other".to_string()));
        }
        self.enc[0].check(self.input(), h, "enc.0")?;
        self.enc[1].check(h, h, "enc.1")?;
        if self.rounds.len() != ROUNDS {
            return Err(ModelError::Shape(format!("{ROUNDS} rounds")));
        }
        for layer in &self.rounds {
            layer.check(h * 3, h, "round")?;
        }
        self.mix[0].check(h * 4 + GLOBAL, h, "mix.0")?;
        self.mix[1].check(h, h, "mix.1")?;
        if !(V1_CANDIDATE_KINDS..=kind_index(ComponentKind::DropCable) + 1)
            .contains(&self.kind_embedding.len())
            || self
                .kind_embedding
                .iter()
                .any(|k| k.len() != KIND_EMBEDDING)
        {
            return Err(ModelError::Shape("kind embedding".to_string()));
        }
        self.score[0].check(h + KIND_EMBEDDING, h, "score.0")?;
        self.score[1].check(h, 1, "score.1")?;
        Ok(())
    }

    fn input(&self) -> usize {
        WIDTH * 2 + self.node_kinds.len() + 1
    }

    /// One score per candidate, before the temperature.
    pub fn scores(
        &self,
        nodes: &[GraphNode],
        rows: &[ComponentRow],
    ) -> Result<Vec<f64>, ModelError> {
        let n = nodes.len();
        let h = self.hidden;
        let mut root = 0usize;
        let mut by_id = BTreeMap::new();
        let mut by_edge = BTreeMap::new();
        let mut parent: Vec<Option<usize>> = Vec::with_capacity(n);
        let mut modems = Vec::new();
        let mut hidden: Vec<Vec<f64>> = Vec::with_capacity(n);
        for (i, node) in nodes.iter().enumerate() {
            let up = usize::try_from(node.parent).ok().filter(|p| *p < n);
            if up.is_none() {
                root = i;
            }
            parent.push(up);
            by_id.insert(node.id.as_str(), i);
            if !node.edge.is_empty() {
                by_edge.insert(node.edge.as_str(), i);
            }
            if node.kind == "modem" {
                modems.push(i);
            }
            let input = self.input();
            let mut x = vec![0.0; input];
            for (j, value) in node.x.iter().take(WIDTH).enumerate() {
                // A non-finite value is written as null in the rows it trained on, so it's missing here too.
                if let Some(v) = value.filter(|v| v.is_finite()) {
                    x[j] = scaled(v);
                    x[WIDTH + j] = 1.0;
                }
            }
            let kind = self
                .node_kinds
                .iter()
                .position(|k| *k == node.kind)
                .or_else(|| self.node_kinds.iter().position(|k| k == "other"))
                .unwrap_or(0);
            x[WIDTH * 2 + kind] = 1.0;
            x[input - 1] = node.len_ft.ln_1p() / 8.0;
            hidden.push(self.enc[1].apply(&self.enc[0].apply_relu(&x)));
        }

        let mut child_count = vec![0.0f64; n];
        for up in parent.iter().flatten() {
            child_count[*up] += 1.0;
        }
        for layer in &self.rounds {
            let mut from_children = vec![vec![0.0; h]; n];
            for (i, up) in parent.iter().enumerate() {
                if let Some(up) = up {
                    add(&mut from_children[*up], &hidden[i]);
                }
            }
            let mut next = Vec::with_capacity(n);
            let zero = vec![0.0; h];
            for i in 0..n {
                let count = child_count[i].max(1.0);
                let mut input = hidden[i].clone();
                input.extend(from_children[i].iter().map(|v| v / count));
                input.extend_from_slice(parent[i].map_or(&zero, |p| &hidden[p]));
                let mut out = layer.apply_relu(&input);
                add(&mut out, &hidden[i]);
                next.push(out);
            }
            hidden = next;
        }

        // What's below each element, and what isn't.
        let mut below_sum = vec![vec![0.0; h]; n];
        let mut below_n = vec![0.0f64; n];
        let mut total = vec![0.0; h];
        for &modem in &modems {
            add(&mut total, &hidden[modem]);
            let mut at = Some(modem);
            // The bound holds for a file that isn't a tree.
            for _ in 0..=n {
                let Some(here) = at else { break };
                add(&mut below_sum[here], &hidden[modem]);
                below_n[here] += 1.0;
                at = parent[here];
            }
        }
        let m = modems.len().max(1) as f64;
        let global: Vec<f64> = (0..GLOBAL)
            .map(|j| {
                rows.first()
                    .and_then(|r| r.x.get(j).copied().flatten())
                    .filter(|v| v.is_finite())
                    .map_or(0.0, scaled)
            })
            .collect();

        let mut memo: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let place = match row.kind {
                ComponentKind::Span | ComponentKind::DropCable => {
                    by_edge.get(row.component.as_str()).copied()
                }
                ComponentKind::Amp | ComponentKind::Drop => {
                    by_id.get(row.component.as_str()).copied()
                }
                // A part in a home is an element; its in-home cable an edge.
                ComponentKind::Premises => by_id
                    .get(row.component.as_str())
                    .or_else(|| by_edge.get(row.component.as_str()))
                    .copied(),
                _ => Some(root),
            }
            .ok_or_else(|| ModelError::UnknownComponent(row.component.clone()))?;
            let Some(embedding) = self.kind_embedding.get(kind_index(row.kind)) else {
                return Err(ModelError::UnknownComponent(row.component.clone()));
            };
            let z = memo.entry(place).or_insert_with(|| {
                let count = below_n[place].max(1.0);
                let rest = (m - below_n[place]).max(1.0);
                let mut input = hidden[place].clone();
                input.extend(below_sum[place].iter().map(|v| v / count));
                input.extend(
                    total
                        .iter()
                        .zip(&below_sum[place])
                        .map(|(t, b)| (t - b) / rest),
                );
                input.extend(total.iter().map(|t| t / m));
                input.extend_from_slice(&global);
                self.mix[1].apply_relu(&self.mix[0].apply_relu(&input))
            });
            let mut input = z.clone();
            input.extend_from_slice(embedding);
            let score = self.score[1].apply(&self.score[0].apply_relu(&input));
            out.push(score.first().copied().unwrap_or(0.0));
        }
        Ok(out)
    }

    /// Scores every candidate: a softmax across them, so they sum to 1. Best first, ties by name. It names no cause.
    pub fn propose(
        &self,
        nodes: &[GraphNode],
        rows: &[ComponentRow],
    ) -> Result<Vec<Proposal>, ModelError> {
        let scores: Vec<f64> = self
            .scores(nodes, rows)?
            .into_iter()
            .map(|s| s / self.temperature)
            .collect();
        let peak = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let total: f64 = scores.iter().map(|s| (s - peak).exp()).sum();
        let mut out: Vec<Proposal> = rows
            .iter()
            .zip(&scores)
            .map(|(row, score)| Proposal {
                component: row.component.clone(),
                probability: (score - peak).exp() / total,
                class: None,
            })
            .collect();
        out.sort_by(|a, b| {
            b.probability
                .total_cmp(&a.probability)
                .then(a.component.cmp(&b.component))
        });
        Ok(out)
    }
}

/// Either kind of proposal model.
#[derive(Debug, Clone, PartialEq)]
pub enum Proposer {
    /// Boosted trees on hand features.
    Trees(Box<Model>),
    /// The graph network.
    Graph(Box<GraphModel>),
}

impl Proposer {
    /// How often its verified top answer is right, where that's been fitted.
    pub fn calibration(&self) -> Option<&Calibration> {
        match self {
            Proposer::Trees(model) => model.calibration.as_ref(),
            Proposer::Graph(model) => model.calibration.as_ref(),
        }
    }

    /// The plant it trained on: none is version 1.
    pub fn plant_schema(&self) -> Option<&str> {
        match self {
            Proposer::Trees(model) => model.plant_schema.as_deref(),
            Proposer::Graph(model) => model.plant_schema.as_deref(),
        }
    }

    /// Reads and checks a model of either kind. A graph model's file says so.
    pub fn from_json(text: &str) -> Result<Proposer, Box<dyn std::error::Error + Send + Sync>> {
        let value: serde_json::Value = serde_json::from_str(text)?;
        if value.get("architecture").is_some() {
            let model: GraphModel = serde_json::from_value(value)?;
            model.check()?;
            Ok(Proposer::Graph(Box::new(model)))
        } else {
            let model: Model = serde_json::from_value(value)?;
            model.check()?;
            Ok(Proposer::Trees(Box::new(model)))
        }
    }

    /// What it proposes for one plant at one step, best first.
    pub fn propose(
        &self,
        graph: &PlantGraph<'_>,
        plant: &Plant,
        features: &StepFeatures,
        rows: &[ComponentRow],
    ) -> Result<Vec<Proposal>, ModelError> {
        match self {
            Proposer::Trees(model) => Ok(model.propose(rows)),
            Proposer::Graph(model) => model.propose(&graph_nodes(graph, plant, features), rows),
        }
    }
}
