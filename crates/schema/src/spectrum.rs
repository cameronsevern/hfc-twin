//! What a spectrum model reads and learns: one modem's OFDM capture on a fixed grid, and the shapes
//! in it (`docs/tasks/17-spectrum-model.md`). Labels are derived by the sim, per invariant 1.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{ChannelId, ElementId, Origin, ProfileId};
use crate::work_order::CauseClass;

/// Version of this schema. Version 2 only widens `causes`: `CauseClass` took work order version 5's causes (task 24).
pub const SCHEMA_VERSION: &str = "2";

/// Whether a model trained on spectrum rows of `version` reads rows of this one: version 2 only added causes, which a
/// model of version 1 never learnt and never reads, since it learns shapes.
pub fn model_can_read(version: &str) -> bool {
    matches!(version, "1" | "2")
}

/// Brings a spectrum row from an earlier version to this one. Version 2 only added causes, so a version 1 row means the
/// same.
pub fn migrate(mut row: SpectrumRow) -> Result<SpectrumRow, String> {
    match row.schema_version.as_str() {
        "1" | "2" => {
            row.schema_version = SCHEMA_VERSION.to_string();
            Ok(row)
        }
        other => Err(format!("no migration from spectrum schema version {other}")),
    }
}

/// What a stretch of the curve looks like. Not a cause: one roll-off can have several.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Band of noise, 1 MHz or wider.
    Ingress,
    /// Narrow noise, under 1 MHz, alone or as a comb.
    Spike,
    /// A rounded dip, from a suckout or a resonance.
    Dip,
    /// A gradual fall towards a band edge.
    RollOff,
    /// A band missing, with sharp sides.
    BandLoss,
    /// Periodic ripple from one echo.
    Ripple,
    /// Non-periodic waves.
    Wave,
    /// Lower across the channel.
    LowMer,
    /// A fall at the channel's edge from what's next to it.
    Edge,
}

impl Shape {
    /// Every shape, in bit order.
    pub const ALL: [Shape; 9] = [
        Shape::Ingress,
        Shape::Spike,
        Shape::Dip,
        Shape::RollOff,
        Shape::BandLoss,
        Shape::Ripple,
        Shape::Wave,
        Shape::LowMer,
        Shape::Edge,
    ];

    /// This shape's bit in a bin's mask.
    pub fn bit(self) -> u16 {
        1 << Shape::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// One shape in one capture.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ShapeLabel {
    /// The shape.
    pub shape: Shape,
    /// The largest fall this shape's faults alone cause, dB.
    pub worst_fall_db: f64,
    /// Bins where they cause a visible fall.
    pub bins: u32,
}

/// One capture and its labels.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpectrumRow {
    /// Version of this schema.
    pub schema_version: String,
    /// Whether this is simulated or real.
    pub origin: Origin,
    /// The observation schema it was made under.
    pub observation_schema: String,
    /// The fault schema it was made under.
    pub fault_schema: String,
    /// The street map, where there is one.
    pub map: Option<String>,
    /// The architecture profile.
    pub profile: ProfileId,
    /// The node's service group.
    pub service_group: String,
    /// The modem.
    pub modem: ElementId,
    /// The OFDM channel.
    pub channel: ChannelId,
    /// Centre of the first bin, MHz.
    pub first_mhz: f64,
    /// The seed that made it.
    pub seed: u64,
    /// Causes in the fault set.
    pub causes: Vec<CauseClass>,
    /// Shapes visible on this modem.
    pub shapes: Vec<ShapeLabel>,
    /// Largest fall against the healthy run, all faults together, dB.
    pub worst_fall_db: f64,
    /// Per bin, lowest subcarrier in quarter dB, in hex. `ff` marks padding.
    pub min_hex: String,
    /// Per bin, mean in quarter dB, in hex. `ff` marks padding.
    pub mean_hex: String,
    /// Per bin, the fall in quarter dB, in hex.
    pub fall_hex: String,
    /// Per bin, a mask of `Shape::bit`, as little-endian u16, in hex.
    pub shapes_hex: String,
}
