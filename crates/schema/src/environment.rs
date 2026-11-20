//! Inputs that are neither plant nor fault (§10).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::Timestamp;
use crate::fault::Schedule;

/// Version of this schema. Version 2 added ambient sources, wind and sun, each optional, so version 1 means the same.
pub const SCHEMA_VERSION: &str = "2";

/// The series applied to one plant. Sample spacing sets the sim's time step (§7.4).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    /// Version of this schema.
    pub schema_version: String,
    /// Samples in time order.
    pub samples: Vec<EnvironmentSample>,
    /// Radio around the plant that healthy homes' wiring picks up (§18). Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ambient_sources: Vec<AmbientSource>,
}

/// One ambient source: a band of radio at a level, on an hourly schedule (§18). Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AmbientSource {
    /// Stable id.
    pub id: String,
    /// Lower edge, MHz.
    pub lo_mhz: f64,
    /// Upper edge, MHz.
    pub hi_mhz: f64,
    /// What wiring with no shielding would pick up, dBmV per MHz: the level at zero coupling.
    pub level_dbmv_per_mhz: f64,
    /// Power multiplier by hour of day, as a fault's (§8).
    pub schedule: Schedule,
}

/// One point in the series.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentSample {
    /// Simulated time, seconds.
    pub t: Timestamp,
    /// Air temperature for aerial spans (§1.2), °C.
    pub temp_c: f64,
    /// Ground temperature for underground spans (§1.2), °C.
    pub ground_temp_c: f64,
    /// Rainfall for the water-ingress term (§8), mm per hour.
    pub rain_mm_per_hr: f64,
    /// Wind speed, m/s, for a connector fault's wind coupling (§19). Absent is calm. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_m_per_s: Option<f64>,
    /// Sunlight on aerial plant, W/m², for an aerial span's solar gain (§19). Absent is none. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solar_w_per_m2: Option<f64>,
}

/// Brings an environment from an earlier version to this one. Version 2 only added optional fields, so a version 1
/// series means the same: calm, no sun, no ambient sources.
pub fn migrate(mut environment: Environment) -> Result<Environment, String> {
    match environment.schema_version.as_str() {
        "1" | "2" => {
            environment.schema_version = SCHEMA_VERSION.to_string();
            Ok(environment)
        }
        other => Err(format!(
            "no migration from environment schema version {other}"
        )),
    }
}
