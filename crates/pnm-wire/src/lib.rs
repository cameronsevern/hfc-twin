//! The real wire formats, so telemetry from the emulator travels the path a real device's would. See
//! `docs/tasks/07-emulator-adapter-validation.md`.
#![deny(missing_docs)]

pub mod oids;
pub mod pnm;
pub mod snmp;
pub mod tftp;
