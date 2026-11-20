//! An RPD's own state, which an analog node doesn't have. A new schema, so the observation schema's version holds
//! (invariant 3). The names follow [R-PHY §6.9, §7.2, §B.5]; each comes out of a fault's parameters and timers, never
//! the other way (invariant 2). See `docs/tasks/10-daa-and-migration.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{ElementId, Origin, Timestamp};

/// Version of this schema.
pub const SCHEMA_VERSION: &str = "1";

/// `TopLevelRPDState`, as far as this model moves through it.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpdState {
    /// Running.
    Operational,
    /// Starting up.
    Initializing,
    /// Not reachable.
    Offline,
}

/// `RpdGcpConnectionStatus`.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcpStatus {
    /// Connected.
    Connected,
    /// Trying again.
    Reconnecting,
    /// Not trying.
    Inactive,
}

/// `PtpResult` [R-PHY §B.5].
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtpResult {
    /// Locked.
    Synchronized,
    /// Holding over, in spec.
    HoldoverWithinSpec,
    /// Holding over, out of spec.
    HoldoverOutOfSpec,
    /// Nothing to go on: not reachable, or not started.
    FreeRunning,
}

/// What one RPD shows at one time step.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RpdObservation {
    /// Version of this schema.
    pub schema_version: String,
    /// Simulated time, seconds.
    pub t: Timestamp,
    /// Whether this is simulated or measured (invariant 11).
    pub origin: Origin,
    /// The RPD.
    pub element_id: ElementId,
    /// Its state.
    pub top_level_state: RpdState,
    /// Its control plane.
    pub gcp_status: GcpStatus,
    /// Its timing.
    pub ptp_result: PtpResult,
    /// Whether its RF port is passing signal.
    pub rf_output_up: bool,
    /// Whether its supply sees it drawing power: the difference between a dead RPD and a cut fiber.
    pub drawing_power: bool,
}
