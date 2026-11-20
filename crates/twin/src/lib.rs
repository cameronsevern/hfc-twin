//! The twin: one plant graph per node, and every operation over it. Nothing derived is stored
//! (invariant 1): a telemetry call simulates, and a diagnosis re-derives. See
//! `docs/tasks/06-mcp-server-and-min-ui.md`.
#![deny(missing_docs)]

pub mod cache;
pub mod lifecycle;
pub mod operate;
pub mod state;
pub mod tools;

pub use design::osm::OsmReport;
pub use state::{Twin, TwinError, load_as_built, load_one_street};
pub use tools::{ToolError, ToolSpec, call_tool, tool_specs};
