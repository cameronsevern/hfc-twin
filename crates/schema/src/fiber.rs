//! The fiber plant: one per region, from the CMTS port in the hub to where light meets each node. Stored plant, with a
//! record status on everything, so a fiber part can be faulted at a location (`docs/as-built-plan.md`, rule 8). See
//! `docs/tasks/21-as-built-schemas.md` and `docs/reference/fiber-optics.md`.
//!
//! A link is a path of hops. Its budget is a sum over them, and a cut on a cable reaches every link with a hop on it,
//! so one cut can take several nodes.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::common::{
    ComponentId, FiberCableId, FiberElementId, HubId, LinkId, LonLat, Origin, PortId,
    ServiceGroupId, StructureId,
};
use crate::component::{ComponentClass, ComponentLibrary, ComponentSpec, OpticalDirection};
use crate::plant::{Installation, PartsLine, RecordStatus, RoutePoint, Structure};

/// Version of this schema.
pub const SCHEMA_VERSION: &str = "1";

/// One region's fiber plant.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FiberPlant {
    /// Version of this schema.
    pub schema_version: String,
    /// Stable id, which a region's `fiber_ref` names.
    pub id: String,
    /// Whether it's simulated or real (invariant 11).
    pub origin: Origin,
    /// Hubs.
    pub hubs: Vec<Hub>,
    /// Everything that isn't cable.
    pub elements: Vec<FiberElement>,
    /// Cables, between two elements.
    pub cables: Vec<FiberCable>,
    /// Optical links, each a path of hops.
    pub links: Vec<OpticalLink>,
    /// Which CMTS port serves each service group.
    pub bindings: Vec<ServiceGroupBinding>,
    /// Poles, pedestals, vaults and cabinets the fiber plant uses.
    pub structures: Vec<Structure>,
}

/// A hub or headend.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Hub {
    /// Stable id.
    pub id: HubId,
    /// Where it is.
    pub geo: LonLat,
    /// Where this record came from.
    pub record_status: RecordStatus,
}

/// The kinds of fiber plant element.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
pub enum FiberElementKind {
    /// A CMTS or Remote PHY core.
    Cmts,
    /// An analog optical transmitter.
    OpticalTx,
    /// An analog optical receiver.
    OpticalRx,
    /// An optical amplifier.
    OpticalAmp,
    /// A splitter, multiplexer or demultiplexer.
    OpticalPassive,
    /// A switch in the converged interconnect network.
    Switch,
    /// A pluggable digital optic in the hub.
    Transceiver,
    /// A timing source.
    TimingSource,
    /// A patch panel.
    PatchPanel,
    /// A splice closure.
    SpliceClosure,
    /// Where a link meets one node's coax plant. Its part is the node's or RPD's, and its optics are fitted on the
    /// plant's root.
    NodeEnd {
        /// The node's service group.
        service_group: ServiceGroupId,
    },
}

impl FiberElementKind {
    /// The component classes an element of this kind may reference.
    pub fn component_classes(&self) -> &'static [ComponentClass] {
        match self {
            FiberElementKind::Cmts => &[ComponentClass::Cmts],
            FiberElementKind::OpticalTx => &[ComponentClass::OpticalTx],
            FiberElementKind::OpticalRx => &[ComponentClass::OpticalRx],
            FiberElementKind::OpticalAmp => &[ComponentClass::OpticalAmp],
            FiberElementKind::OpticalPassive => &[ComponentClass::OpticalPassive],
            FiberElementKind::Switch => &[ComponentClass::Switch],
            FiberElementKind::Transceiver => &[ComponentClass::Transceiver],
            FiberElementKind::TimingSource => &[ComponentClass::TimingSource],
            FiberElementKind::PatchPanel => &[ComponentClass::PatchPanel],
            FiberElementKind::SpliceClosure => &[ComponentClass::SpliceClosure],
            FiberElementKind::NodeEnd { .. } => &[ComponentClass::Node, ComponentClass::Rpd],
        }
    }

    /// Whether a link in this direction can start here.
    fn transmits(&self, direction: OpticalDirection) -> bool {
        match self {
            FiberElementKind::OpticalTx | FiberElementKind::Transceiver => true,
            FiberElementKind::NodeEnd { .. } => direction != OpticalDirection::Forward,
            _ => false,
        }
    }

    /// Whether a link in this direction can end here.
    fn receives(&self, direction: OpticalDirection) -> bool {
        match self {
            FiberElementKind::OpticalRx | FiberElementKind::Transceiver => true,
            FiberElementKind::NodeEnd { .. } => direction != OpticalDirection::Return,
            _ => false,
        }
    }
}

/// Anything in the fiber plant that isn't cable.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FiberElement {
    /// Stable id.
    pub id: FiberElementId,
    /// What it is.
    pub kind: FiberElementKind,
    /// The part it's built from.
    pub component_ref: ComponentId,
    /// Where it is, if known.
    pub geo: Option<LonLat>,
    /// The hub it's in, if it's in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub: Option<HubId>,
    /// The structure it's on or in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<StructureId>,
    /// The ports a link passes through or a binding names. An element no link passes through needs none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortId>,
    /// Where this record came from.
    pub record_status: RecordStatus,
}

/// A fiber cable between two elements.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FiberCable {
    /// Stable id.
    pub id: FiberCableId,
    /// The cable, a `FiberCable` part, which gives its fiber count.
    pub component_ref: ComponentId,
    /// One end.
    pub from: FiberElementId,
    /// The other end.
    pub to: FiberElementId,
    /// Sheath length, feet, slack included.
    pub len_ft: f64,
    /// How it's installed.
    pub installation: Installation,
    /// The points it passes between its ends, in order from `from`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<RoutePoint>,
    /// The strand it's lashed to or the conduit it's in, by the foot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support: Option<ComponentId>,
    /// Where this record came from.
    pub record_status: RecordStatus,
}

/// One optical link: a wavelength from a transmitter to a receiver.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpticalLink {
    /// Stable id.
    pub id: LinkId,
    /// The service group it serves.
    pub service_group: ServiceGroupId,
    /// Which way it carries signal.
    pub direction: OpticalDirection,
    /// Wavelength, nm.
    pub wavelength_nm: f64,
    /// Its path, in order from the transmitting end.
    pub hops: Vec<Hop>,
}

/// One step of a link's path.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum Hop {
    /// Along one strand of a cable, from the end the link is at to the other.
    Fiber {
        /// The cable.
        cable: FiberCableId,
        /// The strand, numbered from 1.
        strand: u32,
    },
    /// A fusion or mechanical splice joining two strands, in a closure or a panel.
    Splice {
        /// Where.
        at: FiberElementId,
        /// Its measured loss, dB, if known. `None` takes the design allowance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        loss_db: Option<f64>,
    },
    /// A mated connector pair, at an element.
    Connector {
        /// Where.
        at: FiberElementId,
        /// The connector, a `FiberConnector`.
        component_ref: ComponentId,
    },
    /// Through an element, from one of its ports to another.
    Through {
        /// The element.
        element: FiberElementId,
        /// Where light goes in.
        port_in: PortId,
        /// Where it comes out.
        port_out: PortId,
    },
}

impl Hop {
    /// The element a hop is at, or `None` for a fiber.
    pub fn element(&self) -> Option<&FiberElementId> {
        match self {
            Hop::Fiber { .. } => None,
            Hop::Splice { at, .. } | Hop::Connector { at, .. } => Some(at),
            Hop::Through { element, .. } => Some(element),
        }
    }
}

/// Which CMTS port serves a service group.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ServiceGroupBinding {
    /// The service group.
    pub service_group: ServiceGroupId,
    /// The CMTS, a `Cmts` element.
    pub cmts: FiberElementId,
    /// The port on it.
    pub port: PortId,
}

/// A problem with a fiber plant.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum FiberError {
    /// Two things of one kind share an id.
    #[error("duplicate id {0}")]
    DuplicateId(String),
    /// A reference to something that isn't in the plant.
    #[error("{referrer} refers to unknown {what} {id}")]
    Unknown {
        /// What holds the reference.
        referrer: String,
        /// What kind of thing is missing.
        what: &'static str,
        /// Its id.
        id: String,
    },
    /// A referenced part isn't in the library.
    #[error("{referrer} refers to unknown component {component}")]
    UnknownComponent {
        /// What holds the reference.
        referrer: String,
        /// The part.
        component: ComponentId,
    },
    /// A referenced part is the wrong class.
    #[error("{referrer} can't use {component}, a {found:?}")]
    ComponentClassMismatch {
        /// What holds the reference.
        referrer: String,
        /// The part.
        component: ComponentId,
        /// Its class.
        found: ComponentClass,
    },
    /// A cable isn't longer than zero.
    #[error("cable {cable} has length {len_ft} ft")]
    NonPositiveLength {
        /// The cable.
        cable: FiberCableId,
        /// Its length, feet.
        len_ft: f64,
    },
    /// A link's hops don't join up.
    #[error("link {link}, hop {hop}: {problem}")]
    NotContiguous {
        /// The link.
        link: LinkId,
        /// The hop's index, from 0.
        hop: usize,
        /// What's wrong.
        problem: String,
    },
    /// A link doesn't start at something that transmits in its direction.
    #[error("link {link} starts at {element}, which doesn't transmit that way")]
    BadStart {
        /// The link.
        link: LinkId,
        /// Where it starts.
        element: String,
    },
    /// A link doesn't end at something that receives in its direction.
    #[error("link {link} ends at {element}, which doesn't receive that way")]
    BadEnd {
        /// The link.
        link: LinkId,
        /// Where it ends.
        element: String,
    },
    /// A link's node end serves another service group.
    #[error("link {link} serves {link_group} but its node end is {node_group}")]
    WrongNode {
        /// The link.
        link: LinkId,
        /// The link's service group.
        link_group: ServiceGroupId,
        /// The node end's.
        node_group: ServiceGroupId,
    },
    /// A strand number past the cable's count.
    #[error("link {link}: strand {strand} of cable {cable}, which has {count}")]
    StrandOutOfRange {
        /// The link.
        link: LinkId,
        /// The cable.
        cable: FiberCableId,
        /// The strand.
        strand: u32,
        /// The cable's fiber count.
        count: u32,
    },
    /// Two links on one strand, one direction and one wavelength.
    #[error("links {first} and {second} share strand {strand} of {cable} at {wavelength_nm} nm")]
    StrandShared {
        /// One link.
        first: LinkId,
        /// The other.
        second: LinkId,
        /// The cable.
        cable: FiberCableId,
        /// The strand.
        strand: u32,
        /// The wavelength, nm.
        wavelength_nm: f64,
    },
    /// A port a hop or binding names isn't one of the element's.
    #[error("{referrer} names port {port} on {element}, which has no such port")]
    UnknownPort {
        /// What names it.
        referrer: String,
        /// The element.
        element: FiberElementId,
        /// The port.
        port: PortId,
    },
    /// A splice somewhere splices can't be made.
    #[error("link {link}: splice at {element}, which isn't a closure or panel")]
    SpliceNotInClosure {
        /// The link.
        link: LinkId,
        /// Where.
        element: FiberElementId,
    },
    /// A binding to something that isn't a CMTS.
    #[error("binding for {0} isn't to a CMTS")]
    BindingNotCmts(ServiceGroupId),
}

impl FiberPlant {
    /// Finds an element by id.
    pub fn element(&self, id: &FiberElementId) -> Option<&FiberElement> {
        self.elements.iter().find(|e| &e.id == id)
    }

    /// Finds a cable by id.
    pub fn cable(&self, id: &FiberCableId) -> Option<&FiberCable> {
        self.cables.iter().find(|c| &c.id == id)
    }

    /// The node end for a service group, if there is one.
    pub fn node_end(&self, service_group: &ServiceGroupId) -> Option<&FiberElement> {
        self.elements.iter().find(
            |e| matches!(&e.kind, FiberElementKind::NodeEnd { service_group: g } if g == service_group),
        )
    }

    /// Checks the fiber plant is well formed against this library. Reports every problem found.
    pub fn validate(&self, library: &ComponentLibrary) -> Result<(), Vec<FiberError>> {
        let mut errors = Vec::new();
        self.check_ids(&mut errors);
        self.check_references(library, &mut errors);
        for link in &self.links {
            self.check_link(link, library, &mut errors);
        }
        self.check_strands(&mut errors);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn check_ids(&self, errors: &mut Vec<FiberError>) {
        let mut check = |ids: Vec<&str>| {
            let mut seen = BTreeSet::new();
            for id in ids {
                if !seen.insert(id) {
                    errors.push(FiberError::DuplicateId(id.to_string()));
                }
            }
        };
        check(self.hubs.iter().map(|h| h.id.as_str()).collect());
        check(self.elements.iter().map(|e| e.id.as_str()).collect());
        check(self.cables.iter().map(|c| c.id.as_str()).collect());
        check(self.links.iter().map(|l| l.id.as_str()).collect());
        check(self.structures.iter().map(|s| s.id.as_str()).collect());
    }

    fn check_references(&self, library: &ComponentLibrary, errors: &mut Vec<FiberError>) {
        let hubs: BTreeSet<&HubId> = self.hubs.iter().map(|h| &h.id).collect();
        let structures: BTreeSet<&StructureId> = self.structures.iter().map(|s| &s.id).collect();
        let unknown = |referrer: String, what: &'static str, id: &str| FiberError::Unknown {
            referrer,
            what,
            id: id.to_string(),
        };
        for element in &self.elements {
            let referrer = format!("element {}", element.id);
            if let Some(hub) = &element.hub
                && !hubs.contains(hub)
            {
                errors.push(unknown(referrer.clone(), "hub", hub.as_str()));
            }
            if let Some(structure) = &element.structure
                && !structures.contains(structure)
            {
                errors.push(unknown(referrer, "structure", structure.as_str()));
            }
        }
        for cable in &self.cables {
            let referrer = format!("cable {}", cable.id);
            for end in [&cable.from, &cable.to] {
                if self.element(end).is_none() {
                    errors.push(unknown(referrer.clone(), "element", end.as_str()));
                }
            }
            for point in &cable.path {
                if let Some(structure) = &point.structure
                    && !structures.contains(structure)
                {
                    errors.push(unknown(referrer.clone(), "structure", structure.as_str()));
                }
            }
        }
        for binding in &self.bindings {
            let referrer = format!("binding for {}", binding.service_group);
            match self.element(&binding.cmts) {
                None => errors.push(unknown(referrer, "element", binding.cmts.as_str())),
                Some(element) if element.kind != FiberElementKind::Cmts => {
                    errors.push(FiberError::BindingNotCmts(binding.service_group.clone()));
                }
                Some(element) if !element.ports.contains(&binding.port) => {
                    errors.push(FiberError::UnknownPort {
                        referrer,
                        element: element.id.clone(),
                        port: binding.port.clone(),
                    });
                }
                Some(_) => {}
            }
        }

        let mut class =
            |referrer: String, component: &ComponentId, allowed: &[ComponentClass]| match library
                .get(component)
            {
                None => errors.push(FiberError::UnknownComponent {
                    referrer,
                    component: component.clone(),
                }),
                Some(part) if !allowed.contains(&part.class()) => {
                    errors.push(FiberError::ComponentClassMismatch {
                        referrer,
                        component: component.clone(),
                        found: part.class(),
                    });
                }
                Some(_) => {}
            };
        for element in &self.elements {
            class(
                format!("element {}", element.id),
                &element.component_ref,
                element.kind.component_classes(),
            );
        }
        for cable in &self.cables {
            class(
                format!("cable {}", cable.id),
                &cable.component_ref,
                &[ComponentClass::FiberCable],
            );
            if let Some(support) = &cable.support {
                class(
                    format!("cable {} support", cable.id),
                    support,
                    &[ComponentClass::Strand, ComponentClass::Conduit],
                );
            }
        }
        for structure in &self.structures {
            class(
                format!("structure {}", structure.id),
                &structure.component_ref,
                &[ComponentClass::Structure],
            );
        }
        for cable in &self.cables {
            if cable.len_ft <= 0.0 {
                errors.push(FiberError::NonPositiveLength {
                    cable: cable.id.clone(),
                    len_ft: cable.len_ft,
                });
            }
        }
    }

    /// Hops join up end to end, a link starts at something that transmits and ends at something that receives,
    /// strands are within their cable, splices are where splices go, and ports exist.
    fn check_link(
        &self,
        link: &OpticalLink,
        library: &ComponentLibrary,
        errors: &mut Vec<FiberError>,
    ) {
        let not_contiguous = |hop: usize, problem: String| FiberError::NotContiguous {
            link: link.id.clone(),
            hop,
            problem,
        };
        let mut at: Option<&FiberElementId> = None;
        let mut previous_was_fiber = false;
        let mut start: Option<&FiberElementId> = None;
        for (index, hop) in link.hops.iter().enumerate() {
            match hop {
                Hop::Fiber { cable, strand } => {
                    let Some(found) = self.cable(cable) else {
                        errors.push(FiberError::Unknown {
                            referrer: format!("link {}", link.id),
                            what: "cable",
                            id: cable.to_string(),
                        });
                        return;
                    };
                    if previous_was_fiber {
                        errors.push(not_contiguous(
                            index,
                            "two fibers with no splice or connector between".into(),
                        ));
                    }
                    let count = match library.get(&found.component_ref).map(|c| &c.spec) {
                        Some(ComponentSpec::FiberCable(spec)) => spec.fiber_count,
                        _ => 0,
                    };
                    if *strand == 0 || *strand > count {
                        errors.push(FiberError::StrandOutOfRange {
                            link: link.id.clone(),
                            cable: cable.clone(),
                            strand: *strand,
                            count,
                        });
                    }
                    at = match at {
                        None => {
                            errors.push(not_contiguous(
                                index,
                                "a link starts at an element, not a fiber".into(),
                            ));
                            return;
                        }
                        Some(here) if here == &found.from => Some(&found.to),
                        Some(here) if here == &found.to => Some(&found.from),
                        Some(here) => {
                            errors.push(not_contiguous(
                                index,
                                format!("cable {cable} doesn't reach {here}"),
                            ));
                            return;
                        }
                    };
                    previous_was_fiber = true;
                }
                hop => {
                    let Some(element_id) = hop.element() else {
                        continue;
                    };
                    let Some(element) = self.element(element_id) else {
                        errors.push(FiberError::Unknown {
                            referrer: format!("link {}", link.id),
                            what: "element",
                            id: element_id.to_string(),
                        });
                        return;
                    };
                    match at {
                        None => {
                            at = Some(element_id);
                            start = Some(element_id);
                        }
                        Some(here) if here != element_id => {
                            errors.push(not_contiguous(
                                index,
                                format!("at {element_id}, but the link is at {here}"),
                            ));
                            return;
                        }
                        Some(_) => {}
                    }
                    match hop {
                        Hop::Splice { .. } => {
                            if !matches!(
                                element.kind,
                                FiberElementKind::SpliceClosure | FiberElementKind::PatchPanel
                            ) {
                                errors.push(FiberError::SpliceNotInClosure {
                                    link: link.id.clone(),
                                    element: element_id.clone(),
                                });
                            }
                        }
                        Hop::Connector { component_ref, .. } => match library.get(component_ref) {
                            None => errors.push(FiberError::UnknownComponent {
                                referrer: format!("link {}", link.id),
                                component: component_ref.clone(),
                            }),
                            Some(part) if part.class() != ComponentClass::FiberConnector => {
                                errors.push(FiberError::ComponentClassMismatch {
                                    referrer: format!("link {}", link.id),
                                    component: component_ref.clone(),
                                    found: part.class(),
                                });
                            }
                            Some(_) => {}
                        },
                        Hop::Through {
                            port_in, port_out, ..
                        } => {
                            for port in [port_in, port_out] {
                                if !element.ports.contains(port) {
                                    errors.push(FiberError::UnknownPort {
                                        referrer: format!("link {}", link.id),
                                        element: element_id.clone(),
                                        port: port.clone(),
                                    });
                                }
                            }
                        }
                        Hop::Fiber { .. } => {}
                    }
                    previous_was_fiber = false;
                }
            }
        }
        let kind_of =
            |id: Option<&FiberElementId>| id.and_then(|id| self.element(id)).map(|e| &e.kind);
        let name =
            |id: Option<&FiberElementId>| id.map_or("nothing".to_string(), ToString::to_string);
        if !kind_of(start).is_some_and(|k| k.transmits(link.direction)) {
            errors.push(FiberError::BadStart {
                link: link.id.clone(),
                element: name(start),
            });
        }
        if !kind_of(at).is_some_and(|k| k.receives(link.direction)) {
            errors.push(FiberError::BadEnd {
                link: link.id.clone(),
                element: name(at),
            });
        }
        for end in [start, at] {
            if let Some(FiberElementKind::NodeEnd { service_group }) = kind_of(end)
                && service_group != &link.service_group
            {
                errors.push(FiberError::WrongNode {
                    link: link.id.clone(),
                    link_group: link.service_group.clone(),
                    node_group: service_group.clone(),
                });
            }
        }
    }

    /// No two links on one strand in one direction at one wavelength. A bidirectional link holds both directions.
    fn check_strands(&self, errors: &mut Vec<FiberError>) {
        let mut used: BTreeMap<(&FiberCableId, u32, OpticalDirection, u64), &LinkId> =
            BTreeMap::new();
        for link in &self.links {
            let directions: &[OpticalDirection] = match link.direction {
                OpticalDirection::Both => &[OpticalDirection::Forward, OpticalDirection::Return],
                OpticalDirection::Forward => &[OpticalDirection::Forward],
                OpticalDirection::Return => &[OpticalDirection::Return],
            };
            for hop in &link.hops {
                let Hop::Fiber { cable, strand } = hop else {
                    continue;
                };
                for direction in directions {
                    let key = (cable, *strand, *direction, link.wavelength_nm.to_bits());
                    if let Some(first) = used.insert(key, &link.id)
                        && first != &link.id
                    {
                        errors.push(FiberError::StrandShared {
                            first: first.clone(),
                            second: link.id.clone(),
                            cable: cable.clone(),
                            strand: *strand,
                            wavelength_nm: link.wavelength_nm,
                        });
                    }
                }
            }
        }
    }

    /// The parts this fiber plant needs, sorted by component id. Derived, never stored (invariant 1). Elements,
    /// structures and connectors count by the unit, and a connector two links share (at one element on one strand)
    /// counts once. Cable, strand and conduit count by the foot. A node end's part is the node's, which its own plant
    /// counts. Splices have no part; [`FiberPlant::splices`] counts them.
    pub fn parts_list(&self) -> Vec<PartsLine> {
        let mut lines: BTreeMap<&ComponentId, (u32, Option<f64>)> = BTreeMap::new();
        let mut units = |component_ref, count: u32| {
            lines.entry(component_ref).or_insert((0, None)).0 += count;
        };
        for element in &self.elements {
            if !matches!(element.kind, FiberElementKind::NodeEnd { .. }) {
                units(&element.component_ref, 1);
            }
        }
        for structure in &self.structures {
            units(&structure.component_ref, 1);
        }
        let mut connectors = BTreeSet::new();
        for link in &self.links {
            for (index, hop) in link.hops.iter().enumerate() {
                if let Hop::Connector { at, component_ref } = hop {
                    connectors.insert((at, adjacent_strand(&link.hops, index), component_ref));
                }
            }
        }
        for (_, _, component_ref) in &connectors {
            units(component_ref, 1);
        }
        for cable in &self.cables {
            for (component_ref, len_ft) in std::iter::once(&cable.component_ref)
                .chain(&cable.support)
                .map(|c| (c, cable.len_ft))
            {
                let line = lines.entry(component_ref).or_insert((0, None));
                line.0 += 1;
                line.1 = Some(line.1.unwrap_or(0.0) + len_ft);
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

    /// Splices, counted once each: one per pair of strands joined at an element, however many links cross it.
    pub fn splices(&self) -> usize {
        let mut splices = BTreeSet::new();
        for link in &self.links {
            for (index, hop) in link.hops.iter().enumerate() {
                if let Hop::Splice { at, .. } = hop {
                    let before = index.checked_sub(1).and_then(|i| fiber_hop(&link.hops, i));
                    let after = fiber_hop(&link.hops, index + 1);
                    let mut pair = [before, after];
                    pair.sort();
                    splices.insert((at, pair));
                }
            }
        }
        splices.len()
    }
}

/// The (cable, strand) of a fiber hop at this index.
fn fiber_hop(hops: &[Hop], index: usize) -> Option<(&FiberCableId, u32)> {
    match hops.get(index) {
        Some(Hop::Fiber { cable, strand }) => Some((cable, *strand)),
        _ => None,
    }
}

/// The strand a connector terminates: the fiber after it, else the one before.
fn adjacent_strand(hops: &[Hop], index: usize) -> Option<(&FiberCableId, u32)> {
    fiber_hop(hops, index + 1).or_else(|| index.checked_sub(1).and_then(|i| fiber_hop(hops, i)))
}
