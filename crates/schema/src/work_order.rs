//! The output contract, and the operator-owned table that decides the fix.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::{ElementId, LonLat, ServiceGroupId, Timestamp};
use crate::component::ComponentClass;
use crate::fault::{Fault, FaultKind};

/// Version of this schema.
pub const SCHEMA_VERSION: &str = "6";

/// Namespace for work order ids: UUID v5 of `hfc-twin/work-order` in the URL namespace.
fn namespace() -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, b"hfc-twin/work-order")
}

/// What caused a problem: one variant per fault class in §8, plus `Unknown`.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CauseClass {
    /// `FaultKind::WaterIngress`.
    WaterIngress,
    /// `FaultKind::ConnectorReflection`.
    ConnectorReflection,
    /// `FaultKind::Suckout`.
    Suckout,
    /// `FaultKind::FrequencySelectiveLoss`.
    FrequencySelectiveLoss,
    /// `FaultKind::AmpDrift`.
    AmpDrift,
    /// `FaultKind::UpstreamIngress`.
    UpstreamIngress,
    /// `FaultKind::DownstreamIngress`.
    DownstreamIngress,
    /// `FaultKind::PowerSupplySag`.
    PowerSupplySag,
    /// `FaultKind::ResonantPeak`.
    ResonantPeak,
    /// `FaultKind::AdjacentLevelOffset`.
    AdjacentLevelOffset,
    /// `FaultKind::ForwardOptics`.
    ForwardOptics,
    /// `FaultKind::CommonPathDistortion`.
    CommonPathDistortion,
    /// `FaultKind::BadDrop`.
    BadDrop,
    /// An RPD stopped.
    RpdFailure,
    /// The fiber to an RPD is cut.
    CinLinkCut,
    /// The control plane to an RPD is lost.
    GcpLoss,
    /// An RPD lost its timing.
    TimingLoss,
    /// An RPD's RF output is set wrong.
    RfmMisalignment,
    /// A fault inside one home, past its ground block: a part, a connector or the cable between them. Version 5.
    InHomeWiring,
    /// `FaultKind::Misalignment`: a wrong pad or equalizer at a fitted active. Version 5.
    Misalignment,
    /// `FiberFaultKind::FiberCut`. Version 5.
    FiberCut,
    /// `FiberFaultKind::EventLoss`: a lossy splice, connector or bend. Version 5.
    FiberEventLoss,
    /// `FiberFaultKind::TransmitterDrift`. Version 5.
    OpticalTransmitterDrift,
    /// No fault class explains the telemetry.
    Unknown,
}

impl CauseClass {
    /// Every cause class.
    pub const ALL: [CauseClass; 24] = [
        CauseClass::WaterIngress,
        CauseClass::ConnectorReflection,
        CauseClass::Suckout,
        CauseClass::FrequencySelectiveLoss,
        CauseClass::AmpDrift,
        CauseClass::UpstreamIngress,
        CauseClass::DownstreamIngress,
        CauseClass::PowerSupplySag,
        CauseClass::BadDrop,
        CauseClass::ResonantPeak,
        CauseClass::AdjacentLevelOffset,
        CauseClass::ForwardOptics,
        CauseClass::CommonPathDistortion,
        CauseClass::RpdFailure,
        CauseClass::CinLinkCut,
        CauseClass::GcpLoss,
        CauseClass::TimingLoss,
        CauseClass::RfmMisalignment,
        CauseClass::InHomeWiring,
        CauseClass::Misalignment,
        CauseClass::FiberCut,
        CauseClass::FiberEventLoss,
        CauseClass::OpticalTransmitterDrift,
        CauseClass::Unknown,
    ];

    /// A stable name, used to derive work order ids.
    pub fn name(self) -> &'static str {
        match self {
            CauseClass::WaterIngress => "WaterIngress",
            CauseClass::ConnectorReflection => "ConnectorReflection",
            CauseClass::Suckout => "Suckout",
            CauseClass::FrequencySelectiveLoss => "FrequencySelectiveLoss",
            CauseClass::AmpDrift => "AmpDrift",
            CauseClass::UpstreamIngress => "UpstreamIngress",
            CauseClass::DownstreamIngress => "DownstreamIngress",
            CauseClass::PowerSupplySag => "PowerSupplySag",
            CauseClass::BadDrop => "BadDrop",
            CauseClass::ResonantPeak => "ResonantPeak",
            CauseClass::AdjacentLevelOffset => "AdjacentLevelOffset",
            CauseClass::ForwardOptics => "ForwardOptics",
            CauseClass::CommonPathDistortion => "CommonPathDistortion",
            CauseClass::RpdFailure => "RpdFailure",
            CauseClass::CinLinkCut => "CinLinkCut",
            CauseClass::GcpLoss => "GcpLoss",
            CauseClass::TimingLoss => "TimingLoss",
            CauseClass::RfmMisalignment => "RfmMisalignment",
            CauseClass::InHomeWiring => "InHomeWiring",
            CauseClass::Misalignment => "Misalignment",
            CauseClass::FiberCut => "FiberCut",
            CauseClass::FiberEventLoss => "FiberEventLoss",
            CauseClass::OpticalTransmitterDrift => "OpticalTransmitterDrift",
            CauseClass::Unknown => "Unknown",
        }
    }
}

impl From<&FaultKind> for CauseClass {
    fn from(kind: &FaultKind) -> Self {
        match kind {
            FaultKind::WaterIngress { .. } => CauseClass::WaterIngress,
            FaultKind::ConnectorReflection { .. } => CauseClass::ConnectorReflection,
            FaultKind::Suckout { .. } => CauseClass::Suckout,
            FaultKind::FrequencySelectiveLoss { .. } => CauseClass::FrequencySelectiveLoss,
            FaultKind::AmpDrift { .. } => CauseClass::AmpDrift,
            // A wrong plug-in is fixed by realigning, not by replacing the amp: a cause of its own (version 5).
            FaultKind::Misalignment { .. } => CauseClass::Misalignment,
            FaultKind::UpstreamIngress { .. } => CauseClass::UpstreamIngress,
            FaultKind::DownstreamIngress { .. } => CauseClass::DownstreamIngress,
            FaultKind::PowerSupplySag { .. } => CauseClass::PowerSupplySag,
            FaultKind::ResonantPeak { .. } => CauseClass::ResonantPeak,
            FaultKind::AdjacentLevelOffset { .. } => CauseClass::AdjacentLevelOffset,
            FaultKind::ForwardOptics { .. } => CauseClass::ForwardOptics,
            FaultKind::CommonPathDistortion { .. } => CauseClass::CommonPathDistortion,
            FaultKind::BadDrop { .. } => CauseClass::BadDrop,
            FaultKind::RpdFailure { .. } => CauseClass::RpdFailure,
            FaultKind::CinLinkCut { .. } => CauseClass::CinLinkCut,
            FaultKind::GcpLoss { .. } => CauseClass::GcpLoss,
            FaultKind::TimingLoss { .. } => CauseClass::TimingLoss,
            FaultKind::RfmMisalignment { .. } => CauseClass::RfmMisalignment,
            // Task 19's damage events are read by `diagnosis::outage`, which raises no work order; they get causes of their
            // own when the twin wires them in, with this schema's next version.
            FaultKind::SpanCut { .. }
            | FaultKind::AnalogNodeFailure { .. }
            | FaultKind::WaterInDevice { .. }
            | FaultKind::PowerShort { .. }
            | FaultKind::DamagedDevice { .. }
            | FaultKind::UtilityOutage { .. } => CauseClass::Unknown,
        }
    }
}

impl From<&crate::fiber_fault::FiberFaultKind> for CauseClass {
    fn from(kind: &crate::fiber_fault::FiberFaultKind) -> Self {
        use crate::fiber_fault::FiberFaultKind;
        match kind {
            FiberFaultKind::FiberCut { .. } => CauseClass::FiberCut,
            FiberFaultKind::EventLoss { .. } => CauseClass::FiberEventLoss,
            FiberFaultKind::TransmitterDrift { .. } => CauseClass::OpticalTransmitterDrift,
        }
    }
}

/// The operator-owned table from cause and component to fix.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PolicyTable {
    /// Version of this schema.
    pub schema_version: String,
    /// The rows.
    pub entries: Vec<PolicyEntry>,
}

/// One row of the policy table.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PolicyEntry {
    /// The cause this row answers.
    pub cause: CauseClass,
    /// The component class it applies to, or `None` for any.
    pub component_class: Option<ComponentClass>,
    /// What to do.
    pub action: String,
    /// Parts to bring.
    pub parts: Vec<String>,
    /// Which crew does it.
    pub crew: String,
}

/// Where a work order is in its lifecycle.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkOrderStatus {
    /// Issued by diagnosis, not yet sent.
    Proposed,
    /// Sent to a crew.
    Dispatched,
    /// The crew reports it done.
    Completed,
    /// Telemetry shows the symptoms cleared.
    Verified,
    /// Telemetry shows the symptoms did not clear.
    NotVerified,
}

/// One fix to one problem.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkOrder {
    /// Version of this schema.
    pub schema_version: String,
    /// See [`WorkOrder::derive_id`].
    pub id: Uuid,
    /// The service group affected.
    pub service_group: ServiceGroupId,
    /// Simulated time of the diagnosis, seconds.
    pub t: Timestamp,
    /// The element to go to.
    pub element_id: ElementId,
    /// Where that element is, if known.
    pub geo: Option<LonLat>,
    /// The diagnosed cause.
    pub cause: CauseClass,
    /// The fault parameters that best explain the telemetry.
    pub parameters: Fault,
    /// Posterior probability of this cause, 0 to 1.
    pub confidence: f32,
    /// Number of modems affected.
    pub affected_modems: u32,
    /// What to do, from the policy table.
    pub action: String,
    /// Parts to bring, from the policy table.
    pub parts: Vec<String>,
    /// Why, rendered from a template rather than generated.
    pub explanation: String,
    /// Where the work order is in its lifecycle.
    pub status: WorkOrderStatus,
}

impl WorkOrder {
    /// The id is a v5 UUID of service group, `t`, element and cause, so the same diagnosis
    /// always gets the same id (invariant 4).
    pub fn derive_id(
        service_group: &ServiceGroupId,
        t: Timestamp,
        element_id: &ElementId,
        cause: CauseClass,
    ) -> Uuid {
        let name = format!("{service_group}\0{}\0{element_id}\0{}", t.0, cause.name());
        Uuid::new_v5(&namespace(), name.as_bytes())
    }
}

/// Brings a policy table from an earlier version to this one. Versions 2 and 3 only added causes, so an earlier table
/// still means the same, and those causes fall to its `Unknown` entry until the operator adds them. Version 4 changed
/// nothing of its own: `PolicyEntry::component_class` borrows `ComponentClass`, which component schema version 2 widened,
/// and a borrowed enum that widens is a schema change too (invariant 3). Version 5 adds causes again: in-home wiring,
/// misalignment, which version 4 read as a drifted amp, and three fiber causes. Version 6 changes nothing of its own: a work
/// order's faults are fault schema version 5's, which added task 19's damage events.
pub fn migrate_policy(mut table: PolicyTable) -> Result<PolicyTable, String> {
    match table.schema_version.as_str() {
        "1" | "2" | "3" | "4" | "5" | "6" => {
            table.schema_version = SCHEMA_VERSION.to_string();
            Ok(table)
        }
        other => Err(format!(
            "no migration from work order schema version {other}"
        )),
    }
}

/// Brings a work order from an earlier version to this one. Version 4 is version 3, as for the policy table, version 5
/// only adds causes, and version 6 only follows fault schema version 5.
pub fn migrate_work_order(mut order: WorkOrder) -> Result<WorkOrder, String> {
    match order.schema_version.as_str() {
        "1" | "2" | "3" | "4" | "5" | "6" => {
            order.schema_version = SCHEMA_VERSION.to_string();
            Ok(order)
        }
        other => Err(format!(
            "no migration from work order schema version {other}"
        )),
    }
}
