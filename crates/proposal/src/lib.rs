//! Amortized inference: a model trained on sim samples proposes where a fault is, so the sim only has to verify the top few
//! (`docs/DESIGN.md`). Trained in Python on Rust's files, and a pure function here. See `docs/tasks/12-training-and-model-selection.md`.
#![deny(missing_docs)]

pub mod gnn;
pub mod graph;
pub mod model;
pub mod rows;
pub mod spectrum;

pub use gnn::{GraphModel, Proposer};
pub use graph::{GraphNode, NODE_KINDS, graph_nodes};
pub use model::{
    CALIBRATION_FEATURES, Calibration, Model, ModelError, Proposal, calibration_features,
    trained_for,
};
pub use rows::{ComponentRow, FEATURE_NAMES, component_rows};
