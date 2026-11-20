//! Every worked example in `docs/physics.md` §1–§7, the test oracle (CLAUDE.md).

use std::collections::BTreeMap;
use std::fs;

use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId, PortId, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::EnvironmentSample;
use schema::fault::Location;
use schema::plant::Plant;
use schema::profile::Profile;
use sim::ac::{FeedSegment, solve_ac, solve_feed};
use sim::budget::{check_physics, level_budget};
use sim::cable::{atten_db_per_100ft, cable_loss_db, delay_us};
use sim::echo::{
    cavity_delay_us, cavity_len_from_ripple_ft, cavity_level_db, ripple_period_mhz, ripple_pp_db,
};
use sim::fec;
use sim::noise::*;
use sim::overlay::{Overlay, PointImpairment, ShapedLoss, dawson};
use sim::passive::{coupler_through_db, table_loss_db};
use sim::preeq::*;
use sim::transfer::transfer;
use sim::units::{C64, db, lin};
use sim::upstream::*;
use sim::walk::{Band, Grid, ModemView, Walker, out_level_dbmv};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

#[derive(Clone)]
struct Fixture {
    plant: Plant,
    library: ComponentLibrary,
    profile: Profile,
}

fn low() -> Fixture {
    Fixture {
        plant: read("one_street/plant_lowsplit_iccap.json"),
        library: read("library/generic.json"),
        profile: read("profiles/d31_lowsplit_1002_iccap.json"),
    }
}

fn high() -> Fixture {
    Fixture {
        plant: read("one_street/plant_highsplit_rpd.json"),
        library: read("library/generic.json"),
        profile: read("profiles/d31_highsplit_1218_rpd.json"),
    }
}

impl Fixture {
    fn graph(&self) -> PlantGraph<'_> {
        PlantGraph::new(&self.plant, &self.library, &self.profile).unwrap()
    }
}

fn at(temp_c: f64) -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(0),
        temp_c,
        ground_temp_c: temp_c,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

fn close(actual: f64, expected: f64, tol: f64) {
    assert!(
        (actual - expected).abs() <= tol,
        "{actual} is not within {tol} of {expected}"
    );
}

/// `f(view)` for one modem at one frequency.
fn at_modem<T>(
    graph: &PlantGraph<'_>,
    sample: &EnvironmentSample,
    overlay: &Overlay,
    band: Band,
    modem: &str,
    f_mhz: f64,
    f: impl Fn(&ModemView<'_>) -> T,
) -> T {
    let walker = Walker::new(graph, sample, overlay).unwrap();
    let grid = Grid::new(vec![f_mhz]);
    let mut out = None;
    walker
        .run(&grid, band, &mut |view: &ModemView<'_>| {
            if view.modem.id.as_str() == modem {
                out = Some(f(view));
            }
            Ok(())
        })
        .unwrap();
    out.unwrap()
}

fn cable_500(fixture: &Fixture) -> schema::component::CableSpec {
    let component = fixture
        .library
        .get(&schema::common::ComponentId::new("cable-500-p3"))
        .unwrap();
    match &component.spec {
        schema::component::ComponentSpec::Cable(spec) => spec.clone(),
        _ => panic!(),
    }
}

#[test]
fn section_1_cable() {
    let fixture = high();
    let spec = cable_500(&fixture);
    close(atten_db_per_100ft(&spec, 750.0), 1.98, 0.005);
    close(cable_loss_db(&spec, 750.0, 1250.0, 20.0), 24.8, 0.02);
    close(cable_loss_db(&spec, 750.0, 1250.0, 45.0), 26.04, 0.03);
    close(cable_loss_db(&spec, 750.0, 1250.0, -20.0), 22.82, 0.03);
    close(delay_us(&spec, 200.0), 0.2337, 0.0001);
    close(atten_db_per_100ft(&spec, 30.0), 0.341, 0.001);
}

#[test]
fn section_2_passives() {
    let fixture = high();
    let tap = fixture
        .library
        .get(&schema::common::ComponentId::new("tap-generic-23db-4p"))
        .unwrap();
    let schema::component::ComponentSpec::Tap(spec) = &tap.spec else {
        panic!()
    };
    close(
        table_loss_db(&spec.insertion_loss, tap, 750.0).unwrap(),
        1.24,
        0.005,
    );
    close(
        table_loss_db(&spec.insertion_loss, tap, 204.0).unwrap(),
        0.89,
        0.005,
    );
    close(
        table_loss_db(&spec.insertion_loss, tap, 30.0).unwrap(),
        0.80,
        1e-9,
    );
    close(
        table_loss_db(&spec.insertion_loss, tap, 1218.0).unwrap(),
        1.40,
        1e-9,
    );
    assert!(table_loss_db(&spec.insertion_loss, tap, 1300.0).is_err());
    assert!(table_loss_db(&spec.insertion_loss, tap, 1.0).is_err());
    close(coupler_through_db(8.0, 0.3), 1.05, 0.005);
}

#[test]
fn section_3_1_output() {
    close(out_level_dbmv(&high().profile, 750.0), 46.10, 0.005);
    close(out_level_dbmv(&low().profile, 750.0), 46.28, 0.005);
    close(out_level_dbmv(&high().profile, 1218.0), 52.0, 1e-9);
}

#[test]
fn section_3_2_modem() {
    for (fixture, expected) in [(high(), 7.24), (low(), 7.42)] {
        let graph = fixture.graph();
        let level = at_modem(
            &graph,
            &at(20.0),
            &Overlay::default(),
            Band::Down,
            "modem-4",
            750.0,
            |v| v.level_dbmv[0],
        );
        close(level, expected, 0.01);
    }
}

#[test]
fn section_3_2_whole_fixture() {
    let cases = [
        (high(), 5.08, 13.97, 4.84, 34.0),
        (low(), 5.81, 14.12, 6.82, 33.6),
    ];
    for (fixture, lo_expected, hi_expected, spread_expected, total_expected) in cases {
        let graph = fixture.graph();
        let (mut lo, mut hi, mut spread, mut total) =
            (f64::INFINITY, f64::NEG_INFINITY, 0f64, f64::NEG_INFINITY);
        for temp_c in [-20.0, 20.0, 45.0] {
            let budget = level_budget(&graph, &at(temp_c), &Overlay::default()).unwrap();
            for modem in budget.modems.values() {
                lo = lo.min(modem.level_min_dbmv);
                hi = hi.max(modem.level_max_dbmv);
                spread = spread.max(modem.spread_db);
                total = total.max(modem.total_input_dbmv);
            }
        }
        close(lo, lo_expected, 0.02);
        close(hi, hi_expected, 0.02);
        close(spread, spread_expected, 0.02);
        close(total, total_expected, 0.25);
    }
}

#[test]
fn clean_fixture_has_no_physics_issue() {
    for fixture in [high(), low()] {
        let graph = fixture.graph();
        let samples = [at(-20.0), at(20.0), at(45.0)];
        let issues = check_physics(&graph, &samples, &Overlay::default()).unwrap();
        assert_eq!(issues, vec![]);
        assert_eq!(graph.check_design(), vec![]);
    }
}

#[test]
fn section_4_1_to_4_4() {
    close(n_floor_dbmv(25_000.0), -81.22, 0.005);
    close(n_floor_6mhz_dbmv(), -57.42, 0.005);
    close(n_floor_dbmv(50_000.0), -78.21, 0.01);
    close(cnr_db(18.0, 7.0, -57.4), 68.4, 1e-9);
    close(cascade_cnr_db(vec![68.4; 6]), 60.62, 0.005);
    close(ctb_db(70.0, 53.0, 52.0), 68.0, 1e-9);
    close(cso_db(72.0, 53.0, 52.0), 71.0, 1e-9);
    close(cin_db(58.0, 53.0, 52.0, None), 57.0, 1e-9);
    close(cin_db(58.0, 53.0, 52.0, Some(52.5)), 56.0, 1e-9);
}

/// §4.5: combines what the walk gives with the modem's own terms.
fn mer_at_modem(fixture: &Fixture, modem: &str, f_mhz: f64, overlay: &Overlay) -> (f64, f64) {
    let graph = fixture.graph();
    at_modem(&graph, &at(20.0), overlay, Band::Down, modem, f_mhz, |v| {
        let level = v.level_dbmv[0];
        let inv = v.inv_plant[0] + lin(-cnr_cm_db(level, 10.0)) + lin(-44.0);
        (-db(inv), level)
    })
}

#[test]
fn section_4_5_mer() {
    let (mer, _) = mer_at_modem(&high(), "modem-4", 750.0, &Overlay::default());
    close(mer, 41.88, 0.02);
    // Adding CNR_opt = 48 dB to the high-split modem as a what-if on an analog node.
    close(-db(lin(-mer) + lin(-48.0)), 40.93, 0.02);
    // The term itself, with the same terms the document lists.
    close(
        mer_from_terms_db([47.0, 72.71, 58.0, 54.64, 44.0]),
        41.88,
        0.02,
    );
    // CNR_opt is what the low-split node actually carries.
    let (with_optics, level) = mer_at_modem(&low(), "modem-4", 750.0, &Overlay::default());
    assert!(with_optics < mer);
    close(level, 7.42, 0.01);
}

#[test]
fn section_4_5_ingress() {
    close(mer_ingress_db(7.24, -30.0, 10.0), 39.46, 0.005);
    let mut overlay = Overlay::default();
    overlay.port_points.insert(
        (ElementId::new("modem-4"), PortId::new("rf")),
        vec![PointImpairment {
            ds_ingress: Some(sim::overlay::Ingress {
                f_mhz: 750.0,
                bw_mhz: 10.0,
                power_dbmv: -30.0,
                peak_over_avg_db: 0.0,
            }),
            ..Default::default()
        }],
    );
    let (mer, _) = mer_at_modem(&high(), "modem-4", 750.0, &overlay);
    close(mer, 37.49, 0.02);
    // Only that modem: entry on its own port.
    let (neighbour, _) = mer_at_modem(&high(), "modem-3", 750.0, &overlay);
    assert!(neighbour > 40.0);
}

#[test]
fn section_4_5_isi() {
    assert_eq!(inv_mer_isi_ofdm(2.0, 2.5, 0.1, 20.0), 0.0);
    close(
        inv_mer_isi_ofdm(3.5, 2.5, 0.1, 20.0),
        2.0 * 1.0 * 0.01 / 20.0,
        1e-12,
    );
}

#[test]
fn section_4_6_fec() {
    let thr = 35.0;
    let k = fec::ofdm_k(schema::profile::Qam::Qam4096);
    close(k, 10.0, 1e-9);
    close(fec::ofdm_k(schema::profile::Qam::Qam256), 15.0, 1e-9);
    close(fec::ofdm_k(schema::profile::Qam::Qam1024), 12.5, 1e-9);

    let flat = vec![41.88; 1000];
    let m = fec::effective_margin_db(&flat, thr);
    close(m, 1.25, 1e-6);
    let p = fec::p_uncorr_ofdm(m, k);
    assert!((p - 1.5e-8).abs() < 0.1e-8, "{p}");
    close(fec::p_corr_ofdm(41.88 - thr, m, k), 0.455, 0.002);

    let notch = |band_mhz: f64, active_mhz: f64, mer_in_band: f64| -> f64 {
        let n = (active_mhz / 0.05) as usize;
        let n_band = (band_mhz / 0.05) as usize;
        let mut rxmer = vec![41.88; n];
        rxmer[..n_band].iter_mut().for_each(|x| *x = mer_in_band);
        fec::effective_margin_db(&rxmer, thr)
    };
    let m = notch(8.0, 190.0, 9.0);
    close(m, 0.12, 0.02);
    let p = fec::p_uncorr_ofdm(m, k);
    assert!((p - 1.2e-3).abs() < 0.3e-3, "{p}");
    let m = notch(8.0, 94.0, 15.0);
    close(m, -0.55, 0.03);
    close(fec::p_uncorr_ofdm(m, k), 0.49, 0.06);
}

#[test]
fn section_5_1_and_5_2_echo() {
    close(cavity_delay_us(150.0, 0.87), 0.3506, 0.0001);
    close(cavity_level_db(10.0, 18.0, 1.98, 150.0), -33.94, 0.005);
    close(ripple_pp_db(-33.9), 0.35, 0.005);
    close(ripple_period_mhz(0.3506), 2.85, 0.005);
    close(ripple_pp_db(-10.0), 5.69, 0.005);
    close(ripple_period_mhz(0.5), 2.0, 1e-9);
    close(cavity_len_from_ripple_ft(0.87, 8.0), 53.5, 0.05);
    close(cavity_len_from_ripple_ft(0.87, 2.85), 150.0, 0.2);
}

fn echo_h(e_db: f64, tau_us: f64) -> impl Fn(f64) -> C64 {
    let rho = 10f64.powf(e_db / 20.0);
    move |f_mhz: f64| C64::ONE + C64::from_polar(rho, -2.0 * std::f64::consts::PI * f_mhz * tau_us)
}

#[test]
fn section_5_3_preeq() {
    // At 30 MHz, §5.1 gives −29.02 dB at 0.3506 µs.
    let preeq = sc_qam_preeq(30.0, 5.12, echo_h(-29.02, 0.3506));
    let mags: Vec<f64> = preeq.taps.iter().map(|t| t.norm_sqr()).collect();
    let peak_after_main = mags[8..]
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i + 9)
        .unwrap();
    assert_eq!(peak_after_main, 10);
    assert!(mags[8] > mags[11]);
    close(preeq.mtr_db, 29.3, 0.1);
    close(preeq.nmter_db, -29.3, 0.1);
    close(preeq.mtc_db, 0.005, 0.002);
    assert_eq!(preeq.main_tap_index, 8);
}

#[test]
fn section_5_3_shape_with_tolerance() {
    let preeq = sc_qam_preeq(30.0, 5.12, echo_h(-10.0, 0.5));
    let mag = |i: usize| db(preeq.taps[i - 1].norm_sqr() / preeq.taps[7].norm_sqr());
    // The guide's field string peaks at taps 10 and 11 (−17.1, −14.4), with 9 and 12 lower.
    assert!(mag(10) > mag(9) && mag(11) > mag(12));
    close(mag(10), -17.1, 3.0);
    close(mag(11), -14.4, 3.0);
}

#[test]
fn section_5_4_ofdma_normalization() {
    let h: Vec<C64> = (0..2000)
        .map(|i| echo_h(-30.0, 0.3506)(100.0 + i as f64 * 0.05).scale(0.01))
        .collect();
    let c = ofdma_coefficients(&h);
    let mean: f64 = c.iter().map(|x| x.norm_sqr()).sum::<f64>() / c.len() as f64;
    close(mean, 1.0, 1e-9);
    // Flat loss doesn't reach the coefficients: mag_mean_db near 0 dB (§5.4).
    let f: Vec<f64> = (0..2000).map(|i| 100.0 + i as f64 * 0.05).collect();
    let summary = response_summary(&c, &f, 50_000.0);
    close(summary.mag_mean_db, 0.0, 0.01);
    // Ripple period 1/τ means an echo is visible in magnitude.
    assert!(summary.mag_ripple_pp_db > 0.4);
}

#[test]
fn section_5_5_summaries() {
    let mut rxmer = vec![40.0; 97];
    rxmer.extend([30.0; 3]);
    let (mean, std, p2) = rxmer_summary(&rxmer).unwrap();
    close(mean, 39.7, 1e-9);
    close(std, 1.71, 0.005);
    close(p2, 30.0, 1e-9);

    let (slope, rms, pp) = slope_and_ripple(&[0.0, 1.0, 2.0, 3.0], &[0.0, 2.0, 0.0, 2.0]);
    close(slope, 0.4, 1e-9);
    close(rms, 0.894, 0.001);
    close(pp, 2.4, 1e-9);

    let phases: Vec<f64> = (0..100)
        .map(|i| -2.0 * std::f64::consts::PI * 0.3506e-6 * 25_000.0 * i as f64)
        .collect();
    close(phases[1] - phases[0], -0.05507, 0.00001);
    let gd = group_delay_ns(&unwrap_phase(&phases), 25_000.0);
    assert_eq!(gd.len(), 99);
    close(gd[10], 350.6, 0.01);
}

#[test]
fn section_6_power() {
    let v = solve_feed(
        90.0,
        &[60.0, 60.0],
        &[FeedSegment {
            r_ohm: 1.7 * 1.5,
            beyond: vec![0, 1],
        }],
        3,
    );
    close(v[0], 86.47, 0.01);
    close(v[1], 86.47, 0.01);

    // The fixture, by hand: 1,200 ft to amp-1, 800 ft more to amp-2.
    for fixture in [low(), high()] {
        let graph = fixture.graph();
        let solution = solve_ac(&graph, &Overlay::default()).unwrap();
        close(solution.actives[&ElementId::new("amp-1")].v_in, 87.18, 0.01);
        close(solution.actives[&ElementId::new("amp-2")].v_in, 86.23, 0.01);
        assert_eq!(solution.browned_out(), Vec::<ElementId>::new());
        let supply = &solution.supplies[&ElementId::new("psu-1")];
        close(
            supply.i_total,
            150.0 / 90.0 + 60.0 / 87.18 + 60.0 / 86.23,
            0.01,
        );
    }
}

#[test]
fn section_6_sag_browns_out() {
    let fixture = high();
    let graph = fixture.graph();
    let mut overlay = Overlay::default();
    overlay.psu_v_delta.insert(ElementId::new("psu-1"), -28.0);
    let solution = solve_ac(&graph, &overlay).unwrap();
    assert!(solution.browned_out().contains(&ElementId::new("amp-2")));
}

#[test]
fn section_7_2_limits() {
    close(p1p6hi_dbmv(65.0, 0.0, 80.0), 48.01, 0.005);
    close(p1p6hi_dbmv(65.0, 0.0, 96.0), 47.22, 0.005);
    close(p1p6hi_dbmv(65.0, 0.0, 192.0), 44.21, 0.005);
    close(p1p6hi_dbmv(65.0, 0.0, 37.0), 51.20, 0.005);
    close(p1p6hi_dbmv(65.0, 0.0, 199.0), 44.03, 0.005);
    close(p1p6hi_dbmv(65.0, 0.0, 20.0), 53.2, 1e-9);
    close(per_1p6_from_per_6p4_db(), 6.02, 0.005);
}

#[test]
fn section_7_2_modem() {
    let cases = [
        (low(), 30.0, 8.0, 21.52, 29.52, 23.50),
        (high(), 204.0, 10.0, 28.01, 38.01, 31.99),
    ];
    for (fixture, f_mhz, p_in, loss, tx, reported) in cases {
        let graph = fixture.graph();
        let loss_up = at_modem(
            &graph,
            &at(20.0),
            &Overlay::default(),
            Band::Up,
            "modem-4",
            f_mhz,
            |v| v.loss_up_db[0],
        );
        close(loss_up, loss, 0.02);
        close(p_tx_dbmv_per_6p4(p_in, loss_up), tx, 0.02);
        close(tx - per_1p6_from_per_6p4_db(), reported, 0.02);
    }
}

#[test]
fn section_7_2_whole_fixture() {
    let cases = [(low(), 21.09, 28.57, 51.20), (high(), 23.09, 35.13, 44.03)];
    for (fixture, lo_expected, hi_expected, limit) in cases {
        let graph = fixture.graph();
        let budget = level_budget(&graph, &at(20.0), &Overlay::default()).unwrap();
        let lo = budget
            .modems
            .values()
            .map(|m| m.tx_min_dbmv_per_1p6mhz)
            .fold(f64::INFINITY, f64::min);
        let hi = budget
            .modems
            .values()
            .map(|m| m.tx_max_dbmv_per_1p6mhz)
            .fold(f64::NEG_INFINITY, f64::max);
        close(lo, lo_expected, 0.02);
        close(hi, hi_expected, 0.02);
        for (id, modem) in &budget.modems {
            let generation_d31 =
                !["modem-1", "modem-3", "modem-5", "modem-7"].contains(&id.as_str());
            if generation_d31 {
                close(modem.tx_limit_dbmv_per_1p6mhz, limit, 0.005);
            }
            assert!(modem.tx_min_dbmv_per_1p6mhz >= 17.0);
            assert!(modem.tx_max_dbmv_per_1p6mhz <= modem.tx_limit_dbmv_per_1p6mhz);
        }
    }
}

#[test]
fn section_7_3_funneling() {
    let n = n_coax_dbmv(6.4e6, 10.0, 2);
    close(n, -44.13, 0.01);
    close(us_snr_db(8.0, n, None), 52.13, 0.01);
    close(us_snr_db(8.0, n, Some(40.0)), 39.74, 0.01);
    close(us_snr_db(10.0, n, None), 54.13, 0.01);

    // The walk's funneling matches the formula on the fixture.
    for fixture in [low(), high()] {
        let graph = fixture.graph();
        let sample = at(20.0);
        let overlay = Overlay::default();
        let walker = Walker::new(&graph, &sample, &overlay).unwrap();
        let grid = Grid::new(vec![30.0]);
        let noise = walker
            .run(&grid, Band::Up, &mut |_: &ModemView<'_>| Ok(()))
            .unwrap();
        let per_hz = noise.avg_per_hz[&PortId::new("rf-1")][0];
        close(db(per_hz * 6.4e6), -44.13, 0.01);
    }
}

#[test]
fn return_attenuation_is_the_switch_test() {
    // §7.3: attenuating one leg lowers its share of the node's noise.
    let mut fixture = high();
    for element in &mut fixture.plant.elements {
        if element.id.as_str() == "amp-2" {
            element.kind = schema::plant::ElementKind::Amp {
                return_atten_db: 100.0,
            };
        }
    }
    let graph = fixture.graph();
    let sample = at(20.0);
    let overlay = Overlay::default();
    let walker = Walker::new(&graph, &sample, &overlay).unwrap();
    let noise = walker
        .run(&Grid::new(vec![30.0]), Band::Up, &mut |_: &ModemView<
            '_,
        >| Ok(()))
        .unwrap();
    close(
        db(noise.avg_per_hz[&PortId::new("rf-1")][0] * 6.4e6),
        n_coax_dbmv(6.4e6, 10.0, 1),
        0.01,
    );
}

// ---- beyond the oracles ----

#[test]
fn a_grid_of_one_equals_a_larger_grid() {
    for fixture in [high(), low()] {
        let graph = fixture.graph();
        let sample = at(35.0);
        let overlay = Overlay::default();
        let walker = Walker::new(&graph, &sample, &overlay).unwrap();
        let freqs = [600.0, 750.0, 801.0];
        let mut wide: BTreeMap<ElementId, Vec<(f64, f64)>> = BTreeMap::new();
        walker
            .run(
                &Grid::new(freqs.to_vec()),
                Band::Down,
                &mut |v: &ModemView<'_>| {
                    wide.insert(
                        v.modem.id.clone(),
                        v.level_dbmv
                            .iter()
                            .copied()
                            .zip(v.inv_plant.iter().copied())
                            .collect(),
                    );
                    Ok(())
                },
            )
            .unwrap();
        for (j, f) in freqs.iter().enumerate() {
            walker
                .run(&Grid::new(vec![*f]), Band::Down, &mut |v: &ModemView<
                    '_,
                >| {
                    let (level, inv) = wide[&v.modem.id][j];
                    assert_eq!(level.to_bits(), v.level_dbmv[0].to_bits());
                    assert_eq!(inv.to_bits(), v.inv_plant[0].to_bits());
                    Ok(())
                })
                .unwrap();
        }
    }
}

#[test]
fn transfer_matches_the_walk() {
    for fixture in [high(), low()] {
        let graph = fixture.graph();
        let sample = at(20.0);
        let overlay = Overlay::default();
        let f_mhz = 700.0;
        let root_out = 0.0;
        let _ = root_out;
        let amp2_out = Location::Port {
            element_id: ElementId::new("amp-2"),
            port: PortId::new("out"),
        };
        let modem8 = Location::Port {
            element_id: ElementId::new("modem-8"),
            port: PortId::new("rf"),
        };
        let t = transfer(&graph, &sample, &amp2_out, &modem8, f_mhz).unwrap();
        let level = at_modem(
            &graph,
            &sample,
            &overlay,
            Band::Down,
            "modem-8",
            f_mhz,
            |v| v.level_dbmv[0],
        );
        close(
            level,
            out_level_dbmv(graph.profile(), f_mhz) - t.loss_db,
            1e-9,
        );
        assert!(t.actives.is_empty());
        let rev = transfer(&graph, &sample, &modem8, &amp2_out, f_mhz).unwrap();
        close(rev.loss_db, t.loss_db, 1e-12);
        // Through amp-1 the path reports the active, and doesn't price it.
        let m1 = Location::Port {
            element_id: ElementId::new("modem-1"),
            port: PortId::new("rf"),
        };
        let through = transfer(&graph, &sample, &m1, &modem8, f_mhz).unwrap();
        assert_eq!(through.actives, vec![ElementId::new("amp-2")]);
    }
}

fn suckout_on(span: &str, depth_db: f64) -> Overlay {
    let mut overlay = Overlay::default();
    overlay.span_points.insert(
        EdgeId::new(span),
        vec![(
            100.0,
            PointImpairment {
                loss: Some(ShapedLoss::Gaussian {
                    f0_mhz: 700.0,
                    depth_db,
                    width_mhz: 5.0,
                }),
                ..Default::default()
            },
        )],
    );
    overlay
}

#[test]
fn a_suckout_passes_through_amps() {
    let fixture = high();
    let graph = fixture.graph();
    let sample = at(20.0);
    let clean = Overlay::default();
    let level = |overlay: &Overlay, modem: &str, f: f64| {
        at_modem(&graph, &sample, overlay, Band::Down, modem, f, |v| {
            v.level_dbmv[0]
        })
    };
    // On span-1 it's at every modem, through both amps. Its peak isn't at the pilot, so AGC leaves it.
    let high_up = suckout_on("span-1", 6.0);
    for modem in ["modem-1", "modem-4", "modem-8"] {
        close(
            level(&clean, modem, 700.0) - level(&high_up, modem, 700.0),
            6.0,
            1e-6,
        );
        close(
            level(&clean, modem, 800.0),
            level(&high_up, modem, 800.0),
            1e-6,
        );
    }
    // On span-3 it's only below it.
    let lower = suckout_on("span-3", 6.0);
    close(
        level(&clean, "modem-1", 700.0),
        level(&lower, "modem-1", 700.0),
        1e-9,
    );
    close(
        level(&clean, "modem-3", 700.0) - level(&lower, "modem-3", 700.0),
        6.0,
        1e-6,
    );
    close(
        level(&clean, "modem-8", 700.0) - level(&lower, "modem-8", 700.0),
        6.0,
        1e-6,
    );
}

#[test]
fn a_suckout_has_group_delay() {
    let fixture = high();
    let graph = fixture.graph();
    let sample = at(20.0);
    let overlay = suckout_on("span-3", 10.0);
    let walker = Walker::new(&graph, &sample, &overlay).unwrap();
    let f: Vec<f64> = (0..400).map(|i| 690.0 + i as f64 * 0.05).collect();
    let mut gd = Vec::new();
    walker
        .run(&Grid::new(f.clone()), Band::Down, &mut |v: &ModemView<
            '_,
        >| {
            if v.modem.id.as_str() == "modem-4" {
                let phase: Vec<f64> = v.h.iter().map(|z| z.arg()).collect();
                gd = group_delay_ns(&unwrap_phase(&phase), 50_000.0);
            }
            Ok(())
        })
        .unwrap();
    // What the notch adds, against the same modem with no notch.
    let clean = Overlay::default();
    let clean_walker = Walker::new(&graph, &sample, &clean).unwrap();
    let mut base = Vec::new();
    clean_walker
        .run(&Grid::new(f.clone()), Band::Down, &mut |v: &ModemView<
            '_,
        >| {
            if v.modem.id.as_str() == "modem-4" {
                let phase: Vec<f64> = v.h.iter().map(|z| z.arg()).collect();
                base = group_delay_ns(&unwrap_phase(&phase), 50_000.0);
            }
            Ok(())
        })
        .unwrap();
    let added: Vec<f64> = gd.iter().zip(&base).map(|(a, b)| a - b).collect();
    // A minimum-phase notch has its most negative delay at the centre, by the derivative of D at 0:
    // α·(2/√π)/(2π·w) cycles per MHz.
    let lowest = added
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| f[i])
        .unwrap();
    close(lowest, 700.0, 0.1);
    let alpha = 10.0 / 8.685889638;
    let expected_ns =
        alpha * std::f64::consts::FRAC_2_SQRT_PI / (5.0 * 2.0 * std::f64::consts::PI) * 1e3;
    let min = added.iter().copied().fold(f64::INFINITY, f64::min);
    close(min, -expected_ns, expected_ns * 0.02);
}

#[test]
fn dawson_matches_known_values() {
    close(dawson(0.0), 0.0, 1e-12);
    close(dawson(0.1), 0.0993359, 1e-6);
    close(dawson(1.0), 0.5380795, 1e-6);
    close(dawson(2.0), 0.3013404, 1e-6);
    close(dawson(-1.0), -0.5380795, 1e-6);
}

#[test]
fn piecewise_phase_is_real_and_causal_shaped() {
    let shape = ShapedLoss::selective(schema::fault::LossShape::BandStop, 700.0, 720.0, 20.0);
    close(shape.loss_db(710.0), 20.0, 1e-9);
    close(shape.loss_db(600.0), 0.0, 1e-9);
    // Group delay at a stop band's edges is where a filter's delay peaks.
    let gd_at = |f: f64| {
        let d = 0.01;
        -(shape.phase_rad(f + d) - shape.phase_rad(f - d)) / (2.0 * d * 2.0 * std::f64::consts::PI)
            * 1e3
    };
    assert!(gd_at(697.0) > gd_at(650.0));
    assert!(gd_at(650.0) > 0.0);
    assert!(gd_at(710.0) < 0.0);

    let hp = ShapedLoss::selective(schema::fault::LossShape::HighPass, 500.0, 0.0, 30.0);
    close(hp.loss_db(100.0), 30.0, 1e-9);
    close(hp.loss_db(700.0), 0.0, 1e-9);
    let lp = ShapedLoss::selective(schema::fault::LossShape::LowPass, 0.0, 900.0, 30.0);
    close(lp.loss_db(1000.0), 30.0, 1e-9);
    close(lp.loss_db(800.0), 0.0, 1e-9);
    // A low-pass's delay rises toward its edge from the passband, the ordinary filter result.
    let lp_gd = |f: f64| {
        let d = 0.01;
        -(lp.phase_rad(f + d) - lp.phase_rad(f - d)) / (2.0 * d * 2.0 * std::f64::consts::PI) * 1e3
    };
    assert!(lp_gd(890.0) > lp_gd(600.0));
    assert!(lp_gd(600.0) > 0.0);
}

#[test]
fn agc_holds_inside_its_range_and_not_past_it() {
    let fixture = high();
    let graph = fixture.graph();
    let overlay = Overlay::default();
    let target = |f: f64| out_level_dbmv(graph.profile(), f);
    // A modem right below amp-2, at 45 °C: only its own section's temperature change shows.
    let m = "modem-5";
    let walk_level = |temp_c: f64, f: f64| {
        at_modem(&graph, &at(temp_c), &overlay, Band::Down, m, f, |v| {
            v.level_dbmv[0]
        })
    };
    let a = Location::Port {
        element_id: ElementId::new("amp-2"),
        port: PortId::new("out"),
    };
    let b = Location::Port {
        element_id: ElementId::new(m),
        port: PortId::new("rf"),
    };
    for f in [300.0, 750.0, 1218.0] {
        let loss = transfer(&graph, &at(45.0), &a, &b, f).unwrap().loss_db;
        close(walk_level(45.0, f), target(f) - loss, 1e-9);
    }
    // At 80 °C the first span's change is past amp-1's 3 dB range, so amp-1's output falls.
    // 1,200 ft at 1218 MHz is 31.8 dB; ×0.002×60 is 3.81 dB, which is 0.81 dB past the range.
    let loss = transfer(&graph, &at(80.0), &a, &b, 1218.0).unwrap().loss_db;
    // amp-2 sees that 0.81 dB and its own section's 2.54 dB, 3.35 dB in all against a 3 dB range.
    let extra = target(1218.0) - loss - walk_level(80.0, 1218.0);
    close(extra, 0.35, 0.02);
    let amp1_below = "modem-1";
    let near = at_modem(
        &graph,
        &at(80.0),
        &overlay,
        Band::Down,
        amp1_below,
        1218.0,
        |v| v.level_dbmv[0],
    );
    let a1 = Location::Port {
        element_id: ElementId::new("amp-1"),
        port: PortId::new("out"),
    };
    let b1 = Location::Port {
        element_id: ElementId::new(amp1_below),
        port: PortId::new("rf"),
    };
    let loss1 = transfer(&graph, &at(80.0), &a1, &b1, 1218.0)
        .unwrap()
        .loss_db;
    close(target(1218.0) - loss1 - near, 0.81, 0.02);
}

#[test]
fn walks_are_byte_identical_and_order_free() {
    let digest = |fixture: &Fixture| {
        let graph = fixture.graph();
        let sample = at(33.0);
        let overlay = suckout_on("span-4", 3.0);
        let walker = Walker::new(&graph, &sample, &overlay).unwrap();
        let mut bytes = Vec::new();
        let grid = Grid::uniform(800.0, 0.05, 200);
        walker
            .run(&grid, Band::Down, &mut |v: &ModemView<'_>| {
                bytes.extend_from_slice(v.modem.id.as_str().as_bytes());
                for i in 0..v.grid.f_mhz.len() {
                    bytes.extend_from_slice(&v.level_dbmv[i].to_le_bytes());
                    bytes.extend_from_slice(&v.h[i].re.to_le_bytes());
                    bytes.extend_from_slice(&v.inv_plant[i].to_le_bytes());
                }
                Ok(())
            })
            .unwrap();
        bytes
    };
    let a = high();
    let mut b = high();
    assert_eq!(digest(&a), digest(&b));
    b.plant.elements.reverse();
    b.plant.edges.reverse();
    assert_eq!(digest(&a), digest(&b));
}

/// §4.5: the modem's ceiling ripples through the echo, [HRANAC2019 Test Cases 2B, 2C].
#[test]
fn a_shaped_ceiling_matches_the_lab() {
    let rho = 0.38100;
    let points: Vec<f64> = (0..720)
        .map(|k| {
            let theta = 2.0 * std::f64::consts::PI * f64::from(k) / 720.0;
            let h_sqr = 1.0 + rho * rho + 2.0 * rho * theta.cos();
            -sim::units::db(sim::telemetry::inv_impl_mer(46.79, h_sqr))
        })
        .collect();
    let mean = points.iter().sum::<f64>() / 720.0;
    let std = (points.iter().map(|m| (m - mean).powi(2)).sum::<f64>() / 720.0).sqrt();
    let highest = points.iter().copied().fold(f64::MIN, f64::max);
    let lowest = points.iter().copied().fold(f64::MAX, f64::min);
    assert!((mean - 46.65).abs() < 0.01, "{mean}");
    assert!((std - 1.60).abs() < 0.01, "{std}");
    assert!((highest - lowest - 4.72).abs() < 0.02);
}

/// §4.5.1's oracles.
#[test]
fn adjacent_leakage_matches_its_oracles() {
    use sim::telemetry::adjacent_leakage;
    let db = |x, t_w, t_u| -sim::units::db(adjacent_leakage(x, t_w, t_u));
    assert!((db(20.0, 0.3125, 20.0) - 29.55).abs() < 0.01);
    assert!((db(40.0, 0.3125, 20.0) - 42.85).abs() < 0.01);
    assert!((db(20.0, 3.75, 20.0) - 71.0).abs() < 0.05);
}

/// §8: a resonant peak's CIN and a weaker optical link are plain dB arithmetic, read through the sim's own helper.
#[test]
fn common_path_density_matches_its_oracle() {
    let mut profile: schema::profile::Profile = {
        let full = format!(
            "{}/../../fixtures/profiles/d31_lowsplit_1002_iccap.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(full).unwrap()).unwrap()
    };
    // Two occupied bands, 6 MHz apart: at 6 MHz only one pair overlaps, half what zero spacing has.
    profile.channel_plan.ds_sc_qam.truncate(2);
    profile.channel_plan.ofdm.clear();
    let at = sim::faults::cpd_density_per_mhz(&profile, 5.75, 6.25, 0.0, -300.0);
    let (f_mhz, dbmv) = at[0];
    assert!((f_mhz - 6.0).abs() < 1e-9);
    assert!((dbmv + 3.01).abs() < 0.05, "{dbmv}");
}

/// §13's plant: `one_street` low-split with `fixtures/one_street_as_built/`'s homes hung on its taps, no fittings. Home 2 on
/// `tap-1` port 2 in place of its drop, home 8 on port 3, home 3 (the drop amp) on port 4, home 4 (the filter) on `tap-2` port 3.
/// Ids that `one_street` already uses get an `x`.
fn with_homes() -> Fixture {
    use schema::common::ComponentId;
    use schema::plant::EdgeKind;
    let mut fixture = low();
    let as_built: Plant = read("one_street_as_built/plant_lowsplit_iccap.json");
    let plant = &mut fixture.plant;
    plant.edges.retain(|e| e.id.as_str() != "drop-2");
    plant.elements.retain(|e| e.id.as_str() != "modem-2");
    let rename = |id: &str| -> String {
        match id {
            "modem-3" | "modem-4" | "drop-3" | "drop-4" | "drop-8" => format!("{id}x"),
            other => other.to_string(),
        }
    };
    let keep = [
        "gb-2", "hsp-2", "modem-2", "stb-2", "gb-8", "hsp-8", "modem-8a", "modem-8b", "gb-3",
        "damp-3", "modem-3", "gb-4", "filter-4", "modem-4",
    ];
    for element in &as_built.elements {
        if keep.contains(&element.id.as_str()) {
            let mut element = element.clone();
            element.id = ElementId::new(rename(element.id.as_str()));
            plant.elements.push(element);
        }
    }
    let edges = [
        ("drop-2", Some(("tap-1", "tap-2"))),
        ("house-2a", None),
        ("house-2b", None),
        ("house-2c", None),
        ("drop-8", Some(("tap-1", "tap-3"))),
        ("house-8a", None),
        ("house-8b", None),
        ("house-8c", None),
        ("drop-3", Some(("tap-1", "tap-4"))),
        ("house-3a", None),
        ("house-3b", None),
        ("drop-4", Some(("tap-2", "tap-3"))),
        ("direct-4", None),
        ("house-4", None),
    ];
    for (id, from) in edges {
        let mut edge = as_built
            .edges
            .iter()
            .find(|e| e.id.as_str() == id)
            .unwrap()
            .clone();
        edge.id = EdgeId::new(rename(id));
        edge.to.element_id = ElementId::new(rename(edge.to.element_id.as_str()));
        if let Some((element, port)) = from {
            edge.from.element_id = ElementId::new(element);
            edge.from.port = PortId::new(port);
        }
        match &mut edge.kind {
            EdgeKind::Drop { fittings, .. } | EdgeKind::House { fittings, .. } => fittings.clear(),
            _ => {}
        }
        plant.edges.push(edge);
    }
    let _ = ComponentId::new("unused");
    fixture
}

#[test]
fn section_13_premises() {
    let fixture = with_homes();
    let graph = fixture.graph();
    let sample = at(20.0);
    let clean = Overlay::default();
    let level = |modem: &str| {
        at_modem(&graph, &sample, &clean, Band::Down, modem, 750.0, |v| {
            v.level_dbmv[0]
        })
    };
    close(level("modem-2"), 1.458, 0.005);
    close(level("modem-8a"), 4.546, 0.005);
    close(level("modem-8b"), 3.704, 0.005);
    close(level("modem-3x"), 19.032, 0.005);
    close(level("modem-4x"), 4.878, 0.005);

    // Upstream at 30 MHz: Loss_up, and P_tx at 8 dBmV into the node (§7.2).
    let loss_up = |modem: &str| {
        at_modem(&graph, &sample, &clean, Band::Up, modem, 30.0, |v| {
            v.loss_up_db[0]
        })
    };
    close(loss_up("modem-2"), 29.958, 0.005);
    close(8.0 + loss_up("modem-2") - db(4.0), 31.937, 0.005);
    close(loss_up("modem-3x"), 27.227, 0.005);

    // In-home echoes at 750 MHz: the modem's own cavity, and each device seen through the splitter's isolation.
    let echoes = |modem: &str| {
        at_modem(&graph, &sample, &clean, Band::Down, modem, 750.0, |v| {
            v.echoes.to_vec()
        })
    };
    let has = |modem: &str, tau_us: f64, e_db: f64| {
        let found = echoes(modem);
        assert!(
            found.iter().any(
                |e| (e.tau_us - tau_us).abs() < 1e-4 && (db(e.rho * e.rho) - e_db).abs() < 0.01
            ),
            "{modem}: no echo at {tau_us} µs, {e_db} dB in {found:?}"
        );
    };
    has("modem-2", 0.05980, -38.807);
    has("modem-2", 0.09569, -36.492);
    has("modem-8a", 0.08373, -43.930);
    has("modem-8b", 0.04784, -42.246);

    // The drop amp's own noise: the only §4.5 term `modem-3x` has that `modem-1`, on the same tap, doesn't.
    let inv = |modem: &str| {
        at_modem(&graph, &sample, &clean, Band::Down, modem, 750.0, |v| {
            v.inv_plant[0]
        })
    };
    close(db(inv("modem-3x") - inv("modem-1")), -62.573, 0.05);
}

#[test]
fn section_13_temperatures_and_filters() {
    use schema::plant::{EdgeKind, Installation};
    // A buried drop follows the ground; in-home cable sits at 20 °C whatever the air does.
    let mut fixture = with_homes();
    for edge in &mut fixture.plant.edges {
        if edge.id.as_str() == "drop-2"
            && let EdgeKind::Drop { installation, .. } = &mut edge.kind
        {
            *installation = Some(Installation::Underground);
        }
    }
    let graph = fixture.graph();
    let clean = Overlay::default();
    let mut warm_air = at(20.0);
    warm_air.temp_c = 40.0;
    let buried = at_modem(
        &graph,
        &warm_air,
        &clean,
        Band::Down,
        "modem-2",
        750.0,
        |v| v.level_dbmv[0],
    );
    let aerial_fixture = with_homes();
    let aerial_graph = aerial_fixture.graph();
    let aerial = at_modem(
        &aerial_graph,
        &warm_air,
        &clean,
        Band::Down,
        "modem-2",
        750.0,
        |v| v.level_dbmv[0],
    );
    // Only the drop moved: 140 ft of series 6 at 750 MHz, 20 °C warmer at 0.2 %/°C (§1.2).
    close(buried - aerial, 7.861 * 0.002 * 20.0, 0.005);
    close(
        sim::cable::exposure_temp_c(&warm_air, sim::cable::Exposure::Indoor),
        20.0,
        0.0,
    );

    // A point-of-entry low-pass: its pass loss to 1002 MHz, ramping over its 123 MHz edge to 40 dB more from 1125.
    let library = &fixture.library;
    let part = library
        .get(&schema::common::ComponentId::new("filter-generic-poe-1002"))
        .unwrap();
    let schema::component::ComponentSpec::Filter(spec) = &part.spec else {
        panic!()
    };
    let shape = sim::passive::filter_shape(spec);
    for (f, expected) in [
        (750.0, 1.6),
        (1002.0, 1.6),
        (1063.5, 21.6),
        (1125.0, 41.6),
        (1400.0, 41.6),
    ] {
        close(
            sim::passive::filter_loss_db(spec, &shape, f),
            expected,
            1e-9,
        );
    }
    let rl = sim::passive::filter_return_loss(spec, &shape);
    close(rl.at(750.0), 18.0, 1e-9);
    close(rl.at(1200.0), 1.0, 1e-9);
}

fn echo_at(found: &[sim::walk::Echo], tau_us: f64, e_db: f64) -> bool {
    found
        .iter()
        .any(|e| (e.tau_us - tau_us).abs() < 1e-4 && (db(e.rho * e.rho) - e_db).abs() < 0.01)
}

#[test]
fn section_14_two_seven_db_reflectors() {
    use schema::common::ComponentId;
    use schema::component::{
        Component, ComponentSpec, ConnectorKind, ConnectorSpec, SpecConfidence,
    };
    use schema::plant::{EdgeKind, Fitting};
    let mut fixture = low();
    fixture.library.components.push(Component {
        id: ComponentId::new("conn-7db"),
        version: 1,
        vendor: "Test".into(),
        model: "7 dB reflector".into(),
        source_url: None,
        spec_confidence: SpecConfidence::Generic,
        rated_lo_mhz: Some(5.0),
        rated_hi_mhz: Some(3000.0),
        tolerance: None,
        spec: ComponentSpec::Connector(ConnectorSpec {
            kind: ConnectorKind::HardlineSplice,
            cable_family: None,
            insertion_loss_db: 0.0,
            return_loss_db: 7.0,
            return_loss_table: Vec::new(),
        }),
    });
    for edge in &mut fixture.plant.edges {
        if edge.id.as_str() == "span-3"
            && let EdgeKind::Span { fittings, .. } = &mut edge.kind
        {
            for pos_ft in [100.0, 200.0] {
                fittings.push(Fitting {
                    pos_ft,
                    component_ref: ComponentId::new("conn-7db"),
                });
            }
        }
    }
    let graph = fixture.graph();
    let f = 224.796_233;
    let spec = cable_500(&fixture);
    close(atten_db_per_100ft(&spec, f), 1.0, 1e-6);
    for modem in ["modem-3", "modem-4", "modem-8"] {
        let found = at_modem(
            &graph,
            &at(20.0),
            &Overlay::default(),
            Band::Down,
            modem,
            f,
            |v| v.echoes.to_vec(),
        );
        assert!(echo_at(&found, 0.233_72, -16.0), "{modem}: {found:?}");
    }
    let above = at_modem(
        &graph,
        &at(20.0),
        &Overlay::default(),
        Band::Down,
        "modem-1",
        f,
        |v| v.echoes.to_vec(),
    );
    assert!(
        !echo_at(&above, 0.233_72, -16.0),
        "modem-1 is above the cavity"
    );
}

#[test]
fn section_14_an_open_port_on_a_low_value_tap() {
    use schema::common::ComponentId;
    use schema::plant::{Fitted, PassiveFitted, PortState, PortStateEntry};
    let mut fixture = low();
    for element in &mut fixture.plant.elements {
        if element.id.as_str() == "tap-4" {
            element.component_ref = ComponentId::new("tap-generic-8db-4p");
            element.fitted = Some(Fitted::Passive(PassiveFitted {
                conditioner: None,
                ports: vec![PortStateEntry {
                    port: PortId::new("tap-3"),
                    state: PortState::Open,
                }],
            }));
        }
    }
    let graph = fixture.graph();
    for modem in ["modem-7", "modem-8"] {
        let (found, discarded) = at_modem(
            &graph,
            &at(20.0),
            &Overlay::default(),
            Band::Down,
            modem,
            750.0,
            |v| (v.echoes.to_vec(), v.discarded_echo_power),
        );
        assert!(echo_at(&found, 0.701_15, -45.896), "{modem}: {found:?}");
        assert!(echo_at(&found, 0.701_15, -47.896), "{modem}: {found:?}");
        assert!(
            discarded > 0.0 && db(discarded) < -40.0,
            "{modem}: {discarded}"
        );
    }
    // Terminated instead, at 30 dB: 30 + 16 + 18 + 11.9 is under the floor, so the echo is gone.
    for element in &mut fixture.plant.elements {
        if let Some(Fitted::Passive(passive)) = &mut element.fitted {
            passive.ports[0].state = PortState::Terminated {
                component_ref: ComponentId::new("term-generic"),
            };
        }
    }
    let graph = fixture.graph();
    let found = at_modem(
        &graph,
        &at(20.0),
        &Overlay::default(),
        Band::Down,
        "modem-7",
        750.0,
        |v| v.echoes.to_vec(),
    );
    assert!(!echo_at(&found, 0.701_15, -45.896));
}

#[test]
fn section_14_fittings_inserters_and_tables() {
    use schema::common::ComponentId;
    use schema::plant::{EdgeKind, Fitting, PortRole};
    let base = low();
    let mut fixture = low();
    for edge in &mut fixture.plant.edges {
        if edge.id.as_str() == "span-2"
            && let EdgeKind::Span { fittings, .. } = &mut edge.kind
        {
            for pos_ft in [0.0, 400.0] {
                fittings.push(Fitting {
                    pos_ft,
                    component_ref: ComponentId::new("conn-generic-pin-500"),
                });
            }
        }
    }
    let level = |f: &Fixture, modem: &str| {
        at_modem(
            &f.graph(),
            &at(20.0),
            &Overlay::default(),
            Band::Down,
            modem,
            750.0,
            |v| v.level_dbmv[0],
        )
    };
    close(
        level(&base, "modem-1") - level(&fixture, "modem-1"),
        0.10,
        1e-9,
    );
    // `amp-2` is a new section, and its AGC restores what the pins took from its input.
    close(
        level(&base, "modem-6") - level(&fixture, "modem-6"),
        0.0,
        1e-9,
    );

    let sourced: ComponentLibrary = read("library/sourced.json");
    let inserter = base
        .library
        .get(&ComponentId::new("pi-generic-1218"))
        .unwrap();
    let schema::component::ComponentSpec::PowerInserter(spec) = &inserter.spec else {
        panic!()
    };
    close(
        table_loss_db(&spec.insertion_loss, inserter, 750.0).unwrap(),
        0.4735,
        1e-4,
    );
    let splitter = sourced
        .get(&ComponentId::new("splitter-abs-1802h"))
        .unwrap();
    let rl = sim::passive::part_return_loss(splitter, PortRole::Leg).unwrap();
    close(rl.at(1500.0), 18.0 - 2.0 * 282.0 / 582.0, 1e-9);
    close(rl.at(3.0), 18.0, 1e-9);
}

/// The as-built fixture on one profile, with loop resistance held at 20 °C, which §19 prices.
fn as_built(plant: &str, profile: &str) -> Fixture {
    let library: ComponentLibrary = read("library/generic.json");
    Fixture {
        plant: read(&format!("one_street_as_built/{plant}")),
        library,
        profile: read(&format!("profiles/{profile}")),
    }
}

fn as_built_low() -> Fixture {
    as_built("plant_lowsplit_iccap.json", "d31_lowsplit_1002_iccap.json")
}

/// `plant` without its root's optics, which §16 prices: §15's oracles read RF levels only.
fn without_optics(mut plant: Plant) -> Plant {
    use schema::plant::Fitted;
    for element in &mut plant.elements {
        if let Some(Fitted::Active(fitted)) = &mut element.fitted {
            fitted.forward_rx = None;
            fitted.return_tx = None;
            fitted.transceivers.clear();
        }
    }
    plant
}

#[test]
fn section_15_the_two_equalizer_shapes_at_558_mhz() {
    use schema::component::ComponentSpec;
    let library: ComponentLibrary = read("library/generic.json");
    let loss = |id: &str, f: f64| {
        let part = library.get(&schema::common::ComponentId::new(id)).unwrap();
        let ComponentSpec::PlugIn(spec) = &part.spec else {
            panic!("{id} isn't a plug-in");
        };
        sim::passive::plugin_loss_db(spec, f)
    };
    // §15's oracle: √f 4.893 dB, the feeder's two-term shape 5.280 dB, both 10.8 at 54 MHz and 0.8 at 1218.
    assert!((loss("eq-generic-10db", 558.0) - 4.893).abs() < 1e-3);
    assert!((loss("eq-generic-cable-10db", 558.0) - 5.280).abs() < 1e-3);
    for id in ["eq-generic-10db", "eq-generic-cable-10db"] {
        assert!((loss(id, 54.0) - 10.8).abs() < 1e-9, "{id}");
        assert!((loss(id, 1218.0) - 0.8).abs() < 1e-9, "{id}");
    }
}

#[test]
fn section_15_align_holding_keeps_the_equalizer_it_is_given() {
    use schema::common::ComponentId;
    let fixture = as_built_low();
    let amp = ElementId::new("amp-1");
    let held: BTreeMap<ElementId, ComponentId> =
        [(amp.clone(), ComponentId::new("eq-generic-cable-20db"))].into();
    let chosen =
        sim::align::align_holding(&fixture.plant, &fixture.library, &fixture.profile, &held)
            .unwrap();
    assert_eq!(
        chosen[&amp].input_eq,
        Some(ComponentId::new("eq-generic-cable-20db"))
    );
    // Holding nothing is `align`.
    let none = sim::align::align_holding(
        &fixture.plant,
        &fixture.library,
        &fixture.profile,
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        none,
        sim::align::align(&fixture.plant, &fixture.library, &fixture.profile).unwrap()
    );
}

#[test]
fn section_15_align_picks_the_hand_worked_plug_ins() {
    use schema::common::ComponentId;
    let fixture = as_built_low();
    let chosen = sim::align::align(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
    let amp1 = &chosen[&ElementId::new("amp-1")];
    let id = |s: &str| Some(ComponentId::new(s));
    assert_eq!(amp1.input_pad, id("pad-generic-10db"));
    assert_eq!(amp1.input_eq, id("eq-generic-20db"));
    assert!(amp1.outputs.iter().all(|o| o.pad == id("pad-generic-0db")));
    assert_eq!(amp1.return_pad, id("rpad-generic-12db"));
    assert_eq!(amp1.return_eq, id("req-generic-6db"));
    // The fixture holds what alignment chose, on both profiles, and alignment is deterministic.
    for fixture in [
        as_built_low(),
        as_built("plant_highsplit_rpd.json", "d31_highsplit_1218_rpd.json"),
    ] {
        let first = sim::align::align(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
        let again = sim::align::align(&fixture.plant, &fixture.library, &fixture.profile).unwrap();
        assert_eq!(first, again);
        for (amp, fitted) in &first {
            let stored = fixture
                .plant
                .elements
                .iter()
                .find(|e| &e.id == amp)
                .unwrap();
            assert_eq!(
                stored.fitted,
                Some(schema::plant::Fitted::Active(fitted.clone())),
                "{amp}"
            );
        }
    }
}

#[test]
fn section_15_the_first_amps_output_and_residual() {
    let fixture = as_built_low();
    let plant = without_optics(fixture.plant.clone());
    let graph = PlantGraph::new(&plant, &fixture.library, &fixture.profile).unwrap();
    let clean = Overlay::default();
    let sample = at(20.0);
    let walker = Walker::new(&graph, &sample, &clean).unwrap();
    let grid = Grid::new(vec![54.0, 550.0, 1002.0]);
    let capture = walker.capture(&grid, Band::Down).unwrap();
    let input = &capture.inputs[&ElementId::new("amp-1")];
    close(input[0], 30.893, 0.005);
    close(input[2], 28.108, 0.005);
    for port in ["out-1", "out-2"] {
        let out = &capture.outputs[&(ElementId::new("amp-1"), PortId::new(port))];
        close(out[0], 35.493, 0.005);
        close(out[1], 43.765, 0.005);
        close(out[2], 50.352, 0.005);
    }

    // Return: the residual at 5 and 42 MHz, what's left in `modem-1`'s path loss beyond the loss below `amp-1`.
    for (f, residual) in [(5.0, -0.690), (42.0, -0.243)] {
        let loss_up = at_modem(&graph, &sample, &clean, Band::Up, "modem-1", f, |v| {
            v.loss_up_db[0]
        });
        let a = Location::Port {
            element_id: ElementId::new("amp-1"),
            port: PortId::new("out-1"),
        };
        let b = Location::Port {
            element_id: ElementId::new("modem-1"),
            port: PortId::new("rf"),
        };
        let below = transfer(&graph, &sample, &a, &b, f).unwrap().loss_db;
        close(loss_up - below, residual, 0.005);
    }

    // Check, not an oracle: the residual through the cascade, at `amp-2`'s output, inside N/10 + 3 dB for N = 2.
    let band = Grid::uniform(54.0, 6.0, 159);
    let capture = walker.capture(&band, Band::Down).unwrap();
    let out = &capture.outputs[&(ElementId::new("amp-2"), PortId::new("out"))];
    let residual: Vec<f64> = band
        .f_mhz
        .iter()
        .zip(out)
        .map(|(f, l)| l - out_level_dbmv(&fixture.profile, *f))
        .collect();
    let spread = residual.iter().copied().fold(f64::MIN, f64::max)
        - residual.iter().copied().fold(f64::MAX, f64::min);
    assert!(spread <= 3.2, "peak to valley {spread} dB");
}

// §16: optical links. All synthetic: the fixture and its fiber plant are designed, not measured.

fn fiber() -> schema::fiber::FiberPlant {
    read("one_street_as_built/fiber.json")
}

fn as_built_high() -> Fixture {
    as_built("plant_highsplit_rpd.json", "d31_highsplit_1218_rpd.json")
}

fn node_link(fixture: &Fixture, fiber: &schema::fiber::FiberPlant) -> sim::optics::NodeLink {
    sim::optics::node_link(
        fiber,
        &fixture.plant,
        &fixture.library,
        &fixture.profile.channel_plan,
    )
    .unwrap()
    .unwrap()
}

fn part<'l>(library: &'l ComponentLibrary, id: &str) -> &'l schema::component::ComponentSpec {
    &library
        .get(&schema::common::ComponentId::new(id))
        .unwrap()
        .spec
}

#[test]
fn section_16_scte_270s_worked_example() {
    use sim::optics::{edfa_term_db, noise_terms_db};
    // [SCTE 270 §25.7.6]: m 0.0358 in 4 MHz, RIN −160, P_R 0 dBm, R 1.0, I_r 7, EDFA NF 5.5 at +5 dBm in.
    let (rin, shot, thermal) = noise_terms_db(0.0358, 0.0, -160.0, 1.0, 7.0, 4.0e6);
    close(rin, 62.05, 0.01);
    close(shot, 56.99, 0.01);
    close(thermal, 65.14, 0.01);
    let edfa = edfa_term_db(0.0358, 5.0, 5.5, 4.0e6);
    close(edfa, 56.78, 0.01);
    close(
        -db(lin(-rin) + lin(-shot) + lin(-thermal) + lin(-edfa)),
        52.98,
        0.01,
    );
    close(-db(lin(-rin) + lin(-shot) + lin(-thermal)), 55.33, 0.01);
}

#[test]
fn section_16_the_clipping_ratio() {
    use sim::optics::clip_ratio;
    // (1 + a²)·Q(a) − a·φ(a), worked by hand with erfc, against the sim's integral.
    for (a, rho) in [
        (1.0, 7.533_978_334e-2),
        (3.0, 2.034_350_805e-4),
        (4.95, 2.547_210_113e-8),
        (6.85, 1.428_268_693e-13),
    ] {
        let found = clip_ratio(1.0 / a);
        assert!(
            (found / rho - 1.0).abs() < 1e-6,
            "a {a}: {found} against {rho}"
        );
    }
    assert_eq!(clip_ratio(0.0), 0.0);
    assert_eq!(clip_ratio(0.01), 0.0);
}

#[test]
fn section_16_the_fixtures_forward_link() {
    let fixture = as_built_low();
    let link = node_link(&fixture, &fiber());
    let forward = link.forward.as_ref().unwrap();
    assert!(forward.analog && forward.up);
    assert_eq!(forward.hops.len(), 11);
    close(forward.loss_db.unwrap(), 12.1252, 0.0001);
    close(forward.received_dbm.unwrap(), -2.1252, 0.0001);
    close(forward.fiber_ft, 3030.0, 1e-9);
    close(forward.latency_us, 4.618, 0.001);
    let cnr = forward.cnr.as_ref().unwrap();
    close(cnr.rin_db, 60.286, 0.001);
    close(cnr.shot_db, 52.395, 0.001);
    close(cnr.thermal_db, 57.722, 0.001);
    close(cnr.clip_db.unwrap(), 75.959, 0.001);
    close(cnr.rms_index, 0.2020, 0.0001);
    close(cnr.cnr_db, 50.751, 0.001);
    close(link.forward_cnr_db().unwrap(), 50.751, 0.001);
    assert!(!link.forward_dark() && !link.digital_down());
}

#[test]
fn section_16_the_fixtures_return_link() {
    let fixture = as_built_low();
    let link = node_link(&fixture, &fiber());
    let ret = link.ret.as_ref().unwrap();
    assert!(ret.analog && ret.up);
    close(ret.loss_db.unwrap(), 1.3206, 0.0001);
    close(ret.received_dbm.unwrap(), 1.6794, 0.0001);
    close(ret.latency_us, 4.595, 0.001);
    let cnr = ret.cnr.as_ref().unwrap();
    close(cnr.rin_db, 59.208, 0.001);
    close(cnr.shot_db, 64.859, 0.001);
    close(cnr.thermal_db, 82.670, 0.001);
    close(cnr.clip_db.unwrap(), 128.343, 0.001);
    close(cnr.cnr_db, 58.147, 0.001);
    close(link.return_snr_db().unwrap(), 58.147, 0.001);
}

#[test]
fn section_16_the_return_anchors() {
    use schema::component::ComponentSpec;
    // [TR-MIDSPLIT §6.1.1.1]: about 35.5 dB at 7 dBmV total input and 52.5 at 26, for one tested laser.
    let library: ComponentLibrary = read("library/generic.json");
    let ComponentSpec::OpticalTx(tx) = part(&library, "rtx-generic-1310-npr") else {
        panic!("not a transmitter");
    };
    let ComponentSpec::OpticalRx(rx) = part(&library, "rrx-generic") else {
        panic!("not a receiver");
    };
    // 5-85 MHz loaded, at −6 dBm received (both approx).
    let channels = 80.0 / 6.0;
    let snr = |total_dbmv: f64| {
        let per_channel = total_dbmv - db(channels);
        let m = tx.omi_pct / 100.0 * 10f64.powf((per_channel - tx.ref_rf_in_dbmv) / 20.0);
        sim::optics::analog_cnr(m, channels, -6.0, tx, rx, Vec::new()).cnr_db
    };
    close(snr(7.0), 35.517, 0.001);
    close(snr(20.0), 48.517, 0.001);
    close(snr(26.0), 52.541, 0.001);
    close(snr(27.0), 47.467, 0.001);
    close(snr(28.0), 40.959, 0.001);
    // Within the stated tolerance of the source's two points.
    assert!((snr(7.0) - 35.5).abs() <= 0.5 && (snr(26.0) - 52.5).abs() <= 0.5);
    // Rising on the noise side, falling on the clipping side.
    let peak = (70..=280)
        .map(|k| (k as f64 / 10.0, snr(k as f64 / 10.0)))
        .fold(
            (0.0, f64::MIN),
            |best, p| if p.1 > best.1 { p } else { best },
        );
    assert!(peak.0 > 23.0 && peak.0 < 26.5, "peak at {} dBmV", peak.0);
}

#[test]
fn section_16_the_digital_links_margin() {
    let fixture = as_built_high();
    let fiber = fiber();
    let link = node_link(&fixture, &fiber);
    for one in [link.forward.as_ref().unwrap(), link.ret.as_ref().unwrap()] {
        assert!(!one.analog && one.up && one.cnr.is_none());
        close(one.launch_dbm, -3.85, 1e-9);
        close(one.loss_db.unwrap(), 1.3206, 0.0001);
        close(one.received_dbm.unwrap(), -5.1706, 0.0001);
        close(one.margin_db.unwrap(), 9.2294, 0.0001);
        close(one.overload_margin_db.unwrap(), 5.6706, 0.0001);
    }
    assert!(!link.digital_down());
    assert_eq!(link.forward_cnr_db(), None);
    assert_eq!(link.return_snr_db(), None);

    // 70,000 ft more on its own span: 8.54 dB more loss, still up; 80,000 ft, 9.76 dB more, below sensitivity.
    for (more_ft, up) in [(70_000.0, true), (80_000.0, false)] {
        let mut longer = fiber.clone();
        for cable in &mut longer.cables {
            if cable.id.as_str() == "fiber-3" {
                cable.len_ft += more_ft;
            }
        }
        let link = node_link(&fixture, &longer);
        let forward = link.forward.as_ref().unwrap();
        close(
            forward.margin_db.unwrap(),
            9.2294 - more_ft * 0.122 / 1000.0,
            0.0001,
        );
        assert_eq!(forward.up, up);
        assert_eq!(link.digital_down(), !up);
    }
}

#[test]
fn section_16_a_splice_with_no_loss_anywhere_is_an_invalid_plant() {
    let fixture = as_built_low();
    let mut fiber = fiber();
    for element in &mut fiber.elements {
        if element.id.as_str() == "closure-1" {
            // A closure part that states no splice loss.
            element.component_ref =
                schema::common::ComponentId::new("closure-commscope-fosc450-d6");
        }
    }
    let library: ComponentLibrary = read("library/sourced.json");
    let result = sim::optics::node_link(
        &fiber,
        &fixture.plant,
        &library,
        &fixture.profile.channel_plan,
    );
    assert!(
        matches!(&result, Err(sim::PhysicsError::InvalidPlant(m)) if m.contains("closure-1")),
        "{result:?}"
    );
    // A measured loss on the hop is enough.
    for link in &mut fiber.links {
        for hop in &mut link.hops {
            if let schema::fiber::Hop::Splice { loss_db, .. } = hop {
                *loss_db = Some(0.02);
            }
        }
    }
    let link = sim::optics::node_link(
        &fiber,
        &fixture.plant,
        &library,
        &fixture.profile.channel_plan,
    )
    .unwrap()
    .unwrap();
    close(link.ret.unwrap().loss_db.unwrap(), 1.3206 - 0.03, 0.0001);
}

#[test]
fn section_16_the_sim_reads_the_link() {
    use sim::telemetry::{Detail, Scenario, SimOptions, simulate};
    let fixture = as_built_low();
    let graph = fixture.graph();
    let link = node_link(&fixture, &fiber());
    let environment = schema::environment::Environment {
        schema_version: "1".into(),
        samples: vec![at(20.0)],
        ambient_sources: Vec::new(),
    };
    let faults = schema::fault::FaultSet {
        schema_version: "3".into(),
        seed: 1,
        faults: Vec::new(),
    };
    let run = |link: Option<&sim::optics::NodeLink>| {
        simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults: &faults,
            seed: 5,
            options: SimOptions {
                detail: Detail::Full,
                ..SimOptions::default()
            },
            link,
        })
        .unwrap()
    };
    let constant = run(None);
    let linked = run(Some(&link));
    assert_eq!(run(Some(&link)), linked, "deterministic");
    // The link's 50.75 dB is cleaner than the 48 dB constant, so downstream RxMER rises.
    let mean = |steps: &[sim::telemetry::StepObservation]| -> f64 {
        let values: Vec<f64> = steps[0]
            .modems
            .iter()
            .flat_map(|m| m.rxmer.iter().flatten())
            .filter_map(|r| r.rxmer_mean_db)
            .collect();
        values.iter().sum::<f64>() / values.len() as f64
    };
    assert!(
        mean(&linked) > mean(&constant),
        "{} {}",
        mean(&linked),
        mean(&constant)
    );
}

#[test]
fn section_16_a_digital_link_below_sensitivity_cuts_the_rpd_off() {
    use sim::telemetry::{Scenario, SimOptions, simulate};
    let fixture = as_built_high();
    let graph = fixture.graph();
    let mut far = fiber();
    for cable in &mut far.cables {
        if cable.id.as_str() == "fiber-3" {
            cable.len_ft += 80_000.0;
        }
    }
    let environment = schema::environment::Environment {
        schema_version: "1".into(),
        samples: vec![at(20.0)],
        ambient_sources: Vec::new(),
    };
    let faults = schema::fault::FaultSet {
        schema_version: "3".into(),
        seed: 1,
        faults: Vec::new(),
    };
    for (plant, cut) in [(fiber(), false), (far, true)] {
        let link = node_link(&fixture, &plant);
        let steps = simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults: &faults,
            seed: 5,
            options: SimOptions::default(),
            link: Some(&link),
        })
        .unwrap();
        let rpd = steps[0].rpd.as_ref().unwrap();
        assert_eq!(rpd.rf_output_up, !cut);
        assert_eq!(rpd.top_level_state == schema::rpd::RpdState::Offline, cut);
        // Still powered: the optic is dark, not the RPD.
        assert!(rpd.drawing_power);
    }
}

/// §14's figure, which waited for the whole as-built fixture to walk (§16): per modem, the summed power of the echoes the
/// per-pair floor drops, and of those it keeps, at §5.1's reference, relative to the main signal.
fn floor_figures(fixture: &Fixture) -> Vec<(String, usize, f64, f64)> {
    let graph = fixture.graph();
    let sample = at(20.0);
    let overlay = Overlay::default();
    let walker = Walker::new(&graph, &sample, &overlay).unwrap();
    let grid = Grid::new(vec![100.0]);
    let mut out = Vec::new();
    walker
        .run(&grid, Band::Down, &mut |view: &ModemView<'_>| {
            let kept: f64 = view.echoes.iter().map(|e| e.rho * e.rho).sum();
            out.push((
                view.modem.id.to_string(),
                view.echoes.len(),
                db(view.discarded_echo_power.max(1e-300)),
                db(kept.max(1e-300)),
            ));
            Ok(())
        })
        .unwrap();
    out
}

#[test]
fn section_14_what_the_floor_discards_on_the_whole_fixture() {
    for fixture in [as_built_low(), as_built_high()] {
        let rows = floor_figures(&fixture);
        assert_eq!(rows.len(), 9);
        for (modem, kept_n, discarded_db, kept_db) in &rows {
            assert!(
                (-41.8..=-38.4).contains(discarded_db),
                "{modem}: dropped {discarded_db}"
            );
            assert!((-32.5..=-28.5).contains(kept_db), "{modem}: kept {kept_db}");
            assert!(kept_db - discarded_db >= 9.0, "{modem}");
            assert!(*kept_n >= 7, "{modem}");
        }
        let (_, _, first, _) = &rows[0];
        close(*first, -41.747, 0.001);
    }
}

// §17: source MER, receiver ceiling and loading. The tables are test inputs, not library values. All synthetic.

/// `high()` with §17's oracle tables on the RPD and the amp part.
fn high_with_tables() -> Fixture {
    use schema::component::{CinPoint, ComponentSpec, MerPoint, NoiseFigurePoint};
    let mut fixture = high();
    for part in &mut fixture.library.components {
        match (part.id.as_str(), &mut part.spec) {
            ("rpd-generic-1218", ComponentSpec::Rpd(rpd)) => {
                rpd.source_mer = vec![
                    MerPoint {
                        f_mhz: 258.0,
                        mer_db: 49.0,
                    },
                    MerPoint {
                        f_mhz: 1218.0,
                        mer_db: 46.0,
                    },
                ];
            }
            ("amp-generic-1218", ComponentSpec::Amp(amp)) => {
                amp.nf_table = vec![
                    NoiseFigurePoint {
                        f_mhz: 1218.0,
                        nf_db: 9.5,
                    },
                    NoiseFigurePoint {
                        f_mhz: 54.0,
                        nf_db: 6.5,
                    },
                ];
                amp.cin_table = vec![
                    CinPoint {
                        f_mhz: 54.0,
                        cin_db: 61.0,
                    },
                    CinPoint {
                        f_mhz: 1218.0,
                        cin_db: 57.0,
                    },
                ];
                amp.ref_composite_dbmv = Some(66.0);
            }
            _ => {}
        }
    }
    fixture
}

#[test]
fn section_17_composite_power() {
    use sim::noise::composite_dbmv;
    let fixture = high();
    let plan = &fixture.profile.channel_plan;
    close(
        composite_dbmv(plan, 52.0, 12.1, 258.0, 1218.0),
        68.5606,
        0.0001,
    );
    // Flat, it's the level plus the loaded bandwidth in 6 MHz channels: 192 + 189.95 + 189.95 + two subcarriers.
    let channels = (192.0 + 2.0 * (189.95 + 0.05)) / 6.0;
    close(
        composite_dbmv(plan, 52.0, 0.0, 258.0, 1218.0),
        52.0 + db(channels),
        1e-9,
    );
}

#[test]
fn section_17_the_4_5_modem_with_tables() {
    let fixture = high_with_tables();
    // The amp crossed is the one §4.5 names, at its input level and the profile's top level.
    let (mer, level) = mer_at_modem(&fixture, "modem-4", 750.0, &Overlay::default());
    close(level, 7.24, 0.01);
    close(
        mer_from_terms_db([47.4625, 71.435, 56.048, 54.658, 44.0]),
        41.957,
        0.001,
    );
    close(mer, 41.957, 0.02);
    // Each table alone moves MER the way its term says.
    let (plain, _) = mer_at_modem(&high(), "modem-4", 750.0, &Overlay::default());
    close(plain, 41.88, 0.02);
    assert!(mer > plain);
}

#[test]
fn section_17_a_receiver_ceiling() {
    use sim::telemetry::{Scenario, SimOptions, simulate};
    close(-db(lin(-54.128) + lin(-50.0)), 48.581, 0.001);
    let environment = schema::environment::Environment {
        schema_version: "1".into(),
        samples: vec![at(20.0)],
        ambient_sources: Vec::new(),
    };
    let faults = schema::fault::FaultSet {
        schema_version: "3".into(),
        seed: 1,
        faults: Vec::new(),
    };
    let snrs = |fixture: &Fixture| -> Vec<f64> {
        let graph = fixture.graph();
        let steps = simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults: &faults,
            seed: 5,
            options: SimOptions {
                measurement_noise_db: 0.0,
                ..SimOptions::default()
            },
            link: None,
        })
        .unwrap();
        steps[0]
            .service_group
            .us_snr
            .iter()
            .map(|r| r.us_snr_db)
            .collect()
    };
    let plain = snrs(&high());
    let mut capped = high();
    for part in &mut capped.library.components {
        if let schema::component::ComponentSpec::Rpd(rpd) = &mut part.spec {
            rpd.us_rx_ceiling_db = Some(50.0);
        }
    }
    let with = snrs(&capped);
    assert!(!plain.is_empty() && plain.len() == with.len());
    // Reported to 0.1 dB, so within half a step of each.
    for (a, b) in plain.iter().zip(&with) {
        close(*b, -db(lin(-a) + lin(-50.0)), 0.1);
    }
    close(plain[0], 54.13, 0.051);
    close(with[0], 48.58, 0.051);
}

#[test]
fn section_17_the_cmts_behind_an_analog_node() {
    use schema::component::{ComponentSpec, MerPoint};
    use sim::telemetry::{Scenario, SimOptions, simulate};
    let flat = |mer_db: f64| {
        vec![
            MerPoint {
                f_mhz: 54.0,
                mer_db,
            },
            MerPoint {
                f_mhz: 1218.0,
                mer_db,
            },
        ]
    };
    let mut fixture = as_built_low();
    for part in &mut fixture.library.components {
        if let ComponentSpec::Cmts(cmts) = &mut part.spec {
            cmts.source_mer = flat(44.0);
            cmts.us_rx_ceiling_db = Some(45.0);
        }
    }
    let link = node_link(&fixture, &fiber());
    assert_eq!(link.cmts_source_mer, flat(44.0));
    assert_eq!(link.cmts_us_rx_ceiling_db, Some(45.0));

    let environment = schema::environment::Environment {
        schema_version: "1".into(),
        samples: vec![at(20.0)],
        ambient_sources: Vec::new(),
    };
    let faults = schema::fault::FaultSet {
        schema_version: "3".into(),
        seed: 1,
        faults: Vec::new(),
    };
    let run = |fixture: &Fixture, link: &sim::optics::NodeLink| {
        let graph = fixture.graph();
        simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults: &faults,
            seed: 5,
            options: SimOptions {
                measurement_noise_db: 0.0,
                ..SimOptions::default()
            },
            link: Some(link),
        })
        .unwrap()
    };
    let plain_fixture = as_built_low();
    let plain_link = node_link(&plain_fixture, &fiber());
    let plain = run(&plain_fixture, &plain_link);
    let tabled = run(&fixture, &link);
    let ds_mean = |steps: &[sim::telemetry::StepObservation]| -> f64 {
        let values: Vec<f64> = steps[0]
            .modems
            .iter()
            .flat_map(|m| m.rxmer.iter().flatten())
            .filter_map(|r| r.rxmer_mean_db)
            .collect();
        values.iter().sum::<f64>() / values.len() as f64
    };
    // A 44 dB source is below the 47 to 50 dB floors it replaces, and a 45 dB ceiling below the 58 dB optics.
    assert!(ds_mean(&tabled) < ds_mean(&plain) - 0.5);
    let us = |steps: &[sim::telemetry::StepObservation]| steps[0].service_group.us_snr[0].us_snr_db;
    assert!(us(&tabled) < 45.0 && us(&plain) > 45.0);

    // The node's own RF stages are one more term.
    let mut with_node = fixture.clone();
    for part in &mut with_node.library.components {
        if let ComponentSpec::Node(node) = &mut part.spec {
            node.source_mer = flat(44.0);
        }
    }
    assert!(ds_mean(&run(&with_node, &link)) < ds_mean(&tabled) - 0.5);
}

// §18: upstream baseline. Couplings and the source are test inputs. All synthetic.

fn ambient_source(
    level_dbmv_per_mhz: f64,
    multiplier_at_0h: f64,
) -> schema::environment::AmbientSource {
    let mut hourly_multipliers = [1.0; 24];
    hourly_multipliers[0] = multiplier_at_0h;
    hourly_multipliers[3] = 0.0;
    schema::environment::AmbientSource {
        id: "ambient-1".into(),
        lo_mhz: 5.0,
        hi_mhz: 42.0,
        level_dbmv_per_mhz,
        schedule: schema::fault::Schedule { hourly_multipliers },
    }
}

/// `with_homes()` with §18's couplings: `gb-2`, `gb-8`, `gb-3` at 55, 40 and 30 dB, and home 4's ground block with none fitted.
fn three_homes() -> Fixture {
    use schema::plant::{Fitted, GroundBlockFitted};
    let mut fixture = with_homes();
    for element in &mut fixture.plant.elements {
        let coupling_db = match element.id.as_str() {
            "gb-2" => Some(55.0),
            "gb-8" => Some(40.0),
            "gb-3" => Some(30.0),
            _ => None,
        };
        if let Some(coupling_db) = coupling_db {
            element.fitted = Some(Fitted::GroundBlock(GroundBlockFitted { coupling_db }));
        } else if element.id.as_str() == "gb-4" {
            element.fitted = None;
        }
    }
    fixture
}

fn node_noise_db(
    fixture: &Fixture,
    sources: &[schema::environment::AmbientSource],
    t: i64,
    f_mhz: f64,
) -> f64 {
    let graph = fixture.graph();
    let mut sample = at(20.0);
    sample.t = Timestamp(t);
    let overlay = Overlay {
        ambient: sim::faults::ambient_at(sources, sample.t),
        ..Overlay::default()
    };
    let walker = Walker::new(&graph, &sample, &overlay).unwrap();
    let noise = walker
        .run(&Grid::new(vec![f_mhz]), Band::Up, &mut |_: &ModemView<
            '_,
        >| Ok(()))
        .unwrap();
    db(noise.avg_per_hz[&PortId::new("rf-1")][0] * 6.4e6)
}

#[test]
fn section_18_three_homes_under_one_source() {
    let fixture = three_homes();
    let source = [ambient_source(0.0, 4.0)];
    // No source: §7.3's thermal funnel alone.
    close(node_noise_db(&fixture, &[], 0, 30.0), -44.13, 0.01);
    let with = node_noise_db(&fixture, &source, 0, 30.0);
    close(with, -39.21, 0.01);
    close(8.0 - with, 47.21, 0.01);
    // The sum by hand, home by home.
    let n_coax = -44.128;
    close(
        db(lin(n_coax) + lin(-67.032) + lin(-51.479) + lin(-41.313)),
        -39.213,
        0.001,
    );
    // Hour 3's schedule turns it off; hour 1's multiplier of 1 is 6.02 dB less from each home.
    close(
        node_noise_db(&fixture, &source, 3 * 3600, 30.0),
        -44.13,
        0.01,
    );
    let hour_1 = node_noise_db(&fixture, &source, 3600, 30.0);
    close(
        hour_1,
        db(lin(n_coax) + lin(-67.032 - 6.021) + lin(-51.479 - 6.021) + lin(-41.313 - 6.021)),
        0.01,
    );
    // Outside the band nothing changes, and a plant with no ground blocks fitted hears nothing.
    close(
        node_noise_db(&fixture, &source, 0, 45.0),
        node_noise_db(&fixture, &[], 0, 45.0),
        1e-12,
    );
    close(node_noise_db(&low(), &source, 0, 30.0), -44.13, 0.01);
}

#[test]
fn section_18_an_opened_shield_lets_ambient_in() {
    use schema::fault::{ConnectorCouplings, Fault, FaultKind, FaultSet, Onset};
    let fixture = low();
    let graph = fixture.graph();
    let source = [ambient_source(0.0, 1.0)];
    let fault = |ambient_coupling_db: Option<f64>, start: i64| FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.into(),
        seed: 1,
        faults: vec![Fault {
            onset: if start == 0 {
                Onset::Instant
            } else {
                Onset::Ramp {
                    start: Timestamp(start),
                    end: Timestamp(start + 1),
                }
            },
            kind: FaultKind::ConnectorReflection {
                location: Location::Port {
                    element_id: ElementId::new("modem-2"),
                    port: PortId::new("rf"),
                },
                return_loss_db: 30.0,
                couplings: Some(ConnectorCouplings {
                    ambient_coupling_db,
                    ..ConnectorCouplings::default()
                }),
            },
        }],
    };
    let noise = |faults: &FaultSet| {
        let sample = at(20.0);
        let mut overlay = Overlay::build(&graph, faults, &sample).unwrap();
        overlay.ambient = sim::faults::ambient_at(&source, sample.t);
        let walker = Walker::new(&graph, &sample, &overlay).unwrap();
        let noise = walker
            .run(&Grid::new(vec![30.0]), Band::Up, &mut |_: &ModemView<
                '_,
            >| Ok(()))
            .unwrap();
        db(noise.avg_per_hz[&PortId::new("rf-1")][0] * 6.4e6)
    };
    let sealed = noise(&fault(None, 0));
    close(sealed, -44.13, 0.01);
    let opened = noise(&fault(Some(20.0), 0));
    // At the modem's port: `Loss_up` to `amp-1` at 30 MHz is §7.2's 1.37 + 23 + 1.66 for `drop-2`'s 150 ft.
    let reach = 1.3654 + 23.0 + 150.0 * 1.1066 / 100.0;
    let entering = 0.0 - 60.0 - 20.0 - reach + db(6.4e6);
    close(opened, db(lin(-44.128) + lin(entering)), 0.02);
    // Before the fault starts, nothing.
    close(noise(&fault(Some(20.0), 1_000_000)), sealed, 1e-12);
}

#[test]
fn section_18_simulate_reads_the_environments_sources() {
    use sim::telemetry::{Scenario, SimOptions, simulate};
    let fixture = three_homes();
    let graph = fixture.graph();
    let faults = schema::fault::FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.into(),
        seed: 1,
        faults: Vec::new(),
    };
    let snrs = |ambient_sources: Vec<schema::environment::AmbientSource>| -> Vec<f64> {
        let environment = schema::environment::Environment {
            schema_version: schema::environment::SCHEMA_VERSION.into(),
            samples: vec![at(20.0)],
            ambient_sources,
        };
        let steps = simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults: &faults,
            seed: 5,
            options: SimOptions {
                measurement_noise_db: 0.0,
                ..SimOptions::default()
            },
            link: None,
        })
        .unwrap();
        steps[0]
            .service_group
            .us_snr
            .iter()
            .map(|r| r.us_snr_db)
            .collect()
    };
    let quiet = snrs(Vec::new());
    let loud = snrs(vec![ambient_source(0.0, 4.0)]);
    assert_eq!(quiet.len(), loud.len());
    // Every channel is in the source's band. Coax SNR falls about 5 dB, but the node's 40 dB optics constant is in line, so the
    // reading falls by about half a dB: at 30 MHz, 52.13 and 40 combine to 39.74, and 47.21 and 40 to 39.24.
    close(-db(lin(-52.13) + lin(-40.0)), 39.74, 0.01);
    close(-db(lin(-47.21) + lin(-40.0)), 39.24, 0.01);
    for (q, l) in quiet.iter().zip(&loud) {
        assert!(q - l > 0.3 && q - l < 0.8, "{q} {l}");
    }
}

// §19: environment. All synthetic.

#[test]
fn section_19_the_6_2_feed_at_45_c() {
    use sim::cable::{Exposure, exposure_temp_c, r_loop_ohm_per_kft};
    let mut sample = at(30.0);
    sample.solar_w_per_m2 = Some(750.0);
    let temp_c = exposure_temp_c(&sample, Exposure::Air);
    close(temp_c, 45.0, 1e-9);
    // Sun doesn't reach buried or in-home cable.
    close(
        exposure_temp_c(&sample, Exposure::Ground),
        sample.ground_temp_c,
        1e-12,
    );
    close(exposure_temp_c(&sample, Exposure::Indoor), 20.0, 1e-12);
    let mut cable = cable_500(&low());
    cable.r_loop_ohm_per_kft = 1.7;
    close(r_loop_ohm_per_kft(&cable, temp_c), 1.7, 1e-12);
    cable.r_loop_k_temp = Some(0.004);
    let r = r_loop_ohm_per_kft(&cable, temp_c);
    close(r, 1.87, 1e-12);
    let feed = |r_ohm_per_kft: f64, passes: usize| {
        solve_feed(
            90.0,
            &[60.0, 60.0],
            &[FeedSegment {
                r_ohm: r_ohm_per_kft * 1.5,
                beyond: vec![0, 1],
            }],
            passes,
        )[0]
    };
    close(feed(r, 1), 86.26, 0.005);
    close(feed(r, 2), 86.098, 0.0005);
    close(feed(r, 3), 86.090, 0.0005);
    close(feed(r, 6), 86.09, 0.005);
    close(feed(1.7, 6) - feed(r, 6), 0.37, 0.005);
}

#[test]
fn section_19_the_as_built_plant_sags_in_the_heat() {
    use sim::ac::{solve_ac, solve_ac_at};
    // `.750` states `r_loop_k_temp`; the fixture's express spans are aerial.
    let fixture = as_built_low();
    let graph = fixture.graph();
    let overlay = Overlay::default();
    let reference = solve_ac(&graph, &overlay).unwrap();
    close(
        reference.actives[&ElementId::new("amp-1")].v_in,
        solve_ac_at(&graph, &overlay, &at(20.0)).unwrap().actives[&ElementId::new("amp-1")].v_in,
        1e-12,
    );
    let mut hot = at(30.0);
    hot.solar_w_per_m2 = Some(750.0);
    let heated = solve_ac_at(&graph, &overlay, &hot).unwrap();
    let cold = solve_ac_at(&graph, &overlay, &at(-20.0)).unwrap();
    for (id, active) in &reference.actives {
        assert!(heated.actives[id].v_in <= active.v_in, "{id}");
        assert!(cold.actives[id].v_in >= active.v_in, "{id}");
    }
    let id = ElementId::new("amp-2");
    assert!(heated.actives[&id].v_in < reference.actives[&id].v_in - 0.01);
    // `one_street`'s cable states no temperature term, so nothing moves.
    let plain = low();
    let graph = plain.graph();
    assert_eq!(
        solve_ac(&graph, &overlay).unwrap(),
        solve_ac_at(&graph, &overlay, &hot).unwrap()
    );
}

#[test]
fn section_19_a_coupled_connector_at_two_samples() {
    use schema::fault::{ConnectorCouplings, Fault, FaultKind, FaultSet, Onset};
    use sim::faults::connector_rl_db;
    let couplings = ConnectorCouplings {
        temp_db_per_c: Some(0.2),
        wind_db_per_m_per_s: Some(-0.8),
        ambient_coupling_db: None,
    };
    close(connector_rl_db(14.0, &couplings, 20.0, 0.0), 14.0, 1e-12);
    close(connector_rl_db(14.0, &couplings, 20.0, 10.0), 6.0, 1e-12);
    close(connector_rl_db(14.0, &couplings, -5.0, 10.0), 1.0, 1e-12);
    close(connector_rl_db(14.0, &couplings, -30.0, 15.0), 0.0, 1e-12);

    // In the walk: the fault's echo on `span-3` grows by the change in its return loss, the cable unchanged.
    let fixture = low();
    let graph = fixture.graph();
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.into(),
        seed: 1,
        faults: vec![Fault {
            onset: Onset::Instant,
            kind: FaultKind::ConnectorReflection {
                location: Location::Span {
                    span_id: EdgeId::new("span-3"),
                    pos_ft: 150.0,
                },
                return_loss_db: 14.0,
                couplings: Some(couplings),
            },
        }],
    };
    let strongest_fault_echo = |sample: &EnvironmentSample| -> (f64, f64) {
        let overlay = Overlay::build(&graph, &faults, sample).unwrap();
        at_modem(
            &graph,
            sample,
            &overlay,
            Band::Down,
            "modem-4",
            600.0,
            |v| {
                v.echoes
                    .iter()
                    .filter(|e| e.fault)
                    .map(|e| (e.tau_us, 20.0 * e.rho.log10()))
                    .fold(
                        (0.0, f64::MIN),
                        |best, e| if e.1 > best.1 { e } else { best },
                    )
            },
        )
    };
    let calm = at(20.0);
    let mut windy = at(20.0);
    windy.wind_m_per_s = Some(10.0);
    let (tau_calm, e_calm) = strongest_fault_echo(&calm);
    let (tau_windy, e_windy) = strongest_fault_echo(&windy);
    close(tau_windy, tau_calm, 1e-12);
    close(e_windy - e_calm, 8.0, 1e-9);
    // No coupling stated: the series changes nothing.
    let mut plain = faults.clone();
    if let FaultKind::ConnectorReflection { couplings, .. } = &mut plain.faults[0].kind {
        *couplings = None;
    }
    let a = Overlay::build(&graph, &plain, &calm).unwrap();
    let b = Overlay::build(&graph, &plain, &windy).unwrap();
    assert_eq!(a, b);
}

/// A home's echoes are synthesized by chunks on an evenly spaced grid (approx, numerical). The same walk on the same frequencies in
/// descending order has no ascending stretch, so every echo takes the exact path: the two agree to under 1e-6.
#[test]
fn a_homes_echoes_by_chunks_match_the_exact_ones() {
    let fixture = as_built_low();
    let graph = fixture.graph();
    let sample = at(20.0);
    let overlay = Overlay::default();
    let walker = Walker::new(&graph, &sample, &overlay).unwrap();
    let up: Vec<f64> = (0..400).map(|i| 600.0 + 0.05 * i as f64).collect();
    let mut down = up.clone();
    down.reverse();
    let responses = |grid: &Grid| {
        let mut out: BTreeMap<String, Vec<C64>> = BTreeMap::new();
        walker
            .run(grid, Band::Down, &mut |view: &ModemView<'_>| {
                out.insert(view.modem.id.to_string(), view.h.to_vec());
                Ok(())
            })
            .unwrap();
        out
    };
    let chunked = responses(&Grid::new(up));
    let exact = responses(&Grid::new(down));
    let mut worst: f64 = 0.0;
    let mut homes = 0;
    for (modem, h) in &chunked {
        let e = &exact[modem];
        let n = h.len();
        for i in 0..n {
            let d = C64 {
                re: h[i].re - e[n - 1 - i].re,
                im: h[i].im - e[n - 1 - i].im,
            };
            worst = worst.max(d.norm_sqr().sqrt());
        }
        if modem.starts_with("modem-8") || modem == "modem-2" {
            homes += 1;
        }
    }
    assert_eq!(homes, 3);
    assert!(worst < 1e-6, "worst {worst}");
    // And it isn't vacuous: the homes' responses carry echoes.
    let ripple = |h: &Vec<C64>| {
        let mags: Vec<f64> = h.iter().map(|c| c.norm_sqr().sqrt()).collect();
        mags.iter().copied().fold(f64::MIN, f64::max)
            - mags.iter().copied().fold(f64::MAX, f64::min)
    };
    assert!(ripple(&chunked["modem-2"]) > 1e-3);
}

#[test]
fn section_4_5_an_sc_qam_equalizer_takes_out_an_echo_within_its_span() {
    // Task 27: −15 dBc at 0.29 µs leaves −45 dBc, −10 dBc at 0.5 µs leaves −40 dBc, and −15 dBc past the span all of it.
    let rho = |e_db: f64| 10f64.powf(e_db / 20.0);
    let mer = |tau_us: f64, e_db: f64| -db(inv_mer_isi_sc(tau_us, rho(e_db)));
    assert!((mer(0.29, -15.0) - 45.0).abs() < 1e-9);
    assert!((mer(0.5, -10.0) - 40.0).abs() < 1e-9);
    assert!((mer(2.0, -15.0) - 15.0).abs() < 1e-9);
}
