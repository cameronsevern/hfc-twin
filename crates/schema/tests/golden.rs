//! Round-trip and golden tests for every top-level schema. `--features regen` rewrites
//! `fixtures/schemas/`; otherwise the files must be unchanged.

mod samples;

use std::fmt::Debug;
use std::fs;
use std::path::{Path, PathBuf};

use schema::common::Timestamp;
use schema::component::ComponentLibrary;
use schema::environment::Environment;
use schema::fault::FaultSet;
use schema::observation::{ActiveObservation, ModemObservation, ServiceGroupObservation};
use schema::plant::Plant;
use schema::profile::Profile;
use schema::work_order::{CauseClass, PolicyTable, WorkOrder, WorkOrderStatus};
use schemars::{JsonSchema, schema_for};
use serde::Serialize;
use serde::de::DeserializeOwned;

fn schemas_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/schemas")
}

fn round_trip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) {
    let text = serde_json::to_string(value).unwrap();
    let back: T = serde_json::from_str(&text).unwrap();
    assert_eq!(&back, value);
}

fn check_golden(file: &str, value: &impl Serialize) {
    let path = schemas_dir().join(file);
    let text = serde_json::to_string_pretty(value).unwrap() + "\n";
    if cfg!(feature = "regen") {
        fs::write(&path, text).unwrap();
    } else {
        let golden = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; run with --features regen", path.display()));
        assert_eq!(
            text, golden,
            "{file} changed; schema changes are new versions"
        );
    }
}

fn check<T>(name: &str, sample: &T)
where
    T: Serialize + DeserializeOwned + PartialEq + Debug + JsonSchema,
{
    round_trip(sample);
    check_golden(&format!("{name}.json"), sample);
    check_golden(&format!("{name}.schema.json"), &schema_for!(T));
}

fn region() -> schema::region::Region {
    use schema::common::{Origin, ProfileId, ServiceGroupId};
    use schema::region::{Region, RegionNode};
    Region {
        schema_version: schema::region::SCHEMA_VERSION.to_string(),
        id: "region-1".into(),
        map_id: "map-1".into(),
        origin: Origin::Synthetic,
        nodes: vec![RegionNode {
            service_group: ServiceGroupId::new("sg-1"),
            profile_ref: ProfileId::new("d31_lowsplit_1002_iccap"),
            homes_passed: 240,
        }],
        fiber_ref: None,
    }
}

fn street_map() -> schema::region::StreetMap {
    use schema::common::LonLat;
    use schema::plant::Installation;
    use schema::region::{Home, LayoutFamily, MapSource, Street, StreetMap};
    let at = |lon_deg, lat_deg| LonLat { lon_deg, lat_deg };
    StreetMap {
        schema_version: schema::region::SCHEMA_VERSION.to_string(),
        id: "map-1".into(),
        source: MapSource::Generated {
            family: LayoutFamily::UrbanGrid,
            seed: 7,
        },
        streets: vec![Street {
            id: "street-1".into(),
            points: vec![at(-105.1, 39.9), at(-105.09, 39.9)],
            installation: Installation::Aerial,
        }],
        homes: vec![Home {
            id: "home-1".into(),
            geo: at(-105.095, 39.9001),
        }],
        headend: at(-105.11, 39.9),
    }
}

#[test]
fn street_map_and_region() {
    use schema::region::{DesignControls, MapSource, PinnedNode, Region, StreetMap};
    check::<StreetMap>("street_map", &street_map());
    let osm = StreetMap {
        source: MapSource::OpenStreetMap {
            attribution: "© OpenStreetMap contributors".into(),
        },
        ..street_map()
    };
    round_trip(&osm);
    let mut controls = DesignControls::default();
    controls.pinned_nodes.push(PinnedNode {
        geo: schema::common::LonLat {
            lon_deg: -105.1,
            lat_deg: 39.9,
        },
    });
    controls.rules_id = Some("generic".into());
    controls.mode = Some(schema::region::DesignMode::LegacyThenUpgrade {
        legacy_rules_id: "legacy-1002".into(),
        legacy_profile_ref: schema::common::ProfileId::new("d30_lowsplit_750_legacy"),
    });
    check::<DesignControls>("design_controls", &controls);
    round_trip(&DesignControls {
        mode: Some(schema::region::DesignMode::Greenfield),
        ..controls.clone()
    });
    check::<Region>("region", &region());
    // Version 2's one new field, which the golden leaves out so version 1's file migrates to it.
    round_trip(&Region {
        fiber_ref: Some("fiber-1".into()),
        ..region()
    });
}

#[test]
fn rpd_observation() {
    use schema::common::{ElementId, Origin};
    use schema::rpd::{GcpStatus, PtpResult, RpdObservation, RpdState, SCHEMA_VERSION};
    check::<RpdObservation>(
        "rpd_observation",
        &RpdObservation {
            schema_version: SCHEMA_VERSION.to_string(),
            t: Timestamp(300),
            origin: Origin::Synthetic,
            element_id: ElementId::new("rpd-1"),
            top_level_state: RpdState::Operational,
            gcp_status: GcpStatus::Reconnecting,
            ptp_result: PtpResult::HoldoverWithinSpec,
            rf_output_up: true,
            drawing_power: true,
        },
    );
}

fn frozen(file: &str) -> String {
    fs::read_to_string(schemas_dir().join(file)).unwrap()
}

/// Invariant 3: version 1's files stay readable, forever, and come forward by migration.
#[test]
fn version_1_files_migrate() {
    let v1: FaultSet = serde_json::from_str(&frozen("fault_set.v1.json")).unwrap();
    assert_eq!(v1.schema_version, "1");
    let v2 = schema::fault::migrate(v1.clone()).unwrap();
    assert_eq!(v2.schema_version, schema::fault::SCHEMA_VERSION);
    assert_eq!(v2.faults, v1.faults);
    assert_eq!(v2, samples::fault_set());

    let v1: PolicyTable = serde_json::from_str(&frozen("policy_table.v1.json")).unwrap();
    let v2 = schema::work_order::migrate_policy(v1.clone()).unwrap();
    assert_eq!(v2.entries, v1.entries);
    assert_eq!(v2.schema_version, schema::work_order::SCHEMA_VERSION);

    let v1: WorkOrder = serde_json::from_str(&frozen("work_order.v1.json")).unwrap();
    let v2 = schema::work_order::migrate_work_order(v1.clone()).unwrap();
    assert_eq!(v2.id, v1.id);
    assert_eq!(v2.schema_version, schema::work_order::SCHEMA_VERSION);

    let mut future = samples::fault_set();
    future.schema_version = "99".into();
    assert!(schema::fault::migrate(future).is_err());
}

/// Invariant 3: version 3's policy table and work order migrate to version 4 unchanged apart from the version. Version 4 is
/// the widened `ComponentClass` the policy table borrows.
#[test]
fn version_1_spectrum_row_migrates() {
    let v1: schema::spectrum::SpectrumRow =
        serde_json::from_str(&frozen("spectrum_row.v1.json")).unwrap();
    assert_eq!(v1.schema_version, "1");
    let v2 = schema::spectrum::migrate(v1.clone()).unwrap();
    assert_eq!(v2.schema_version, schema::spectrum::SCHEMA_VERSION);
    assert_eq!(v2.causes, v1.causes);
    assert!(schema::spectrum::model_can_read("1"));
}

#[test]
fn version_5_policy_and_work_order_migrate() {
    let v5: PolicyTable = serde_json::from_str(&frozen("policy_table.v5.json")).unwrap();
    assert_eq!(v5.schema_version, "5");
    let v6 = schema::work_order::migrate_policy(v5.clone()).unwrap();
    assert_eq!(v6.schema_version, schema::work_order::SCHEMA_VERSION);
    assert_eq!(v6.entries, v5.entries);
    let v5: WorkOrder = serde_json::from_str(&frozen("work_order.v5.json")).unwrap();
    assert_eq!(v5.schema_version, "5");
    let v6 = schema::work_order::migrate_work_order(v5.clone()).unwrap();
    assert_eq!(v6.schema_version, schema::work_order::SCHEMA_VERSION);
    assert_eq!(v6.id, v5.id);
}

#[test]
fn version_4_policy_and_work_order_migrate() {
    let v4: PolicyTable = serde_json::from_str(&frozen("policy_table.v4.json")).unwrap();
    assert_eq!(v4.schema_version, "4");
    let v5 = schema::work_order::migrate_policy(v4.clone()).unwrap();
    assert_eq!(v5.schema_version, schema::work_order::SCHEMA_VERSION);
    assert_eq!(v5.entries, v4.entries);
    let v4: WorkOrder = serde_json::from_str(&frozen("work_order.v4.json")).unwrap();
    assert_eq!(v4.schema_version, "4");
    let v5 = schema::work_order::migrate_work_order(v4.clone()).unwrap();
    assert_eq!(v5.schema_version, schema::work_order::SCHEMA_VERSION);
    assert_eq!(v5.id, v4.id);
}

#[test]
fn version_3_policy_and_work_order_migrate() {
    let v3: PolicyTable = serde_json::from_str(&frozen("policy_table.v3.json")).unwrap();
    assert_eq!(v3.schema_version, "3");
    let v4 = schema::work_order::migrate_policy(v3.clone()).unwrap();
    assert_eq!(v4.schema_version, schema::work_order::SCHEMA_VERSION);
    assert_eq!(v4, samples::policy_table());

    let v3: WorkOrder = serde_json::from_str(&frozen("work_order.v3.json")).unwrap();
    assert_eq!(v3.schema_version, "3");
    let v4 = schema::work_order::migrate_work_order(v3.clone()).unwrap();
    assert_eq!(v4.schema_version, schema::work_order::SCHEMA_VERSION);
    assert_eq!(v4, samples::work_order(WorkOrderStatus::Proposed));

    let mut future = samples::policy_table();
    future.schema_version = "99".into();
    assert!(schema::work_order::migrate_policy(future).is_err());
}

/// Invariant 3, for task 21: version 1 of the plant, component and region schemas stays readable and migrates to
/// version 2 unchanged apart from the version string, since version 2 is additive.
#[test]
fn version_1_plant_library_and_region_migrate() {
    let v1: Plant = serde_json::from_str(&frozen("plant.v1.json")).unwrap();
    assert_eq!(v1.schema_version, "1");
    let v2 = schema::plant::migrate(v1).unwrap();
    assert_eq!(v2.schema_version, schema::plant::SCHEMA_VERSION);
    assert_eq!(v2, samples::plant());

    let v1: ComponentLibrary = serde_json::from_str(&frozen("component_library.v1.json")).unwrap();
    assert_eq!(v1.schema_version, "1");
    let v2 = schema::component::migrate(v1).unwrap();
    assert_eq!(v2.schema_version, schema::component::SCHEMA_VERSION);
    assert_eq!(v2, samples::component_library());

    let v1: schema::region::Region = serde_json::from_str(&frozen("region.v1.json")).unwrap();
    assert_eq!(v1.schema_version, "1");
    let v2 = schema::region::migrate(v1).unwrap();
    assert_eq!(v2.schema_version, schema::region::SCHEMA_VERSION);
    assert_eq!(v2, region());

    let mut future = samples::plant();
    future.schema_version = "99".into();
    assert!(schema::plant::migrate(future).is_err());
    let mut future = samples::component_library();
    future.schema_version = "99".into();
    assert!(schema::component::migrate(future).is_err());
}

/// Version 2's new parts, in samples of their own so the main samples stay what version 1's files migrate to.
#[test]
fn version_2_parts() {
    let plant = samples::plant_as_built();
    round_trip(&plant);
    check_golden("plant_as_built.json", &plant);
    let library = samples::component_library_as_built();
    round_trip(&library);
    check_golden("component_library_as_built.json", &library);
}

#[test]
fn fiber_plant() {
    check::<schema::fiber::FiberPlant>("fiber_plant", &samples::fiber_plant());
}

#[test]
fn design_rules() {
    check::<schema::design_rules::DesignRules>("design_rules", &samples::design_rules());
    let v2 = samples::design_rules_v2_fields();
    round_trip(&v2);
    check_golden("design_rules_v2_fields.json", &v2);
}

/// Invariant 3, for task 23: version 1 of the design rules, and version 2 of the region and its design controls, stay
/// readable and migrate unchanged apart from the version string, since both new versions only add optional fields.
#[test]
fn version_1_rules_and_version_2_region_migrate() {
    let v1: schema::design_rules::DesignRules =
        serde_json::from_str(&frozen("design_rules.v1.json")).unwrap();
    assert_eq!(v1.schema_version, "1");
    let v2 = schema::design_rules::migrate(v1).unwrap();
    assert_eq!(v2.schema_version, schema::design_rules::SCHEMA_VERSION);
    assert_eq!(v2, samples::design_rules());
    assert!(v2.for_design().is_err());
    let mut future = samples::design_rules();
    future.schema_version = "99".into();
    assert!(schema::design_rules::migrate(future).is_err());

    let v2: schema::region::Region = serde_json::from_str(&frozen("region.v2.json")).unwrap();
    assert_eq!(v2.schema_version, "2");
    let v3 = schema::region::migrate(v2).unwrap();
    assert_eq!(v3.schema_version, schema::region::SCHEMA_VERSION);
    assert_eq!(v3, region());

    // Design controls carry no version string of their own: version 2's file reads as version 3's with the new fields
    // absent.
    let v2: schema::region::DesignControls =
        serde_json::from_str(&frozen("design_controls.v2.json")).unwrap();
    assert_eq!(v2.rules_id, None);
    assert_eq!(v2.mode, None);
}

/// Invariant 3: version 2's files too. Version 3 only added variants, causes and an optional field.
#[test]
fn version_2_files_migrate() {
    let v2: FaultSet = serde_json::from_str(&frozen("fault_set.v2.json")).unwrap();
    assert_eq!(v2.schema_version, "2");
    let v3 = schema::fault::migrate(v2.clone()).unwrap();
    assert_eq!(v3.schema_version, schema::fault::SCHEMA_VERSION);
    assert_eq!(v3.faults, v2.faults);
    assert_eq!(v3, samples::fault_set());

    let v2: PolicyTable = serde_json::from_str(&frozen("policy_table.v2.json")).unwrap();
    let v3 = schema::work_order::migrate_policy(v2.clone()).unwrap();
    assert_eq!(v3.entries, v2.entries);
    assert_eq!(v3.schema_version, schema::work_order::SCHEMA_VERSION);

    let v2: WorkOrder = serde_json::from_str(&frozen("work_order.v2.json")).unwrap();
    let v3 = schema::work_order::migrate_work_order(v2.clone()).unwrap();
    assert_eq!(v3.id, v2.id);
    assert_eq!(v3.schema_version, schema::work_order::SCHEMA_VERSION);
}

/// Invariant 3, for task 22: version 1 of the environment and version 3 of the fault set stay readable, and migrate unchanged
/// apart from the version, since both new versions only added optional fields.
#[test]
fn version_1_environment_and_version_3_faults_migrate() {
    let v1: Environment = serde_json::from_str(&frozen("environment.v1.json")).unwrap();
    assert_eq!(v1.schema_version, "1");
    let v2 = schema::environment::migrate(v1).unwrap();
    assert_eq!(v2.schema_version, schema::environment::SCHEMA_VERSION);
    assert_eq!(v2, samples::environment());
    let mut future = samples::environment();
    future.schema_version = "99".into();
    assert!(schema::environment::migrate(future).is_err());

    let v3: FaultSet = serde_json::from_str(&frozen("fault_set.v3.json")).unwrap();
    assert_eq!(v3.schema_version, "3");
    let v4 = schema::fault::migrate(v3.clone()).unwrap();
    assert_eq!(v4.schema_version, schema::fault::SCHEMA_VERSION);
    assert_eq!(v4.faults, v3.faults);
    assert_eq!(v4, samples::fault_set());
    // A model trained on version 3 still reads version 4's faults.
    assert!(schema::fault::model_can_read("3"));
    assert!(schema::fault::model_can_read("4"));
}

/// Task 19: version 4's fault sets come forward unchanged, and a model trained on them still reads version 5's.
#[test]
fn fault_set_version_4_migrates() {
    let v4: FaultSet = serde_json::from_str(&frozen("fault_set.v4.json")).unwrap();
    assert_eq!(v4.schema_version, "4");
    let v5 = schema::fault::migrate(v4.clone()).unwrap();
    assert_eq!(v5.schema_version, schema::fault::SCHEMA_VERSION);
    assert_eq!(v5.faults, v4.faults);
    assert_eq!(v5, samples::fault_set());
    assert!(schema::fault::model_can_read("4"));
    assert!(schema::fault::model_can_read("5"));
}

/// Task 19's damage events, one of each, in a sample of their own.
#[test]
fn task_19_events() {
    let faults = samples::fault_set_v5();
    round_trip(&faults);
    check_golden("fault_set_v5_fields.json", &faults);
}

/// Task 22's new fields, in samples of their own so the main samples stay what the frozen files migrate to.
#[test]
fn task_22_fields() {
    let environment = samples::environment_v2();
    round_trip(&environment);
    check_golden("environment_v2_fields.json", &environment);
    let faults = samples::fault_set_v4();
    round_trip(&faults);
    check_golden("fault_set_v4_fields.json", &faults);
    let group = samples::service_group_observation_v2();
    round_trip(&group);
    check_golden("service_group_observation_v2_fields.json", &group);
    let active = samples::active_observation_v2();
    round_trip(&active);
    check_golden("active_observation_v2_fields.json", &active);
    let profile = samples::profile_v2();
    round_trip(&profile);
    check_golden("profile_v2_fields.json", &profile);
}

/// The fiber fault document, new in task 22, with its own version.
#[test]
fn fiber_fault_set() {
    check::<schema::fiber_fault::FiberFaultSet>("fiber_fault_set", &samples::fiber_fault_set());
    let mut future = samples::fiber_fault_set();
    future.schema_version = "99".into();
    assert!(schema::fiber_fault::migrate(future).is_err());
    assert_eq!(
        schema::fiber_fault::migrate(samples::fiber_fault_set()).unwrap(),
        samples::fiber_fault_set()
    );
}

/// Invariant 3, for task 22: version 1 observations and profiles stay readable and migrate unchanged apart from the version, since
/// version 2 of each only added optional fields; and a model trained on version 1 observations still reads them.
#[test]
fn version_1_observations_and_profiles_migrate() {
    let v1: ServiceGroupObservation =
        serde_json::from_str(&frozen("service_group_observation.v1.json")).unwrap();
    assert_eq!(v1.schema_version, "1");
    let v2 = ServiceGroupObservation {
        schema_version: schema::observation::migrate_version(&v1.schema_version).unwrap(),
        ..v1
    };
    assert_eq!(v2, samples::service_group_observation());
    let v1: ActiveObservation =
        serde_json::from_str(&frozen("active_observation.v1.json")).unwrap();
    let v2 = ActiveObservation {
        schema_version: schema::observation::migrate_version(&v1.schema_version).unwrap(),
        ..v1
    };
    assert_eq!(v2, samples::active_observation());
    let v1: ModemObservation = serde_json::from_str(&frozen("modem_observation.v1.json")).unwrap();
    let v2 = ModemObservation {
        schema_version: schema::observation::migrate_version(&v1.schema_version).unwrap(),
        ..v1
    };
    assert_eq!(v2, samples::modem_observation());
    assert!(schema::observation::migrate_version("99").is_err());
    assert!(schema::observation::model_can_read("1"));
    assert!(schema::observation::model_can_read("2"));
    assert!(!schema::observation::model_can_read("0"));

    let v1: Profile = serde_json::from_str(&frozen("profile.v1.json")).unwrap();
    assert_eq!(v1.schema_version, "1");
    let v2 = schema::profile::migrate(v1).unwrap();
    assert_eq!(v2.schema_version, schema::profile::SCHEMA_VERSION);
    assert_eq!(v2, samples::profile());
    let mut future = samples::profile();
    future.schema_version = "99".into();
    assert!(schema::profile::migrate(future).is_err());
}

#[test]
fn spectrum_row() {
    check::<schema::spectrum::SpectrumRow>("spectrum_row", &samples::spectrum_row());
}

#[test]
fn plant() {
    check::<Plant>("plant", &samples::plant());
}

#[test]
fn profile() {
    check::<Profile>("profile", &samples::profile());
}

#[test]
fn component_library() {
    check::<ComponentLibrary>("component_library", &samples::component_library());
}

#[test]
fn fault_set() {
    check::<FaultSet>("fault_set", &samples::fault_set());
}

#[test]
fn modem_observation() {
    check::<ModemObservation>("modem_observation", &samples::modem_observation());
    round_trip(&samples::modem_observation_masked());
}

#[test]
fn active_observation() {
    check::<ActiveObservation>("active_observation", &samples::active_observation());
}

#[test]
fn service_group_observation() {
    check::<ServiceGroupObservation>(
        "service_group_observation",
        &samples::service_group_observation(),
    );
}

#[test]
fn environment() {
    check::<Environment>("environment", &samples::environment());
}

#[test]
fn policy_table() {
    check::<PolicyTable>("policy_table", &samples::policy_table());
}

#[test]
fn work_order() {
    check::<WorkOrder>(
        "work_order",
        &samples::work_order(WorkOrderStatus::Proposed),
    );
    for status in [
        WorkOrderStatus::Dispatched,
        WorkOrderStatus::Completed,
        WorkOrderStatus::Verified,
        WorkOrderStatus::NotVerified,
    ] {
        round_trip(&samples::work_order(status));
    }
}

#[test]
fn work_order_id_is_derived_from_its_inputs() {
    let order = samples::work_order(WorkOrderStatus::Proposed);
    assert_eq!(order.id.get_version_num(), 5);
    let derive = |t, cause| WorkOrder::derive_id(&order.service_group, t, &order.element_id, cause);
    assert_eq!(order.id, derive(order.t, order.cause));
    assert_ne!(order.id, derive(Timestamp(301), order.cause));
    assert_ne!(order.id, derive(order.t, CauseClass::Unknown));
}

#[test]
fn ops_records() {
    check("ops_records", &samples::ops_records());
}

#[test]
fn the_ops_rates_file_reads() {
    let text = fs::read_to_string(schemas_dir().join("../ops/rates.json")).unwrap();
    let rates: schema::ops::OpsRates = serde_json::from_str(&text).unwrap();
    round_trip(&rates);
    assert_eq!(rates.schema_version, schema::ops::SCHEMA_VERSION);
    // Every rate says what it rests on, and a published one where.
    for (name, r) in &rates.rates {
        assert!(!r.note.is_empty(), "{name}");
        if r.confidence == schema::ops::Confidence::Published {
            assert!(r.source_url.is_some(), "{name}");
        }
    }
    check_golden("ops_rates.schema.json", &schema_for!(schema::ops::OpsRates));
}
