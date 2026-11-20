//! §8: randomization draws within the tolerance a part states, each draw on its own stream by element, so a part that states
//! none draws nothing and existing plants don't move (`docs/tasks/22-as-built-physics.md`). All synthetic.

use std::fs;

use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId, Timestamp};
use schema::component::{ComponentLibrary, Tolerance};
use schema::environment::EnvironmentSample;
use schema::fault::FaultSet;
use schema::plant::Plant;
use schema::profile::Profile;
use sim::Overlay;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn none() -> Tolerance {
    Tolerance {
        loss_db: None,
        gain_db: None,
        r_loop_pct: None,
        return_loss_db: None,
        nf_db: None,
        impl_mer_db: None,
    }
}

fn state(library: &mut ComponentLibrary, id: &str, tolerance: Tolerance) {
    for part in &mut library.components {
        if part.id.as_str() == id {
            part.tolerance = Some(tolerance.clone());
        }
    }
}

fn overlay(plant: &Plant, library: &ComponentLibrary, profile: &Profile, seed: u64) -> Overlay {
    let graph = PlantGraph::new(plant, library, profile).unwrap();
    let sample = EnvironmentSample {
        t: Timestamp(0),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        wind_m_per_s: None,
        solar_w_per_m2: None,
    };
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.into(),
        seed: 1,
        faults: Vec::new(),
    };
    let mut overlay = Overlay::build(&graph, &faults, &sample).unwrap();
    overlay.randomize(&graph, seed);
    overlay
}

fn as_built() -> (Plant, Profile) {
    (
        read("one_street_as_built/plant_lowsplit_iccap.json"),
        read("profiles/d31_lowsplit_1002_iccap.json"),
    )
}

#[test]
fn an_amps_stated_gain_tolerance_scales_its_draw() {
    let plant: Plant = read("one_street/plant_lowsplit_iccap.json");
    let profile: Profile = read("profiles/d31_lowsplit_1002_iccap.json");
    let mut library: ComponentLibrary = read("library/generic.json");
    let plain = overlay(&plant, &library, &profile, 9);
    state(
        &mut library,
        "amp-generic-1218",
        Tolerance {
            gain_db: Some(0.25),
            ..none()
        },
    );
    let stated = overlay(&plant, &library, &profile, 9);
    for (id, amp) in &plain.amps {
        // The same stream, scaled from ±1 to ±0.25.
        let within = stated.amps[id].gain_delta_db;
        assert!((within - 0.25 * amp.gain_delta_db).abs() < 1e-12, "{id}");
        assert!(within.abs() <= 0.25);
    }
    // Nothing else moved.
    assert_eq!(stated.edge_loss_factor, plain.edge_loss_factor);
    assert_eq!(stated.element_offset_db, plain.element_offset_db);
    assert!(stated.modem_draws.is_empty() && stated.plugin_draws.is_empty());
}

#[test]
fn modems_plug_ins_and_fittings_draw_within_their_tolerance() {
    let (plant, profile) = as_built();
    let mut library: ComponentLibrary = read("library/generic.json");
    let plain = overlay(&plant, &library, &profile, 9);
    assert!(plain.modem_draws.is_empty());
    assert!(plain.plugin_draws.is_empty());
    assert!(plain.fitting_rl_draws.is_empty());

    state(
        &mut library,
        "modem-generic-d31",
        Tolerance {
            nf_db: Some(1.5),
            impl_mer_db: Some(1.0),
            ..none()
        },
    );
    state(
        &mut library,
        "pad-generic-10db",
        Tolerance {
            loss_db: Some(0.5),
            ..none()
        },
    );
    state(
        &mut library,
        "conn-generic-f",
        Tolerance {
            return_loss_db: Some(3.0),
            ..none()
        },
    );
    let drawn = overlay(&plant, &library, &profile, 9);
    // Only D3.1 modems state one.
    assert!(!drawn.modem_draws.is_empty());
    for (id, (nf, ceiling)) in &drawn.modem_draws {
        assert!(nf.abs() <= 1.5 && ceiling.abs() <= 1.0, "{id}");
        let element = plant.elements.iter().find(|e| &e.id == id).unwrap();
        assert_eq!(element.component_ref.as_str(), "modem-generic-d31");
    }
    // `amp-1` and `amp-2`'s input pads are 10 dB on this profile (§15).
    let draws = &drawn.plugin_draws[&ElementId::new("amp-1")];
    assert!(draws.input_pad_db != 0.0 && draws.input_pad_db.abs() <= 0.5);
    assert_eq!(draws.input_eq_db, 0.0);
    // Every F connector on a drop or house cable, each its own draw.
    assert!(drawn.fitting_rl_draws.len() >= 26);
    assert!(drawn.fitting_rl_draws.values().all(|d| d.abs() <= 3.0));
    let distinct: std::collections::BTreeSet<u64> = drawn
        .fitting_rl_draws
        .values()
        .map(|d| d.to_bits())
        .collect();
    assert_eq!(distinct.len(), drawn.fitting_rl_draws.len());
    // Version 1's draws didn't move.
    assert_eq!(drawn.amps.len(), plain.amps.len());
    for (id, amp) in &plain.amps {
        assert_eq!(drawn.amps[id].gain_delta_db, amp.gain_delta_db);
    }
    assert_eq!(drawn.edge_loss_factor, plain.edge_loss_factor);
    // Deterministic, and another seed draws differently.
    assert_eq!(overlay(&plant, &library, &profile, 9), drawn);
    assert_ne!(
        overlay(&plant, &library, &profile, 10).fitting_rl_draws,
        drawn.fitting_rl_draws
    );
    let _ = EdgeId::new("unused");
}

#[test]
fn a_splice_and_a_fiber_connector_draw_within_theirs() {
    let (plant, profile) = as_built();
    let fiber: schema::fiber::FiberPlant = read("one_street_as_built/fiber.json");
    let mut library: ComponentLibrary = read("library/generic.json");
    let plan = &profile.channel_plan;
    let at = |library: &ComponentLibrary, seed: Option<u64>| {
        sim::optics::node_link_drawn(&fiber, &plant, library, plan, &[], Timestamp(0), seed)
            .unwrap()
            .unwrap()
    };
    let nominal = at(&library, None);
    // A seed with nothing stated draws nothing.
    assert_eq!(at(&library, Some(3)), nominal);
    state(
        &mut library,
        "closure-generic",
        Tolerance {
            loss_db: Some(0.03),
            ..none()
        },
    );
    state(
        &mut library,
        "fconn-generic-sc-apc",
        Tolerance {
            loss_db: Some(0.2),
            ..none()
        },
    );
    assert_eq!(at(&library, None), nominal);
    let drawn = at(&library, Some(3));
    let loss = |l: &sim::optics::NodeLink| l.forward.as_ref().unwrap().loss_db.unwrap();
    // Five connector pairs at ±0.2 and a splice at ±0.03.
    let moved = loss(&drawn) - loss(&nominal);
    assert!(moved != 0.0 && moved.abs() <= 5.0 * 0.2 + 0.03, "{moved}");
    assert_eq!(at(&library, Some(3)), drawn);
}
