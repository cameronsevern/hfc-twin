//! Reflections (§5.1, §5.2). An echo needs two reflectors.

use crate::cable::C_FT_PER_US;

/// §5.1: an echo's delay from the spacing between its reflectors, µs.
pub fn cavity_delay_us(d_ft: f64, v_f: f64) -> f64 {
    2.0 * d_ft / (v_f * C_FT_PER_US)
}

/// §5.1: an echo's level relative to the main signal, dB. `atten_db_per_100ft` at the frequency
/// in question.
pub fn cavity_level_db(rl_1_db: f64, rl_2_db: f64, atten_db_per_100ft: f64, d_ft: f64) -> f64 {
    -(rl_1_db + rl_2_db + 2.0 * atten_db_per_100ft * d_ft / 100.0)
}

/// §5.1: an off-path echo, reached by leaking through a tap's isolation, relative to main, dB.
pub fn off_path_level_db(
    isolation_db: f64,
    tap_value_db: f64,
    rl_db: f64,
    atten_db_per_100ft: f64,
    d_ft: f64,
) -> f64 {
    -((isolation_db - tap_value_db) + rl_db + 2.0 * atten_db_per_100ft * d_ft / 100.0)
}

/// §5.1: an echo off another port of the same tap: the signal leaves by that port at the tap's
/// value, as main does, so only the port-to-port isolation separates them, relative to main, dB.
pub fn off_port_level_db(isolation_db: f64, rl_db: f64, atten_db_per_100ft: f64, d_ft: f64) -> f64 {
    -(isolation_db + rl_db + 2.0 * atten_db_per_100ft * d_ft / 100.0)
}

/// §5.2: peak-to-peak ripple from an echo of level `e_db`, dB.
pub fn ripple_pp_db(e_db: f64) -> f64 {
    let rho = 10f64.powf(e_db / 20.0);
    20.0 * ((1.0 + rho) / (1.0 - rho)).log10()
}

/// §5.2: the ripple's period in frequency, MHz, for a delay in µs.
pub fn ripple_period_mhz(tau_us: f64) -> f64 {
    1.0 / tau_us
}

/// §5.4: cavity length from ripple spacing, `D_ft = 492·v_f / Δf`.
pub fn cavity_len_from_ripple_ft(v_f: f64, ripple_spacing_mhz: f64) -> f64 {
    492.0 * v_f / ripple_spacing_mhz
}
