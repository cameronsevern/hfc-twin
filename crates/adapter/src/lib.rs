//! Reads devices over SNMP and TFTP into the sim's observation schema (invariant 2), pseudonymizing at ingest
//! (invariant 10). A vendor or firmware change is a fix here, never a model change. See
//! `docs/tasks/07-emulator-adapter-validation.md`.
#![deny(missing_docs)]

pub mod client;
pub mod collect;
pub mod identity;

pub use client::{SnmpClient, WireError};
pub use collect::{Budget, Collector};
pub use identity::{AddressStore, pseudonym};
