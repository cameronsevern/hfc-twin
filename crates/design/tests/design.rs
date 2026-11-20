//! Design on generated geography. All synthetic (invariant 11).

use std::collections::BTreeSet;
use std::fs;

use design::layout::generate;
use design::optimize::{mark_all_built, record_counts};
use design::osm::from_overpass;
use design::{DesignParts, design_region};
use diagnosis::{DiagnoserConfig, diagnose};
use features::DesignReference;
use features::sampler::sample_fault;
use features::sweep::observe;
use plant_graph::PlantGraph;
use schema::common::{Origin, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::FaultSet;
use schema::observation::RegState;
use schema::plant::{EdgeKind, ElementKind, Installation, RecordStatus};
use schema::profile::Profile;
use schema::region::{DesignControls, LayoutFamily, MapSource, PinnedNode};
use schema::work_order::CauseClass;
use sim::rng::Rng;
use sim::telemetry::{Scenario, SimOptions, simulate};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn library() -> ComponentLibrary {
    read("library/generic.json")
}

fn profiles() -> [Profile; 2] {
    [
        read("profiles/d31_lowsplit_1002_iccap.json"),
        read("profiles/d31_highsplit_1218_rpd.json"),
    ]
}

const FAMILIES: [LayoutFamily; 3] = [
    LayoutFamily::UrbanGrid,
    LayoutFamily::Suburban,
    LayoutFamily::Rural,
];

fn calm() -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(0),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

#[test]
fn every_family_comes_out_valid() {
    let library = library();
    let controls = DesignControls::default();
    for profile in profiles() {
        for family in FAMILIES {
            for seed in 1..=4 {
                let map = generate(family, seed, 500);
                assert!(matches!(map.source, MapSource::Generated { .. }));
                let design =
                    design_region(&map, &profile, &library, &controls, &DesignParts::default())
                        .unwrap();
                assert!(design.unserved.is_empty());
                assert_eq!(design.region.origin, Origin::Synthetic);
                let mut passed = 0;
                for node in &design.nodes {
                    let label = format!(
                        "{} {family:?} {seed} {}",
                        profile.id, node.plant.service_group
                    );
                    assert_eq!(node.topology_issues, Vec::<String>::new(), "{label}");
                    assert_eq!(node.physics_issues, Vec::<String>::new(), "{label}");
                    assert!(node.homes_passed <= controls.homes_per_node, "{label}");
                    assert!(
                        node.max_cascade <= u32::from(profile.max_cascade),
                        "{label}"
                    );
                    assert_eq!(node.plant.origin, Origin::Synthetic);
                    assert!(
                        node.plant
                            .elements
                            .iter()
                            .all(|e| e.record_status == RecordStatus::Designed)
                    );
                    for edge in &node.plant.edges {
                        if let EdgeKind::Drop { len_ft, .. } = edge.kind {
                            assert!(len_ft <= controls.max_drop_ft, "{label}: {len_ft}");
                        }
                    }
                    passed += node.homes_passed as usize;
                }
                assert_eq!(passed, map.homes.len());
            }
        }
    }
}

#[test]
fn design_is_deterministic() {
    let library = library();
    let [profile, _] = profiles();
    let map = generate(LayoutFamily::Suburban, 5, 400);
    let go = || {
        serde_json::to_vec(
            &design_region(
                &map,
                &profile,
                &library,
                &DesignControls::default(),
                &DesignParts::default(),
            )
            .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(go(), go());
    assert_eq!(
        generate(LayoutFamily::Rural, 3, 100),
        generate(LayoutFamily::Rural, 3, 100)
    );
    assert_ne!(
        generate(LayoutFamily::Rural, 3, 100),
        generate(LayoutFamily::Rural, 4, 100)
    );
}

#[test]
fn the_parts_list_adds_up_to_the_plant() {
    let library = library();
    let [profile, _] = profiles();
    let map = generate(LayoutFamily::UrbanGrid, 2, 200);
    let design = design_region(
        &map,
        &profile,
        &library,
        &DesignControls::default(),
        &DesignParts::default(),
    )
    .unwrap();
    for node in &design.nodes {
        let plant = &node.plant;
        let parts = plant.parts_list();
        let quantity = |prefix: &str| -> u32 {
            parts
                .iter()
                .filter(|p| p.component_ref.as_str().starts_with(prefix))
                .map(|p| p.quantity)
                .sum()
        };
        let count = |f: &dyn Fn(&ElementKind) -> bool| {
            plant.elements.iter().filter(|e| f(&e.kind)).count() as u32
        };
        assert_eq!(quantity("tap-"), count(&|k| matches!(k, ElementKind::Tap)));
        assert_eq!(
            quantity("modem-"),
            count(&|k| matches!(k, ElementKind::Modem))
        );
        assert_eq!(
            quantity("amp-"),
            count(&|k| matches!(k, ElementKind::Amp { .. }))
        );
        let hardline: f64 = parts
            .iter()
            .filter(|p| p.component_ref.as_str() == "cable-500-p3")
            .filter_map(|p| p.total_len_ft)
            .sum();
        assert!((hardline - node.hardline_ft).abs() < 1e-6);
    }
}

#[test]
fn controls_change_the_design() {
    let library = library();
    let [profile, _] = profiles();
    let map = generate(LayoutFamily::Suburban, 7, 600);
    let parts = DesignParts::default();
    let base = design_region(&map, &profile, &library, &DesignControls::default(), &parts).unwrap();

    // A tighter homes target makes more nodes.
    let smaller = DesignControls {
        homes_per_node: 100,
        ..DesignControls::default()
    };
    let more = design_region(&map, &profile, &library, &smaller, &parts).unwrap();
    assert!(more.nodes.len() > base.nodes.len());
    assert!(more.nodes.iter().all(|n| n.homes_passed <= 100));

    // A tighter cascade limit splits nodes further down, and holds.
    let shallow = DesignControls {
        cascade_limit: Some(1),
        ..DesignControls::default()
    };
    let short = design_region(&map, &profile, &library, &shallow, &parts).unwrap();
    assert!(short.nodes.iter().all(|n| n.max_cascade <= 1));
    assert!(short.nodes.len() > base.nodes.len());

    // A homes target above the profile's limit doesn't override it.
    let too_many = DesignControls {
        homes_per_node: 100_000,
        ..DesignControls::default()
    };
    let capped = design_region(&map, &profile, &library, &too_many, &parts).unwrap();
    assert!(
        capped
            .nodes
            .iter()
            .all(|n| n.homes_passed <= profile.max_homes_passed)
    );
}

#[test]
fn a_pinned_node_stays_put() {
    let library = library();
    let [profile, _] = profiles();
    let map = generate(LayoutFamily::UrbanGrid, 3, 500);
    let pin = map.homes[10].geo;
    let controls = DesignControls {
        pinned_nodes: vec![PinnedNode { geo: pin }],
        ..DesignControls::default()
    };
    let design =
        design_region(&map, &profile, &library, &controls, &DesignParts::default()).unwrap();
    let root = design.nodes[0].plant.root().unwrap();
    let geo = root.geo.unwrap();
    assert!(design::geo::dist_ft(geo, pin) < 150.0);
}

#[test]
fn the_sim_runs_on_a_designed_plant() {
    let library = library();
    for profile in profiles() {
        for family in FAMILIES {
            let map = generate(family, 11, 300);
            let design = design_region(
                &map,
                &profile,
                &library,
                &DesignControls::default(),
                &DesignParts::default(),
            )
            .unwrap();
            for node in design.nodes.iter().take(3) {
                let graph = PlantGraph::new(&node.plant, &library, &profile).unwrap();
                let environment = Environment {
                    schema_version: "1".into(),
                    samples: vec![calm()],
                    ambient_sources: Vec::new(),
                };
                let faults = FaultSet {
                    schema_version: "1".into(),
                    seed: 1,
                    faults: vec![],
                };
                let steps = simulate(&Scenario {
                    graph: &graph,
                    environment: &environment,
                    faults: &faults,
                    seed: 1,
                    options: SimOptions {
                        detail: sim::telemetry::Detail::Summary { stride: 16 },
                        randomize: None,
                        measurement_noise_db: 0.0,
                        rxmer_scatter_db: 0.0,
                    },
                    link: None,
                })
                .unwrap();
                assert!(!steps[0].modems.is_empty());
                for modem in &steps[0].modems {
                    assert_eq!(
                        modem.reg_state,
                        RegState::Operational,
                        "{}",
                        modem.element_id
                    );
                }
            }
        }
    }
}

#[test]
fn the_diagnoser_works_on_a_designed_plant() {
    let library = library();
    let [_, profile] = profiles();
    let map = generate(LayoutFamily::Suburban, 2, 120);
    let design = design_region(
        &map,
        &profile,
        &library,
        &DesignControls::default(),
        &DesignParts::default(),
    )
    .unwrap();
    let node = &design.nodes[0];
    let graph = PlantGraph::new(&node.plant, &library, &profile).unwrap();
    let reference = DesignReference::compute(&graph).unwrap();
    let mut rng = Rng::new(9);
    let fault = sample_fault(&graph, CauseClass::BadDrop, &mut rng).unwrap();
    let truth = diagnosis::engine::fault_component(&fault);
    let faults = FaultSet {
        schema_version: "1".into(),
        seed: 9,
        faults: vec![fault],
    };
    let observed = observe(&graph, &reference, &faults, calm(), 9).unwrap();
    let answer = diagnose(
        &graph,
        &reference,
        &observed,
        &calm(),
        &DiagnoserConfig::default(),
    )
    .unwrap();
    assert_eq!(answer.hypotheses[0].class, Some(CauseClass::BadDrop));
    assert!(answer.hypotheses[0].components.contains(&truth));
}

#[test]
fn records_move_from_designed_to_built() {
    let library = library();
    let [profile, _] = profiles();
    let map = generate(LayoutFamily::UrbanGrid, 1, 60);
    let mut design = design_region(
        &map,
        &profile,
        &library,
        &DesignControls::default(),
        &DesignParts::default(),
    )
    .unwrap();
    let plant = &mut design.nodes[0].plant;
    assert!(record_counts(plant).contains_key("Designed"));
    mark_all_built(plant);
    let counts = record_counts(plant);
    assert!(!counts.contains_key("Designed"));
    assert!(counts.contains_key("Built"));
    assert!(
        plant
            .elements
            .iter()
            .all(|e| e.record_status == RecordStatus::Built)
    );
}

#[test]
fn osm_reads_streets_and_homes_and_skips_the_rest() {
    let text = fs::read_to_string(format!(
        "{}/../../fixtures/osm/small_extract.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let map = from_overpass("osm-small", &text, Installation::Aerial).unwrap();
    let streets: BTreeSet<&str> = map.streets.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(streets, BTreeSet::from(["way-100", "way-101"]));
    let homes: BTreeSet<&str> = map.homes.iter().map(|h| h.id.as_str()).collect();
    assert_eq!(homes, BTreeSet::from(["address-5", "building-104"]));
    assert!(matches!(map.source, MapSource::OpenStreetMap { .. }));
    assert_eq!(map.streets[0].points.len(), 3);

    assert!(from_overpass("x", "not json", Installation::Aerial).is_err());
    assert!(from_overpass("x", "{}", Installation::Aerial).is_err());
    assert!(from_overpass("x", "{\"elements\": []}", Installation::Aerial).is_err());

    // A design on it still comes out valid, and it's still a synthetic plant on real geography.
    let library = library();
    let [profile, _] = profiles();
    let design = design_region(
        &map,
        &profile,
        &library,
        &DesignControls::default(),
        &DesignParts::default(),
    )
    .unwrap();
    assert_eq!(design.region.origin, Origin::Synthetic);
    assert_eq!(design.nodes[0].plant.origin, Origin::Synthetic);
}

/// A node with no amps has no amp noise to funnel, and its upstream SNR has to stay a number.
#[test]
fn an_n_plus_0_node_has_finite_upstream() {
    let library = library();
    for profile in profiles() {
        let map = generate(LayoutFamily::UrbanGrid, 1, 100);
        let controls = DesignControls {
            homes_per_node: 12,
            ..DesignControls::default()
        };
        let design =
            design_region(&map, &profile, &library, &controls, &DesignParts::default()).unwrap();
        let node = design.nodes.iter().find(|n| n.amps == 0).unwrap();
        let graph = PlantGraph::new(&node.plant, &library, &profile).unwrap();
        let environment = Environment {
            schema_version: "1".into(),
            samples: vec![calm()],
            ambient_sources: Vec::new(),
        };
        let faults = FaultSet {
            schema_version: "2".into(),
            seed: 1,
            faults: vec![],
        };
        let steps = simulate(&Scenario {
            graph: &graph,
            environment: &environment,
            faults: &faults,
            seed: 1,
            options: SimOptions {
                detail: sim::telemetry::Detail::Summary { stride: 16 },
                randomize: None,
                measurement_noise_db: 0.0,
                rxmer_scatter_db: 0.0,
            },
            link: None,
        })
        .unwrap();
        assert!(!steps[0].service_group.us_snr.is_empty());
        for record in &steps[0].service_group.us_snr {
            assert!(
                record.us_snr_db.is_finite() && record.us_snr_db < 90.0,
                "{}",
                record.us_snr_db
            );
        }
        // Round trip: JSON has no infinity, so one would come back as a refusal.
        let text = serde_json::to_string(&steps).unwrap();
        let _: Vec<sim::telemetry::StepObservation> = serde_json::from_str(&text).unwrap();
    }
}

/// At full take every served home has a modem, so a home can't fall out between the partition and the taps.
#[test]
fn every_served_home_gets_a_modem_at_full_take() {
    let library = library();
    let mut total = (0u32, 0u32);
    for family in [
        LayoutFamily::UrbanGrid,
        LayoutFamily::Suburban,
        LayoutFamily::Rural,
    ] {
        for profile in profiles() {
            let map = generate(family, 3, 800);
            let controls = DesignControls {
                take_rate: 1.0,
                homes_per_node: 120,
                ..DesignControls::default()
            };
            let design =
                design_region(&map, &profile, &library, &controls, &DesignParts::default())
                    .unwrap();
            for node in &design.nodes {
                let modems = node
                    .plant
                    .elements
                    .iter()
                    .filter(|e| matches!(e.kind, ElementKind::Modem))
                    .count() as u32;
                println!(
                    "{family:?} {} {} modems of {} homes",
                    profile.id, modems, node.homes_passed
                );
                total.0 += modems;
                total.1 += node.homes_passed;
            }
        }
    }
    assert_eq!(total.0, total.1);
}
