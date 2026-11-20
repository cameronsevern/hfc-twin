//! The data contracts everything else in hfc-twin reads. Types only: no physics, no
//! I/O beyond JSON. See `docs/tasks/01-schemas.md`.
//!
//! Every top-level file carries `schema_version`, so a migration can identify it, and
//! each module has its own `SCHEMA_VERSION` (CLAUDE.md invariant 3). All floats are
//! finite: JSON has no `Infinity` or `NaN`, so absent or unbounded values are an
//! `Option` or an explicit enum variant.
#![deny(missing_docs)]

pub mod capability;
pub mod common;
pub mod component;
pub mod design_rules;
pub mod environment;
pub mod fault;
pub mod fiber;
pub mod fiber_fault;
pub mod observation;
pub mod ops;
pub mod plant;
pub mod profile;
pub mod region;
pub mod rpd;
pub mod spectrum;
pub mod work_order;
