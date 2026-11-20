//! The level budget the stages design with: the forward band's check points at the reference temperature and both ends
//! of the range, by the sim's own loss functions (`docs/physics.md` §1, §2, §3, §15). Every decision reads these, as
//! version 1's `place` does, and the sim checks the result.

use schema::common::Timestamp;
use schema::component::{CableSpec, Component, ComponentSpec};
use schema::environment::EnvironmentSample;
use schema::profile::Profile;
use sim::cable::cable_loss_db;
use sim::passive::{coupler_through_db, interpolate, line_eq_loss_db, plugin_loss_db};
use sim::walk::{ds_band, us_band};

/// The reference temperature alignment is done at, then the two ends of the range a design holds across (§1.2), °C.
pub const TEMPS: [f64; 3] = [20.0, -20.0, 45.0];
/// Index of the reference temperature.
pub const REF: usize = 0;
/// Check points across the forward band: its two edges and three between.
pub const NF: usize = 5;
/// Index of the top of the band.
pub const HI: usize = NF - 1;
/// Index of the bottom.
pub const LO: usize = 0;

/// A design sample at a temperature, as the sim reads it: calm, no sun.
pub fn sample(temp_c: f64) -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(0),
        temp_c,
        ground_temp_c: temp_c,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

/// The check points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    /// Forward check points, MHz, bottom first.
    pub f: [f64; NF],
    /// The return band's edges, MHz.
    pub us: [f64; 2],
}

impl Band {
    /// For a profile.
    pub fn new(profile: &Profile) -> Band {
        let (lo, hi) = ds_band(profile);
        let (us_lo, us_hi) = us_band(profile);
        Band {
            f: std::array::from_fn(|i| lo + (hi - lo) * i as f64 / (NF - 1) as f64),
            us: [us_lo, us_hi],
        }
    }

    /// The bottom of the forward band, MHz.
    pub fn lo(&self) -> f64 {
        self.f[LO]
    }

    /// The top.
    pub fn hi(&self) -> f64 {
        self.f[HI]
    }
}

/// A signal at the check points, `[temperature][frequency]`, dBmV per 6 MHz.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Levels(pub [[f64; NF]; 3]);

impl Levels {
    /// A station's output: `out_hi` at the top with `tilt` across the band, linear in frequency (§3.1), held at every
    /// temperature.
    pub fn station(band: &Band, out_hi: f64, tilt: f64) -> Levels {
        let (lo, hi) = (band.lo(), band.hi());
        let row: [f64; NF] =
            std::array::from_fn(|i| out_hi - tilt * (hi - band.f[i]) / (hi - lo).max(1e-9));
        Levels([row; 3])
    }

    /// Less a loss that depends on frequency and temperature.
    pub fn less(&self, band: &Band, loss: impl Fn(f64, f64) -> f64) -> Levels {
        let mut out = *self;
        for (t, temp_c) in TEMPS.iter().enumerate() {
            for (i, f) in band.f.iter().enumerate() {
                out.0[t][i] -= loss(*f, *temp_c);
            }
        }
        out
    }

    /// The top of the band at temperature index `t`.
    pub fn hi(&self, t: usize) -> f64 {
        self.0[t][HI]
    }

    /// The bottom.
    pub fn lo(&self, t: usize) -> f64 {
        self.0[t][LO]
    }

    /// The tilt, top less bottom, at `t`.
    pub fn tilt(&self, t: usize) -> f64 {
        self.hi(t) - self.lo(t)
    }

    /// The lowest at the top of the band over the temperatures.
    pub fn min_hi(&self) -> f64 {
        (0..3).map(|t| self.hi(t)).fold(f64::INFINITY, f64::min)
    }

    /// The highest at the top of the band over the temperatures.
    pub fn max_hi(&self) -> f64 {
        (0..3).map(|t| self.hi(t)).fold(f64::NEG_INFINITY, f64::max)
    }
}

/// Loss through `len_ft` of a cable, at the cable's temperature (§1.2): air temperature or the ground's, which a design
/// sample sets equal.
pub fn cable(spec: &CableSpec, len_ft: f64) -> impl Fn(f64, f64) -> f64 + '_ {
    move |f, temp| cable_loss_db(spec, f, len_ft, temp)
}

/// A part's flat or tabled loss through its main path: a tap's insertion loss, a splitter's leg, a coupler's through or
/// coupled leg, an inserter, a line equalizer, a connector.
pub fn through(component: &Component, f_mhz: f64) -> f64 {
    match &component.spec {
        ComponentSpec::Tap(t) => interpolate(&t.insertion_loss, f_mhz),
        ComponentSpec::Splitter(s) => interpolate(&s.leg_loss, f_mhz),
        ComponentSpec::DirectionalCoupler(c) => {
            coupler_through_db(c.coupled_value_db, c.excess_loss_db)
        }
        ComponentSpec::PowerInserter(p) => interpolate(&p.insertion_loss, f_mhz),
        ComponentSpec::LineEqualizer(e) => line_eq_loss_db(e, f_mhz),
        ComponentSpec::Connector(c) => c.insertion_loss_db,
        ComponentSpec::PlugIn(p) => plugin_loss_db(p, f_mhz),
        ComponentSpec::GroundBlock(g) => interpolate(&g.insertion_loss, f_mhz),
        _ => 0.0,
    }
}

/// A coupler's coupled leg, dB.
pub fn coupled_leg(component: &Component) -> f64 {
    match &component.spec {
        ComponentSpec::DirectionalCoupler(c) => c.coupled_value_db + c.excess_loss_db,
        _ => 0.0,
    }
}

/// A tap's port loss, dB.
pub fn tap_port(component: &Component, f_mhz: f64) -> f64 {
    match &component.spec {
        ComponentSpec::Tap(t) => interpolate(&t.tap_loss, f_mhz),
        _ => 0.0,
    }
}

/// The return path's loss from a point back to its active, at the return band's two edges and each temperature.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Up(pub [[f64; 2]; 3]);

impl Up {
    /// Plus a loss that depends on frequency and temperature.
    pub fn plus(&self, band: &Band, loss: impl Fn(f64, f64) -> f64) -> Up {
        let mut out = *self;
        for (t, temp_c) in TEMPS.iter().enumerate() {
            for (i, f) in band.us.iter().enumerate() {
                out.0[t][i] += loss(*f, *temp_c);
            }
        }
        out
    }
}
