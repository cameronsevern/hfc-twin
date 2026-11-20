//! The physics simulation: what a real plant would measure, computed from the plant graph,
//! profile, library, environment and fault set. See `docs/physics.md` and `docs/tasks/03-physics-sim.md`.
#![deny(missing_docs)]

pub mod ac;
pub mod align;
pub mod budget;
pub mod cable;
pub mod echo;
pub mod error;
pub mod faults;
pub mod fec;
pub mod noise;
pub mod optics;
pub mod overlay;
pub mod passive;
pub mod preeq;
pub mod rng;
pub mod rpd;
pub mod state;
pub mod support;
pub mod telemetry;
pub mod transfer;
pub mod units;
pub mod upstream;
pub mod walk;

pub use error::PhysicsError;
pub use overlay::Overlay;
pub use walk::{Band, Grid, ModemView, Walker};
