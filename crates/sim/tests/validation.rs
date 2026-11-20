//! `docs/physics.md` §12: the sim against published results, the only outside check there is. Passing
//! shows it reproduces what CableLabs measured in those setups, not that it matches any plant.
//! A row that misses is reported here and in `docs/validation.md`, and never tuned quietly.

use std::fs;

use plant_graph::PlantGraph;
use schema::common::Timestamp;
use schema::component::{ComponentLibrary, ComponentSpec, Duplex, GroupDelayPoint};
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::FaultSet;
use schema::plant::Plant;
use schema::profile::{Profile, Qam};
use sim::echo::{cavity_len_from_ripple_ft, ripple_period_mhz, ripple_pp_db};
use sim::fec;
use sim::preeq::{preeq_samples, remove_bulk_delay, sc_qam_preeq, sc_qam_preeq_from_samples};
use sim::telemetry::{Scenario, SimOptions, simulate};
use sim::units::{C64, db};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn within(name: &str, actual: f64, target: f64, tolerance: f64) {
    assert!(
        (actual - target).abs() <= tolerance,
        "{name}: {actual:.2} is not within {tolerance} of {target}"
    );
}

fn find_50_percent(f: impl Fn(f64) -> f64, lo: f64, hi: f64) -> f64 {
    let (mut a, mut b) = (lo, hi);
    for _ in 0..80 {
        let mid = (a + b) / 2.0;
        if f(mid) > 0.5 {
            a = mid;
        } else {
            b = mid;
        }
    }
    (a + b) / 2.0
}

#[test]
fn ofdm_uncorrectable_curve() {
    let profile: Profile = read("profiles/d31_highsplit_1218_rpd.json");
    for (qam, published) in [(Qam::Qam256, 23.45), (Qam::Qam4096, 34.45)] {
        let thr = fec::thr_db(&profile, qam).unwrap();
        let k = fec::ofdm_k(qam);
        let at = find_50_percent(
            |mer| fec::p_uncorr_ofdm(fec::effective_margin_db(&[mer; 16], thr), k),
            0.0,
            60.0,
        );
        within("uncorrectable 50 %", at, published, 1.0);
    }
}

#[test]
fn ofdm_correctable_curve() {
    let at = find_50_percent(|m| fec::p_corr_ofdm(m, 9.0, 15.0), -5.0, 30.0);
    within("correctable 50 %", at, 6.8, 1.0);
}

#[test]
fn narrowband_ingress_on_4096() {
    let k = fec::ofdm_k(Qam::Qam4096);
    // The guide's two figures read at different points on the curve, and don't say which, so each row
    // is checked where §4.6's oracle reads it: onset in 192 MHz, where failure is the wide band's limit,
    // and about half the codewords in 96 MHz.
    let in_band_at = |active_mhz: f64, rate: f64| {
        let n = (active_mhz / 0.05) as usize;
        let n_band = (8.0 / 0.05) as usize;
        find_50_percent(
            |band_mer| {
                let mut rxmer = vec![41.88; n];
                rxmer[..n_band].iter_mut().for_each(|x| *x = band_mer);
                let p = fec::p_uncorr_ofdm(fec::effective_margin_db(&rxmer, 35.0), k);
                if p > rate { 1.0 } else { 0.0 }
            },
            0.0,
            41.0,
        )
    };
    within("192 MHz", in_band_at(190.0, 1e-3), 8.5, 2.0);
    within("96 MHz", in_band_at(94.0, 0.49), 15.0, 2.0);
}

fn field_string() -> sim::preeq::ScQamPreeq {
    let rho = 10f64.powf(-10.0 / 20.0);
    sc_qam_preeq(30.0, 5.12, |f| {
        C64::ONE + C64::from_polar(rho, -2.0 * std::f64::consts::PI * f * 0.5)
    })
}

#[test]
fn reflection_taps_match_the_field_string() {
    let preeq = field_string();
    let tap = |i: usize| db(preeq.taps[i - 1].norm_sqr());
    within("tap 10", tap(10), -17.1, 2.0);
    within("tap 11", tap(11), -14.4, 2.0);
}

/// The one row that misses: a full inverse has about 1.1 dB more off-main energy than the published
/// string, against a ±1 dB tolerance. Recorded rather than tuned; see `docs/validation.md`. It fails loudly
/// if the miss grows or if it starts to pass, so this note can't go stale.
#[test]
fn reflection_nmter_known_miss() {
    let preeq = field_string();
    let miss = preeq.nmter_db - -11.61;
    assert!(
        miss > 1.0 && miss < 1.3,
        "NMTER {:.2}, miss {miss:.2}",
        preeq.nmter_db
    );
}

fn phase_of(table: &[GroupDelayPoint], f_mhz: f64, cascade: f64) -> C64 {
    // §5.6: φ = −2π∫τ df. A fine numeric integral, independent of the walk's closed form.
    let step = 0.01;
    let mut area = 0.0;
    let mut x = 0.0;
    while x < f_mhz {
        let tau = sim::passive::interpolate(
            &table
                .iter()
                .map(|p| schema::component::LossPoint {
                    f_mhz: p.f_mhz,
                    loss_db: p.delay_ns,
                })
                .collect::<Vec<_>>(),
            x,
        );
        area += tau * step;
        x += step;
    }
    C64::from_polar(1.0, -2.0 * std::f64::consts::PI * cascade * area * 1e-3)
}

#[test]
fn band_edge_group_delay_at_n_plus_5() {
    let library: ComponentLibrary = read("library/generic.json");
    let amp = library
        .get(&schema::common::ComponentId::new("amp-generic-1218"))
        .unwrap();
    let ComponentSpec::Amp(active) = &amp.spec else {
        panic!()
    };
    let Duplex::Diplexed { splits, .. } = &active.duplex else {
        panic!()
    };
    let split = splits.iter().find(|s| s.us_hi_mhz == 42.0).unwrap();
    // The top channel that fits below the 42 MHz edge, through N+5: a node and five amps.
    let samples = preeq_samples(38.8, 5.12);
    let response: Vec<C64> = samples
        .iter()
        .map(|s| phase_of(&split.group_delay_ns, s.f_mhz, 6.0))
        .collect();
    let preeq = sc_qam_preeq_from_samples(&samples, &remove_bulk_delay(&response));
    within("band-edge NMTER", preeq.nmter_db, -15.9, 3.0);
}

#[test]
fn echo_ripple() {
    within("5.69 dB p-p", ripple_pp_db(-10.0), 5.69, 0.1);
    within("2 MHz period", ripple_period_mhz(0.5), 2.0, 0.1);
    within(
        "8 MHz → 53.5 ft",
        cavity_len_from_ripple_ft(0.87, 8.0),
        53.5,
        0.5,
    );
}

#[test]
fn rxmer_on_healthy_plant() {
    let plant: Plant = read("one_street/plant_highsplit_rpd.json");
    let library: ComponentLibrary = read("library/generic.json");
    let profile: Profile = read("profiles/d31_highsplit_1218_rpd.json");
    let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
    let environment = Environment {
        schema_version: "1".into(),
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
        schema_version: "1".into(),
        seed: 0,
        faults: vec![],
    };
    let steps = simulate(&Scenario {
        graph: &graph,
        environment: &environment,
        faults: &faults,
        seed: 0,
        options: SimOptions::default(),
        link: None,
    })
    .unwrap();
    let means: Vec<f64> = steps[0]
        .modems
        .iter()
        .filter_map(|m| m.rxmer.as_ref())
        .flat_map(|r| r.iter().filter_map(|c| c.rxmer_mean_db))
        .collect();
    assert!(!means.is_empty());
    for mean in means {
        within("healthy RxMER", mean, 41.2, 2.0);
    }
}

// Task 22's anchors (§16): the analog link's four terms against the source's own worked example, the fitted return laser against the
// published points it was fitted to, and the digital budget and fiber delay against the standards' numbers.

#[test]
fn analog_link_cnr_scte_270() {
    use sim::optics::{edfa_term_db, noise_terms_db};
    use sim::units::lin;
    let (rin, shot, thermal) = noise_terms_db(0.0358, 0.0, -160.0, 1.0, 7.0, 4.0e6);
    let edfa = edfa_term_db(0.0358, 5.0, 5.5, 4.0e6);
    within("RIN", rin, 62.05, 0.02);
    within("EDFA", edfa, 56.78, 0.02);
    within("shot", shot, 56.99, 0.02);
    within("thermal", thermal, 65.14, 0.02);
    let link = -db(lin(-rin) + lin(-shot) + lin(-thermal) + lin(-edfa));
    within("link", link, 52.99, 0.02);
}

#[test]
fn analog_return_laser_tr_midsplit() {
    let library: ComponentLibrary = read("library/generic.json");
    let part = |id: &str| {
        &library
            .get(&schema::common::ComponentId::new(id))
            .unwrap()
            .spec
    };
    let (ComponentSpec::OpticalTx(tx), ComponentSpec::OpticalRx(rx)) =
        (part("rtx-generic-1310-npr"), part("rrx-generic"))
    else {
        panic!("parts");
    };
    let channels = 80.0 / 6.0;
    let snr = |total_dbmv: f64| {
        let m =
            tx.omi_pct / 100.0 * 10f64.powf((total_dbmv - db(channels) - tx.ref_rf_in_dbmv) / 20.0);
        sim::optics::analog_cnr(m, channels, -6.0, tx, rx, Vec::new()).cnr_db
    };
    within("7 dBmV", snr(7.0), 35.5, 0.5);
    within("26 dBmV", snr(26.0), 52.5, 0.5);
}

#[test]
fn digital_link_and_fiber_delay() {
    // 10GBASE-LR's launch range and sensitivity [IEEE 802.3 Cl.52], at the fixture's 1.3206 dB of loss.
    let launch_dbm = (-8.2 + 0.5) / 2.0;
    within("margin", launch_dbm - 1.3206 + 14.4, 9.2294, 0.001);
    // 500 µs per 100 km [TR-DCA §11.9.3].
    within(
        "delay",
        sim::optics::FIBER_DELAY_US_PER_KFT,
        500.0 / (100.0 * 3280.8399) * 1000.0,
        0.001,
    );
}
