//! Return-path formulas (§7.1–§7.3).

use crate::noise::n_floor_dbmv;
use crate::units::{db, lin};

/// §7.2: dB between a 6.4 MHz and a 1.6 MHz reference.
pub fn per_1p6_from_per_6p4_db() -> f64 {
    db(6.4 / 1.6)
}

/// §7.2: `P_tx = P_in,node + Loss_up(f)`, dBmV per 6.4 MHz.
pub fn p_tx_dbmv_per_6p4(p_in_node_dbmv: f64, loss_up_db: f64) -> f64 {
    p_in_node_dbmv + loss_up_db
}

/// §7.2: `P1.6hi` for a D3.1 modem, from `Pmax` and the Transmit Channel Set's widths.
pub fn p1p6hi_dbmv(p_max_dbmv: f64, bw_legacy_mhz: f64, bw_ofdma_mhz: f64) -> f64 {
    let n_eq = (bw_legacy_mhz / 1.6).ceil() + (bw_ofdma_mhz / 1.6).ceil();
    let limit = p_max_dbmv - db(n_eq);
    if bw_legacy_mhz + bw_ofdma_mhz <= 24.0 {
        limit.min(53.2 + (p_max_dbmv - 65.0))
    } else {
        limit
    }
}

/// §7.3: `N_coax` for `m` identical amps in a node, dBmV in `bandwidth_hz`.
pub fn n_coax_dbmv(bandwidth_hz: f64, nf_ret_db: f64, m: usize) -> f64 {
    n_floor_dbmv(bandwidth_hz) + nf_ret_db + db(m as f64)
}

/// §7.3: upstream SNR at the CMTS, combined in linear with the analog optics where there are any.
pub fn us_snr_db(p_rx_dbmv: f64, n_node_dbmv: f64, optics_snr_db: Option<f64>) -> f64 {
    let coax = p_rx_dbmv - n_node_dbmv;
    match optics_snr_db {
        Some(optics) => -db(lin(-coax) + lin(-optics)),
        None => coax,
    }
}
