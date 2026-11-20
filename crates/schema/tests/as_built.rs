//! `fixtures/one_street_as_built/`: the same street as `one_street`, with every version 2 part (`docs/tasks/21-as-built-schemas.md`,
//! deliverable 7). It validates, one hand-broken variant fails for each new error, and its parts lists match a table counted by
//! hand. All synthetic.

use std::fs;
use std::path::{Path, PathBuf};

use schema::common::{ComponentId, ElementId, FiberCableId, Origin, PortId, StructureId};
use schema::component::{ComponentLibrary, OpticalDirection, qualifies};
use schema::fiber::{FiberElementKind, FiberError, FiberPlant, Hop};
use schema::plant::{
    EdgeKind, ElementKind, Fitted, Fitting, Plant, PlantError, PortState, PortStateEntry,
    RecordStatus,
};
use schema::profile::Profile;
use schema::region::Region;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

fn fixture_path(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(file)
}

/// Loads a fixture and checks nothing in the file is lost or invented on the way through the type.
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
    (
        load("one_street_as_built/plant_lowsplit_iccap.json"),
        load("profiles/d31_lowsplit_1002_iccap.json"),
    )
}
fn high() -> (Plant, Profile) {
    (
        load("one_street_as_built/plant_highsplit_rpd.json"),
        load("profiles/d31_highsplit_1218_rpd.json"),
    )
}
fn fiber() -> FiberPlant {
    load("one_street_as_built/fiber.json")
}

fn element<'a>(plant: &'a mut Plant, id: &str) -> &'a mut schema::plant::Element {
    plant
        .elements
        .iter_mut()
        .find(|e| e.id.as_str() == id)
        .unwrap()
}

fn errors_of(plant: &Plant, profile: &Profile) -> Vec<PlantError> {
    plant.validate(&library(), profile).unwrap_err()
}

#[test]
fn both_plants_and_the_fiber_plant_validate() {
    let library = library();
    for (plant, profile) in [low(), high()] {
        plant.validate(&library, &profile).unwrap();
        assert_eq!(plant.schema_version, schema::plant::SCHEMA_VERSION);
        assert_eq!(plant.origin, Origin::Synthetic);
        assert!(
            plant
                .elements
                .iter()
                .all(|e| e.record_status == RecordStatus::Built)
        );
        assert!(
            plant
                .structures
                .iter()
                .all(|s| s.record_status == RecordStatus::Built)
        );
        for line in plant.parts_list() {
            let part = library.get(&line.component_ref).unwrap();
            assert!(qualifies(part, &profile), "{} on {}", part.id, profile.id);
        }
    }
    let fiber = fiber();
    fiber.validate(&library).unwrap();
    assert_eq!(fiber.origin, Origin::Synthetic);
    let region: Region = load("one_street_as_built/region.json");
    assert_eq!(region.fiber_ref.as_deref(), Some(fiber.id.as_str()));
    assert_eq!(region.origin, Origin::Synthetic);
}

/// Every row of `docs/as-built-plan.md`'s "Where each component lands" that task 21 can place on a plant has an instance here.
#[test]
fn the_fixture_has_every_part() {
    let library = library();
    let (plant, _) = low();
    let class_of = |id: &ComponentId| library.get(id).unwrap().class();
    let kinds: Vec<String> = plant
        .elements
        .iter()
        .map(|e| format!("{:?}", class_of(&e.component_ref)))
        .collect();
    for kind in [
        "Node",
        "Amp",
        "Tap",
        "Splitter",
        "DirectionalCoupler",
        "PowerInserter",
        "Terminator",
        "Modem",
        "LineEqualizer",
        "GroundBlock",
        "DropAmp",
        "Filter",
        "Cpe",
    ] {
        assert!(kinds.iter().any(|k| k == kind), "no {kind}");
    }
    let parts = plant.parts_list();
    let classes: Vec<String> = parts
        .iter()
        .map(|l| format!("{:?}", class_of(&l.component_ref)))
        .collect();
    for class in [
        "PlugIn",
        "Connector",
        "Battery",
        "Structure",
        "Strand",
        "Conduit",
        "OpticalRx",
        "OpticalTx",
        "Cable",
        "Drop",
    ] {
        assert!(classes.iter().any(|c| c == class), "no {class}");
    }
    let (high, _) = high();
    assert!(
        high.parts_list()
            .iter()
            .any(|l| format!("{:?}", class_of(&l.component_ref)) == "Transceiver")
    );
    // Taps of 2, 4 and 8 ports, one terminating, one with a conditioner; unused ports terminated and one open.
    let taps: Vec<(u8, bool)> = plant
        .elements
        .iter()
        .filter_map(|e| match &library.get(&e.component_ref).unwrap().spec {
            schema::component::ComponentSpec::Tap(t) => Some((t.ports, t.terminating)),
            _ => None,
        })
        .collect();
    for ports in [2, 4, 8] {
        assert!(taps.iter().any(|t| t.0 == ports));
    }
    assert!(taps.iter().any(|t| t.1));
    let states: Vec<&PortState> = plant
        .elements
        .iter()
        .filter_map(|e| match &e.fitted {
            Some(Fitted::Passive(p)) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.ports.iter().map(|s| &s.state))
        .collect();
    assert!(states.iter().any(|s| **s == PortState::Open));
    assert!(
        states
            .iter()
            .any(|s| matches!(s, PortState::Terminated { .. }))
    );
    let fiber = fiber();
    for kind in [
        "Cmts",
        "OpticalTx",
        "OpticalRx",
        "Switch",
        "Transceiver",
        "TimingSource",
        "PatchPanel",
        "SpliceClosure",
    ] {
        assert!(
            fiber
                .elements
                .iter()
                .any(|e| format!("{:?}", e.kind) == kind),
            "no {kind}"
        );
    }
    for direction in [OpticalDirection::Forward, OpticalDirection::Return] {
        assert!(fiber.links.iter().any(|l| l.direction == direction));
    }
}

/// The hand count in `docs/tasks/21-as-built-schemas.md`, "Decisions made during the task".
#[test]
fn parts_lists_match_the_hand_count() {
    let units = |plant: &Plant, id: &str, quantity: u32| {
        let line = plant
            .parts_list()
            .into_iter()
            .find(|l| l.component_ref.as_str() == id)
            .unwrap_or_else(|| panic!("{id} missing"));
        assert_eq!((line.quantity, line.total_len_ft), (quantity, None), "{id}");
    };
    let feet = |lines: &[schema::plant::PartsLine], id: &str, runs: u32, len_ft: f64| {
        let line = lines
            .iter()
            .find(|l| l.component_ref.as_str() == id)
            .unwrap_or_else(|| panic!("{id} missing"));
        assert_eq!(
            (line.quantity, line.total_len_ft),
            (runs, Some(len_ft)),
            "{id}"
        );
    };
    for (plant, _) in [low(), high()] {
        let low = plant.profile_ref.as_str() == "d31_lowsplit_1002_iccap";
        // Coax.
        units(
            &plant,
            if low {
                "node-generic-1002-modular"
            } else {
                "rpd-generic-1218-modular"
            },
            1,
        );
        units(&plant, "pi-generic-1218", 1);
        units(&plant, "amp-generic-1218-bridger", 1);
        units(&plant, "amp-generic-1218-le", 1);
        units(&plant, "tap-generic-23db-4p-fp", 1);
        units(&plant, "tap-generic-17db-8p-fp", 1);
        units(&plant, "tap-generic-4db-2p-term", 1);
        units(&plant, "tap-generic-20db-4p", 1);
        units(&plant, "tap-generic-17db-4p", 1);
        units(&plant, "leq-generic-6db", 1);
        units(&plant, "dc-generic-12db", 1);
        units(&plant, "splitter-generic-2way", 1);
        units(&plant, "term-generic", 16);
        // Plug-ins: `sim::align`'s choice, which differs by profile at `amp-2` (`docs/physics.md` §15).
        units(&plant, "pad-generic-0db", 3);
        units(&plant, "pad-generic-2db", 1);
        units(&plant, "eq-generic-0db", 1);
        units(&plant, "rpad-generic-0db", 1);
        units(&plant, "rpad-generic-12db", 2);
        units(&plant, "req-generic-6db", 2);
        if low {
            units(&plant, "pad-generic-10db", 1);
            units(&plant, "pad-generic-11db", 1);
            units(&plant, "eq-generic-18db", 1);
            units(&plant, "eq-generic-20db", 1);
        } else {
            units(&plant, "pad-generic-10db", 2);
            units(&plant, "eq-generic-20db", 2);
        }
        units(&plant, "cond-generic-2db", 1);
        units(&plant, "conn-generic-pin-750", 4);
        units(&plant, "conn-generic-splice-750", 1);
        units(&plant, "conn-generic-pin-500", 18);
        units(&plant, "conn-generic-f", 34);
        // Premises.
        units(&plant, "modem-generic-d30", 2);
        units(&plant, "modem-generic-d31", 7);
        units(&plant, "gb-generic", 4);
        units(&plant, "splitter-generic-2way-home", 2);
        units(&plant, "cpe-generic-settop", 1);
        units(&plant, "dropamp-generic-1218", 1);
        units(&plant, "filter-generic-poe", 1);
        // Power and structures.
        units(&plant, "psu-generic-90v", 1);
        units(&plant, "battery-generic-12v", 3);
        units(&plant, "structure-generic-pole", 16);
        units(&plant, "structure-generic-pedestal", 1);
        units(&plant, "structure-generic-cabinet", 1);
        // Optics fitted on the root.
        if low {
            units(&plant, "frx-generic", 1);
            units(&plant, "rtx-generic-1310", 1);
        } else {
            units(&plant, "sfp-generic-10g-lr", 1);
        }
        // By the foot.
        let lines = plant.parts_list();
        feet(&lines, "cable-generic-750", 2, 1200.0);
        feet(&lines, "cable-500-p3", 9, 1900.0);
        feet(&lines, "drop-rg6", 16, 867.0);
        feet(&lines, "drop-generic-rg11", 1, 300.0);
        feet(&lines, "strand-generic", 10, 2850.0);
        feet(&lines, "conduit-generic", 1, 250.0);
        assert_eq!(lines.len(), if low { 48 } else { 45 });
    }

    let fiber = fiber();
    let lines = fiber.parts_list();
    let one = |id: &str, quantity: u32| {
        let line = lines
            .iter()
            .find(|l| l.component_ref.as_str() == id)
            .unwrap();
        assert_eq!((line.quantity, line.total_len_ft), (quantity, None), "{id}");
    };
    for id in [
        "cmts-generic-iccap",
        "ftx-generic-1310",
        "osplit-generic-1x8",
        "rrx-generic-hfc",
        "core-generic-rphy",
        "switch-generic-48",
        "sfp-generic-10g-lr",
        "timing-generic-gm",
        "panel-generic-24",
        "closure-generic",
    ] {
        one(id, 1);
    }
    one("structure-generic-pole", 4);
    // Three pairs on each link, and two more at the forward coupler.
    one("fconn-generic-sc-apc", 14);
    feet(&lines, "fiber-generic-jumper", 3, 45.0);
    feet(&lines, "fiber-generic-jumper-duplex", 1, 15.0);
    feet(&lines, "fiber-generic-48", 1, 1700.0);
    feet(&lines, "fiber-generic-12", 2, 2600.0);
    feet(&lines, "strand-generic", 3, 4300.0);
    assert_eq!(lines.len(), 17);
    assert_eq!(fiber.splices(), 4);
}

fn has(errors: &[PlantError], pred: impl Fn(&PlantError) -> bool) -> bool {
    errors.iter().any(pred)
}

#[test]
fn validate_rejects_each_new_plant_error() {
    let (base, profile) = low();

    // A modem fed by house cable straight from a tap: the tap is no home part, and the cable has no ground block above it.
    let mut plant = base.clone();
    let edge = plant
        .edges
        .iter_mut()
        .find(|e| e.id.as_str() == "drop-1")
        .unwrap();
    edge.kind = EdgeKind::House {
        cable_ref: ComponentId::new("drop-rg6"),
        len_ft: 80.0,
        fittings: Vec::new(),
    };
    let mut errors = errors_of(&plant, &profile);
    assert!(
        has(
            &errors,
            |e| matches!(e, PlantError::NotThroughPremises { device, element }
        if device.as_str() == "modem-1" && element.as_str() == "tap-1")
        ),
        "{errors:?}"
    );
    assert!(
        has(&errors, |e| matches!(
            e,
            PlantError::HouseNotBelowGroundBlock(_)
        )),
        "{errors:?}"
    );

    // Customer equipment with no drop above it.
    let mut plant = base.clone();
    let edge = plant
        .edges
        .iter_mut()
        .find(|e| e.id.as_str() == "drop-7")
        .unwrap();
    edge.to.element_id = ElementId::new("stb-2");
    plant.edges.retain(|e| e.id.as_str() != "house-2c");
    let modem = plant
        .elements
        .iter()
        .position(|e| e.id.as_str() == "modem-7")
        .unwrap();
    plant.elements.remove(modem);
    plant
        .edges
        .iter_mut()
        .find(|e| e.id.as_str() == "drop-7")
        .unwrap()
        .kind = EdgeKind::Span {
        cable_ref: ComponentId::new("cable-500-p3"),
        len_ft: 80.0,
        installation: schema::plant::Installation::Aerial,
        power_passing: false,
        fittings: Vec::new(),
        support: None,
        path: Vec::new(),
    };
    errors = errors_of(&plant, &profile);
    assert!(
        has(
            &errors,
            |e| matches!(e, PlantError::CpeNotOnDrop(id) if id.as_str() == "stb-2")
        ),
        "{errors:?}"
    );

    // A ground block in the outside plant: on a span, not a drop.
    let mut plant = base.clone();
    plant
        .edges
        .iter_mut()
        .find(|e| e.id.as_str() == "drop-3")
        .unwrap()
        .kind = EdgeKind::Span {
        cable_ref: ComponentId::new("cable-500-p3"),
        len_ft: 75.0,
        installation: schema::plant::Installation::Aerial,
        power_passing: false,
        fittings: Vec::new(),
        support: None,
        path: Vec::new(),
    };
    errors = errors_of(&plant, &profile);
    assert!(
        has(
            &errors,
            |e| matches!(e, PlantError::PremisesPartOutside(id) if id.as_str() == "gb-3")
        ),
        "{errors:?}"
    );

    // Fitted on the wrong kind: a ground block's coupling on a tap.
    let mut plant = base.clone();
    element(&mut plant, "tap-4").fitted =
        Some(Fitted::GroundBlock(schema::plant::GroundBlockFitted {
            coupling_db: 60.0,
        }));
    errors = errors_of(&plant, &profile);
    assert!(
        has(&errors, |e| matches!(e, PlantError::FittedWrongKind { .. })),
        "{errors:?}"
    );

    // Analog optics on an amp.
    let mut plant = base.clone();
    if let Some(Fitted::Active(active)) = &mut element(&mut plant, "amp-2").fitted {
        active.forward_rx = Some(ComponentId::new("frx-generic"));
    }
    errors = errors_of(&plant, &profile);
    assert!(
        has(
            &errors,
            |e| matches!(e, PlantError::FittedWrongKind { what, .. } if what == "analog optics")
        ),
        "{errors:?}"
    );

    // A return pad in the input pad's slot, and a return receiver as the node's forward receiver.
    let mut plant = base.clone();
    if let Some(Fitted::Active(active)) = &mut element(&mut plant, "amp-2").fitted {
        active.input_pad = Some(ComponentId::new("rpad-generic-2db"));
    }
    if let Some(Fitted::Active(active)) = &mut element(&mut plant, "node-1").fitted {
        active.forward_rx = Some(ComponentId::new("rrx-generic"));
    }
    errors = errors_of(&plant, &profile);
    assert!(
        has(
            &errors,
            |e| matches!(e, PlantError::FittedWrongPart { slot, .. } if slot == "input_pad")
        ),
        "{errors:?}"
    );
    assert!(
        has(
            &errors,
            |e| matches!(e, PlantError::FittedWrongPart { slot, .. } if slot == "forward_rx")
        ),
        "{errors:?}"
    );

    // A plug-in on a host of another family: the version 1 amp has none.
    let mut plant = base.clone();
    element(&mut plant, "amp-2").component_ref = ComponentId::new("amp-generic-1218");
    errors = errors_of(&plant, &profile);
    assert!(
        has(&errors, |e| matches!(
            e,
            PlantError::PlugInFamily { host: None, .. }
        )),
        "{errors:?}"
    );

    // A port state on a port that has a drop, and a fitted output that isn't an output.
    let mut plant = base.clone();
    if let Some(Fitted::Passive(passive)) = &mut element(&mut plant, "tap-1").fitted {
        passive.ports.push(PortStateEntry {
            port: PortId::new("tap-1"),
            state: PortState::Open,
        });
    }
    if let Some(Fitted::Active(active)) = &mut element(&mut plant, "amp-1").fitted {
        active.outputs[0].port = PortId::new("in");
    }
    errors = errors_of(&plant, &profile);
    assert!(
        has(
            &errors,
            |e| matches!(e, PlantError::FittedPort { port, .. } if port.as_str() == "tap-1")
        ),
        "{errors:?}"
    );
    assert!(
        has(
            &errors,
            |e| matches!(e, PlantError::FittedPort { port, .. } if port.as_str() == "in")
        ),
        "{errors:?}"
    );

    // A terminating tap with a through port in use.
    let mut plant = base.clone();
    element(&mut plant, "tap-4").component_ref = ComponentId::new("tap-generic-8db-4p-term");
    if let Some(Fitted::Passive(passive)) = &mut element(&mut plant, "tap-4").fitted {
        passive.ports.clear();
    }
    errors = errors_of(&plant, &profile);
    assert!(
        has(
            &errors,
            |e| matches!(e, PlantError::TerminatingTapHasThrough(id) if id.as_str() == "tap-4")
        ),
        "{errors:?}"
    );

    // A fitting off its edge, and a .500 pin on .750 cable.
    let mut plant = base.clone();
    if let EdgeKind::Span { fittings, .. } = &mut plant
        .edges
        .iter_mut()
        .find(|e| e.id.as_str() == "span-1")
        .unwrap()
        .kind
    {
        fittings[1].pos_ft = 601.0;
        fittings.push(Fitting {
            pos_ft: 100.0,
            component_ref: ComponentId::new("conn-generic-pin-500"),
        });
    }
    errors = errors_of(&plant, &profile);
    assert!(
        has(
            &errors,
            |e| matches!(e, PlantError::FittingOffEdge { pos_ft, .. } if *pos_ft == 601.0)
        ),
        "{errors:?}"
    );
    assert!(
        has(&errors, |e| matches!(
            e,
            PlantError::FittingWrongCable { .. }
        )),
        "{errors:?}"
    );

    // Structures: a duplicate, a dangling reference, a kind that isn't its part's, and no batteries in a bank.
    let mut plant = base.clone();
    let mut copy = plant.structures[0].clone();
    plant.structures.push(copy.clone());
    copy.id = StructureId::new("pole-odd");
    copy.kind = schema::component::StructureKind::Vault;
    plant.structures.push(copy);
    element(&mut plant, "tap-2").structure = Some(StructureId::new("pole-missing"));
    plant.power_supplies[0].batteries.as_mut().unwrap().strings = 0;
    errors = errors_of(&plant, &profile);
    assert!(
        has(&errors, |e| matches!(e, PlantError::DuplicateStructure(_))),
        "{errors:?}"
    );
    assert!(
        has(&errors, |e| matches!(
            e,
            PlantError::UnknownStructure { .. }
        )),
        "{errors:?}"
    );
    assert!(
        has(
            &errors,
            |e| matches!(e, PlantError::StructureKindMismatch(id) if id.as_str() == "pole-odd")
        ),
        "{errors:?}"
    );
    assert!(
        has(&errors, |e| matches!(e, PlantError::EmptyBatteryBank(_))),
        "{errors:?}"
    );
}

fn fiber_errors(fiber: &FiberPlant) -> Vec<FiberError> {
    fiber.validate(&library()).unwrap_err()
}

#[test]
fn validate_rejects_each_new_fiber_error() {
    let base = fiber();
    let link = |f: &mut FiberPlant, id: &str| -> usize {
        f.links.iter().position(|l| l.id.as_str() == id).unwrap()
    };

    // Duplicate ids, a dangling hub, a part of the wrong class, an unknown part and a cable of no length.
    let mut f = base.clone();
    f.elements.push(f.elements[0].clone());
    f.elements[1].hub = Some(schema::common::HubId::new("hub-missing"));
    f.elements[2].component_ref = ComponentId::new("cable-500-p3");
    f.elements[3].component_ref = ComponentId::new("nothing");
    f.cables[0].len_ft = 0.0;
    let errors = fiber_errors(&f);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::DuplicateId(_))),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::Unknown { what: "hub", .. })),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::ComponentClassMismatch { .. })),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::UnknownComponent { .. })),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::NonPositiveLength { .. })),
        "{errors:?}"
    );

    // Two fibers with no joint between, a cable that doesn't reach where the link is, a strand past the count.
    let mut f = base.clone();
    let i = link(&mut f, "link-low-fwd");
    f.links[i].hops.remove(8); // the splice
    let errors = fiber_errors(&f);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::NotContiguous { hop: 8, .. })),
        "{errors:?}"
    );
    let mut f = base.clone();
    let i = link(&mut f, "link-low-fwd");
    f.links[i].hops[9] = Hop::Fiber {
        cable: FiberCableId::new("jumper-2"),
        strand: 1,
    };
    let errors = fiber_errors(&f);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::NotContiguous { hop: 9, .. })),
        "{errors:?}"
    );
    let mut f = base.clone();
    let i = link(&mut f, "link-low-fwd");
    f.links[i].hops[9] = Hop::Fiber {
        cable: FiberCableId::new("fiber-2"),
        strand: 13,
    };
    let errors = fiber_errors(&f);
    assert!(
        errors.iter().any(|e| matches!(
            e,
            FiberError::StrandOutOfRange {
                strand: 13,
                count: 12,
                ..
            }
        )),
        "{errors:?}"
    );

    // A forward link run backwards: it starts at the node, which doesn't transmit forward, and ends at the transmitter.
    let mut f = base.clone();
    let i = link(&mut f, "link-low-fwd");
    f.links[i].hops.reverse();
    let errors = fiber_errors(&f);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::BadStart { .. })),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::BadEnd { .. })),
        "{errors:?}"
    );

    // A link for one service group ending at another's node.
    let mut f = base.clone();
    let i = link(&mut f, "link-low-fwd");
    f.links[i].service_group = schema::common::ServiceGroupId::new("sg-as-built-highsplit");
    let errors = fiber_errors(&f);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::WrongNode { .. })),
        "{errors:?}"
    );

    // Two links on one strand, one direction, one wavelength.
    let mut f = base.clone();
    let i = link(&mut f, "link-high-fwd");
    f.links[i].hops[3] = Hop::Fiber {
        cable: FiberCableId::new("fiber-1"),
        strand: 1,
    };
    let errors = fiber_errors(&f);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::StrandShared { strand: 1, .. })),
        "{errors:?}"
    );

    // A splice at a transmitter, a port that isn't there, and a binding to something that isn't a CMTS.
    let mut f = base.clone();
    let i = link(&mut f, "link-low-fwd");
    f.links[i].hops.insert(
        1,
        Hop::Splice {
            at: schema::common::FiberElementId::new("ftx-1"),
            loss_db: None,
        },
    );
    f.links[i].hops.insert(
        1,
        Hop::Through {
            element: schema::common::FiberElementId::new("ftx-1"),
            port_in: PortId::new("a"),
            port_out: PortId::new("b"),
        },
    );
    f.bindings[0].cmts = schema::common::FiberElementId::new("switch-1");
    let errors = fiber_errors(&f);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::SpliceNotInClosure { .. })),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::UnknownPort { .. })),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, FiberError::BindingNotCmts(_))),
        "{errors:?}"
    );
    assert!(
        base.elements
            .iter()
            .any(|e| matches!(e.kind, FiberElementKind::NodeEnd { .. }))
    );
}

/// A version 1 plant has no version 2 parts; the as-built one names each.
#[test]
fn version_2_parts_are_named() {
    let library = library();
    let one_street: Plant = load("one_street/plant_lowsplit_iccap.json");
    assert!(one_street.version_2_parts(&library).is_empty());
    let (plant, _) = low();
    let parts = plant.version_2_parts(&library);
    for what in [
        "kind GroundBlock: gb-2",
        "fitted Active: amp-1",
        "house cable: house-2a",
        "support: span-11",
        "structure record: ped-1",
        "batteries: psu-1",
        "fittings: drop-1",
        "drop installation: drop-6",
        "part field plugin_family: amp-generic-1218-bridger",
        "part field outputs: amp-generic-1218-bridger",
    ] {
        assert!(parts.iter().any(|p| p == what), "{what} not in {parts:?}");
    }
    let _ = ElementKind::Cpe;
}
