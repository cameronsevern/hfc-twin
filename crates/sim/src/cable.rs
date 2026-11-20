//! Coaxial cable (§1).

use schema::component::CableSpec;
use schema::environment::EnvironmentSample;
use schema::plant::Installation;

/// Speed of light, ft/µs (§1.3).
pub const C_FT_PER_US: f64 = 983.6;

/// §1.1: `A(f) = a·√f + b·f`, dB/100 ft at 20 °C.
pub fn atten_db_per_100ft(spec: &CableSpec, f_mhz: f64) -> f64 {
    spec.atten_a * f_mhz.sqrt() + spec.atten_b * f_mhz
}

/// §19: loop resistance at `temp_c`, Ω per 1,000 ft. A part that states no `r_loop_k_temp` holds its 20 °C value.
pub fn r_loop_ohm_per_kft(spec: &CableSpec, temp_c: f64) -> f64 {
    match spec.r_loop_k_temp {
        Some(k) => spec.r_loop_ohm_per_kft * (1.0 + k * (temp_c - 20.0)),
        None => spec.r_loop_ohm_per_kft,
    }
}

/// §1.2: the factor attenuation scales by at `temp_c`.
pub fn temp_factor(spec: &CableSpec, temp_c: f64) -> f64 {
    1.0 + spec.k_temp * (temp_c - 20.0)
}

/// §1.1, §1.2: loss over `len_ft` at `temp_c`, dB.
pub fn cable_loss_db(spec: &CableSpec, f_mhz: f64, len_ft: f64, temp_c: f64) -> f64 {
    atten_db_per_100ft(spec, f_mhz) * temp_factor(spec, temp_c) * len_ft / 100.0
}

/// §1.3: one-way delay over `len_ft`, µs.
pub fn delay_us(spec: &CableSpec, len_ft: f64) -> f64 {
    len_ft / (spec.v_f * C_FT_PER_US)
}

/// §1.2, §10: the temperature a cable sees. A drop has no installation, and hangs in air.
pub fn cable_temp_c(sample: &EnvironmentSample, installation: Option<Installation>) -> f64 {
    exposure_temp_c(sample, Exposure::of(installation))
}

/// §13: in-home cable's temperature, °C (approx).
pub const INDOOR_TEMP_C: f64 = 20.0;

/// §19: how far sunlight warms aerial cable above the air, °C per W/m² (approx, our estimate).
pub const SOLAR_GAIN_C_PER_W_M2: f64 = 0.02;

/// What sets a cable's temperature (§1.2, §13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exposure {
    /// Aerial, or a drop with no installation: air temperature.
    Air,
    /// Buried: ground temperature.
    Ground,
    /// In a home: `INDOOR_TEMP_C`.
    Indoor,
}

impl Exposure {
    /// A span's or drop's exposure from its installation. A drop with none hangs in air, as in version 1.
    pub fn of(installation: Option<Installation>) -> Exposure {
        match installation {
            Some(Installation::Underground) => Exposure::Ground,
            Some(Installation::Aerial) | None => Exposure::Air,
        }
    }
}

/// §1.2, §13: the temperature a cable at this exposure sees.
pub fn exposure_temp_c(sample: &EnvironmentSample, exposure: Exposure) -> f64 {
    match exposure {
        // §19: in sunlight, warmer than the air.
        Exposure::Air => match sample.solar_w_per_m2 {
            Some(solar) => sample.temp_c + SOLAR_GAIN_C_PER_W_M2 * solar,
            None => sample.temp_c,
        },
        Exposure::Ground => sample.ground_temp_c,
        Exposure::Indoor => INDOOR_TEMP_C,
    }
}
