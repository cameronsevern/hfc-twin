//! A designed region run day to day (`docs/tasks/26-operations-sim.md`). Things happen to the plant as physical faults,
//! households react to what their own modems show, a call centre takes their calls, and trucks drive the region's streets to
//! jobs a dispatcher sends them on, from calls alone or with the twin's reading of telemetry. The dispatcher never sees the
//! fault list.
//!
//! A world is a fold (invariant 4): its state at a time is a pure function of its inputs, its seed and the log of actions,
//! each stamped with the world time it took effect. The step is one minute of world time; how fast a page shows it is the
//! page's business. Everything a world writes is `Synthetic` (invariant 11).
#![deny(missing_docs)]

pub mod field;
pub mod hazards;
pub mod scenario;
pub mod world;

pub use scenario::{Planned, Scenario, Scenarios};
pub use world::{
    Action, NodeInput, Pair, Policy, Sample, Sky, Standing, TruthItem, World, WorldError,
    WorldInputs,
};
