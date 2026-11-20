//! Seeded randomness, passed explicitly (invariant 4). `xoshiro256**` seeded through SplitMix64.

/// FNV-1a over a string, for deriving a stream from an element's id. Stable across runs and builds,
/// unlike `std`'s hasher.
pub fn hash_str(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A seeded random stream.
#[derive(Debug, Clone)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// A stream from a seed.
    pub fn new(seed: u64) -> Rng {
        let mut state = seed;
        Rng {
            s: [
                splitmix(&mut state),
                splitmix(&mut state),
                splitmix(&mut state),
                splitmix(&mut state),
            ],
        }
    }

    /// A stream for one use of one thing at one time, independent of every other.
    pub fn derive(seed: u64, parts: &[u64]) -> Rng {
        let mut state = seed;
        for part in parts {
            state = splitmix(&mut state) ^ part.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        }
        Rng::new(state)
    }

    /// The next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform in [0, 1).
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in [lo, hi).
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.uniform()
    }

    /// Uniform index below `n`.
    pub fn index(&mut self, n: usize) -> usize {
        ((self.uniform() * n as f64) as usize).min(n.saturating_sub(1))
    }

    /// Standard normal, by Box-Muller.
    pub fn gaussian(&mut self) -> f64 {
        let u1 = self.uniform().max(1e-300);
        let u2 = self.uniform();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }

    /// A binomial draw: exact for few trials, Poisson or normal for many, which is all a counter of
    /// millions of codewords can tell apart.
    pub fn binomial(&mut self, n: u64, p: f64) -> u64 {
        if n == 0 || p <= 0.0 {
            return 0;
        }
        if p >= 1.0 {
            return n;
        }
        if n <= 64 {
            return (0..n).filter(|_| self.uniform() < p).count() as u64;
        }
        let mean = n as f64 * p;
        if mean < 30.0 {
            // Knuth's Poisson.
            let limit = (-mean).exp();
            let mut k = 0u64;
            let mut product = 1.0;
            loop {
                product *= self.uniform();
                if product <= limit {
                    return k.min(n);
                }
                k += 1;
            }
        }
        if (n as f64) * (1.0 - p) < 30.0 {
            return n - self.binomial(n, 1.0 - p);
        }
        let draw = mean + (mean * (1.0 - p)).sqrt() * self.gaussian();
        draw.round().clamp(0.0, n as f64) as u64
    }
}
