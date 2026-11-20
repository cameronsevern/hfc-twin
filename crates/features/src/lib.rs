//! The feature layer: what models and the diagnoser read, derived purely from observation, plant and
//! profile (`docs/DESIGN.md`, "Data contract"). Values are relative, not absolute, so they hold across
//! profiles and hardware generations. See `docs/tasks/04-faults-and-telemetry.md`.
#![deny(missing_docs)]

pub mod extract;
pub mod reference;
pub mod sampler;
pub mod spectrum;
pub mod sweep;

pub use extract::{ModemFeatures, ServiceGroupFeatures, StepFeatures, features_for};
pub use reference::DesignReference;
