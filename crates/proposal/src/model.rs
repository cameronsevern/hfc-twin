//! The exported model: boosted trees as a data file, evaluated in-process. Records what it was trained on (invariant 3), and
//! refuses to run on anything else.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::rows::{ComponentRow, FEATURE_NAMES};

/// Why a model can't be used.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ModelError {
    /// It was trained on another observation schema.
    #[error("model trained on observation schema {found}, this is {expected}")]
    ObservationSchema {
        /// What it records.
        found: String,
        /// What this build has.
        expected: String,
    },
    /// It was trained on another fault schema.
    #[error("model trained on fault schema {found}, this is {expected}")]
    FaultSchema {
        /// What it records.
        found: String,
        /// What this build has.
        expected: String,
    },
    /// Its features aren't these.
    #[error("model's {0} features aren't this build's")]
    Features(usize),
    /// It reads a feature this build doesn't compute.
    #[error("model reads feature {0}, which this build doesn't compute")]
    UnknownFeature(String),
    /// Its weights don't have the shape its architecture says.
    #[error("model's weights are the wrong shape: {0}")]
    Shape(String),
    /// A candidate has no place in the plant's graph.
    #[error("candidate {0} isn't in the plant's graph")]
    UnknownComponent(String),
}

/// One tree node. Leaves have `left` and `right` of -1.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Node {
    /// Feature index.
    pub f: i32,
    /// Threshold: at or below goes left.
    pub t: f64,
    /// Left child.
    pub l: i32,
    /// Right child.
    pub r: i32,
    /// Leaf value.
    pub v: f64,
    /// Whether a missing value goes left.
    pub m: bool,
}

/// One boosted ensemble.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Ensemble {
    /// Added to every prediction.
    pub baseline: Vec<f64>,
    /// `trees[iteration][class]`. One class for a binary head.
    pub trees: Vec<Vec<Vec<Node>>>,
}

fn eval_tree(tree: &[Node], x: &[Option<f64>]) -> f64 {
    let mut at = 0usize;
    // A well-formed tree can't loop, and the bound holds for a file that isn't.
    for _ in 0..tree.len() {
        let Some(node) = tree.get(at) else {
            return 0.0;
        };
        if node.l < 0 {
            return node.v;
        }
        let go_left = match x.get(node.f as usize).copied().flatten() {
            Some(value) => value <= node.t,
            None => node.m,
        };
        at = (if go_left { node.l } else { node.r }) as usize;
    }
    0.0
}

impl Ensemble {
    /// Raw scores, one per class.
    pub fn raw(&self, x: &[Option<f64>]) -> Vec<f64> {
        let mut out = self.baseline.clone();
        for iteration in &self.trees {
            for (class, tree) in iteration.iter().enumerate() {
                if let Some(slot) = out.get_mut(class) {
                    *slot += eval_tree(tree, x);
                }
            }
        }
        out
    }
}

/// An exported proposal model.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Model {
    /// Which candidate this is.
    pub name: String,
    /// The observation schema it trained on (invariant 3).
    pub observation_schema: String,
    /// The fault schema it trained on.
    pub fault_schema: String,
    /// Always `Synthetic` until field data exists (invariant 11).
    pub trained_on: schema::common::Origin,
    /// Its feature names, in order.
    pub features: Vec<String>,
    /// Where a fault is.
    pub component: Ensemble,
    /// What it is, given where.
    pub class: Ensemble,
    /// Class names, in the class head's order.
    pub classes: Vec<String>,
    /// Softens scores across candidates, fitted on data it didn't train on.
    pub temperature: f64,
    /// How often a verified top answer is right. Fitted on held-out runs of the whole diagnoser.
    #[serde(default)]
    pub calibration: Option<Calibration>,
    /// The plant it trained on: `"2"` for version 2 plant, with premises, fittings and fiber (task 24). A file from before
    /// names none, and was trained on version 1 plant. The twin asks a model only about the plant it trained on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plant_schema: Option<String>,
}

/// Whether a model trained on `plant_schema` (none is version 1) is for a plant that does or doesn't have version 2 parts.
pub fn trained_for(plant_schema: Option<&str>, has_version_2_parts: bool) -> bool {
    match plant_schema {
        None | Some("1") => !has_version_2_parts,
        Some(_) => has_version_2_parts,
    }
}

/// A logistic fit of how often a verified top answer turns out right.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Calibration {
    /// One per [`calibration_features`].
    pub coef: Vec<f64>,
    /// The intercept.
    pub intercept: f64,
}

/// How many features a [`Calibration`] reads.
pub const CALIBRATION_FEATURES: usize = 5;

/// What a calibrator reads off a verified answer. χ² treats correlated features as independent, so the raw posterior says "certain" far
/// too readily, and what it says has to be measured against how often it's right.
pub fn calibration_features(
    posterior: f64,
    chi2_per_dof: f64,
    model_probability: f64,
    healthy: bool,
    affected_share: f64,
) -> [f64; CALIBRATION_FEATURES] {
    let logit = |p: f64| {
        let p = p.clamp(1e-4, 1.0 - 1e-4);
        (p / (1.0 - p)).ln()
    };
    [
        logit(posterior),
        (1.0 + chi2_per_dof.max(0.0)).ln(),
        logit(model_probability),
        if healthy { 1.0 } else { 0.0 },
        affected_share,
    ]
}

impl Calibration {
    /// The probability the top answer's right.
    pub fn confidence(&self, x: &[f64; CALIBRATION_FEATURES]) -> f64 {
        let z: f64 = self.intercept + self.coef.iter().zip(x).map(|(c, v)| c * v).sum::<f64>();
        1.0 / (1.0 + (-z).exp())
    }
}

/// What a model says about one place.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Proposal {
    /// The component. Empty for healthy.
    pub component: String,
    /// Its probability across candidates.
    pub probability: f64,
    /// The cause it most likely is, if it's a fault.
    pub class: Option<String>,
}

impl Model {
    /// Reads and checks a model.
    pub fn from_json(text: &str) -> Result<Model, Box<dyn std::error::Error + Send + Sync>> {
        let model: Model = serde_json::from_str(text)?;
        model.check()?;
        Ok(model)
    }

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
        self.columns()?;
        Ok(())
    }

    /// Where each feature it reads is in this build's rows. A model names the features it reads and is given those, so a
    /// feature added later doesn't stop an earlier model loading or change what it says (task 24).
    pub fn columns(&self) -> Result<Vec<usize>, ModelError> {
        let index: std::collections::BTreeMap<&str, usize> = FEATURE_NAMES
            .iter()
            .enumerate()
            .map(|(i, name)| (name.as_str(), i))
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        self.features
            .iter()
            .map(|name| {
                if !seen.insert(name.as_str()) {
                    return Err(ModelError::Features(self.features.len()));
                }
                index
                    .get(name.as_str())
                    .copied()
                    .ok_or_else(|| ModelError::UnknownFeature(name.clone()))
            })
            .collect()
    }

    /// A row's values in this model's own feature order.
    fn project<'r>(
        &self,
        x: &'r [Option<f64>],
        columns: &[usize],
    ) -> std::borrow::Cow<'r, [Option<f64>]> {
        let identity = columns.len() == x.len() && columns.iter().enumerate().all(|(i, c)| i == *c);
        if identity {
            std::borrow::Cow::Borrowed(x)
        } else {
            std::borrow::Cow::Owned(
                columns
                    .iter()
                    .map(|c| x.get(*c).copied().flatten())
                    .collect(),
            )
        }
    }

    /// Scores every candidate: a softmax across them, so they sum to 1. Best first, ties by name.
    pub fn propose(&self, rows: &[ComponentRow]) -> Vec<Proposal> {
        // Checked when it was read.
        let columns = self.columns().unwrap_or_default();
        let xs: Vec<std::borrow::Cow<'_, [Option<f64>]>> =
            rows.iter().map(|r| self.project(&r.x, &columns)).collect();
        let scores: Vec<f64> = xs
            .iter()
            .map(|x| self.component.raw(x).first().copied().unwrap_or(0.0) / self.temperature)
            .collect();
        let peak = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let total: f64 = scores.iter().map(|s| (s - peak).exp()).sum();
        let mut out: Vec<Proposal> = rows
            .iter()
            .zip(&scores)
            .zip(&xs)
            .map(|((row, score), x)| {
                let class = (!row.component.is_empty())
                    .then(|| {
                        let raw = self.class.raw(x);
                        raw.iter()
                            .enumerate()
                            .max_by(|a, b| a.1.total_cmp(b.1))
                            .and_then(|(i, _)| self.classes.get(i).cloned())
                    })
                    .flatten();
                Proposal {
                    component: row.component.clone(),
                    probability: (score - peak).exp() / total,
                    class,
                }
            })
            .collect();
        out.sort_by(|a, b| {
            b.probability
                .total_cmp(&a.probability)
                .then(a.component.cmp(&b.component))
        });
        out
    }
}
