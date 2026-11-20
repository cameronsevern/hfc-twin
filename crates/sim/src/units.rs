//! Decibel conversions and a small complex type. `dB` is a ratio and `dBmV` an absolute level
//! (`docs/physics.md`, conventions).

use std::ops::{Add, Mul};

/// `10^(x_db/10)`, for power quantities.
pub fn lin(x_db: f64) -> f64 {
    10f64.powf(x_db / 10.0)
}

/// `10·log10(x)`, for power quantities.
pub fn db(x_lin: f64) -> f64 {
    10.0 * x_lin.log10()
}

/// `10^(x_db/20)`, for amplitude quantities.
pub fn amp(x_db: f64) -> f64 {
    10f64.powf(x_db / 20.0)
}

/// Sum in linear: `10·log10(Σ 10^(x_i/10))`.
pub fn sum_lin_db(values_db: impl IntoIterator<Item = f64>) -> f64 {
    db(values_db.into_iter().map(lin).sum())
}

/// A complex number. `schema::common::Complex` is the wire form; this one does arithmetic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C64 {
    /// Real part.
    pub re: f64,
    /// Imaginary part.
    pub im: f64,
}

impl C64 {
    /// One.
    pub const ONE: C64 = C64 { re: 1.0, im: 0.0 };
    /// Zero.
    pub const ZERO: C64 = C64 { re: 0.0, im: 0.0 };

    /// From magnitude and phase in radians.
    pub fn from_polar(mag: f64, phase_rad: f64) -> C64 {
        C64 {
            re: mag * phase_rad.cos(),
            im: mag * phase_rad.sin(),
        }
    }

    /// `|z|²`.
    pub fn norm_sqr(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    /// Phase in radians.
    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }

    /// `1/z`, or zero for zero.
    pub fn inv(self) -> C64 {
        let n = self.norm_sqr();
        if n == 0.0 {
            return C64::ZERO;
        }
        C64 {
            re: self.re / n,
            im: -self.im / n,
        }
    }

    /// `z` scaled by a real.
    pub fn scale(self, k: f64) -> C64 {
        C64 {
            re: self.re * k,
            im: self.im * k,
        }
    }
}

impl Add for C64 {
    type Output = C64;
    fn add(self, other: C64) -> C64 {
        C64 {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }
}

impl Mul for C64 {
    type Output = C64;
    fn mul(self, other: C64) -> C64 {
        C64 {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }
}

impl From<C64> for schema::common::Complex {
    fn from(z: C64) -> Self {
        schema::common::Complex { re: z.re, im: z.im }
    }
}
