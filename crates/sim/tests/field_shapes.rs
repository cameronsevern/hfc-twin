//! Task 16: the sim draws what CableLabs lists, from physical faults (invariant 5). Targets are readings from published captures,
//! `docs/reference/rxmer-impairments.md` §2, by eye, so each carries a tolerance. Everything here is synthetic. Passing says the sim
//! can reach these shapes and sizes, not that its mix of faults matches any real network.
//!
//! `GALLERY_OUT=path cargo test -p sim --test field_shapes gallery` writes every curve for `docs/rxmer-gallery.md`.

use std::fs;

use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{
    AmpState, Burst, Fault, FaultKind, FaultSet, Location, LossShape, Onset, Schedule,
};
use schema::observation::{DsRxmerRecord, ModemObservation};
use schema::plant::Plant;
use schema::profile::Profile;
use serde_json::json;
use sim::telemetry::{Detail, Scenario, SimOptions, simulate};

/// The scatter measured on the public capture, `fixtures/field/README.md`, dB.
const SCATTER_DB: f64 = 0.8;
/// A modem at the far end of the street.
const FAR: &str = "modem-8";
/// A centre for narrow faults, well inside the low-split OFDM channel, MHz.
const F0_MHZ: f64 = 900.0;
/// Well under the design's output at the band's top, as a badly set amp, so distortion climbs (§4.4), dBmV.
const COMPRESSION_ONSET_DBMV: f64 = 36.0;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

#[derive(Clone)]
struct World {
    plant: Plant,
    library: ComponentLibrary,
    profile: Profile,
}

fn low() -> World {
    World {
        plant: read("one_street/plant_lowsplit_iccap.json"),
        library: read("library/generic.json"),
        profile: read("profiles/d31_lowsplit_1002_iccap.json"),
    }
}

fn high() -> World {
    World {
        plant: read("one_street/plant_highsplit_rpd.json"),
        library: read("library/generic.json"),
        profile: read("profiles/d31_highsplit_1218_rpd.json"),
    }
}

fn observe(world: &World, kinds: Vec<FaultKind>, modem: &str) -> ModemObservation {
    let graph = PlantGraph::new(&world.plant, &world.library, &world.profile).unwrap();
    let environment = Environment {
        schema_version: schema::environment::SCHEMA_VERSION.to_string(),
        samples: vec![EnvironmentSample {
            t: Timestamp(0),
            temp_c: 20.0,
            ground_temp_c: 20.0,
            rain_mm_per_hr: 0.0,
            solar_w_per_m2: None,
            wind_m_per_s: None,
        }],
        ambient_sources: Vec::new(),
    };
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.to_string(),
        seed: 7,
        faults: kinds
            .into_iter()
            .map(|kind| Fault {
                onset: Onset::Instant,
                kind,
            })
            .collect(),
    };
    let steps = simulate(&Scenario {
        graph: &graph,
        environment: &environment,
        faults: &faults,
        seed: 11,
        options: SimOptions {
            detail: Detail::Full,
            randomize: None,
            measurement_noise_db: 0.0,
            rxmer_scatter_db: SCATTER_DB,
        },
        link: None,
    })
    .unwrap();
    steps[0]
        .modems
        .iter()
        .find(|m| m.element_id.as_str() == modem)
        .unwrap()
        .clone()
}

#[derive(Clone)]
struct Curve {
    f_mhz: Vec<f64>,
    db: Vec<f64>,
}

fn axis_f(axis: &schema::observation::SubcarrierAxis, n: usize) -> Vec<f64> {
    (0..n)
        .map(|k| {
            axis.zero_freq_mhz
                + (f64::from(axis.first_active_index) + k as f64) * axis.spacing_khz / 1000.0
        })
        .collect()
}

fn ds_curve(world: &World, kinds: Vec<FaultKind>, modem: &str) -> Curve {
    let m = observe(world, kinds, modem);
    let record: &DsRxmerRecord = &m.rxmer.as_ref().unwrap()[0];
    let db: Vec<f64> = record
        .rxmer_db
        .as_ref()
        .unwrap()
        .iter()
        .map(|v| v.unwrap())
        .collect();
    Curve {
        f_mhz: axis_f(&record.axis, db.len()),
        db,
    }
}

fn us_curve(world: &World, kinds: Vec<FaultKind>, modem: &str) -> Curve {
    let m = observe(world, kinds, modem);
    let record = &m.us_rxmer.as_ref().unwrap()[0];
    let db: Vec<f64> = record
        .us_rxmer_db
        .as_ref()
        .unwrap()
        .iter()
        .map(|v| v.unwrap())
        .collect();
    Curve {
        f_mhz: axis_f(&record.axis, db.len()),
        db,
    }
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

/// A moving average over `n` points, with the edges averaged over what's there.
fn smooth(v: &[f64], n: usize) -> Vec<f64> {
    (0..v.len())
        .map(|i| {
            let a = i.saturating_sub(n / 2);
            let b = (i + n / 2 + 1).min(v.len());
            mean(&v[a..b])
        })
        .collect()
}

impl Curve {
    /// Points per MHz.
    fn per_mhz(&self) -> usize {
        ((self.db.len() - 1) as f64 / (self.f_mhz[self.f_mhz.len() - 1] - self.f_mhz[0])).round()
            as usize
    }

    /// Smoothed to 0.5 MHz, past the scatter.
    fn smoothed(&self) -> Vec<f64> {
        smooth(&self.db, self.per_mhz() / 2)
    }

    /// Change against `base` per point, smoothed: most negative first.
    fn drop_from(&self, base: &Curve) -> Vec<f64> {
        self.smoothed()
            .iter()
            .zip(base.smoothed())
            .map(|(a, b)| a - b)
            .collect()
    }

    /// The largest fall against `base`, dB, as a positive number.
    fn worst_drop(&self, base: &Curve) -> f64 {
        -self.drop_from(base).into_iter().fold(0.0, f64::min)
    }

    /// How wide the part fallen by at least half the worst drop is, MHz.
    fn width_mhz(&self, base: &Curve) -> f64 {
        let drop = self.drop_from(base);
        let half = self.worst_drop(base) / 2.0;
        drop.iter().filter(|d| -**d >= half).count() as f64 / self.per_mhz() as f64
    }

    /// Mean over a band, dB.
    fn band_mean(&self, lo_mhz: f64, hi_mhz: f64) -> f64 {
        let v: Vec<f64> = self
            .f_mhz
            .iter()
            .zip(&self.db)
            .filter(|(f, _)| **f >= lo_mhz && **f <= hi_mhz)
            .map(|(_, d)| *d)
            .collect();
        mean(&v)
    }

    /// What's left after a 30 MHz trend comes off the 0.5 MHz curve.
    fn detrended(&self) -> Vec<f64> {
        let s = self.smoothed();
        let trend = smooth(&s, self.per_mhz() * 30);
        s.iter().zip(trend).map(|(a, b)| a - b).collect()
    }

    /// Peak to peak of the ripple, between its 2nd and 98th percentiles, dB.
    fn ripple_pp(&self) -> f64 {
        let mut d = self.detrended();
        d.sort_by(f64::total_cmp);
        d[d.len() * 98 / 100] - d[d.len() * 2 / 100]
    }

    /// The strongest period in the ripple, MHz, and its share of the ripple's energy. One echo puts most
    /// of its energy at one period, and water spreads it [PNM-HFC §5.2.6.12.2].
    fn strongest_period(&self) -> (f64, f64) {
        let d = self.detrended();
        let n = d.len();
        let span_mhz = self.f_mhz[n - 1] - self.f_mhz[0];
        let total: f64 = d.iter().map(|v| v * v).sum::<f64>() * n as f64 / 2.0;
        let (mut best, mut best_k) = (0.0, 1);
        for k in 2..(span_mhz * 2.0) as usize {
            let w = 2.0 * std::f64::consts::PI * k as f64 / n as f64;
            let (mut re, mut im) = (0.0, 0.0);
            for (i, v) in d.iter().enumerate() {
                re += v * (w * i as f64).cos();
                im += v * (w * i as f64).sin();
            }
            let power = re * re + im * im;
            if power > best {
                best = power;
                best_k = k;
            }
        }
        (span_mhz / best_k as f64, best / total)
    }

    /// Scatter from one subcarrier to the next, past anything slower than 2 MHz, dB.
    fn scatter_db(&self) -> f64 {
        let s = smooth(&self.db, self.per_mhz() * 2);
        let d: Vec<f64> = self.db.iter().zip(s).map(|(a, b)| a - b).collect();
        let m = mean(&d);
        (d.iter().map(|v| (v - m).powi(2)).sum::<f64>() / d.len() as f64).sqrt()
    }
}

fn near(what: &str, got: f64, want: f64, tolerance: f64) {
    eprintln!("{what}: {got:.2}, target {want} ±{tolerance}");
    assert!(
        (got - want).abs() <= tolerance,
        "{what}: {got:.2}, target {want} ±{tolerance}"
    );
}

fn span(id: &str, pos_ft: f64) -> Location {
    Location::Span {
        span_id: EdgeId::new(id),
        pos_ft,
    }
}

fn always() -> Schedule {
    Schedule {
        hourly_multipliers: [1.0; 24],
    }
}

fn ingress(f_mhz: f64, bw_mhz: f64, power_dbmv: f64) -> FaultKind {
    FaultKind::DownstreamIngress {
        entry_location: span("span-6", 50.0),
        f_mhz,
        bw_mhz,
        power_dbmv,
        schedule: always(),
    }
}

fn low_drop(extra_loss_db: f64) -> FaultKind {
    FaultKind::BadDrop {
        modem_id: ElementId::new(FAR),
        extra_loss_db,
        return_loss_db: None,
    }
}

fn suckout(depth_db: f64) -> Vec<FaultKind> {
    vec![
        low_drop(14.0),
        FaultKind::Suckout {
            location: span("span-5", 50.0),
            f0_mhz: F0_MHZ,
            depth_db,
            width_mhz: 8.0,
        },
    ]
}

fn roll_off() -> Vec<FaultKind> {
    vec![FaultKind::FrequencySelectiveLoss {
        location: span("span-5", 50.0),
        shape: LossShape::LowPass,
        f_lo_mhz: 0.0,
        f_hi_mhz: 1001.0,
        depth_db: 34.0,
        stop_return_loss_db: 20.0,
        skirt_mhz: Some(70.0),
    }]
}

/// Two bad reflectors 57 ft apart, on a modem at low level so its own noise, which comes after them, dominates.
fn ripple() -> Vec<FaultKind> {
    vec![
        low_drop(15.0),
        FaultKind::ConnectorReflection {
            location: span("span-6", 100.0),
            return_loss_db: 3.5,
            couplings: None,
        },
        FaultKind::ConnectorReflection {
            location: span("span-6", 157.0),
            return_loss_db: 3.5,
            couplings: None,
        },
    ]
}

fn water() -> Vec<FaultKind> {
    vec![FaultKind::WaterIngress {
        span_id: EdgeId::new("drop-8"),
        pos_ft: 20.0,
        affected_len_ft: 70.0,
        k_w_db_per_100ft: 40.0,
        rain_coupling_per_mm_per_hr: 0.0,
        impedance_variation_pct: 70.0,
    }]
}

fn optics() -> Vec<FaultKind> {
    vec![FaultKind::ForwardOptics { cnr_loss_db: 18.0 }]
}

fn filter() -> Vec<FaultKind> {
    vec![FaultKind::FrequencySelectiveLoss {
        location: span("drop-8", 5.0),
        shape: LossShape::BandStop,
        f_lo_mhz: 880.0,
        f_hi_mhz: 920.0,
        depth_db: 40.0,
        stop_return_loss_db: 3.0,
        skirt_mhz: None,
    }]
}

fn resonant(gain_db: f64) -> Vec<FaultKind> {
    vec![FaultKind::ResonantPeak {
        amp_id: ElementId::new("amp-2"),
        f0_mhz: F0_MHZ,
        gain_db,
        width_mhz: 8.0,
    }]
}

fn pullout() -> Vec<FaultKind> {
    vec![FaultKind::FrequencySelectiveLoss {
        location: span("span-5", 50.0),
        shape: LossShape::HighPass,
        f_lo_mhz: 830.0,
        f_hi_mhz: 0.0,
        depth_db: 36.0,
        stop_return_loss_db: 20.0,
        skirt_mhz: Some(100.0),
    }]
}

fn sweep_comb() -> Vec<FaultKind> {
    (0..30)
        .map(|k| ingress(815.025 + 6.0 * f64::from(k), 0.09, -5.5))
        .collect()
}

/// An SC-QAM just below the OFDM channel, and the channel set to Ncp, Nrp in µs.
fn with_neighbour(cp_us: f64, rolloff_us: f64) -> World {
    let mut world = low();
    let plan = &mut world.profile.channel_plan;
    let mut sc = plan.ds_sc_qam.last().unwrap().clone();
    sc.channel_id = schema::common::ChannelId::new("ds-sc-neighbour");
    sc.center_mhz = plan.ofdm[0].first_active_mhz - 4.0;
    plan.ds_sc_qam.push(sc);
    plan.ofdm[0].cp_us = cp_us;
    plan.ofdm[0].rolloff_us = rolloff_us;
    world
}

#[test]
fn healthy_is_like_the_public_capture() {
    let curve = ds_curve(&low(), vec![], FAR);
    let m = mean(&curve.db);
    eprintln!("mean {m:.2}");
    assert!((40.0..=47.0).contains(&m), "{m}");
    near("scatter", curve.scatter_db(), 0.79, 0.15);
}

#[test]
fn ingress_reaches_published_drops() {
    let base = ds_curve(&low(), vec![], FAR);
    for (power, want) in [(4.5, 6.0), (8.5, 10.0), (18.5, 20.0)] {
        let curve = ds_curve(&low(), vec![ingress(F0_MHZ, 10.0, power)], FAR);
        near("ingress drop", curve.worst_drop(&base), want, 3.0);
        near("ingress width", curve.width_mhz(&base), 10.0, 3.0);
    }
}

#[test]
fn several_ingress_bands_at_once() {
    let base = ds_curve(&low(), vec![], FAR);
    let curve = ds_curve(
        &low(),
        vec![
            ingress(850.0, 10.0, 18.5),
            ingress(900.0, 5.0, 14.5),
            ingress(950.0, 15.0, 8.5),
        ],
        FAR,
    );
    near(
        "band one",
        base.band_mean(848.0, 852.0) - curve.band_mean(848.0, 852.0),
        20.0,
        3.0,
    );
    near(
        "band three",
        base.band_mean(948.0, 952.0) - curve.band_mean(948.0, 952.0),
        10.0,
        3.0,
    );
    assert!((curve.band_mean(870.0, 880.0) - base.band_mean(870.0, 880.0)).abs() < 0.5);
}

#[test]
fn suckout_reaches_published_drops_on_a_modem_low_in_its_window() {
    let base = ds_curve(&low(), vec![low_drop(14.0)], FAR);
    eprintln!("baseline {:.1}", mean(&base.db));
    near("baseline, 38 in the field", mean(&base.db), 38.0, 3.0);
    for (depth, want) in [(16.0, 13.0), (20.0, 18.0), (32.0, 30.0)] {
        let curve = ds_curve(&low(), suckout(depth), FAR);
        near("suckout drop", curve.worst_drop(&base), want, 3.0);
        // Both sides fall: it's a dip, with no step left behind.
        assert!((curve.band_mean(860.0, 870.0) - base.band_mean(860.0, 870.0)).abs() < 1.0);
        assert!((curve.band_mean(930.0, 940.0) - base.band_mean(930.0, 940.0)).abs() < 1.0);
        let width = curve.width_mhz(&base);
        eprintln!("width {width:.1}");
        assert!((3.5..=26.0).contains(&width), "{width}");
    }
}

#[test]
fn roll_off_falls_gradually_at_the_edge() {
    let base = ds_curve(&low(), vec![], FAR);
    let curve = ds_curve(&low(), roll_off(), FAR);
    near("roll-off drop", curve.worst_drop(&base), 21.5, 4.0);
    let width = curve.width_mhz(&base);
    near("roll-off half width", width, 20.0, 12.0);
    let s = curve.smoothed();
    let per_mhz = curve.per_mhz();
    let steepest = s
        .windows(per_mhz + 1)
        .map(|w| w[0] - w[per_mhz])
        .fold(0.0, f64::max);
    eprintln!("steepest {steepest:.2} dB per MHz");
    assert!(steepest < 2.0, "a cliff: {steepest} dB per MHz");
}

#[test]
fn ripple_appears_when_the_noise_comes_after_the_echo() {
    let curve = ds_curve(&low(), ripple(), FAR);
    near("ripple", curve.ripple_pp(), 5.0, 2.0);
    let (period, share) = curve.strongest_period();
    near("ripple period", period, 7.5, 1.5);
    eprintln!("one period holds {share:.2} of the energy");
    assert!(share > 0.5, "{share}");
    let m = mean(&curve.db);
    eprintln!("mean {m:.2}");
}

#[test]
fn ripple_stays_flat_when_the_noise_comes_before_the_echo() {
    // [HRANAC2019 Test Case 3]: the plant's noise passes through the echo with the signal. The modem still rides high here.
    let weak_optics = vec![FaultKind::ForwardOptics { cnr_loss_db: 18.0 }];
    let mut with_echo = weak_optics.clone();
    with_echo.extend(ripple().into_iter().skip(1));
    let curve = ds_curve(&low(), with_echo, FAR);
    eprintln!("ripple {:.2}", curve.ripple_pp());
    assert!(curve.ripple_pp() < 1.5, "{}", curve.ripple_pp());
}

#[test]
fn water_makes_non_periodic_waves_on_a_falling_baseline() {
    let base = ds_curve(&low(), vec![], FAR);
    let curve = ds_curve(&low(), water(), FAR);
    let fall = mean(&base.db) - mean(&curve.db);
    eprintln!("fall {fall:.1}");
    assert!(fall >= 10.0, "{fall}");
    near("waves", curve.ripple_pp(), 5.0, 2.5);
    let (_, share) = curve.strongest_period();
    let (_, echo_share) = ds_curve(&low(), ripple(), FAR).strongest_period();
    eprintln!(
        "strongest period holds {share:.2} of the energy, against {echo_share:.2} for one echo"
    );
    assert!(share < echo_share * 0.7, "{share} vs {echo_share}");
}

#[test]
fn low_mer_from_the_optics_is_flat_and_node_wide() {
    let base = ds_curve(&low(), vec![], FAR);
    let curve = ds_curve(&low(), optics(), FAR);
    near("low MER", mean(&base.db) - mean(&curve.db), 12.5, 3.0);
    let near_modem_base = ds_curve(&low(), vec![], "modem-2");
    let near_modem = ds_curve(&low(), optics(), "modem-2");
    near(
        "everyone",
        mean(&near_modem_base.db) - mean(&near_modem.db),
        12.5,
        3.0,
    );
    assert!(curve.ripple_pp() < 1.0);
}

#[test]
fn flat_loss_lowers_rxmer_and_raises_transmit() {
    let base_m = observe(&low(), vec![], FAR);
    let loss_m = observe(&low(), vec![low_drop(30.0)], FAR);
    let base = ds_curve(&low(), vec![], FAR);
    let curve = ds_curve(&low(), vec![low_drop(30.0)], FAR);
    near("flat drop", mean(&base.db) - mean(&curve.db), 16.0, 3.0);
    // A flat loss adds no ripple of its own: the plant's own, about 1 dB here, stays what it was. (A bar of 1.0 dB flat sat
    // 0.001 dB from the curve and flipped on task 27's change, which moves no OFDM physics.)
    assert!(
        curve.ripple_pp() < base.ripple_pp() + 0.25,
        "{} vs {}",
        curve.ripple_pp(),
        base.ripple_pp()
    );
    let tx = |m: &ModemObservation| m.us_tx[0].psd_dbmv_per_1p6mhz;
    eprintln!("tx {:.1} to {:.1}", tx(&base_m), tx(&loss_m));
    assert!(tx(&loss_m) > tx(&base_m) + 3.0);
}

#[test]
fn edge_roll_off_follows_ncp_and_nrp() {
    let plain = with_neighbour(5.0, 1.25);
    let base = ds_curve(&plain, vec![], FAR);
    let short = ds_curve(&with_neighbour(0.9375, 0.625), vec![], FAR);
    let edge = |c: &Curve| c.band_mean(811.0, 811.3);
    near(
        "edge drop at Ncp 192, Nrp 128",
        edge(&base) - edge(&short),
        9.0,
        6.0,
    );
    assert!((base.band_mean(814.0, 900.0) - short.band_mean(814.0, 900.0)).abs() < 0.5);
    // Moved to Ncp 1024, Nrp 256, it's gone [HRANAC2019 §3.2], against a channel with no neighbour at all.
    let alone = ds_curve(&low(), vec![], FAR);
    assert!((edge(&base) - edge(&alone)).abs() < 1.0);
}

#[test]
fn a_high_block_of_channels_hurts_its_ofdm_neighbour() {
    let world = with_neighbour(0.9375, 0.625);
    let base = ds_curve(&world, vec![], FAR);
    let offset = FaultKind::AdjacentLevelOffset {
        f_lo_mhz: 600.0,
        f_hi_mhz: 810.0,
        offset_db: 8.0,
    };
    let curve = ds_curve(&world, vec![offset.clone()], FAR);
    let edge = |c: &Curve| c.band_mean(811.0, 811.3);
    eprintln!("edge {:.1} to {:.1}", edge(&base), edge(&curve));
    assert!(edge(&base) - edge(&curve) > 3.0);
    // Everywhere else in OFDM, and the block's own level, say what happened.
    assert!((base.band_mean(830.0, 990.0) - curve.band_mean(830.0, 990.0)).abs() < 0.5);
    let m = observe(&world, vec![offset], FAR);
    let b = observe(&world, vec![], FAR);
    let level = |o: &ModemObservation| match &o.ds_rx[0].ds_rx_dbmv {
        schema::observation::DsRxLevels::ScQam { level_dbmv } => *level_dbmv,
        other => panic!("{other:?}"),
    };
    near("block level", level(&m) - level(&b), 8.0, 0.5);
}

#[test]
fn a_spike_is_under_three_subcarriers() {
    let base = ds_curve(&low(), vec![], FAR);
    let curve = ds_curve(&low(), vec![ingress(900.025, 0.09, -12.0)], FAR);
    let hit = curve
        .db
        .iter()
        .zip(&base.db)
        .filter(|(a, b)| *b - *a > 5.0)
        .count();
    eprintln!("{hit} subcarriers");
    assert!((1..3).contains(&hit), "{hit}");
    let deepest = curve
        .db
        .iter()
        .zip(&base.db)
        .map(|(a, b)| b - a)
        .fold(0.0, f64::max);
    near("spike depth", deepest, 10.0, 3.0);
}

#[test]
fn a_sweep_comb_is_narrow_and_regular() {
    let base = ds_curve(&low(), vec![], FAR);
    let curve = ds_curve(&low(), sweep_comb(), FAR);
    let deepest = curve
        .db
        .iter()
        .zip(&base.db)
        .map(|(a, b)| b - a)
        .fold(0.0, f64::max);
    near("comb depth", deepest, 17.0, 3.0);
    let hits: Vec<f64> = curve
        .f_mhz
        .iter()
        .zip(curve.db.iter().zip(&base.db))
        .filter(|(_, (a, b))| *b - *a > 8.0)
        .map(|(f, _)| *f)
        .collect();
    eprintln!("{} subcarriers hit", hits.len());
    assert!(hits.len() >= 28 && hits.len() <= 80);
}

#[test]
fn a_filter_stops_a_band_on_one_drop() {
    let base = ds_curve(&low(), vec![], FAR);
    let curve = ds_curve(&low(), filter(), FAR);
    let inside = base.band_mean(885.0, 915.0) - curve.band_mean(885.0, 915.0);
    eprintln!("inside {inside:.1}");
    assert!(inside > 15.0, "{inside}");
    assert!(
        ds_curve(&low(), filter(), "modem-2").worst_drop(&ds_curve(&low(), vec![], "modem-2"))
            < 0.6
    );
}

#[test]
fn a_resonant_peak_lifts_the_level_and_dips_rxmer() {
    let base_o = observe(&low(), vec![], FAR);
    let peak_o = observe(&low(), resonant(17.0), FAR);
    let base = ds_curve(&low(), vec![], FAR);
    let curve = ds_curve(&low(), resonant(17.0), FAR);
    let dip = base.band_mean(898.0, 902.0) - curve.band_mean(898.0, 902.0);
    eprintln!("dip {dip:.1}");
    assert!(dip > 5.0, "{dip}");
    // [PNM-HFC §5.2.6.8] 17 dB of rise in FBC.
    let at = |o: &ModemObservation, f: f64| {
        let fbc = o.fbc.as_ref().unwrap();
        fbc.fbc_dbmv[((f - fbc.first_bin_center_mhz) * 1000.0 / fbc.bin_width_khz) as usize]
    };
    let rise = smooth_at(&peak_o, F0_MHZ, &at) - smooth_at(&base_o, F0_MHZ, &at);
    near("rise in FBC", rise, 17.0, 1.5);
    // Off the peak it's untouched.
    assert!((curve.band_mean(960.0, 990.0) - base.band_mean(960.0, 990.0)).abs() < 0.5);
}

fn smooth_at(o: &ModemObservation, f: f64, at: &dyn Fn(&ModemObservation, f64) -> f64) -> f64 {
    at(o, f)
}

#[test]
fn tilt_only_matters_when_excessive() {
    let drift = |tilt_delta_db| {
        vec![FaultKind::AmpDrift {
            amp_id: ElementId::new("amp-2"),
            state: AmpState::Drift {
                gain_delta_db: 0.0,
                tilt_delta_db,
                compression_onset_dbmv: None,
            },
        }]
    };
    // Tilt pivots at the top of the band, so it shows at OFDM's low end, on a modem with no headroom to spare.
    let with_low = |tilt: f64| {
        let mut kinds = drift(tilt);
        kinds.push(low_drop(14.0));
        kinds
    };
    let base = ds_curve(&low(), vec![low_drop(14.0)], FAR);
    let slope = |c: &Curve| c.band_mean(815.0, 835.0) - c.band_mean(980.0, 1000.0);
    let small = ds_curve(&low(), with_low(3.0), FAR);
    // SID's largest allowed tilt [PNM-HFC §5.2.6.13.1].
    let large = ds_curve(&low(), with_low(15.0), FAR);
    eprintln!(
        "slope {:.2} small {:.2} large {:.2}",
        slope(&base),
        slope(&small),
        slope(&large)
    );
    assert!((slope(&small) - slope(&base)).abs() < 1.0);
    assert!((slope(&large) - slope(&base)).abs() > 1.5);
}

#[test]
fn pullout_rolls_off_towards_low_frequency_and_raises_transmit() {
    let base = ds_curve(&low(), vec![], FAR);
    let curve = ds_curve(&low(), pullout(), FAR);
    let low_end = base.band_mean(811.0, 816.0) - curve.band_mean(811.0, 816.0);
    let high_end = base.band_mean(990.0, 1000.0) - curve.band_mean(990.0, 1000.0);
    eprintln!("low end {low_end:.1}, high end {high_end:.1}");
    assert!(low_end > 8.0 && high_end < 0.5);
    let tx = |m: &ModemObservation| m.us_tx[0].psd_dbmv_per_1p6mhz;
    let (b, p) = (
        observe(&low(), vec![], FAR),
        observe(&low(), pullout(), FAR),
    );
    eprintln!("tx {:.1} to {:.1}", tx(&b), tx(&p));
    assert!(tx(&p) > tx(&b) + 3.0);
}

#[test]
fn compression_lowers_rxmer_as_noise() {
    let squeezed = vec![FaultKind::AmpDrift {
        amp_id: ElementId::new("amp-2"),
        state: AmpState::Drift {
            gain_delta_db: 0.0,
            tilt_delta_db: 0.0,
            compression_onset_dbmv: Some(COMPRESSION_ONSET_DBMV),
        },
    }];
    let base = ds_curve(&low(), vec![], FAR);
    let curve = ds_curve(&low(), squeezed, FAR);
    let fall = mean(&base.db) - mean(&curve.db);
    eprintln!("fall {fall:.1}");
    assert!(fall > 2.0, "{fall}");
    assert!(curve.ripple_pp() < 1.0);
}

#[test]
fn a_roll_off_shows_phase_distortion_at_its_edge() {
    let gd = |o: &ModemObservation| {
        o.ds_chan_est.as_ref().unwrap()[0]
            .summary
            .as_ref()
            .unwrap()
            .gd_ripple_pp_ns
    };
    let base = gd(&observe(&low(), vec![], FAR));
    let hit = gd(&observe(&low(), roll_off(), FAR));
    eprintln!("group delay {base:.1} to {hit:.1} ns");
    assert!(hit > base + 5.0);
}

#[test]
fn common_path_distortion_raises_the_return_floor() {
    let base = us_curve(&high(), vec![], FAR);
    let cpd = FaultKind::CommonPathDistortion {
        location: span("span-5", 50.0),
        second_order_dbmv_per_mhz: -22.0,
        third_over_second_db: -6.0,
        schedule: always(),
    };
    let curve = us_curve(&high(), vec![cpd.clone()], FAR);
    let fall = mean(&base.db) - mean(&curve.db);
    eprintln!(
        "return RxMER {:.1} to {:.1}",
        mean(&base.db),
        mean(&curve.db)
    );
    assert!(fall > 3.0, "{fall}");
    // A raised floor, not a narrow dip: most of the channel moves.
    let moved = curve.drop_from(&base).iter().filter(|d| **d < -1.0).count();
    assert!(moved > curve.db.len() * 3 / 4, "{moved}");
    // Downstream doesn't see it.
    let ds_base = ds_curve(&high(), vec![], FAR);
    let ds = ds_curve(&high(), vec![cpd], FAR);
    assert!((mean(&ds_base.db) - mean(&ds.db)).abs() < 0.1);
}

#[test]
fn return_ingress_dips_ofdma_where_it_lands() {
    let vhf = |f_mhz, bw_mhz, power_dbmv, burst| FaultKind::UpstreamIngress {
        entry_location: span("span-6", 50.0),
        f_mhz,
        bw_mhz,
        power_dbmv,
        schedule: always(),
        burst,
    };
    let base = us_curve(&high(), vec![], FAR);
    // A VHF channel 8 carrier in the high-split return.
    let curve = us_curve(&high(), vec![vhf(183.0, 6.0, 15.0, None)], FAR);
    let dip = base.band_mean(181.0, 185.0) - curve.band_mean(181.0, 185.0);
    eprintln!("dip {dip:.1}");
    assert!(dip > 5.0, "{dip}");
    assert!((base.band_mean(120.0, 170.0) - curve.band_mean(120.0, 170.0)).abs() < 0.5);
    // A burst: RxMER reads time-average power, so it looks far better than the same noise held on.
    let burst = Burst {
        duration_ms: 1.0,
        rate_per_s: 10.0,
    };
    let bursty = us_curve(&high(), vec![vhf(183.0, 6.0, 15.0, Some(burst))], FAR);
    let bursty_dip = base.band_mean(181.0, 185.0) - bursty.band_mean(181.0, 185.0);
    eprintln!("burst dip {bursty_dip:.1}");
    assert!(bursty_dip < dip - 3.0);
}

/// Invariant 4, across every new fault at once: the same inputs give the same bytes.
#[test]
fn the_new_faults_are_deterministic() {
    let kinds = || {
        let mut kinds = water();
        kinds.extend(resonant(12.0));
        kinds.extend(optics());
        kinds.extend(roll_off());
        kinds.push(FaultKind::AdjacentLevelOffset {
            f_lo_mhz: 600.0,
            f_hi_mhz: 810.0,
            offset_db: 8.0,
        });
        kinds.push(FaultKind::CommonPathDistortion {
            location: span("span-5", 50.0),
            second_order_dbmv_per_mhz: -22.0,
            third_over_second_db: -6.0,
            schedule: always(),
        });
        kinds
    };
    let bytes = || serde_json::to_string(&observe(&low(), kinds(), FAR)).unwrap();
    assert_eq!(bytes(), bytes());
}

/// Every curve, for the gallery. Writes only when asked.
#[test]
fn gallery() {
    let Ok(out) = std::env::var("GALLERY_OUT") else {
        return;
    };
    let cases: Vec<(&str, World, Vec<FaultKind>, bool)> = vec![
        ("Healthy", low(), vec![], false),
        (
            "Ingress, 10 MHz",
            low(),
            vec![ingress(F0_MHZ, 10.0, 18.5)],
            false,
        ),
        (
            "Several ingress bands",
            low(),
            vec![
                ingress(850.0, 10.0, 18.5),
                ingress(900.0, 5.0, 14.5),
                ingress(950.0, 15.0, 8.5),
            ],
            false,
        ),
        ("Spike", low(), vec![ingress(900.025, 0.09, -12.0)], false),
        ("Sweep comb", low(), sweep_comb(), false),
        ("Suckout, low-level modem", low(), suckout(20.0), false),
        ("Roll-off", low(), roll_off(), false),
        ("Pullout", low(), pullout(), false),
        ("Ripple", low(), ripple(), false),
        ("Water", low(), water(), false),
        ("Low MER, optics", low(), optics(), false),
        ("Flat loss", low(), vec![low_drop(30.0)], false),
        ("Filter", low(), filter(), false),
        ("Resonant peak", low(), resonant(17.0), false),
        (
            "Edge roll-off",
            with_neighbour(0.9375, 0.625),
            vec![],
            false,
        ),
        (
            "Adjacency, block +8 dB",
            with_neighbour(0.9375, 0.625),
            vec![FaultKind::AdjacentLevelOffset {
                f_lo_mhz: 600.0,
                f_hi_mhz: 810.0,
                offset_db: 8.0,
            }],
            false,
        ),
        (
            "CPD, return",
            high(),
            vec![FaultKind::CommonPathDistortion {
                location: span("span-5", 50.0),
                second_order_dbmv_per_mhz: -22.0,
                third_over_second_db: -6.0,
                schedule: always(),
            }],
            true,
        ),
    ];
    let mut rows = Vec::new();
    for (name, world, faults, up) in cases {
        let curve = if up {
            us_curve(&world, faults, FAR)
        } else {
            ds_curve(&world, faults, FAR)
        };
        rows.push(json!({"case": name, "origin": "Synthetic", "f_mhz": curve.f_mhz, "rxmer_db": curve.db}));
    }
    fs::write(out, serde_json::to_string(&rows).unwrap()).unwrap();
}
