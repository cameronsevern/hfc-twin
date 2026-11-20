//! Diagnosis as the inverse of the sim: which fault parameters best explain what was observed. The
//! plant is the prior, the sim is the likelihood and the policy table says what to do. No training
//! data, so this is the floor every model must beat. See `docs/tasks/05-diagnosis-no-ml.md`.
#![deny(missing_docs)]

pub mod candidates;
pub mod engine;
pub mod fiber;
pub mod fit;
pub mod outage;
pub mod switching;
pub mod who;
pub mod work_order;

pub use engine::{DiagnoserConfig, Diagnosis, Hypothesis, diagnose};
pub use fit::{FitResult, fit};
pub use who::{Who, who_is_affected};
pub use work_order::work_order;
