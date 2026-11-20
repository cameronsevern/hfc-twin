//! Noise, distortion and MER (§4). Levels are dBmV per 6 MHz unless a name says otherwise.

use schema::profile::ChannelPlan;

use crate::units::{db, lin};

/// §4.1: thermal noise in 75 Ω at 290 K, dBmV/Hz.
pub const THERMAL_DBMV_PER_HZ: f64 = -125.2;

/// §4.1: the thermal floor in a bandwidth, dBmV.
pub fn n_floor_dbmv(bandwidth_hz: f64) -> f64 {
    THERMAL_DBMV_PER_HZ + db(bandwidth_hz)
}

/// §4.1: the floor in the 6 MHz all downstream levels are referenced to, dBmV.
pub fn n_floor_6mhz_dbmv() -> f64 {
    n_floor_dbmv(6e6)
}

/// §4.2: CNR of one active, with the level and floor in the same bandwidth.
pub fn cnr_db(l_in_dbmv: f64, nf_db: f64, n_floor_dbmv: f64) -> f64 {
    l_in_dbmv - nf_db - n_floor_dbmv
}

/// §4.3: noise adds in linear power along a cascade.
pub fn cascade_cnr_db(cnrs_db: impl IntoIterator<Item = f64>) -> f64 {
    -db(cnrs_db.into_iter().map(|cnr| lin(-cnr)).sum())
}

/// §4.4: CTB at an output level.
pub fn ctb_db(ctb_ref_db: f64, l_dbmv: f64, l_ref_dbmv: f64) -> f64 {
    ctb_ref_db - 2.0 * (l_dbmv - l_ref_dbmv)
}

/// §4.4: CSO at an output level.
pub fn cso_db(cso_ref_db: f64, l_dbmv: f64, l_ref_dbmv: f64) -> f64 {
    cso_ref_db - (l_dbmv - l_ref_dbmv)
}

/// §4.4: CIN at an output level, degrading a further 2 dB per dB above `compression_onset_dbmv`.
pub fn cin_db(
    cin_ref_db: f64,
    l_dbmv: f64,
    l_ref_dbmv: f64,
    compression_onset_dbmv: Option<f64>,
) -> f64 {
    let over_db = compression_onset_dbmv.map_or(0.0, |onset| (l_dbmv - onset).max(0.0));
    cin_ref_db - (l_dbmv - l_ref_dbmv) - 2.0 * over_db
}

/// §4.5: `MER_src`, the CMTS or RPD output, from PHY Table 41's floors.
pub fn mer_src_db(f_mhz: f64) -> f64 {
    if f_mhz < 600.0 {
        50.0
    } else if f_mhz <= 1002.0 {
        47.0
    } else {
        45.0
    }
}

/// §17: composite power across the plan's downstream channels, dBmV. The per-6 MHz level is `out_at_hi_dbmv` at `hi_mhz`,
/// falling by `tilt_db` to `lo_mhz` and linear in dB, integrated across each channel's occupied band: an SC-QAM's width, an
/// OFDM channel's active span and half a subcarrier either side.
pub fn composite_dbmv(
    plan: &ChannelPlan,
    out_at_hi_dbmv: f64,
    tilt_db: f64,
    lo_mhz: f64,
    hi_mhz: f64,
) -> f64 {
    let slope_db_per_mhz = if hi_mhz > lo_mhz {
        tilt_db / (hi_mhz - lo_mhz)
    } else {
        0.0
    };
    let k = std::f64::consts::LN_10 / 10.0;
    // ∫ lin(L(f)) df / 6 MHz from f1 to f2, with L(f) = out_at_hi − slope·(hi − f).
    let band = |f1: f64, f2: f64| -> f64 {
        let at = |f: f64| out_at_hi_dbmv - slope_db_per_mhz * (hi_mhz - f);
        if slope_db_per_mhz == 0.0 {
            lin(out_at_hi_dbmv) * (f2 - f1) / 6.0
        } else {
            (lin(at(f2)) - lin(at(f1))) / (6.0 * k * slope_db_per_mhz)
        }
    };
    let mut total = 0.0;
    for c in &plan.ds_sc_qam {
        total += band(
            c.center_mhz - c.width_mhz / 2.0,
            c.center_mhz + c.width_mhz / 2.0,
        );
    }
    for c in &plan.ofdm {
        let half = c.subcarrier_spacing_khz / 2000.0;
        total += band(c.first_active_mhz - half, c.last_active_mhz + half);
    }
    db(total)
}

/// §4.5: combine noise-like terms in linear power.
pub fn mer_from_terms_db(terms_db: impl IntoIterator<Item = f64>) -> f64 {
    -db(terms_db.into_iter().map(|t| lin(-t)).sum())
}

/// §4.5: `CNR_cm`, the modem's own noise at its input.
pub fn cnr_cm_db(l_dbmv: f64, nf_cm_db: f64) -> f64 {
    l_dbmv - nf_cm_db - n_floor_6mhz_dbmv()
}

/// §4.5: `MER_ingress` from the signal at the entry point. `Loss(entry → modem)` cancels, since
/// signal and ingress cross the same plant below the entry, and amps gain both alike.
pub fn mer_ingress_db(l_at_entry_dbmv: f64, p_ingress_dbmv: f64, bw_mhz: f64) -> f64 {
    l_at_entry_dbmv - (p_ingress_dbmv + db(6.0 / bw_mhz))
}

/// §4.5 (task 27): how far an SC-QAM demodulator's adaptive equalizer takes down an echo within its span, dB (approx).
pub const SC_EQ_CANCEL_DB: f64 = 30.0;
/// §4.5 (task 27): the longest echo that equalizer reaches, µs: where DOCSIS's assumed channel drops to −30 dBc (approx).
pub const SC_EQ_SPAN_US: f64 = 1.5;

/// §4.5: `MER_isi` an SC-QAM channel keeps from one echo of amplitude `rho` at `tau_us`, as a linear noise-to-signal
/// ratio: the echo less what the equalizer takes out within its span, and all of it beyond.
pub fn inv_mer_isi_sc(tau_us: f64, rho: f64) -> f64 {
    let power = rho * rho;
    if tau_us <= SC_EQ_SPAN_US {
        power * lin(-SC_EQ_CANCEL_DB)
    } else {
        power
    }
}

/// §4.5: `MER_isi` for an echo longer than the cyclic prefix, as a linear noise-to-signal ratio.
/// Zero when the echo fits inside the prefix.
pub fn inv_mer_isi_ofdm(tau_us: f64, cp_us: f64, rho: f64, t_u_us: f64) -> f64 {
    if tau_us <= cp_us {
        return 0.0;
    }
    2.0 * (tau_us - cp_us) * rho * rho / t_u_us
}
