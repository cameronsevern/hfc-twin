//! What can go wrong computing physics on a valid plant.

use plant_graph::QueryError;
use schema::common::{ComponentId, ElementId, PortId};
use thiserror::Error;

/// A physics question that has no answer.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum PhysicsError {
    /// Legacy passives can collapse above their rating, so a lookup there is an error, not an
    /// extrapolation (§2.1, §11).
    #[error("{component} isn't rated at {f_mhz} MHz")]
    OutsideRatedBand {
        /// The part.
        component: ComponentId,
        /// The frequency asked for.
        f_mhz: f64,
    },
    /// A part's spec doesn't say what happens between these two ports.
    #[error("{element}: no loss defined from {in_port} to {out_port}")]
    NoLossDefined {
        /// The element.
        element: ElementId,
        /// One port.
        in_port: PortId,
        /// The other port.
        out_port: PortId,
    },
    /// A topology query failed.
    #[error(transparent)]
    Query(#[from] QueryError),
    /// An element's component is missing or the wrong class. `PlantGraph::new` rules this out.
    #[error("{0} has no usable component")]
    BadComponent(ElementId),
    /// A plant handed to a pass that builds its own graph doesn't validate.
    #[error("invalid plant: {0}")]
    InvalidPlant(String),
    /// The plant has a version 2 part or field with an RF or power effect that has no physics yet, each named as
    /// `what: where`. The sim refuses it rather than treat it as transparent (`docs/tasks/21-as-built-schemas.md`).
    #[error("no physics yet for: {}", .0.join(", "))]
    Unsupported(Vec<String>),
}
