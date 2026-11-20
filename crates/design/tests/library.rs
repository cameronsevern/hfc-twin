//! The sourced library: every claim of a data sheet has a link, the fits match the sheets, and a design runs on the real parts.

use std::fs;

use design::{DesignParts, design_region, generate};
use plant_graph::PlantGraph;
use schema::common::{ComponentId, Origin};
use schema::component::{ComponentLibrary, ComponentSpec, SpecConfidence, qualifies};
use schema::profile::Profile;
use schema::region::{DesignControls, LayoutFamily};
use sim::cable::atten_db_per_100ft;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

#[test]
fn invariant_9_every_datasheet_part_cites_its_sheet() {
    let library: ComponentLibrary = read("library/sourced.json");
    library.validate().unwrap();
    let mut sheets = 0;
    for part in &library.components {
        if part.spec_confidence == SpecConfidence::Datasheet {
            sheets += 1;
            assert!(
                part.source_url
                    .as_deref()
                    .is_some_and(|u| u.starts_with("https://")),
                "{}",
                part.id
            );
        }
    }
    assert_eq!(sheets, 24);
    // And the plain generic library claims nothing it can't back.
    let generic: ComponentLibrary = read("library/generic.json");
    assert!(
        generic
            .components
            .iter()
            .all(|c| c.spec_confidence == SpecConfidence::Generic)
    );
}

#[test]
fn the_p3_625_fit_matches_its_sheet() {
    let library: ComponentLibrary = read("library/sourced.json");
    let part = library
        .get(&ComponentId::new("cable-commscope-p3-625"))
        .unwrap();
    let ComponentSpec::Cable(spec) = &part.spec else {
        panic!()
    };
    // The sheet's dB/100 ft, 5–1218 MHz.
    for (f, sheet) in [
        (5.0, 0.13),
        (55.0, 0.45),
        (204.0, 0.89),
        (550.0, 1.5),
        (750.0, 1.78),
        (1002.0, 2.07),
        (1218.0, 2.33),
    ] {
        let fit = atten_db_per_100ft(spec, f);
        assert!((fit - sheet).abs() < 0.03, "{f} MHz: {fit} against {sheet}");
    }
    assert_eq!(spec.v_f, 0.87);
    assert_eq!(spec.r_loop_ohm_per_kft, 1.1);
}

#[test]
fn the_ble120_and_xm3_read_as_their_sheets_say() {
    let library: ComponentLibrary = read("library/sourced.json");
    let amp = library
        .get(&ComponentId::new("amp-commscope-ble120"))
        .unwrap();
    let ComponentSpec::Amp(a) = &amp.spec else {
        panic!()
    };
    assert_eq!(
        (a.max_gain_db, a.nf_db, a.return_nf_db, a.return_loss_db),
        (38.0, 9.0, 6.0, 16.0)
    );
    assert_eq!((a.power_w, a.v_min, a.return_gain_db), (24.4, 38.0, 27.0));
    for profile in [
        "profiles/d31_lowsplit_1002_iccap.json",
        "profiles/d31_highsplit_1218_rpd.json",
    ] {
        let profile: Profile = read(profile);
        assert!(qualifies(amp, &profile), "{}", profile.id);
    }
    let psu = library
        .get(&ComponentId::new("psu-alpha-xm3-915hp"))
        .unwrap();
    let ComponentSpec::PowerSupply(p) = &psu.spec else {
        panic!()
    };
    assert_eq!((p.v_out, p.i_rated_a), (89.0, 15.0));
}

#[test]
fn a_design_runs_on_the_real_parts() {
    let library: ComponentLibrary = read("library/sourced.json");
    let parts = DesignParts {
        cable: ComponentId::new("cable-commscope-p3-625"),
        amp: ComponentId::new("amp-commscope-ble120"),
        supply: ComponentId::new("psu-alpha-xm3-915hp"),
        ..DesignParts::default()
    };
    for profile in [
        "profiles/d31_lowsplit_1002_iccap.json",
        "profiles/d31_highsplit_1218_rpd.json",
    ] {
        let profile: Profile = read(profile);
        for family in [
            LayoutFamily::UrbanGrid,
            LayoutFamily::Suburban,
            LayoutFamily::Rural,
        ] {
            let map = generate(family, 3, 400);
            let design =
                design_region(&map, &profile, &library, &DesignControls::default(), &parts)
                    .unwrap();
            for node in &design.nodes {
                let label = format!("{} {family:?} {}", profile.id, node.plant.service_group);
                assert_eq!(node.topology_issues, Vec::<String>::new(), "{label}");
                assert_eq!(node.physics_issues, Vec::<String>::new(), "{label}");
                assert_eq!(node.plant.origin, Origin::Synthetic);
                let graph = PlantGraph::new(&node.plant, &library, &profile).unwrap();
                // The indoor tap isn't in an outdoor plant: it doesn't pass power.
                assert!(
                    graph
                        .plant()
                        .elements
                        .iter()
                        .all(|e| e.component_ref.as_str() != "tap-teleste-3poc427")
                );
                assert!(graph.plant().edges.iter().any(|e| matches!(&e.kind, schema::plant::EdgeKind::Span { cable_ref, .. } if cable_ref.as_str() == "cable-commscope-p3-625")));
            }
        }
    }
}
