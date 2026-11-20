//! What happens to the plant, as processes with rates. A hazard never scripts a symptom (invariant 5): it makes a fault of a
//! kind the sim already has, with parameters, and the sim computes what it does. Rates are data (`fixtures/ops/rates.json`).

use serde::{Deserialize, Serialize};

/// One process.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Hazard {
    /// A connector loosening or corroding over days until it reflects.
    ConnectorWear,
    /// An amplifier drifting or failing.
    AmpFault,
    /// Water in a span's cable, more in rain.
    WaterInSpan,
    /// Water inside a housing, more in rain.
    WaterInDevice,
    /// A tap corroding: mismatch and CPD, no blow.
    TapCorrosion,
    /// Limbs, ice or wind on aerial plant: a cut span.
    AerialDamage,
    /// An excavator through a buried span, by day on weekdays.
    DigIn,
    /// A car into a pole or a pedestal.
    VehicleHit,
    /// The power company loses a neighbourhood.
    UtilityOutage,
    /// A fiber cable cut through.
    FiberCut,
}

/// Every process, in the order a step draws them.
pub const HAZARDS: [Hazard; 10] = [
    Hazard::ConnectorWear,
    Hazard::AmpFault,
    Hazard::WaterInSpan,
    Hazard::WaterInDevice,
    Hazard::TapCorrosion,
    Hazard::AerialDamage,
    Hazard::DigIn,
    Hazard::VehicleHit,
    Hazard::UtilityOutage,
    Hazard::FiberCut,
];

impl Hazard {
    /// The rate in `fixtures/ops/rates.json` it runs at.
    pub fn rate_key(self) -> &'static str {
        match self {
            Hazard::ConnectorWear => "connector_wear_per_span_year",
            Hazard::AmpFault => "amp_faults_per_amp_year",
            Hazard::WaterInSpan => "water_per_span_year",
            Hazard::WaterInDevice => "water_per_device_year",
            Hazard::TapCorrosion => "tap_corrosion_per_tap_year",
            Hazard::AerialDamage => "aerial_damage_per_mile_year",
            Hazard::DigIn => "dig_ins_per_underground_mile_year",
            Hazard::VehicleHit => "vehicle_hits_per_structure_year",
            Hazard::UtilityOutage => "utility_outages_per_home_year",
            Hazard::FiberCut => "fiber_cuts_per_mile_year",
        }
    }

    /// What it's called on the page.
    pub fn label(self) -> &'static str {
        match self {
            Hazard::ConnectorWear => "a connector going bad",
            Hazard::AmpFault => "an amplifier fault",
            Hazard::WaterInSpan => "water in a cable",
            Hazard::WaterInDevice => "water in a housing",
            Hazard::TapCorrosion => "a corroded tap",
            Hazard::AerialDamage => "aerial cable brought down",
            Hazard::DigIn => "a dig-in through buried cable",
            Hazard::VehicleHit => "a car into the plant",
            Hazard::UtilityOutage => "a power cut",
            Hazard::FiberCut => "a fiber cut",
        }
    }
}

/// Minutes in a year, for rates per year drawn a minute at a time.
pub const MINUTES_PER_YEAR: f64 = 525_600.0;

/// The chance a process with `per_year` events fires in one minute.
pub fn chance_per_minute(per_year: f64) -> f64 {
    1.0 - (-per_year.max(0.0) / MINUTES_PER_YEAR).exp()
}
