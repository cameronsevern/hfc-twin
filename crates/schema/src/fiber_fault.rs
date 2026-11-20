//! Faults on the fiber plant: a document of its own, with its own version, held per region, since one fault can reach
//! several nodes (`docs/tasks/22-as-built-physics.md`, `docs/physics.md` §16). A fault is physical parameters at a fiber
//! location; what it does to each node is computed by `sim::optics` (invariant 5).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{FiberCableId, FiberElementId};
use crate::fault::Onset;

/// Version of this schema.
pub const SCHEMA_VERSION: &str = "1";

/// One region's fiber faults.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FiberFaultSet {
    /// Version of this schema.
    pub schema_version: String,
    /// The region they're in, by id.
    pub region: String,
    /// The faults.
    pub faults: Vec<FiberFault>,
}

/// One fiber fault.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FiberFault {
    /// When it starts and how it comes on. A cut is all or nothing: it's cut once its onset has begun.
    pub onset: Onset,
    /// What it is.
    pub kind: FiberFaultKind,
}

/// One strand of one cable.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct StrandRef {
    /// The cable.
    pub cable: FiberCableId,
    /// The strand, numbered from 1.
    pub strand: u32,
}

/// Where on the fiber plant a fault is.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum FiberLocation {
    /// A point on a cable: every strand through it.
    Cable {
        /// The cable.
        cable: FiberCableId,
        /// Feet from its `from` end.
        pos_ft: f64,
    },
    /// An element: every link with a connector at it or passing through it.
    Element {
        /// The element.
        element: FiberElementId,
    },
    /// One splice at an element, named by the two strands it joins, since a splice has no part.
    Splice {
        /// The closure or panel.
        element: FiberElementId,
        /// The strands it joins, either order.
        strands: [StrandRef; 2],
    },
}

/// The fiber fault classes.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum FiberFaultKind {
    /// A cable cut through: no light passes the point on any strand. Task 19's `AnalogFiberCut` becomes this when it's built.
    FiberCut {
        /// The cable.
        cable: FiberCableId,
        /// Feet from its `from` end.
        pos_ft: f64,
        /// Seconds from the onset until it's spliced, or `None` while it stays cut.
        repair_s: Option<f64>,
    },
    /// Loss at a point: a bend, a bad splice or a dirty connector.
    EventLoss {
        /// Where.
        location: FiberLocation,
        /// Added loss at full severity, dB.
        loss_db: f64,
    },
    /// A transmitter's output and noise moved: a hub transmitter or optic, or a node's return laser or RPD optic, named by its
    /// fiber element (a node's by its `NodeEnd`).
    TransmitterDrift {
        /// The element.
        element: FiberElementId,
        /// Change in launch power at full severity, dB.
        power_delta_db: f64,
        /// Change in relative intensity noise at full severity, dB/Hz. Analog only.
        rin_delta_db: f64,
    },
}

/// Brings a fiber fault set from an earlier version to this one. There's only version 1.
pub fn migrate(mut set: FiberFaultSet) -> Result<FiberFaultSet, String> {
    match set.schema_version.as_str() {
        "1" => {
            set.schema_version = SCHEMA_VERSION.to_string();
            Ok(set)
        }
        other => Err(format!(
            "no migration from fiber fault schema version {other}"
        )),
    }
}
