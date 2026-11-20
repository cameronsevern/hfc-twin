//! The plant graph: the single source of truth (invariant 1). One plant file is one node's
//! tree, which is one service group.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::common::{
    ComponentId, EdgeId, ElementId, LonLat, Origin, PortId, ProfileId, ServiceGroupId, StructureId,
};
use crate::component::{
    Component, ComponentClass, ComponentLibrary, ComponentSpec, OpticalDirection, PlugInKind,
    StructureKind,
};
use crate::profile::{NodeKind, Profile};

/// Version of this schema. Version 2 added element and edge kinds, what's fitted in an element, structures,
/// fittings and supports, and changed nothing version 1 had (`docs/tasks/21-as-built-schemas.md`).
pub const SCHEMA_VERSION: &str = "2";

/// Brings a plant from an earlier version to this one. Version 2 only added kinds and optional fields that are
/// left out when absent, so a version 1 plant means the same and simulates to the same bytes.
pub fn migrate(mut plant: Plant) -> Result<Plant, String> {
    match plant.schema_version.as_str() {
        "1" | "2" => {
            plant.schema_version = SCHEMA_VERSION.to_string();
            Ok(plant)
        }
        other => Err(format!("no migration from plant schema version {other}")),
    }
}

/// One service group's tree.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Plant {
    /// Version of this schema.
    pub schema_version: String,
    /// The service group this tree serves.
    pub service_group: ServiceGroupId,
    /// The architecture profile this node runs.
    pub profile_ref: ProfileId,
    /// Whether this plant is simulated or real (invariant 11).
    pub origin: Origin,
    /// The devices in the tree.
    pub elements: Vec<Element>,
    /// The connections between them, directed away from the root.
    pub edges: Vec<Edge>,
    /// The powering overlay. Not part of the tree.
    pub power_supplies: Vec<PowerSupply>,
    /// Hashed modem identities. Raw addresses never appear here (invariant 10).
    pub modem_bindings: Vec<ModemBinding>,
    /// Poles, pedestals, vaults and cabinets that elements, supplies and spans are on or in. Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structures: Vec<Structure>,
}

/// Where a plant element came from.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordStatus {
    /// Output of a design; not in the field.
    Designed,
    /// Recorded as built.
    Built,
    /// Imported from an outside system.
    Imported,
}

/// What a port does on its element.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortRole {
    /// Faces the root.
    Input,
    /// An active's output.
    Output,
    /// A tap's or coupler's through port.
    Through,
    /// A tap port, where a drop connects.
    TapPort,
    /// A splitter leg.
    Leg,
    /// A directional coupler's coupled port.
    Coupled,
}

/// One port. Off-path echoes and FDX need port identity, and it can't be rebuilt later.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Port {
    /// Unique on its element.
    pub id: PortId,
    /// What it does.
    pub role: PortRole,
}

/// The kinds of tree element.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum ElementKind {
    /// Analog fiber node.
    Node {
        /// Return attenuation, the input a technician's switch test changes, dB (§7.3).
        #[serde(default)]
        return_atten_db: f64,
    },
    /// Remote PHY device.
    Rpd {
        /// Return attenuation, the input a technician's switch test changes, dB (§7.3).
        #[serde(default)]
        return_atten_db: f64,
    },
    /// RF amplifier.
    Amp {
        /// Return attenuation, the input a technician's switch test changes, dB (§7.3).
        #[serde(default)]
        return_atten_db: f64,
    },
    /// Multi-port tap.
    Tap,
    /// Splitter.
    Splitter,
    /// Directional coupler.
    DirectionalCoupler,
    /// Power inserter.
    PowerInserter,
    /// Terminator.
    Terminator,
    /// Cable modem.
    Modem,
    /// An in-line equalizer in its own housing. Version 2.
    LineEqualizer,
    /// Where a drop enters a home. In-home cable is below one. Version 2.
    GroundBlock,
    /// An amplifier in a home. Version 2.
    DropAmp,
    /// A filter, on a tap port or in a home. Version 2.
    Filter,
    /// Customer equipment that reports nothing. Version 2.
    Cpe,
}

impl ElementKind {
    /// The component class an element of this kind must reference.
    pub fn component_class(&self) -> ComponentClass {
        match self {
            ElementKind::Node { .. } => ComponentClass::Node,
            ElementKind::Rpd { .. } => ComponentClass::Rpd,
            ElementKind::Amp { .. } => ComponentClass::Amp,
            ElementKind::Tap => ComponentClass::Tap,
            ElementKind::Splitter => ComponentClass::Splitter,
            ElementKind::DirectionalCoupler => ComponentClass::DirectionalCoupler,
            ElementKind::PowerInserter => ComponentClass::PowerInserter,
            ElementKind::Terminator => ComponentClass::Terminator,
            ElementKind::Modem => ComponentClass::Modem,
            ElementKind::LineEqualizer => ComponentClass::LineEqualizer,
            ElementKind::GroundBlock => ComponentClass::GroundBlock,
            ElementKind::DropAmp => ComponentClass::DropAmp,
            ElementKind::Filter => ComponentClass::Filter,
            ElementKind::Cpe => ComponentClass::Cpe,
        }
    }

    /// Whether version 1 had this kind.
    pub fn is_version_1(&self) -> bool {
        !matches!(
            self,
            ElementKind::LineEqualizer
                | ElementKind::GroundBlock
                | ElementKind::DropAmp
                | ElementKind::Filter
                | ElementKind::Cpe
        )
    }

    /// Whether an element of this kind can be in a home, between a drop and a device.
    pub fn is_premises(&self) -> bool {
        matches!(
            self,
            ElementKind::GroundBlock
                | ElementKind::Splitter
                | ElementKind::DirectionalCoupler
                | ElementKind::DropAmp
                | ElementKind::Filter
                | ElementKind::Terminator
                | ElementKind::Modem
                | ElementKind::Cpe
        )
    }
}

/// A device in the tree.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Element {
    /// Stable human-assigned id, such as `amp-1`.
    pub id: ElementId,
    /// The kind of device.
    pub kind: ElementKind,
    /// The library entry it is built from.
    pub component_ref: ComponentId,
    /// Where it is, if known.
    pub geo: Option<LonLat>,
    /// Its ports.
    pub ports: Vec<Port>,
    /// Where this record came from.
    pub record_status: RecordStatus,
    /// What's installed in or on it: plug-ins, optics, port states. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fitted: Option<Fitted>,
    /// The structure it's on or in. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<StructureId>,
}

/// What's installed in or on an element, which must suit its kind. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum Fitted {
    /// On a node, RPD or amp.
    Active(ActiveFitted),
    /// On a tap, splitter or directional coupler.
    Passive(PassiveFitted),
    /// On a ground block.
    GroundBlock(GroundBlockFitted),
}

/// What's in an active's housing. Plug-ins are `PlugIn` parts of the active's `plugin_family`. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct ActiveFitted {
    /// Forward input pad, a `Pad`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_pad: Option<ComponentId>,
    /// Forward input equalizer or cable simulator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_eq: Option<ComponentId>,
    /// What's fitted at each output port.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<OutputFitted>,
    /// Return pad, a `ReturnPad`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_pad: Option<ComponentId>,
    /// Return equalizer, a `ReturnEqualizer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_eq: Option<ComponentId>,
    /// The output this station is aligned to, in place of the profile's design level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_point: Option<SetPoint>,
    /// A node's forward optical receiver, an `OpticalRx` that receives forward.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forward_rx: Option<ComponentId>,
    /// A node's return optical transmitter, an `OpticalTx` that transmits return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_tx: Option<ComponentId>,
    /// An RPD's pluggable optics, `Transceiver`s, primary first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transceivers: Vec<ComponentId>,
}

/// What's fitted at one output port of an active. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OutputFitted {
    /// The output port.
    pub port: PortId,
    /// Its pad, a `Pad`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pad: Option<ComponentId>,
    /// Its equalizer or cable simulator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eq: Option<ComponentId>,
}

/// An active's alignment target. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SetPoint {
    /// Output at the band's top frequency, dBmV per 6 MHz.
    pub out_dbmv_at_hi: f64,
    /// Output tilt across the forward band, dB.
    pub tilt_db: f64,
}

/// What's fitted on a passive. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct PassiveFitted {
    /// A tap's faceplate conditioner, a `TapConditioner` of the tap's `plugin_family`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditioner: Option<ComponentId>,
    /// The state of each port with nothing connected. A port with neither an edge nor a state means what it did in
    /// version 1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortStateEntry>,
}

/// One unused port's state. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PortStateEntry {
    /// The port.
    pub port: PortId,
    /// Its state.
    pub state: PortState,
}

/// What's on a port with nothing connected.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum PortState {
    /// A terminator screwed on.
    Terminated {
        /// The terminator, a `Terminator`.
        component_ref: ComponentId,
    },
    /// Nothing at all: a reflector.
    Open,
}

/// What's particular to one home's ground block. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GroundBlockFitted {
    /// How far the home's wiring keeps ambient signals out, as coupling loss from outside, dB.
    pub coupling_db: f64,
}

/// How a span is installed; it selects the §1.2 temperature.
#[derive(
    Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum Installation {
    /// Strung on poles: follows `temp_c`.
    Aerial,
    /// Buried: follows `ground_temp_c`.
    Underground,
}

/// One end of an edge.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PortRef {
    /// The element.
    pub element_id: ElementId,
    /// The port on it.
    pub port: PortId,
}

/// What physically makes up an edge.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum EdgeKind {
    /// Hardline between plant devices.
    Span {
        /// The cable, class `Cable`.
        cable_ref: ComponentId,
        /// Length, feet.
        len_ft: f64,
        /// How it is installed.
        installation: Installation,
        /// Whether it carries AC power (§6.2).
        power_passing: bool,
        /// Connectors, splices and adapters along it, by position. Version 2.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        fittings: Vec<Fitting>,
        /// The strand it's lashed to or the conduit it's in, a `Strand` or `Conduit` part, by the foot. Version 2.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        support: Option<ComponentId>,
        /// The points it passes between its ends, in order from its upstream end. Version 2.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        path: Vec<RoutePoint>,
    },
    /// Drop cable from a tap port to a home or a device.
    Drop {
        /// The cable, class `Drop`.
        cable_ref: ComponentId,
        /// Length, feet.
        len_ft: f64,
        /// Connectors along it, by position. Version 2.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        fittings: Vec<Fitting>,
        /// The points it passes between its ends. Version 2.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        path: Vec<RoutePoint>,
        /// How it's installed. `None` is version 1's, which follows air temperature as `Aerial` does. Version 2.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        installation: Option<Installation>,
    },
    /// A part mounted directly on a port, with no cable.
    Direct,
    /// Cable inside a home, below its ground block. Version 2.
    House {
        /// The cable, class `Drop`.
        cable_ref: ComponentId,
        /// Length, feet.
        len_ft: f64,
        /// Connectors along it, by position.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        fittings: Vec<Fitting>,
    },
}

impl EdgeKind {
    /// The cable on this edge and its length, feet, or `None` for a direct mount.
    pub fn cable(&self) -> Option<(&ComponentId, f64)> {
        match self {
            EdgeKind::Span {
                cable_ref, len_ft, ..
            }
            | EdgeKind::Drop {
                cable_ref, len_ft, ..
            }
            | EdgeKind::House {
                cable_ref, len_ft, ..
            } => Some((cable_ref, *len_ft)),
            EdgeKind::Direct => None,
        }
    }

    /// The fittings along it.
    pub fn fittings(&self) -> &[Fitting] {
        match self {
            EdgeKind::Span { fittings, .. }
            | EdgeKind::Drop { fittings, .. }
            | EdgeKind::House { fittings, .. } => fittings,
            EdgeKind::Direct => &[],
        }
    }

    /// The points it passes between its ends.
    pub fn path(&self) -> &[RoutePoint] {
        match self {
            EdgeKind::Span { path, .. } | EdgeKind::Drop { path, .. } => path,
            EdgeKind::House { .. } | EdgeKind::Direct => &[],
        }
    }
}

/// A connector, splice or adapter at a point along a cabled edge. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Fitting {
    /// Distance from the edge's upstream end, feet: 0 and the edge's length are its two ends.
    pub pos_ft: f64,
    /// The part, a `Connector`.
    pub component_ref: ComponentId,
}

/// A point a cable passes. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RoutePoint {
    /// Where.
    pub geo: LonLat,
    /// The structure there, such as the pole it's attached at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<StructureId>,
}

/// A connection, directed away from the root.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    /// Stable id. A fault's `span_id` refers to this.
    pub id: EdgeId,
    /// The port it leaves.
    pub from: PortRef,
    /// The port it enters.
    pub to: PortRef,
    /// What it is made of.
    pub kind: EdgeKind,
    /// Where this record came from. Cable is plant too. `None` follows the element the edge enters. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_status: Option<RecordStatus>,
}

/// A power supply, in the overlay rather than the tree.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PowerSupply {
    /// Stable human-assigned id.
    pub id: ElementId,
    /// The library entry it is built from. Voltage lives there (§6.3).
    pub component_ref: ComponentId,
    /// The element where its current enters the tree; it flows on from there along
    /// power-passing edges (§6.2).
    pub feeds: ElementId,
    /// Where it is, if known.
    pub geo: Option<LonLat>,
    /// Its standby batteries. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batteries: Option<BatteryBank>,
    /// The cabinet or pole it's in or on. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<StructureId>,
    /// Where this record came from. `None` follows the element it feeds. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_status: Option<RecordStatus>,
}

/// A supply's standby batteries: strings in parallel, each of batteries in series. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BatteryBank {
    /// The battery, a `Battery`.
    pub component_ref: ComponentId,
    /// Batteries in series in each string.
    pub per_string: u8,
    /// Strings in parallel.
    pub strings: u8,
}

/// A pole, pedestal, vault or cabinet. No RF and no power: a record and a parts-list line. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    /// Stable id.
    pub id: StructureId,
    /// What it is. Must be its part's kind.
    pub kind: StructureKind,
    /// The part, a `Structure`.
    pub component_ref: ComponentId,
    /// Where it is, if known.
    pub geo: Option<LonLat>,
    /// Where this record came from.
    pub record_status: RecordStatus,
}

/// Links a plant modem to a hashed identity (invariant 10).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModemBinding {
    /// The modem element.
    pub element_id: ElementId,
    /// The hashed identity.
    pub pseudonym: String,
}

/// One line of a parts list. Derived, never stored (invariant 1).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PartsLine {
    /// The part.
    pub component_ref: ComponentId,
    /// How many: units of a device, or runs of a cable.
    pub quantity: u32,
    /// Total length, feet, for cable. `None` for a device.
    pub total_len_ft: Option<f64>,
}

/// A problem with a plant.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum PlantError {
    /// The plant says it runs a different profile.
    #[error("plant runs profile {plant}, checked against {given}")]
    ProfileMismatch {
        /// The plant's `profile_ref`.
        plant: ProfileId,
        /// The profile it was checked against.
        given: ProfileId,
    },
    /// Two elements or supplies share an id.
    #[error("duplicate element id {0}")]
    DuplicateElement(ElementId),
    /// Two edges share an id.
    #[error("duplicate edge id {0}")]
    DuplicateEdge(EdgeId),
    /// An edge or overlay names an element that isn't in the plant.
    #[error("{referrer} refers to unknown element {element}")]
    UnknownElement {
        /// What holds the reference.
        referrer: String,
        /// The missing element.
        element: ElementId,
    },
    /// An edge names a port its element doesn't have.
    #[error("edge {edge} refers to unknown port {port} on {element}")]
    UnknownPort {
        /// The edge.
        edge: EdgeId,
        /// The element.
        element: ElementId,
        /// The missing port.
        port: PortId,
    },
    /// An edge doesn't point away from the root: it must enter an `Input` port and leave
    /// any other.
    #[error("edge {edge} uses port {port} on {element} the wrong way round")]
    PortDirection {
        /// The edge.
        edge: EdgeId,
        /// The element.
        element: ElementId,
        /// The port.
        port: PortId,
    },
    /// Two edges use one port.
    #[error("port {port} on {element} is used more than once")]
    PortUsedTwice {
        /// The element.
        element: ElementId,
        /// The port.
        port: PortId,
    },
    /// No element is without a parent.
    #[error("the plant has no root")]
    NoRoot,
    /// More than one element is without a parent.
    #[error("the plant has more than one root: {0:?}")]
    MultipleRoots(Vec<ElementId>),
    /// An element has more than one parent.
    #[error("element {0} has more than one parent")]
    MultipleParents(ElementId),
    /// These elements are in, or below, a cycle.
    #[error("cycle through or above {0:?}")]
    Cycle(Vec<ElementId>),
    /// The root isn't the kind of device the profile requires.
    #[error("root {root} doesn't match the profile's node kind {expected:?}")]
    RootKindMismatch {
        /// The root element.
        root: ElementId,
        /// What the profile requires.
        expected: NodeKind,
    },
    /// A referenced component isn't in the library.
    #[error("{referrer} refers to unknown component {component}")]
    UnknownComponent {
        /// What holds the reference.
        referrer: String,
        /// The missing component.
        component: ComponentId,
    },
    /// A referenced component is the wrong class for what refers to it.
    #[error("{referrer} needs a {expected:?} but {component} is a {found:?}")]
    ComponentClassMismatch {
        /// What holds the reference.
        referrer: String,
        /// The component.
        component: ComponentId,
        /// The class needed.
        expected: ComponentClass,
        /// The class it is.
        found: ComponentClass,
    },
    /// A tap's tap ports don't match its component.
    #[error("tap {element} has {found} tap ports but its component has {expected}")]
    TapPortCount {
        /// The tap.
        element: ElementId,
        /// How many its component has.
        expected: u8,
        /// How many it lists.
        found: usize,
    },
    /// A span or drop isn't longer than zero.
    #[error("edge {edge} has length {len_ft} ft")]
    NonPositiveLength {
        /// The edge.
        edge: EdgeId,
        /// Its length, feet.
        len_ft: f64,
    },
    /// A modem isn't at the end of a drop.
    #[error("modem {0} isn't fed by a drop")]
    ModemNotOnDrop(ElementId),
    /// A binding names something other than a modem.
    #[error("binding names {0}, which isn't a modem")]
    BindingNotModem(ElementId),
    /// Between a device and the drop above it is an element that can't be in a home. Version 2.
    #[error("{device} reaches its drop through {element}, which can't be in a home")]
    NotThroughPremises {
        /// The modem or customer device.
        device: ElementId,
        /// What's in the way.
        element: ElementId,
    },
    /// Customer equipment isn't below a drop through premises parts. Version 2.
    #[error("customer device {0} isn't fed by a drop")]
    CpeNotOnDrop(ElementId),
    /// A part that belongs in a home isn't below a drop. Version 2.
    #[error("{0} belongs in a home but isn't below a drop")]
    PremisesPartOutside(ElementId),
    /// In-home cable that isn't below a ground block. Version 2.
    #[error("house cable {0} isn't below a ground block")]
    HouseNotBelowGroundBlock(EdgeId),
    /// What's fitted doesn't suit the element's kind. Version 2.
    #[error("element {element} can't have {what} fitted")]
    FittedWrongKind {
        /// The element.
        element: ElementId,
        /// What was fitted.
        what: String,
    },
    /// A plug-in of the wrong kind for its slot, or an optic facing the wrong way. Version 2.
    #[error("element {element}: {component} doesn't fit slot {slot}")]
    FittedWrongPart {
        /// The element.
        element: ElementId,
        /// The slot.
        slot: String,
        /// The part.
        component: ComponentId,
    },
    /// A plug-in whose family isn't its host's. Version 2.
    #[error("element {element}: plug-in {component} is family {plugin:?}, its host takes {host:?}")]
    PlugInFamily {
        /// The host element.
        element: ElementId,
        /// The plug-in.
        component: ComponentId,
        /// The plug-in's family.
        plugin: String,
        /// The host's, if it has one.
        host: Option<String>,
    },
    /// A fitted output or port state names a port that isn't right for it. Version 2.
    #[error("element {element}: port {port} {problem}")]
    FittedPort {
        /// The element.
        element: ElementId,
        /// The port.
        port: PortId,
        /// What's wrong.
        problem: String,
    },
    /// A terminating tap with something on its through port, or a through port at all. Version 2.
    #[error("terminating tap {0} has a through edge")]
    TerminatingTapHasThrough(ElementId),
    /// A fitting past either end of its edge. Version 2.
    #[error("edge {edge}: fitting at {pos_ft} ft is off its {len_ft} ft")]
    FittingOffEdge {
        /// The edge.
        edge: EdgeId,
        /// Where the fitting is, feet.
        pos_ft: f64,
        /// The edge's length, feet.
        len_ft: f64,
    },
    /// A connector that doesn't fit its cable's size family. Version 2.
    #[error("edge {edge}: connector {component} fits {connector}, the cable is {cable}")]
    FittingWrongCable {
        /// The edge.
        edge: EdgeId,
        /// The connector.
        component: ComponentId,
        /// The family the connector fits.
        connector: String,
        /// The cable's family.
        cable: String,
    },
    /// Two structures share an id. Version 2.
    #[error("duplicate structure id {0}")]
    DuplicateStructure(StructureId),
    /// A reference to a structure that isn't in the plant. Version 2.
    #[error("{referrer} refers to unknown structure {structure}")]
    UnknownStructure {
        /// What holds the reference.
        referrer: String,
        /// The missing structure.
        structure: StructureId,
    },
    /// A structure whose kind isn't its part's. Version 2.
    #[error("structure {0} isn't the kind its part is")]
    StructureKindMismatch(StructureId),
    /// A battery bank with no batteries. Version 2.
    #[error("power supply {0} has an empty battery bank")]
    EmptyBatteryBank(ElementId),
}

impl Plant {
    /// The one element with no parent, if there is exactly one.
    pub fn root(&self) -> Option<&Element> {
        let children: BTreeSet<_> = self.edges.iter().map(|e| &e.to.element_id).collect();
        let mut roots = self.elements.iter().filter(|e| !children.contains(&e.id));
        match (roots.next(), roots.next()) {
            (Some(root), None) => Some(root),
            _ => None,
        }
    }

    /// Finds an element by id.
    pub fn element(&self, id: &ElementId) -> Option<&Element> {
        self.elements.iter().find(|e| &e.id == id)
    }

    /// Checks the plant is a well-formed tree that fits this library and profile. Reports
    /// every problem found rather than stopping at the first.
    pub fn validate(
        &self,
        library: &ComponentLibrary,
        profile: &Profile,
    ) -> Result<(), Vec<PlantError>> {
        let mut errors = Vec::new();
        if self.profile_ref != profile.id {
            errors.push(PlantError::ProfileMismatch {
                plant: self.profile_ref.clone(),
                given: profile.id.clone(),
            });
        }
        self.check_ids(&mut errors);
        self.check_edges(&mut errors);
        self.check_tree(&mut errors);
        self.check_root(profile, &mut errors);
        self.check_components(library, &mut errors);
        self.check_modems(&mut errors);
        self.check_premises(&mut errors);
        self.check_fitted(library, &mut errors);
        self.check_fittings(library, &mut errors);
        self.check_structures(library, &mut errors);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn check_ids(&self, errors: &mut Vec<PlantError>) {
        let mut elements = BTreeSet::new();
        let ids = self
            .elements
            .iter()
            .map(|e| &e.id)
            .chain(self.power_supplies.iter().map(|p| &p.id));
        for id in ids {
            if !elements.insert(id) {
                errors.push(PlantError::DuplicateElement(id.clone()));
            }
        }
        let mut edges = BTreeSet::new();
        for edge in &self.edges {
            if !edges.insert(&edge.id) {
                errors.push(PlantError::DuplicateEdge(edge.id.clone()));
            }
        }
    }

    fn check_edges(&self, errors: &mut Vec<PlantError>) {
        let mut used = BTreeSet::new();
        for edge in &self.edges {
            for end in [&edge.from, &edge.to] {
                let Some(element) = self.element(&end.element_id) else {
                    errors.push(PlantError::UnknownElement {
                        referrer: format!("edge {}", edge.id),
                        element: end.element_id.clone(),
                    });
                    continue;
                };
                match element.ports.iter().find(|p| p.id == end.port) {
                    None => errors.push(PlantError::UnknownPort {
                        edge: edge.id.clone(),
                        element: end.element_id.clone(),
                        port: end.port.clone(),
                    }),
                    Some(port) => {
                        let enters = std::ptr::eq(end, &edge.to);
                        if (port.role == PortRole::Input) != enters {
                            errors.push(PlantError::PortDirection {
                                edge: edge.id.clone(),
                                element: end.element_id.clone(),
                                port: end.port.clone(),
                            });
                        }
                    }
                }
                if !used.insert((&end.element_id, &end.port)) {
                    errors.push(PlantError::PortUsedTwice {
                        element: end.element_id.clone(),
                        port: end.port.clone(),
                    });
                }
            }
            if let Some((_, len_ft)) = edge.kind.cable()
                && len_ft <= 0.0
            {
                errors.push(PlantError::NonPositiveLength {
                    edge: edge.id.clone(),
                    len_ft,
                });
            }
        }
        for supply in &self.power_supplies {
            if self.element(&supply.feeds).is_none() {
                errors.push(PlantError::UnknownElement {
                    referrer: format!("power supply {}", supply.id),
                    element: supply.feeds.clone(),
                });
            }
        }
    }

    /// One root, one parent each, and no cycles, by peeling away parentless elements until
    /// only cycles and whatever hangs below them remain.
    fn check_tree(&self, errors: &mut Vec<PlantError>) {
        let mut parents: BTreeMap<&ElementId, usize> =
            self.elements.iter().map(|e| (&e.id, 0)).collect();
        let mut children: BTreeMap<&ElementId, Vec<&ElementId>> = BTreeMap::new();
        for edge in &self.edges {
            if let Some(count) = parents.get_mut(&edge.to.element_id) {
                *count += 1;
            }
            children
                .entry(&edge.from.element_id)
                .or_default()
                .push(&edge.to.element_id);
        }
        for element in &self.elements {
            if parents[&element.id] > 1 {
                errors.push(PlantError::MultipleParents(element.id.clone()));
            }
        }

        let roots: Vec<&ElementId> = self
            .elements
            .iter()
            .map(|e| &e.id)
            .filter(|id| parents[id] == 0)
            .collect();
        match roots.len() {
            0 => errors.push(PlantError::NoRoot),
            1 => {}
            _ => errors.push(PlantError::MultipleRoots(
                roots.iter().map(|id| (*id).clone()).collect(),
            )),
        }

        let mut remaining = parents;
        let mut peel = roots;
        while let Some(id) = peel.pop() {
            remaining.remove(id);
            for child in children.get(id).into_iter().flatten() {
                if let Some(count) = remaining.get_mut(*child) {
                    *count -= 1;
                    if *count == 0 {
                        peel.push(child);
                    }
                }
            }
        }
        if !remaining.is_empty() {
            errors.push(PlantError::Cycle(
                remaining.keys().map(|id| (*id).clone()).collect(),
            ));
        }
    }

    fn check_root(&self, profile: &Profile, errors: &mut Vec<PlantError>) {
        let Some(root) = self.root() else { return };
        let matches = matches!(
            (profile.node_kind, &root.kind),
            (NodeKind::Analog, ElementKind::Node { .. }) | (NodeKind::Rpd, ElementKind::Rpd { .. })
        );
        if !matches {
            errors.push(PlantError::RootKindMismatch {
                root: root.id.clone(),
                expected: profile.node_kind,
            });
        }
    }

    fn check_components(&self, library: &ComponentLibrary, errors: &mut Vec<PlantError>) {
        let mut check = |referrer: String, component: &ComponentId, expected: ComponentClass| {
            let Some(entry) = library.get(component) else {
                errors.push(PlantError::UnknownComponent {
                    referrer,
                    component: component.clone(),
                });
                return None;
            };
            if entry.class() != expected {
                errors.push(PlantError::ComponentClassMismatch {
                    referrer,
                    component: component.clone(),
                    expected,
                    found: entry.class(),
                });
                return None;
            }
            Some(entry)
        };
        let mut tap_ports = Vec::new();
        for element in &self.elements {
            let referrer = format!("element {}", element.id);
            let entry = check(
                referrer,
                &element.component_ref,
                element.kind.component_class(),
            );
            if let Some(ComponentSpec::Tap(tap)) = entry.map(|c| &c.spec) {
                let found = element
                    .ports
                    .iter()
                    .filter(|p| p.role == PortRole::TapPort)
                    .count();
                if found != usize::from(tap.ports) {
                    tap_ports.push(PlantError::TapPortCount {
                        element: element.id.clone(),
                        expected: tap.ports,
                        found,
                    });
                }
            }
        }
        for edge in &self.edges {
            let referrer = format!("edge {}", edge.id);
            match &edge.kind {
                EdgeKind::Span { cable_ref, .. } => {
                    check(referrer, cable_ref, ComponentClass::Cable);
                }
                EdgeKind::Drop { cable_ref, .. } | EdgeKind::House { cable_ref, .. } => {
                    check(referrer, cable_ref, ComponentClass::Drop);
                }
                EdgeKind::Direct => {}
            }
        }
        for supply in &self.power_supplies {
            let referrer = format!("power supply {}", supply.id);
            check(referrer, &supply.component_ref, ComponentClass::PowerSupply);
        }
        errors.append(&mut tap_ports);
    }

    fn parent_edge(&self, id: &ElementId) -> Option<&Edge> {
        self.edges.iter().find(|edge| &edge.to.element_id == id)
    }

    /// Walks up from a device through `House` and `Direct` edges and premises parts to the drop that feeds it.
    /// `Ok` if it gets there; otherwise the element in the way, if one is, and `None` if it ran out of plant first.
    fn premises_path(&self, device: &ElementId) -> Result<(), Option<ElementId>> {
        let mut current = device.clone();
        // A tree has at most as many edges as elements, so the walk ends even on a plant with a cycle.
        for _ in 0..=self.elements.len() {
            let Some(edge) = self.parent_edge(&current) else {
                return Err(None);
            };
            match edge.kind {
                EdgeKind::Drop { .. } => return Ok(()),
                EdgeKind::Span { .. } => return Err(None),
                EdgeKind::House { .. } | EdgeKind::Direct => {
                    let parent = &edge.from.element_id;
                    match self.element(parent) {
                        Some(element) if element.kind.is_premises() => current = parent.clone(),
                        Some(_) => return Err(Some(parent.clone())),
                        None => return Err(None),
                    }
                }
            }
        }
        Err(None)
    }

    /// Whether a drop is somewhere above this element.
    fn below_drop(&self, id: &ElementId) -> bool {
        let mut current = id.clone();
        for _ in 0..=self.elements.len() {
            let Some(edge) = self.parent_edge(&current) else {
                return false;
            };
            if matches!(edge.kind, EdgeKind::Drop { .. }) {
                return true;
            }
            current = edge.from.element_id.clone();
        }
        false
    }

    fn check_modems(&self, errors: &mut Vec<PlantError>) {
        for element in &self.elements {
            if element.kind != ElementKind::Modem {
                continue;
            }
            // Version 1: a modem's parent edge is a drop. Version 2: or a drop above it, through a home.
            if let Err(blocker) = self.premises_path(&element.id) {
                errors.push(PlantError::ModemNotOnDrop(element.id.clone()));
                if let Some(blocker) = blocker {
                    errors.push(PlantError::NotThroughPremises {
                        device: element.id.clone(),
                        element: blocker,
                    });
                }
            }
        }
        for binding in &self.modem_bindings {
            let is_modem = self
                .element(&binding.element_id)
                .is_some_and(|e| e.kind == ElementKind::Modem);
            if !is_modem {
                errors.push(PlantError::BindingNotModem(binding.element_id.clone()));
            }
        }
    }

    /// Version 2's premises rules: customer equipment hangs below a drop through a home, home parts are below a
    /// drop, and house cable is below a ground block.
    fn check_premises(&self, errors: &mut Vec<PlantError>) {
        for element in &self.elements {
            match element.kind {
                ElementKind::Cpe => match self.premises_path(&element.id) {
                    Ok(()) => {}
                    Err(Some(blocker)) => errors.push(PlantError::NotThroughPremises {
                        device: element.id.clone(),
                        element: blocker,
                    }),
                    Err(None) => errors.push(PlantError::CpeNotOnDrop(element.id.clone())),
                },
                ElementKind::GroundBlock | ElementKind::DropAmp
                    if !self.below_drop(&element.id) =>
                {
                    errors.push(PlantError::PremisesPartOutside(element.id.clone()));
                }
                _ => {}
            }
        }
        for edge in &self.edges {
            if !matches!(edge.kind, EdgeKind::House { .. }) {
                continue;
            }
            let mut current = Some(edge.from.element_id.clone());
            let mut found = false;
            for _ in 0..=self.elements.len() {
                let Some(id) = current else { break };
                if self
                    .element(&id)
                    .is_some_and(|e| e.kind == ElementKind::GroundBlock)
                {
                    found = true;
                    break;
                }
                current = self.parent_edge(&id).map(|e| e.from.element_id.clone());
            }
            if !found {
                errors.push(PlantError::HouseNotBelowGroundBlock(edge.id.clone()));
            }
        }
    }

    /// Version 2: what's fitted suits its element, each part is the right class and kind for its slot, plug-ins are
    /// their host's family, and port states are on unused ports.
    fn check_fitted(&self, library: &ComponentLibrary, errors: &mut Vec<PlantError>) {
        let used: BTreeSet<(&ElementId, &PortId)> = self
            .edges
            .iter()
            .flat_map(|e| {
                [
                    (&e.from.element_id, &e.from.port),
                    (&e.to.element_id, &e.to.port),
                ]
            })
            .collect();
        for element in &self.elements {
            let host = library.get(&element.component_ref);
            if let ElementKind::Tap = element.kind
                && let Some(ComponentSpec::Tap(tap)) = host.map(|c| &c.spec)
                && tap.terminating
            {
                let has_through = element
                    .ports
                    .iter()
                    .any(|p| p.role == PortRole::Through && used.contains(&(&element.id, &p.id)));
                if has_through {
                    errors.push(PlantError::TerminatingTapHasThrough(element.id.clone()));
                }
            }
            let Some(fitted) = &element.fitted else {
                continue;
            };
            let mut slots = FittedCheck {
                element,
                library,
                errors,
            };
            match (fitted, &element.kind) {
                (
                    Fitted::Active(active),
                    ElementKind::Node { .. } | ElementKind::Rpd { .. } | ElementKind::Amp { .. },
                ) => {
                    let family = host
                        .and_then(|c| c.spec.active())
                        .and_then(|a| a.plugin_family.clone());
                    slots.check_active(active, family.as_deref());
                }
                (Fitted::Passive(passive), ElementKind::Tap)
                | (Fitted::Passive(passive), ElementKind::Splitter)
                | (Fitted::Passive(passive), ElementKind::DirectionalCoupler) => {
                    let family = match host.map(|c| &c.spec) {
                        Some(ComponentSpec::Tap(tap)) => tap.plugin_family.clone(),
                        _ => None,
                    };
                    slots.check_passive(passive, family.as_deref(), &used);
                }
                (Fitted::GroundBlock(_), ElementKind::GroundBlock) => {}
                (fitted, _) => slots.errors.push(PlantError::FittedWrongKind {
                    element: element.id.clone(),
                    what: fitted.name().to_string(),
                }),
            }
        }
    }

    /// Version 2: fittings are connectors on their edge, fit the cable's size, and supports are strand or conduit.
    fn check_fittings(&self, library: &ComponentLibrary, errors: &mut Vec<PlantError>) {
        for edge in &self.edges {
            let Some((cable_ref, len_ft)) = edge.kind.cable() else {
                continue;
            };
            let cable_family = match library.get(cable_ref).map(|c| &c.spec) {
                Some(ComponentSpec::Cable(spec) | ComponentSpec::Drop(spec)) => spec.family.clone(),
                _ => None,
            };
            for fitting in edge.kind.fittings() {
                if !(0.0..=len_ft).contains(&fitting.pos_ft) {
                    errors.push(PlantError::FittingOffEdge {
                        edge: edge.id.clone(),
                        pos_ft: fitting.pos_ft,
                        len_ft,
                    });
                }
                let referrer = format!("edge {} fitting", edge.id);
                let Some(part) = check_class(
                    library,
                    referrer,
                    &fitting.component_ref,
                    &[ComponentClass::Connector],
                    errors,
                ) else {
                    continue;
                };
                if let (ComponentSpec::Connector(connector), Some(cable)) =
                    (&part.spec, &cable_family)
                    && let Some(fits) = &connector.cable_family
                    && fits != cable
                {
                    errors.push(PlantError::FittingWrongCable {
                        edge: edge.id.clone(),
                        component: fitting.component_ref.clone(),
                        connector: fits.clone(),
                        cable: cable.clone(),
                    });
                }
            }
            if let EdgeKind::Span {
                support: Some(support),
                ..
            } = &edge.kind
            {
                check_class(
                    library,
                    format!("edge {} support", edge.id),
                    support,
                    &[ComponentClass::Strand, ComponentClass::Conduit],
                    errors,
                );
            }
        }
    }

    /// Version 2: structures are unique and their parts match, every reference to one exists, and batteries are
    /// batteries.
    fn check_structures(&self, library: &ComponentLibrary, errors: &mut Vec<PlantError>) {
        let mut ids = BTreeSet::new();
        for structure in &self.structures {
            if !ids.insert(&structure.id) {
                errors.push(PlantError::DuplicateStructure(structure.id.clone()));
            }
            let referrer = format!("structure {}", structure.id);
            let part = check_class(
                library,
                referrer,
                &structure.component_ref,
                &[ComponentClass::Structure],
                errors,
            );
            if let Some(ComponentSpec::Structure(spec)) = part.map(|c| &c.spec)
                && spec.kind != structure.kind
            {
                errors.push(PlantError::StructureKindMismatch(structure.id.clone()));
            }
        }
        let mut refer = |referrer: String, structure: &StructureId| {
            if !ids.contains(structure) {
                errors.push(PlantError::UnknownStructure {
                    referrer,
                    structure: structure.clone(),
                });
            }
        };
        for element in &self.elements {
            if let Some(structure) = &element.structure {
                refer(format!("element {}", element.id), structure);
            }
        }
        for supply in &self.power_supplies {
            if let Some(structure) = &supply.structure {
                refer(format!("power supply {}", supply.id), structure);
            }
        }
        for edge in &self.edges {
            for point in edge.kind.path() {
                if let Some(structure) = &point.structure {
                    refer(format!("edge {} path", edge.id), structure);
                }
            }
        }
        for supply in &self.power_supplies {
            let Some(bank) = &supply.batteries else {
                continue;
            };
            if bank.per_string == 0 || bank.strings == 0 {
                errors.push(PlantError::EmptyBatteryBank(supply.id.clone()));
            }
            check_class(
                library,
                format!("power supply {} batteries", supply.id),
                &bank.component_ref,
                &[ComponentClass::Battery],
                errors,
            );
        }
    }

    /// Everything in this plant that version 1 couldn't hold, in canonical order, as `what: where`. Empty for a
    /// version 1 plant. The parts it references are asked too, for version 2 fields set on a version 1 class.
    pub fn version_2_parts(&self, library: &ComponentLibrary) -> Vec<String> {
        let mut found = BTreeSet::new();
        for element in &self.elements {
            if !element.kind.is_version_1() {
                found.insert(format!("kind {:?}: {}", element.kind, element.id));
            }
            if let Some(fitted) = &element.fitted {
                found.insert(format!("fitted {}: {}", fitted.name(), element.id));
            }
            if element.structure.is_some() {
                found.insert(format!("structure: {}", element.id));
            }
        }
        for structure in &self.structures {
            found.insert(format!("structure record: {}", structure.id));
        }
        for edge in &self.edges {
            let id = &edge.id;
            if !edge.kind.fittings().is_empty() {
                found.insert(format!("fittings: {id}"));
            }
            if !edge.kind.path().is_empty() {
                found.insert(format!("path: {id}"));
            }
            if edge.record_status.is_some() {
                found.insert(format!("record status: {id}"));
            }
            match &edge.kind {
                EdgeKind::Span {
                    support: Some(_), ..
                } => {
                    found.insert(format!("support: {id}"));
                }
                EdgeKind::Drop {
                    installation: Some(_),
                    ..
                } => {
                    found.insert(format!("drop installation: {id}"));
                }
                EdgeKind::House { .. } => {
                    found.insert(format!("house cable: {id}"));
                }
                _ => {}
            }
        }
        for supply in &self.power_supplies {
            if supply.batteries.is_some() {
                found.insert(format!("batteries: {}", supply.id));
            }
            if supply.structure.is_some() {
                found.insert(format!("structure: {}", supply.id));
            }
            if supply.record_status.is_some() {
                found.insert(format!("record status: {}", supply.id));
            }
        }
        for line in self.parts_list() {
            let Some(part) = library.get(&line.component_ref) else {
                continue;
            };
            for field in part.version_2_fields() {
                found.insert(format!("part field {field}: {}", part.id));
            }
        }
        found.into_iter().collect()
    }

    /// The parts this plant needs, sorted by component id. Derived, never stored
    /// (invariant 1). Devices, structures, fitted parts, fittings and batteries count by the unit; cable, strand and
    /// conduit by the foot.
    pub fn parts_list(&self) -> Vec<PartsLine> {
        let mut lines: BTreeMap<&ComponentId, (u32, Option<f64>)> = BTreeMap::new();
        let mut units = |component_ref, count: u32| {
            lines.entry(component_ref).or_insert((0, None)).0 += count;
        };
        for element in &self.elements {
            units(&element.component_ref, 1);
            for part in element.fitted.iter().flat_map(Fitted::parts) {
                units(part, 1);
            }
        }
        for supply in &self.power_supplies {
            units(&supply.component_ref, 1);
            if let Some(bank) = &supply.batteries {
                units(
                    &bank.component_ref,
                    u32::from(bank.per_string) * u32::from(bank.strings),
                );
            }
        }
        for structure in &self.structures {
            units(&structure.component_ref, 1);
        }
        for edge in &self.edges {
            for fitting in edge.kind.fittings() {
                units(&fitting.component_ref, 1);
            }
        }
        let mut by_foot = |component_ref, len_ft: f64| {
            let line = lines.entry(component_ref).or_insert((0, None));
            line.0 += 1;
            line.1 = Some(line.1.unwrap_or(0.0) + len_ft);
        };
        for edge in &self.edges {
            if let Some((cable_ref, len_ft)) = edge.kind.cable() {
                by_foot(cable_ref, len_ft);
            }
            if let EdgeKind::Span {
                support: Some(support),
                len_ft,
                ..
            } = &edge.kind
            {
                by_foot(support, *len_ft);
            }
        }
        lines
            .into_iter()
            .map(|(component_ref, (quantity, total_len_ft))| PartsLine {
                component_ref: component_ref.clone(),
                quantity,
                total_len_ft,
            })
            .collect()
    }
}

impl Fitted {
    /// Its variant's name, for messages.
    pub fn name(&self) -> &'static str {
        match self {
            Fitted::Active(_) => "Active",
            Fitted::Passive(_) => "Passive",
            Fitted::GroundBlock(_) => "GroundBlock",
        }
    }

    /// Every part it names, in field order: each counts once in a parts list.
    pub fn parts(&self) -> Vec<&ComponentId> {
        match self {
            Fitted::Active(active) => {
                let mut parts: Vec<&ComponentId> = Vec::new();
                parts.extend(&active.input_pad);
                parts.extend(&active.input_eq);
                for output in &active.outputs {
                    parts.extend(&output.pad);
                    parts.extend(&output.eq);
                }
                parts.extend(&active.return_pad);
                parts.extend(&active.return_eq);
                parts.extend(&active.forward_rx);
                parts.extend(&active.return_tx);
                parts.extend(&active.transceivers);
                parts
            }
            Fitted::Passive(passive) => {
                let mut parts: Vec<&ComponentId> = passive.conditioner.iter().collect();
                for entry in &passive.ports {
                    if let PortState::Terminated { component_ref } = &entry.state {
                        parts.push(component_ref);
                    }
                }
                parts
            }
            Fitted::GroundBlock(_) => Vec::new(),
        }
    }
}

/// Checks that a reference is to a part of one of these classes. Returns the part if it is.
fn check_class<'a>(
    library: &'a ComponentLibrary,
    referrer: String,
    component: &ComponentId,
    expected: &[ComponentClass],
    errors: &mut Vec<PlantError>,
) -> Option<&'a Component> {
    let Some(entry) = library.get(component) else {
        errors.push(PlantError::UnknownComponent {
            referrer,
            component: component.clone(),
        });
        return None;
    };
    if !expected.contains(&entry.class()) {
        errors.push(PlantError::ComponentClassMismatch {
            referrer,
            component: component.clone(),
            expected: expected[0],
            found: entry.class(),
        });
        return None;
    }
    Some(entry)
}

/// The checks on one element's fitted parts.
struct FittedCheck<'a, 'e> {
    element: &'a Element,
    library: &'a ComponentLibrary,
    errors: &'e mut Vec<PlantError>,
}

impl FittedCheck<'_, '_> {
    fn part(
        &mut self,
        slot: &str,
        component: &ComponentId,
        class: ComponentClass,
    ) -> Option<&ComponentSpec> {
        let referrer = format!("element {} {slot}", self.element.id);
        let library = self.library;
        check_class(library, referrer, component, &[class], self.errors).map(|c| &c.spec)
    }

    fn wrong(&mut self, slot: &str, component: &ComponentId) {
        self.errors.push(PlantError::FittedWrongPart {
            element: self.element.id.clone(),
            slot: slot.to_string(),
            component: component.clone(),
        });
    }

    fn plugin(
        &mut self,
        slot: &str,
        component: &ComponentId,
        kinds: &[PlugInKind],
        host: Option<&str>,
    ) {
        let Some(ComponentSpec::PlugIn(plugin)) =
            self.part(slot, component, ComponentClass::PlugIn).cloned()
        else {
            return;
        };
        if !kinds.contains(&plugin.kind) {
            self.wrong(slot, component);
        }
        if host != Some(plugin.family.as_str()) {
            self.errors.push(PlantError::PlugInFamily {
                element: self.element.id.clone(),
                component: component.clone(),
                plugin: plugin.family.clone(),
                host: host.map(str::to_string),
            });
        }
    }

    fn port_problem(&mut self, port: &PortId, problem: &str) {
        self.errors.push(PlantError::FittedPort {
            element: self.element.id.clone(),
            port: port.clone(),
            problem: problem.to_string(),
        });
    }

    fn check_active(&mut self, active: &ActiveFitted, family: Option<&str>) {
        const EQ: &[PlugInKind] = &[PlugInKind::Equalizer, PlugInKind::CableSimulator];
        if let Some(id) = &active.input_pad {
            self.plugin("input_pad", id, &[PlugInKind::Pad], family);
        }
        if let Some(id) = &active.input_eq {
            self.plugin("input_eq", id, EQ, family);
        }
        let mut seen = BTreeSet::new();
        for output in &active.outputs {
            let is_output = self
                .element
                .ports
                .iter()
                .any(|p| p.id == output.port && p.role == PortRole::Output);
            if !is_output {
                self.port_problem(&output.port, "isn't an output");
            }
            if !seen.insert(&output.port) {
                self.port_problem(&output.port, "is fitted twice");
            }
            if let Some(id) = &output.pad {
                self.plugin("output pad", id, &[PlugInKind::Pad], family);
            }
            if let Some(id) = &output.eq {
                self.plugin("output eq", id, EQ, family);
            }
        }
        if let Some(id) = &active.return_pad {
            self.plugin("return_pad", id, &[PlugInKind::ReturnPad], family);
        }
        if let Some(id) = &active.return_eq {
            self.plugin("return_eq", id, &[PlugInKind::ReturnEqualizer], family);
        }
        let is_node = matches!(self.element.kind, ElementKind::Node { .. });
        let is_rpd = matches!(self.element.kind, ElementKind::Rpd { .. });
        if (active.forward_rx.is_some() || active.return_tx.is_some()) && !is_node {
            self.errors.push(PlantError::FittedWrongKind {
                element: self.element.id.clone(),
                what: "analog optics".into(),
            });
        }
        if let Some(id) = &active.forward_rx
            && let Some(ComponentSpec::OpticalRx(rx)) = self
                .part("forward_rx", id, ComponentClass::OpticalRx)
                .cloned()
            && rx.direction != OpticalDirection::Forward
        {
            self.wrong("forward_rx", id);
        }
        if let Some(id) = &active.return_tx
            && let Some(ComponentSpec::OpticalTx(tx)) = self
                .part("return_tx", id, ComponentClass::OpticalTx)
                .cloned()
            && tx.direction != OpticalDirection::Return
        {
            self.wrong("return_tx", id);
        }
        if !active.transceivers.is_empty() && !is_rpd {
            self.errors.push(PlantError::FittedWrongKind {
                element: self.element.id.clone(),
                what: "transceivers".into(),
            });
        }
        for id in &active.transceivers {
            self.part("transceivers", id, ComponentClass::Transceiver);
        }
    }

    fn check_passive(
        &mut self,
        passive: &PassiveFitted,
        family: Option<&str>,
        used: &BTreeSet<(&ElementId, &PortId)>,
    ) {
        if let Some(id) = &passive.conditioner {
            if self.element.kind == ElementKind::Tap {
                self.plugin("conditioner", id, &[PlugInKind::TapConditioner], family);
            } else {
                self.errors.push(PlantError::FittedWrongKind {
                    element: self.element.id.clone(),
                    what: "a tap conditioner".into(),
                });
            }
        }
        let mut seen = BTreeSet::new();
        for entry in &passive.ports {
            match self.element.ports.iter().find(|p| p.id == entry.port) {
                None => self.port_problem(&entry.port, "doesn't exist"),
                Some(port) if port.role == PortRole::Input => {
                    self.port_problem(&entry.port, "is the input")
                }
                Some(_) if used.contains(&(&self.element.id, &entry.port)) => {
                    self.port_problem(&entry.port, "has an edge and a state")
                }
                Some(_) => {}
            }
            if !seen.insert(&entry.port) {
                self.port_problem(&entry.port, "has two states");
            }
            if let PortState::Terminated { component_ref } = &entry.state {
                self.part("port state", component_ref, ComponentClass::Terminator);
            }
        }
    }
}
