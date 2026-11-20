//! A read-only, indexed view of a validated plant. See `docs/tasks/02-plant-graph.md`.
//!
//! This crate answers topology questions and returns paths and sets, with feet as the only
//! number. It never computes a dB, a volt or a delay: those are the sim's, built on these
//! answers (`docs/physics.md` §11).
#![deny(missing_docs)]

mod checks;
mod fiber;
mod graph;
mod path;
mod power;
mod premises;

pub use checks::{DesignIssue, FaultRefError, FaultRefProblem};
pub use fiber::{FiberGraph, FiberQueryError, RegionIssue, check_region};
pub use graph::{GraphError, PlantGraph, QueryError};
pub use path::{Path, PathDirection, Segment};
pub use power::{PowerFeed, PowerOverlay, PowerSegment};
pub use premises::Premises;
