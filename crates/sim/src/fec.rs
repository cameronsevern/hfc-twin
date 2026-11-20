//! MER thresholds and codeword errors (§4.6).

use schema::profile::{Profile, Qam};

use crate::units::{db, lin};

/// Bits per symbol.
pub fn bits_per_symbol(qam: Qam) -> f64 {
    match qam {
        Qam::Qam16 => 4.0,
        Qam::Qam64 => 6.0,
        Qam::Qam128 => 7.0,
        Qam::Qam256 => 8.0,
        Qam::Qam512 => 9.0,
        Qam::Qam1024 => 10.0,
        Qam::Qam2048 => 11.0,
        Qam::Qam4096 => 12.0,
    }
}

/// The failure threshold `thr` for a modulation, from the profile's FEC table.
pub fn thr_db(profile: &Profile, qam: Qam) -> Option<f64> {
    profile.fec.iter().find(|r| r.qam == qam).map(|r| r.thr_db)
}

/// §4.6: the SC-QAM threshold, from the table or, where unsourced, `thr` + 2 dB as at 256-QAM.
pub fn sc_qam_thr_db(profile: &Profile, qam: Qam) -> Option<f64> {
    let row = profile.fec.iter().find(|r| r.qam == qam)?;
    Some(row.sc_qam_thr_db.unwrap_or(row.thr_db + 2.0))
}

/// §4.6: the cap on what one subcarrier contributes to effective margin, dB above `thr`.
const MI_CAP_DB: f64 = 1.25;

/// §4.6: `m = RxMER_eff − thr`, from mutual information averaged over the data subcarriers.
pub fn effective_margin_db(rxmer_db: &[f64], thr_db: f64) -> f64 {
    if rxmer_db.is_empty() {
        return f64::NEG_INFINITY;
    }
    let cap = (1.0 + lin(thr_db + MI_CAP_DB)).log2();
    let sum: f64 = rxmer_db
        .iter()
        .map(|&mer| (1.0 + lin(mer)).log2().min(cap))
        .sum();
    db(2f64.powf(sum / rxmer_db.len() as f64) - 1.0) - thr_db
}

/// §4.6: the steepness `k`, 15 at 256-QAM and below, 10 at 4096-QAM, linear in bits between.
pub fn ofdm_k(qam: Qam) -> f64 {
    let bits = bits_per_symbol(qam);
    if bits <= 8.0 {
        15.0
    } else {
        15.0 - (bits - 8.0) * 5.0 / 4.0
    }
}

/// §4.6: OFDM uncorrectable rate at effective margin `m_db`.
pub fn p_uncorr_ofdm(m_db: f64, k: f64) -> f64 {
    1.0 / (1.0 + (k * (m_db + 0.55)).exp())
}

/// §4.6: OFDM correctable rate. Follows the raw margin, because it counts errors before decoding.
pub fn p_corr_ofdm(m_raw_db: f64, m_db: f64, k: f64) -> f64 {
    let p_err = 1.0 / (1.0 + (2.2 * (m_raw_db - 6.8)).exp());
    (p_err - p_uncorr_ofdm(m_db, k)).max(0.0)
}

/// §4.6: SC-QAM uncorrectable rate at margin `m_db` against channel MER.
pub fn p_uncorr_sc(m_db: f64) -> f64 {
    1.0 / (1.0 + (5.0 * (m_db - 0.2)).exp())
}

/// §4.6: SC-QAM correctable rate: the same shape shifted 1.3 dB, less what didn't correct.
pub fn p_corr_sc(m_db: f64) -> f64 {
    (p_uncorr_sc(m_db - 1.3) - p_uncorr_sc(m_db)).max(0.0)
}
