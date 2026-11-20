//! Tests against the shipped files under `fixtures/`.

use std::fs;
use std::path::{Path, PathBuf};

use schema::capability::effective;
use schema::common::{ComponentId, EdgeId, ElementId, Origin, PortId};
use schema::component::{
    self, ComponentLibrary, ComponentSpec, LibraryError, ModemSpec, SpecConfidence, qualifies,
};
use schema::environment::{self, Environment};
use schema::fault::{self, FaultKind, FaultSet, Location};
use schema::plant::{self, EdgeKind, ElementKind, Plant, PlantError, RecordStatus};
use schema::profile::{self, NodeKind, Profile, ProfileError};
use schema::work_order::{self, CauseClass, PolicyTable};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

const LOW_PROFILE: &str = "profiles/d31_lowsplit_1002_iccap.json";
const HIGH_PROFILE: &str = "profiles/d31_highsplit_1218_rpd.json";
const LOW_PLANT: &str = "one_street/plant_lowsplit_iccap.json";
const HIGH_PLANT: &str = "one_street/plant_highsplit_rpd.json";

fn fixture_path(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(file)
}

/// Loads a fixture and checks nothing in the file is lost or invented on the way through
/// the type.
fn load<T: DeserializeOwned + Serialize>(file: &str) -> T {
    let text = fs::read_to_string(fixture_path(file)).unwrap();
    let value: T = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{file}: {e}"));
    let raw: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        serde_json::to_value(&value).unwrap(),
        raw,
        "{file} doesn't round-trip"
    );
    value
}

fn library() -> ComponentLibrary {
    load("library/generic.json")
}
fn low() -> (Plant, Profile) {
    (load(LOW_PLANT), load(LOW_PROFILE))
}
fn high() -> (Plant, Profile) {
    (load(HIGH_PLANT), load(HIGH_PROFILE))
}
fn element_id(id: &str) -> ElementId {
    ElementId::new(id)
}

fn modem_spec<'a>(library: &'a ComponentLibrary, id: &str) -> &'a ModemSpec {
    match &library.get(&ComponentId::new(id)).unwrap().spec {
        ComponentSpec::Modem(spec) => spec,
        other => panic!("{id} is {other:?}"),
    }
}

/// `one_street` stays a version 1 file (`docs/as-built-plan.md`, rule 4), so its plants come forward by migration,
/// unchanged apart from the version. Everything else is on the current version.
#[test]
fn every_file_is_on_the_current_schema_version() {
    assert_eq!(library().schema_version, component::SCHEMA_VERSION);
    for (plant, profile) in [low(), high()] {
        assert_eq!(plant.schema_version, "1");
        let migrated = plant::migrate(plant.clone()).unwrap();
        assert_eq!(migrated.schema_version, plant::SCHEMA_VERSION);
        assert_eq!(
            Plant {
                schema_version: "1".into(),
                ..migrated
            },
            plant
        );
        // The shipped profiles are version 1 files, read through migration since task 22.
        assert_eq!(profile.schema_version, "1");
        assert_eq!(
            profile::migrate(profile.clone()).unwrap().schema_version,
            profile::SCHEMA_VERSION
        );
    }
    // `one_street`'s faults and environment are version 3 and version 1 files, read through migration since task 22.
    let faults: FaultSet = load("one_street/faults.json");
    assert_eq!(faults.schema_version, "3");
    assert!(faults.faults.is_empty());
    let examples: FaultSet = load("one_street/fault_examples.json");
    assert_eq!(examples.schema_version, "3");
    assert_eq!(
        fault::migrate(examples).unwrap().schema_version,
        fault::SCHEMA_VERSION
    );
    let environment: Environment = load("one_street/environment.json");
    assert_eq!(environment.schema_version, "1");
    assert_eq!(
        environment::migrate(environment).unwrap().schema_version,
        environment::SCHEMA_VERSION
    );
    // The shipped policy is a version 5 file: version 5 added causes for the as-built plant (task 24).
    let policy: PolicyTable = load("policy/default.json");
    assert_eq!(policy.schema_version, "5");
    let migrated = work_order::migrate_policy(policy).unwrap();
    assert_eq!(migrated.schema_version, work_order::SCHEMA_VERSION);
}

#[test]
fn library_is_generic_and_complete() {
    let library = library();
    library.validate().unwrap();
    assert!(library.components.len() >= 14);
    for component in &library.components {
        assert_eq!(
            component.spec_confidence,
            SpecConfidence::Generic,
            "{}",
            component.id
        );
    }
    assert!(
        library
            .components
            .iter()
            .any(|c| matches!(c.spec, ComponentSpec::Rpd(_)))
    );
}

#[test]
fn library_validate_rejects_bad_entries() {
    let mut bad = library();
    bad.components.push(bad.components[0].clone());
    bad.components[1].rated_hi_mhz = None;
    let errors = bad.validate().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, LibraryError::DuplicateId(_))),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, LibraryError::BadRatedBand(_))),
        "{errors:?}"
    );
}

#[test]
fn shipped_profiles_are_valid() {
    let (_, low) = low();
    let (_, high) = high();
    low.validate().unwrap();
    high.validate().unwrap();
    assert_eq!(low.node_kind, NodeKind::Analog);
    assert_eq!(high.node_kind, NodeKind::Rpd);
    assert_eq!(low.split_edges(), Some((42.0, 54.0)));
    assert_eq!(high.split_edges(), Some((204.0, 258.0)));
    assert_eq!(low.channel_plan.ds_sc_qam.len(), 32);
    assert_eq!(high.channel_plan.ds_sc_qam.len(), 32);
    assert_eq!(low.channel_plan.ofdm.len(), 1);
    assert_eq!(high.channel_plan.ofdm.len(), 2);
    assert_eq!(low.channel_plan.us_sc_qam.len(), 4);
    assert_eq!(high.channel_plan.us_sc_qam.len(), 4);
    assert_eq!(low.channel_plan.ofdma.len(), 0);
    assert_eq!(high.channel_plan.ofdma.len(), 1);
}

#[test]
fn both_fixture_plants_validate() {
    let library = library();
    let (plant, profile) = low();
    plant.validate(&library, &profile).unwrap();
    let (plant, profile) = high();
    plant.validate(&library, &profile).unwrap();
}

#[test]
fn fixture_plants_differ_only_at_the_root() {
    let (low, _) = low();
    let (high, _) = high();
    assert_eq!(low.elements[1..], high.elements[1..]);
    assert_eq!(low.edges[1..], high.edges[1..]);
    assert_eq!(low.edges[0].kind, high.edges[0].kind);
    assert_eq!(low.edges[0].to, high.edges[0].to);
}

#[test]
fn fixture_is_synthetic_and_built() {
    for (plant, _) in [low(), high()] {
        assert_eq!(plant.origin, Origin::Synthetic);
        for element in &plant.elements {
            assert_eq!(element.record_status, RecordStatus::Built, "{}", element.id);
            assert!(element.geo.is_some(), "{}", element.id);
        }
    }
}

#[test]
fn fixture_mixes_modem_generations_on_every_tap() {
    let library = library();
    let (plant, _) = high();
    let drops: Vec<_> = plant
        .edges
        .iter()
        .filter_map(|edge| match &edge.kind {
            EdgeKind::Drop { len_ft, .. } => {
                Some((&edge.from.element_id, *len_ft, &edge.to.element_id))
            }
            _ => None,
        })
        .collect();
    let lens: Vec<f64> = drops.iter().map(|d| d.1).collect();
    assert_eq!(lens, [80.0, 150.0, 80.0, 120.0, 80.0, 150.0, 80.0, 120.0]);
    for (_, len_ft, modem) in &drops {
        let element = plant.element(modem).unwrap();
        let entry = library.get(&element.component_ref).unwrap();
        let ComponentSpec::Modem(spec) = &entry.spec else {
            panic!("{modem} isn't a modem")
        };
        let expected = if *len_ft == 80.0 {
            component::ModemGeneration::D30
        } else {
            component::ModemGeneration::D31
        };
        assert_eq!(spec.generation, expected, "{modem}");
    }
}

#[test]
fn fixture_amps_stay_in_spec_at_45c() {
    // docs/tasks/01-schemas.md: the 1,200 ft first span at 1218 MHz and 45 °C needs this.
    let library = library();
    let (plant, _) = high();
    for id in ["amp-1", "amp-2"] {
        let element = plant.element(&element_id(id)).unwrap();
        let spec = library
            .get(&element.component_ref)
            .unwrap()
            .spec
            .active()
            .unwrap();
        assert!(spec.min_in_dbmv <= 18.0, "{id}");
        assert!(spec.max_gain_db >= 34.0, "{id}");
    }
}

#[test]
fn power_reaches_both_amps() {
    for (plant, _) in [low(), high()] {
        let supply = &plant.power_supplies[0];
        let mut reached = vec![&supply.feeds];
        let mut i = 0;
        while i < reached.len() {
            for edge in &plant.edges {
                let passes = matches!(
                    edge.kind,
                    EdgeKind::Span {
                        power_passing: true,
                        ..
                    }
                );
                if passes && &edge.from.element_id == reached[i] {
                    reached.push(&edge.to.element_id);
                }
            }
            i += 1;
        }
        assert!(reached.contains(&&element_id("amp-1")));
        assert!(reached.contains(&&element_id("amp-2")));
    }
}

#[test]
fn power_supply_voltage_is_90v() {
    let library = library();
    let (plant, _) = low();
    let entry = library.get(&plant.power_supplies[0].component_ref).unwrap();
    let ComponentSpec::PowerSupply(spec) = &entry.spec else {
        panic!("not a power supply")
    };
    assert_eq!(spec.v_out, 90.0);
}

#[test]
fn validate_rejects_a_profile_mismatch() {
    let library = library();
    let (low_plant, _) = low();
    let (_, high_profile) = high();
    let errors = low_plant.validate(&library, &high_profile).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, PlantError::ProfileMismatch { .. }))
    );
}

#[test]
fn validate_rejects_a_cycle() {
    let library = library();
    let (mut plant, profile) = low();
    let edge = plant
        .edges
        .iter_mut()
        .find(|e| e.id == EdgeId::new("span-4"))
        .unwrap();
    edge.to.element_id = element_id("amp-1");
    let errors = plant.validate(&library, &profile).unwrap_err();
    assert!(
        errors.iter().any(|e| matches!(e, PlantError::Cycle(_))),
        "{errors:?}"
    );
}

#[test]
fn validate_rejects_a_dangling_component_ref() {
    let library = library();
    let (mut plant, profile) = low();
    let missing = ComponentId::new("no-such-part");
    plant.elements[1].component_ref = missing.clone();
    let errors = plant.validate(&library, &profile).unwrap_err();
    assert!(
        errors.iter().any(
            |e| matches!(e, PlantError::UnknownComponent { component, .. } if *component == missing)
        ),
        "{errors:?}"
    );
}

#[test]
fn validate_rejects_a_zero_length_span() {
    let library = library();
    let (mut plant, profile) = low();
    if let EdgeKind::Span { len_ft, .. } = &mut plant.edges[0].kind {
        *len_ft = 0.0;
    }
    let errors = plant.validate(&library, &profile).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, PlantError::NonPositiveLength { .. })),
        "{errors:?}"
    );
}

#[test]
fn validate_rejects_a_root_that_disagrees_with_the_profile() {
    let library = library();
    let (mut plant, profile) = low();
    plant.elements[0].kind = ElementKind::Rpd {
        return_atten_db: 0.0,
    };
    plant.elements[0].component_ref = ComponentId::new("rpd-generic-1218");
    let errors = plant.validate(&library, &profile).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, PlantError::RootKindMismatch { .. })),
        "{errors:?}"
    );
}

#[test]
fn validate_rejects_a_tap_port_used_twice() {
    let library = library();
    let (mut plant, profile) = low();
    let drop_2 = plant
        .edges
        .iter()
        .position(|e| e.id == EdgeId::new("drop-2"))
        .unwrap();
    let port = plant.edges[drop_2 - 1].from.port.clone();
    plant.edges[drop_2].from.port = port;
    let errors = plant.validate(&library, &profile).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, PlantError::PortUsedTwice { .. })),
        "{errors:?}"
    );
}

#[test]
fn validate_rejects_an_edge_pointing_the_wrong_way() {
    let library = library();
    let wrong_way = |edit: fn(&mut Plant)| {
        let (mut plant, profile) = low();
        edit(&mut plant);
        let errors = plant.validate(&library, &profile).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, PlantError::PortDirection { .. })),
            "{errors:?}"
        );
    };
    wrong_way(|plant| plant.edges[2].to.port = PortId::new("through"));
    wrong_way(|plant| plant.edges[3].from.port = PortId::new("in"));
}

#[test]
fn validate_rejects_a_part_of_the_wrong_class() {
    let library = library();
    let (mut plant, profile) = low();
    plant.elements[1].component_ref = ComponentId::new("tap-generic-23db-4p");
    let errors = plant.validate(&library, &profile).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, PlantError::ComponentClassMismatch { .. })),
        "{errors:?}"
    );
}

#[test]
fn parts_list_matches_the_fixture() {
    let (plant, _) = low();
    assert_parts(&plant, "node-generic-1002");
    let (plant, _) = high();
    assert_parts(&plant, "rpd-generic-1218");
}

fn assert_parts(plant: &Plant, root: &str) {
    let parts = plant.parts_list();
    let line = |id: &str| {
        let id = ComponentId::new(id);
        let line = parts.iter().find(|l| l.component_ref == id);
        line.unwrap_or_else(|| panic!("{id} missing")).clone()
    };
    let units = |id: &str, quantity| {
        let line = line(id);
        assert_eq!((line.quantity, line.total_len_ft), (quantity, None), "{id}");
    };
    units(root, 1);
    units("amp-generic-1218", 2);
    units("tap-generic-23db-4p", 2);
    units("tap-generic-17db-4p", 2);
    units("term-generic", 1);
    units("psu-generic-90v", 1);
    units("modem-generic-d30", 4);
    units("modem-generic-d31", 4);
    assert_eq!(line("cable-500-p3").total_len_ft, Some(2700.0));
    assert_eq!(line("drop-rg6").total_len_ft, Some(860.0));
    assert_eq!(parts.len(), 10);
}

#[test]
fn every_generic_part_qualifies_for_a_shipped_profile() {
    let library = library();
    let (_, low) = low();
    let (_, high) = high();
    // Task 23's legacy parts are for the legacy profile a generated estate is first designed under.
    let legacy: Profile = serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/profiles/d30_lowsplit_750_legacy.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for component in &library.components {
        assert!(
            qualifies(component, &low)
                || qualifies(component, &high)
                || qualifies(component, &legacy),
            "{} qualifies for no shipped profile",
            component.id
        );
    }
}

#[test]
fn every_fixture_part_qualifies_for_its_profile() {
    let library = library();
    for (plant, profile) in [low(), high()] {
        for line in plant.parts_list() {
            let component = library.get(&line.component_ref).unwrap();
            assert!(
                qualifies(component, &profile),
                "{} on {}",
                component.id,
                profile.id
            );
        }
    }
}

#[test]
fn qualifies_rejects_a_part_whose_rated_band_falls_short() {
    let library = library();
    let (_, high) = high();
    let node = library.get(&ComponentId::new("node-generic-1002")).unwrap();
    assert!(!qualifies(node, &high));
    let mut cable = library
        .get(&ComponentId::new("cable-500-p3"))
        .unwrap()
        .clone();
    assert!(qualifies(&cable, &high));
    cable.rated_hi_mhz = Some(1002.0);
    assert!(!qualifies(&cable, &high));
}

#[test]
fn qualifies_rejects_an_active_without_the_split() {
    let library = library();
    let (_, low) = low();
    let (_, high) = high();
    // Rated for the band, but only has the high-split diplexer.
    let rpd = library.get(&ComponentId::new("rpd-generic-1218")).unwrap();
    assert!(qualifies(rpd, &high));
    assert!(!qualifies(rpd, &low));
    let amp = library.get(&ComponentId::new("amp-generic-1218")).unwrap();
    assert!(qualifies(amp, &high));
    assert!(qualifies(amp, &low));
}

#[test]
fn a_d30_modem_shares_either_node() {
    let library = library();
    let d30 = library.get(&ComponentId::new("modem-generic-d30")).unwrap();
    assert!(qualifies(d30, &low().1));
    assert!(qualifies(d30, &high().1));
}

#[test]
fn d30_modem_has_no_ofdm_fields_on_either_profile() {
    let library = library();
    let modem = modem_spec(&library, "modem-generic-d30");
    for (_, profile) in [low(), high()] {
        let set = effective(&profile.capability, &modem.capability);
        assert!(!set.has_ofdm_fields(), "{}", profile.id);
        assert!(set.sc_qam_preeq, "{}", profile.id);
        assert!(!set.fbc.unwrap().file, "{}", profile.id);
    }
}

#[test]
fn d31_modem_has_ofdma_fields_only_on_high_split() {
    let library = library();
    let modem = modem_spec(&library, "modem-generic-d31");
    let on_low = effective(&low().1.capability, &modem.capability);
    let on_high = effective(&high().1.capability, &modem.capability);
    assert!(on_low.ds_rxmer_file && on_high.ds_rxmer_file);
    assert!(!on_low.ofdma_preeq_file && on_high.ofdma_preeq_file);
    assert!(!on_low.us_rxmer_summary && on_high.us_rxmer_summary);
}

#[test]
fn effective_fbc_band_is_the_overlap() {
    let library = library();
    let modem = modem_spec(&library, "modem-generic-d30");
    let fbc = effective(&high().1.capability, &modem.capability)
        .fbc
        .unwrap();
    assert_eq!((fbc.lo_mhz, fbc.hi_mhz), (258.0, 1002.0));
}

#[test]
fn fft_and_spacing_that_disagree_fail_validation() {
    let (_, profile) = high();

    let mut bad = profile.clone();
    bad.channel_plan.ofdm[0].subcarrier_spacing_khz = 25.0;
    let errors = bad.validate().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ProfileError::FftSpacingMismatch { .. })),
        "{errors:?}"
    );

    // 4K means 50 kHz downstream but 25 kHz upstream, so the check is per direction.
    let mut bad = profile.clone();
    bad.channel_plan.ofdma[0].fft_size = 4096;
    let errors = bad.validate().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ProfileError::FftSpacingMismatch { .. })),
        "{errors:?}"
    );

    let mut bad = profile.clone();
    bad.channel_plan.ofdm[0].fft_size = 2048;
    let errors = bad.validate().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ProfileError::FftSpacingMismatch { .. })),
        "{errors:?}"
    );
}

#[test]
fn profile_validate_rejects_other_channel_plan_errors() {
    let (_, profile) = high();

    let mut bad = profile.clone();
    bad.channel_plan.ofdm[0].rolloff_us = 2.5;
    let errors = bad.validate().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ProfileError::BadCpRolloff { .. })),
        "{errors:?}"
    );

    let mut bad = profile.clone();
    bad.channel_plan.ofdma[0].first_active_mhz = 90.0;
    let errors = bad.validate().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ProfileError::BadActiveRange(_))),
        "{errors:?}"
    );

    let mut bad = profile.clone();
    bad.channel_plan.ofdm[0].first_active_mhz = 835.01;
    let errors = bad.validate().unwrap_err();
    assert!(
        errors.iter().any(|e| matches!(e, ProfileError::OffGrid(_))),
        "{errors:?}"
    );

    let mut bad = profile.clone();
    bad.channel_plan.ds_sc_qam[0].center_mhz = 100.0;
    let errors = bad.validate().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ProfileError::OutsideBandPlan(_))),
        "{errors:?}"
    );

    let mut bad = profile.clone();
    bad.channel_plan.ofdm[1].channel_id = bad.channel_plan.ofdm[0].channel_id.clone();
    let errors = bad.validate().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ProfileError::DuplicateChannel(_))),
        "{errors:?}"
    );
}

#[test]
fn fault_examples_cover_every_class_at_real_locations() {
    let (plant, _) = low();
    let examples: FaultSet = load("one_street/fault_examples.json");

    let causes: Vec<CauseClass> = examples
        .faults
        .iter()
        .map(|f| CauseClass::from(&f.kind))
        .collect();
    // The coax classes are on either plant; the RPD's are in a file of their own, since only the high-split root is one.
    let rpd_examples: FaultSet = load("one_street/fault_examples_rpd.json");
    assert_eq!(rpd_examples.schema_version, "3");
    let rpd_causes: Vec<CauseClass> = rpd_examples
        .faults
        .iter()
        .map(|f| CauseClass::from(&f.kind))
        .collect();
    // Version 5's causes need plant `one_street` doesn't have (premises, fitted actives, a fiber plant), so its examples
    // stop at version 4's; the as-built fixture's diagnosis tests cover them.
    let as_built = [
        CauseClass::InHomeWiring,
        CauseClass::Misalignment,
        CauseClass::FiberCut,
        CauseClass::FiberEventLoss,
        CauseClass::OpticalTransmitterDrift,
    ];
    let expected: Vec<CauseClass> = CauseClass::ALL
        .into_iter()
        .filter(|c| *c != CauseClass::Unknown && !as_built.contains(c))
        .collect();
    let all: Vec<CauseClass> = causes.iter().chain(&rpd_causes).copied().collect();
    assert_eq!(all, expected);

    let has_edge = |id: &EdgeId| plant.edges.iter().any(|e| &e.id == id);
    let has_element = |id: &ElementId| plant.element(id).is_some();
    let check_location = |location: &Location| match location {
        Location::Span { span_id, .. } => assert!(has_edge(span_id), "{span_id}"),
        Location::Port { element_id, port } => {
            let element = plant.element(element_id).unwrap();
            assert!(
                element.ports.iter().any(|p| &p.id == port),
                "{element_id} {port}"
            );
        }
    };
    for fault in &examples.faults {
        match &fault.kind {
            FaultKind::WaterIngress { span_id, .. } => assert!(has_edge(span_id)),
            FaultKind::ConnectorReflection { location, .. }
            | FaultKind::Suckout { location, .. }
            | FaultKind::FrequencySelectiveLoss { location, .. } => check_location(location),
            FaultKind::AmpDrift { amp_id, .. }
            | FaultKind::ResonantPeak { amp_id, .. }
            | FaultKind::Misalignment {
                active_id: amp_id, ..
            } => {
                assert!(has_element(amp_id));
            }
            FaultKind::AdjacentLevelOffset { .. } | FaultKind::ForwardOptics { .. } => {}
            FaultKind::CommonPathDistortion { location, .. } => check_location(location),
            FaultKind::UpstreamIngress { entry_location, .. }
            | FaultKind::DownstreamIngress { entry_location, .. } => check_location(entry_location),
            FaultKind::PowerSupplySag { psu_id, .. } => {
                assert!(plant.power_supplies.iter().any(|p| &p.id == psu_id));
            }
            FaultKind::BadDrop { modem_id, .. } => assert!(has_element(modem_id)),
            FaultKind::RpdFailure { rpd_id, .. }
            | FaultKind::CinLinkCut { rpd_id, .. }
            | FaultKind::GcpLoss { rpd_id, .. }
            | FaultKind::TimingLoss { rpd_id, .. }
            | FaultKind::RfmMisalignment { rpd_id, .. } => assert!(has_element(rpd_id)),
            FaultKind::SpanCut { span_id, .. } | FaultKind::PowerShort { span_id, .. } => {
                assert!(has_edge(span_id))
            }
            FaultKind::AnalogNodeFailure { node_id, .. } => assert!(has_element(node_id)),
            FaultKind::WaterInDevice { element_id, .. }
            | FaultKind::DamagedDevice { element_id, .. } => assert!(has_element(element_id)),
            FaultKind::UtilityOutage { .. } => {}
        }
    }
}

#[test]
fn policy_has_one_entry_per_cause() {
    let policy: PolicyTable = load("policy/default.json");
    let causes: Vec<CauseClass> = policy.entries.iter().map(|e| e.cause).collect();
    assert_eq!(causes, CauseClass::ALL);
}

#[test]
fn environment_is_one_mild_dry_sample() {
    let environment: Environment = load("one_street/environment.json");
    assert_eq!(environment.samples.len(), 1);
    let sample = &environment.samples[0];
    assert_eq!(sample.t.0, 0);
    assert_eq!(
        (sample.temp_c, sample.ground_temp_c, sample.rain_mm_per_hr),
        (20.0, 20.0, 0.0)
    );
}

/// Task 22: profiles with the band fully loaded, beside the two shipped, which don't change. Same bands, levels and capability as
/// the shipped one each copies, with the plant-side readings on.
#[test]
fn the_full_profiles_load_the_band() {
    for (shipped, full) in [
        ("d31_lowsplit_1002_iccap", "d31_lowsplit_1002_iccap_full"),
        ("d31_highsplit_1218_rpd", "d31_highsplit_1218_rpd_full"),
    ] {
        let base: Profile = load(&format!("profiles/{shipped}.json"));
        let profile: Profile = load(&format!("profiles/{full}.json"));
        assert_eq!(profile.schema_version, profile::SCHEMA_VERSION);
        assert_eq!(profile.id.as_str(), full);
        profile.validate().unwrap();
        assert_eq!(profile.band_plan, base.band_plan);
        assert_eq!(profile.design_levels, base.design_levels);
        assert_eq!(profile.capability, base.capability);
        assert_eq!(profile.node_kind, base.node_kind);
        assert!(profile.plant_reports.node_optical_rx && profile.plant_reports.out_level_per_port);
        let plan = &profile.channel_plan;
        let ds_mhz: f64 = plan.ds_sc_qam.iter().map(|c| c.width_mhz).sum::<f64>()
            + plan
                .ofdm
                .iter()
                .map(|c| c.last_active_mhz - c.first_active_mhz + c.subcarrier_spacing_khz / 1000.0)
                .sum::<f64>();
        let us_mhz: f64 = plan.us_sc_qam.iter().map(|c| c.width_mhz).sum::<f64>()
            + plan
                .ofdma
                .iter()
                .map(|c| c.last_active_mhz - c.first_active_mhz + c.subcarrier_spacing_khz / 1000.0)
                .sum::<f64>();
        let span = |d: profile::Direction| {
            profile
                .band_plan
                .iter()
                .filter(|s| s.carries(d))
                .map(|s| s.hi_mhz - s.lo_mhz)
                .sum::<f64>()
        };
        assert!(
            ds_mhz / span(profile::Direction::Ds) > 0.98,
            "{full}: {ds_mhz}"
        );
        assert!(
            us_mhz / span(profile::Direction::Us) > 0.85,
            "{full}: {us_mhz}"
        );
    }
}
