//! One capture on a fixed grid, for the spectrum model (`docs/tasks/17-spectrum-model.md`). One
//! definition, so training and inference can't disagree.

/// Bin width, MHz.
pub const BIN_MHZ: f64 = 0.2;
/// Bins, enough for the widest OFDM channel, 192 MHz.
pub const BINS: usize = 960;
/// Written for a bin with nothing in it.
pub const PADDING: u8 = 0xff;
/// Quarter dB, as PNM files report it. 63.5 dB is the largest value below `PADDING`.
const STEPS_PER_DB: f64 = 4.0;

/// A capture on the grid. Empty bins hold the capture's mean and `present` says so, which
/// [FERREIRA2020] found the best way to pad.
#[derive(Debug, Clone, PartialEq)]
pub struct Binned {
    /// Centre of the first bin, MHz.
    pub first_mhz: f64,
    /// Lowest subcarrier in each bin, dB, so a spike survives.
    pub min_db: Vec<f64>,
    /// Mean of each bin, dB.
    pub mean_db: Vec<f64>,
    /// Whether the bin holds any subcarrier.
    pub present: Vec<bool>,
}

fn bin_index(k: usize, spacing_khz: f64) -> usize {
    (k as f64 * spacing_khz / 1000.0 / BIN_MHZ).floor() as usize
}

/// Bins a curve starting at `first_mhz`. `None` is an excluded subcarrier.
pub fn bin_curve(first_mhz: f64, spacing_khz: f64, values: &[Option<f64>]) -> Binned {
    let mut low = vec![f64::INFINITY; BINS];
    let mut sum = vec![0.0; BINS];
    let mut count = vec![0u32; BINS];
    for (k, value) in values.iter().enumerate() {
        let (Some(value), i) = (value, bin_index(k, spacing_khz)) else {
            continue;
        };
        if i < BINS {
            low[i] = low[i].min(*value);
            sum[i] += value;
            count[i] += 1;
        }
    }
    let total: f64 = sum.iter().sum();
    let n: u32 = count.iter().sum();
    let overall = if n > 0 { total / f64::from(n) } else { 0.0 };
    let present: Vec<bool> = count.iter().map(|c| *c > 0).collect();
    Binned {
        first_mhz: first_mhz + BIN_MHZ / 2.0,
        min_db: low
            .iter()
            .zip(&present)
            .map(|(v, p)| if *p { *v } else { overall })
            .collect(),
        mean_db: sum
            .iter()
            .zip(&count)
            .map(|(s, c)| if *c > 0 { s / f64::from(*c) } else { overall })
            .collect(),
        present,
    }
}

/// The largest value per bin of a per-subcarrier series, as a fall: 0 where it never fell.
pub fn bin_worst(spacing_khz: f64, values: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0f64; BINS];
    for (k, value) in values.iter().enumerate() {
        let i = bin_index(k, spacing_khz);
        if i < BINS {
            out[i] = out[i].max(*value);
        }
    }
    out
}

/// A value in dB as a quarter-dB byte.
pub fn to_quarter_db(value_db: f64) -> u8 {
    (value_db * STEPS_PER_DB).round().clamp(0.0, 254.0) as u8
}

/// Bytes as hex, for a line of text.
pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    out
}

impl Binned {
    /// The two curves as quarter-dB bytes, with `PADDING` in empty bins.
    pub fn to_bytes(&self) -> (Vec<u8>, Vec<u8>) {
        let encode = |values: &[f64]| -> Vec<u8> {
            values
                .iter()
                .zip(&self.present)
                .map(|(v, p)| if *p { to_quarter_db(*v) } else { PADDING })
                .collect()
        };
        (encode(&self.min_db), encode(&self.mean_db))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_and_mean_by_hand() {
        // 50 kHz: 4 subcarriers to a bin. The third is a spike, and the mean feels it less.
        let curve = [40.0, 40.0, 20.0, 40.0, 30.0, 34.0];
        let values: Vec<Option<f64>> = curve.iter().copied().map(Some).collect();
        let b = bin_curve(800.0, 50.0, &values);
        assert_eq!(b.min_db[0], 20.0);
        assert_eq!(b.mean_db[0], 35.0);
        assert_eq!(b.min_db[1], 30.0);
        assert_eq!(b.mean_db[1], 32.0);
        assert!((b.first_mhz - 800.1).abs() < 1e-12);
        // Padding holds the mean of every subcarrier, and is marked.
        assert!(!b.present[2]);
        assert!((b.mean_db[2] - 34.0).abs() < 1e-12);
        let (low, _) = b.to_bytes();
        assert_eq!(low[0], 80);
        assert_eq!(low[2], PADDING);
    }

    #[test]
    fn a_25_khz_and_a_50_khz_channel_share_the_grid() {
        let fine = vec![Some(40.0); 7600];
        let coarse = vec![Some(40.0); 3800];
        let (a, b) = (
            bin_curve(800.0, 25.0, &fine),
            bin_curve(800.0, 50.0, &coarse),
        );
        assert_eq!(a.mean_db, b.mean_db);
        assert_eq!(a.present.iter().filter(|p| **p).count(), 950);
        assert_eq!(a.present, b.present);
    }

    #[test]
    fn an_excluded_subcarrier_is_left_out() {
        let b = bin_curve(800.0, 50.0, &[Some(40.0), None, Some(20.0), None]);
        assert_eq!(b.mean_db[0], 30.0);
    }

    #[test]
    fn hex_and_falls() {
        assert_eq!(to_hex(&[0, 15, 255]), "000fff");
        let worst = bin_worst(50.0, &[1.0, 3.0, -2.0, 0.0, -1.0]);
        assert_eq!(worst[0], 3.0);
        assert_eq!(worst[1], 0.0);
    }
}
