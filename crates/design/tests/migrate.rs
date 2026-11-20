//! Migration planning on the fixture, by hand. All synthetic.

use std::fs;

use design::migrate::plan_migration;
use design::{DesignParts, design_region, generate};
use schema::common::{ComponentId, Origin};
use schema::component::ComponentLibrary;
use schema::plant::{ElementKind, Plant, RecordStatus};
use schema::profile::{Profile, Qam};
use schema::region::{DesignControls, LayoutFamily};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

struct Setup {
    plant: Plant,
    library: ComponentLibrary,
    low: Profile,
    high: Profile,
}

fn setup() -> Setup {
    Setup {
        plant: read("one_street/plant_lowsplit_iccap.json"),
        library: read("library/generic.json"),
        low: read("profiles/d31_lowsplit_1002_iccap.json"),
        high: read("profiles/d31_highsplit_1218_rpd.json"),
    }
}

fn close(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} against {b}");
}

#[test]
fn the_fixture_low_to_high() {
    let s = setup();
    let plan = plan_migration(&s.plant, &s.library, &s.low, &s.high).unwrap();
    assert_eq!(plan.origin, Origin::Synthetic);
    assert_eq!(plan.to_profile, "d31_highsplit_1218_rpd");
    assert_eq!(plan.modems_losing_service, 0);
    assert_eq!(plan.issues_before, Vec::<String>::new());
    assert_eq!(plan.issues_after, Vec::<String>::new());

    // Only the root changes: the amps, taps and cable all cover 1218 MHz.
    assert_eq!(plan.parts_must_change.len(), 1);
    assert_eq!(plan.parts_must_change[0].from.as_str(), "node-generic-1002");
    assert_eq!(
        plan.parts_must_change[0].to.as_ref().map(|c| c.as_str()),
        Some("rpd-generic-1218")
    );
    let root = plan.converted_plant.root().unwrap();
    assert!(matches!(root.kind, ElementKind::Rpd { .. }));
    assert_eq!(root.record_status, RecordStatus::Designed);
    // Nothing else had to be touched, so it stays as built.
    assert!(
        plan.converted_plant
            .elements
            .iter()
            .filter(|e| e.id != root.id)
            .all(|e| e.record_status == RecordStatus::Built)
    );

    // By hand. Four SC-QAM upstreams at 5.12 Msym/s and 64-QAM: 4 × 30.72 = 122.88 Mbit/s.
    let by = |id: &str| plan.modems.iter().find(|m| m.modem.as_str() == id).unwrap();
    for m in &plan.modems {
        close(m.before.us_capacity_mbps, 122.88, 1e-6);
        assert!(m.before.operational && m.after.operational);
        assert!(m.before.us_best_qam.is_none());
    }
    // A D3.0 modem can't use OFDMA, so high-split gives it nothing upstream.
    close(by("modem-1").after.us_capacity_mbps, 122.88, 1e-6);
    // A D3.1 modem adds 1,899 subcarriers at 12 bits per 22.5 µs: 1012.8 Mbit/s.
    let d31 = by("modem-4");
    assert_eq!(d31.after.us_best_qam, Some(Qam::Qam4096));
    close(
        d31.after.us_capacity_mbps,
        122.88 + 1899.0 * 12.0 / 22.5,
        1.0,
    );

    // Downstream, 32 SC-QAM at 42.88 Mbit/s, which is all a D3.0 modem can use.
    close(
        by("modem-1").before.ds_capacity_mbps,
        32.0 * 5.360537 * 8.0,
        0.01,
    );
    // D3.1 adds one OFDM channel before, and two after.
    assert!(d31.after.ds_capacity_mbps > d31.before.ds_capacity_mbps + 1500.0);

    // §7.2: high-split costs headroom: 51.20 − 28.57 before against 44.03 − 35.13 after, at worst.
    let worst = by("modem-4");
    assert!(worst.after.tx_headroom_db < worst.before.tx_headroom_db - 5.0);
    assert!(plan.us_capacity_after_mbps > plan.us_capacity_before_mbps * 4.0);

    // §4.5's oracle: at 41.88 dB, 48 dB optics cost 0.95 dB. Labeled as the estimate it is.
    let what_if = plan.optics_what_if.as_ref().unwrap();
    assert_eq!(what_if.assumed_cnr_db, 48.0);
    close(what_if.mean_gain_db, 0.95, 0.15);
    assert!(what_if.label.contains("What-if"));
}

#[test]
fn a_part_that_cannot_do_the_band_is_flagged_with_its_replacement() {
    let mut s = setup();
    let mut old = s
        .library
        .get(&ComponentId::new("tap-generic-23db-4p"))
        .unwrap()
        .clone();
    old.id = ComponentId::new("tap-old-1ghz-23db");
    old.rated_hi_mhz = Some(1002.0);
    s.library.components.push(old);
    for element in &mut s.plant.elements {
        if element.id.as_str() == "tap-1" {
            element.component_ref = ComponentId::new("tap-old-1ghz-23db");
        }
    }
    let plan = plan_migration(&s.plant, &s.library, &s.low, &s.high).unwrap();
    let change = plan
        .parts_must_change
        .iter()
        .find(|c| c.from.as_str() == "tap-old-1ghz-23db")
        .unwrap();
    assert_eq!(
        change.to.as_ref().map(|c| c.as_str()),
        Some("tap-generic-23db-4p")
    );
    assert_eq!(change.count, 1);
    assert!(change.reason.contains("1002"));
    let tap = plan
        .converted_plant
        .elements
        .iter()
        .find(|e| e.id.as_str() == "tap-1")
        .unwrap();
    assert_eq!(tap.component_ref.as_str(), "tap-generic-23db-4p");
    assert_eq!(tap.record_status, RecordStatus::Designed);
    // The plant it started from is untouched: this is a plan.
    assert!(
        s.plant
            .elements
            .iter()
            .any(|e| e.component_ref.as_str() == "tap-old-1ghz-23db")
    );
}

#[test]
fn a_designed_node_plans_and_repeats() {
    let s = setup();
    let map = generate(LayoutFamily::Suburban, 4, 200);
    let design = design_region(
        &map,
        &s.low,
        &s.library,
        &DesignControls::default(),
        &DesignParts::default(),
    )
    .unwrap();
    for node in &design.nodes {
        let a = plan_migration(&node.plant, &s.library, &s.low, &s.high).unwrap();
        let b = plan_migration(&node.plant, &s.library, &s.low, &s.high).unwrap();
        assert_eq!(serde_json_bytes(&a), serde_json_bytes(&b));
        assert_eq!(a.issues_before, Vec::<String>::new());
        assert!(!a.modems.is_empty());
    }
}

fn serde_json_bytes(plan: &design::migrate::MigrationPlan) -> Vec<u8> {
    serde_json::to_vec(plan).unwrap()
}

/// §16: the gain from leaving analog optics depends on the link [TR-DCA §6.2.3]. The same node, its link at two lengths: the longer
/// link's lower CNR gives the larger gain. The links are the as-built fixture's, read against its node's fitted optics.
#[test]
fn migration_gain_depends_on_link_length() {
    let s = setup();
    let as_built: Plant = read("one_street_as_built/plant_lowsplit_iccap.json");
    let fiber: schema::fiber::FiberPlant = read("one_street_as_built/fiber.json");
    let link_at = |more_ft: f64| {
        let mut longer = fiber.clone();
        for cable in &mut longer.cables {
            if cable.id.as_str() == "fiber-2" {
                cable.len_ft += more_ft;
            }
        }
        sim::optics::node_link(&longer, &as_built, &s.library, &s.low.channel_plan)
            .unwrap()
            .unwrap()
    };
    let near = link_at(0.0);
    // 40,000 ft more: 4.88 dB less light, still inside the receiver's −8 dBm floor.
    let far = link_at(40_000.0);
    let gain = |link: Option<&sim::optics::NodeLink>| {
        design::migrate::plan_migration_with_link(&s.plant, &s.library, &s.low, &s.high, link)
            .unwrap()
            .optics_what_if
            .unwrap()
    };
    let constant = gain(None);
    assert_eq!(constant.assumed_cnr_db, 48.0);
    let (near_gain, far_gain) = (gain(Some(&near)), gain(Some(&far)));
    close(near_gain.assumed_cnr_db, 50.751, 0.001);
    assert!(far_gain.assumed_cnr_db < near_gain.assumed_cnr_db - 2.0);
    assert!(far_gain.mean_gain_db > near_gain.mean_gain_db);
    assert!(near_gain.mean_gain_db < constant.mean_gain_db);
    assert!(near_gain.label.contains("forward link"));
    assert_eq!(
        plan_migration(&s.plant, &s.library, &s.low, &s.high).unwrap(),
        design::migrate::plan_migration_with_link(&s.plant, &s.library, &s.low, &s.high, None)
            .unwrap()
    );
}
