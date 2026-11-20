//! The index over a plant's flat lists, and the queries that walk the tree.

use std::collections::{BTreeMap, BTreeSet};

use schema::common::{ComponentId, EdgeId, ElementId, PortId};
use schema::component::{Component, ComponentLibrary, LibraryError};
use schema::fault::Location;
use schema::plant::{Edge, Element, ElementKind, Plant, PlantError, Port, PortRole};
use schema::profile::{Profile, ProfileError};
use thiserror::Error;

/// Why a graph couldn't be built: everything wrong with its three inputs.
#[derive(Error, Debug, Clone, PartialEq)]
#[error("invalid input: library {library:?}, profile {profile:?}, plant {plant:?}")]
pub struct GraphError {
    /// Problems with the component library.
    pub library: Vec<LibraryError>,
    /// Problems with the profile.
    pub profile: Vec<ProfileError>,
    /// Problems with the plant.
    pub plant: Vec<PlantError>,
}

/// A query named something the plant doesn't have.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum QueryError {
    /// No element has this id.
    #[error("unknown element {0}")]
    UnknownElement(ElementId),
    /// No edge has this id.
    #[error("unknown edge {0}")]
    UnknownEdge(EdgeId),
    /// The element has no such port.
    #[error("element {element} has no port {port}")]
    UnknownPort {
        /// The element.
        element: ElementId,
        /// The missing port.
        port: PortId,
    },
    /// A point along an edge with no cable.
    #[error("edge {0} is direct, so it has no points along it")]
    NoCable(EdgeId),
    /// A point past either end of an edge.
    #[error("{pos_ft} ft is off edge {edge}, which is {len_ft} ft long")]
    PositionOffEdge {
        /// The edge.
        edge: EdgeId,
        /// The position asked for, feet.
        pos_ft: f64,
        /// The edge's length, feet.
        len_ft: f64,
    },
    /// `common_ancestor` was given nothing to compare.
    #[error("common_ancestor needs at least one element")]
    NoElements,
}

/// Where a resolved `Location` sits in the tree.
#[derive(Clone, Copy)]
pub(crate) enum Anchor<'a> {
    Port(&'a Element, &'a Port),
    Point(&'a Edge, f64),
}

/// A read-only index over one valid plant. It's built from the plant, never stored
/// (invariant 1), and results don't depend on how the file orders its lists.
#[derive(Debug, Clone)]
pub struct PlantGraph<'a> {
    plant: &'a Plant,
    library: &'a ComponentLibrary,
    profile: &'a Profile,
    root: &'a Element,
    elements: BTreeMap<&'a ElementId, &'a Element>,
    edges: BTreeMap<&'a EdgeId, &'a Edge>,
    components: BTreeMap<&'a ComponentId, &'a Component>,
    parent_edge: BTreeMap<&'a ElementId, &'a Edge>,
    child_edges: BTreeMap<&'a ElementId, Vec<&'a Edge>>,
}

impl<'a> PlantGraph<'a> {
    /// Validates all three inputs and indexes the plant.
    pub fn new(
        plant: &'a Plant,
        library: &'a ComponentLibrary,
        profile: &'a Profile,
    ) -> Result<Self, GraphError> {
        let library_errors = library.validate().err().unwrap_or_default();
        let profile_errors = profile.validate().err().unwrap_or_default();
        let plant_errors = plant.validate(library, profile).err().unwrap_or_default();
        let root = plant.root().filter(|_| plant_errors.is_empty());
        let (Some(root), true, true) = (root, library_errors.is_empty(), profile_errors.is_empty())
        else {
            return Err(GraphError {
                library: library_errors,
                profile: profile_errors,
                plant: plant_errors,
            });
        };

        let mut parent_edge = BTreeMap::new();
        let mut child_edges: BTreeMap<_, Vec<&Edge>> = BTreeMap::new();
        for edge in &plant.edges {
            parent_edge.insert(&edge.to.element_id, edge);
            child_edges
                .entry(&edge.from.element_id)
                .or_default()
                .push(edge);
        }
        for children in child_edges.values_mut() {
            children.sort_by(|a, b| a.from.port.cmp(&b.from.port));
        }

        Ok(Self {
            plant,
            library,
            profile,
            root,
            elements: plant.elements.iter().map(|e| (&e.id, e)).collect(),
            edges: plant.edges.iter().map(|e| (&e.id, e)).collect(),
            components: library.components.iter().map(|c| (&c.id, c)).collect(),
            parent_edge,
            child_edges,
        })
    }

    /// The plant this graph indexes.
    pub fn plant(&self) -> &'a Plant {
        self.plant
    }

    /// The component library it was validated against.
    pub fn library(&self) -> &'a ComponentLibrary {
        self.library
    }

    /// The profile it was validated against.
    pub fn profile(&self) -> &'a Profile {
        self.profile
    }

    /// The one element with no parent.
    pub fn root(&self) -> &'a Element {
        self.root
    }

    /// Finds an element by id.
    pub fn element(&self, id: &ElementId) -> Result<&'a Element, QueryError> {
        let found = self.elements.get(id).copied();
        found.ok_or_else(|| QueryError::UnknownElement(id.clone()))
    }

    /// Finds an edge by id.
    pub fn edge(&self, id: &EdgeId) -> Result<&'a Edge, QueryError> {
        let found = self.edges.get(id).copied();
        found.ok_or_else(|| QueryError::UnknownEdge(id.clone()))
    }

    /// Finds a library entry by id. Validation has checked every id in the plant.
    pub fn component(&self, id: &ComponentId) -> Option<&'a Component> {
        self.components.get(id).copied()
    }

    /// The parent element and the edge that reaches it, or `None` for the root.
    pub fn parent(&self, id: &ElementId) -> Result<Option<(&'a Element, &'a Edge)>, QueryError> {
        self.element(id)?;
        match self.parent_edge.get(id).copied() {
            Some(edge) => Ok(Some((self.element(&edge.from.element_id)?, edge))),
            None => Ok(None),
        }
    }

    /// The edges leaving an element, by the id of the port they leave.
    pub fn child_edges(&self, id: &ElementId) -> &[&'a Edge] {
        self.child_edges.get(id).map_or(&[], Vec::as_slice)
    }

    /// The elements directly below this one, by the id of the port they hang from.
    pub fn children(&self, id: &ElementId) -> Result<Vec<&'a Element>, QueryError> {
        self.element(id)?;
        let edges = self.child_edges(id);
        edges
            .iter()
            .map(|edge| self.element(&edge.to.element_id))
            .collect()
    }

    /// The elements above this one, from its parent up to the root.
    pub fn ancestors(&self, id: &ElementId) -> Result<Vec<&'a Element>, QueryError> {
        let mut ancestors = Vec::new();
        let mut current = self.parent(id)?;
        while let Some((element, _)) = current {
            ancestors.push(element);
            current = self.parent(&element.id)?;
        }
        Ok(ancestors)
    }

    /// Every modem, by id.
    pub fn modems(&self) -> Vec<&'a Element> {
        self.elements_where(|kind| matches!(kind, ElementKind::Modem))
    }

    /// Every amp, by id.
    pub fn amps(&self) -> Vec<&'a Element> {
        self.elements_where(|kind| matches!(kind, ElementKind::Amp { .. }))
    }

    /// Every active (node, RPD or amp), by id. These are what draw power (§6.1).
    pub fn actives(&self) -> Vec<&'a Element> {
        self.elements_where(is_active)
    }

    fn elements_where(&self, keep: impl Fn(&ElementKind) -> bool) -> Vec<&'a Element> {
        self.elements
            .values()
            .copied()
            .filter(|e| keep(&e.kind))
            .collect()
    }

    /// The amps between the root and an element, nearest the root first, not counting the root
    /// or the element itself. A modem with a cascade of length 2 is N+2.
    pub fn cascade(&self, id: &ElementId) -> Result<Vec<&'a Element>, QueryError> {
        let mut amps = self.ancestors(id)?;
        amps.retain(|e| matches!(e.kind, ElementKind::Amp { .. }));
        amps.reverse();
        Ok(amps)
    }

    /// The root's RF port an element hangs from, or `None` for the root. Upstream noise
    /// funnels per port (§7.3).
    pub fn root_port(&self, id: &ElementId) -> Result<Option<&'a Port>, QueryError> {
        let mut top = None;
        let mut current = self.parent(id)?;
        while let Some((element, edge)) = current {
            top = Some((element, edge));
            current = self.parent(&element.id)?;
        }
        top.map(|(element, edge)| self.port(element, &edge.from.port))
            .transpose()
    }

    /// The elements downstream of a location, by id: the reach of downstream ingress (§8). For
    /// a point on an edge, that's the edge's far end and everything under it. For an `Input`
    /// port, the element and everything under it. For any other port, what hangs from it.
    pub fn below(&self, location: &Location) -> Result<Vec<&'a Element>, QueryError> {
        let mut starts = Vec::new();
        match self.resolve(location)? {
            Anchor::Point(edge, _) => starts.push(&edge.to.element_id),
            Anchor::Port(element, port) if port.role == PortRole::Input => starts.push(&element.id),
            Anchor::Port(element, port) => {
                let edges = self.child_edges(&element.id);
                let hanging = edges.iter().filter(|edge| edge.from.port == port.id);
                starts.extend(hanging.map(|edge| &edge.to.element_id));
            }
        }
        let mut found = BTreeMap::new();
        while let Some(id) = starts.pop() {
            found.insert(id, self.element(id)?);
            let edges = self.child_edges(id);
            starts.extend(edges.iter().map(|edge| &edge.to.element_id));
        }
        Ok(found.into_values().collect())
    }

    /// The modems downstream of a location, by id.
    pub fn modems_below(&self, location: &Location) -> Result<Vec<&'a Element>, QueryError> {
        let mut below = self.below(location)?;
        below.retain(|e| matches!(e.kind, ElementKind::Modem));
        Ok(below)
    }

    /// The amps downstream of a location, by id.
    pub fn amps_below(&self, location: &Location) -> Result<Vec<&'a Element>, QueryError> {
        let mut below = self.below(location)?;
        below.retain(|e| matches!(e.kind, ElementKind::Amp { .. }));
        Ok(below)
    }

    /// The deepest element above or equal to all of these.
    pub fn common_ancestor(&self, ids: &[ElementId]) -> Result<&'a Element, QueryError> {
        let (first, rest) = ids.split_first().ok_or(QueryError::NoElements)?;
        let mut candidates = vec![self.element(first)?];
        candidates.extend(self.ancestors(first)?);
        for id in rest {
            let mut line: BTreeSet<&ElementId> = BTreeSet::from([&self.element(id)?.id]);
            line.extend(self.ancestors(id)?.iter().map(|e| &e.id));
            candidates.retain(|e| line.contains(&e.id));
        }
        // The root is above everything, so this only falls back for a graph that can't exist.
        Ok(candidates.first().copied().unwrap_or(self.root))
    }

    pub(crate) fn port(&self, element: &'a Element, port: &PortId) -> Result<&'a Port, QueryError> {
        let found = element.ports.iter().find(|p| &p.id == port);
        found.ok_or_else(|| QueryError::UnknownPort {
            element: element.id.clone(),
            port: port.clone(),
        })
    }

    pub(crate) fn resolve(&self, location: &Location) -> Result<Anchor<'a>, QueryError> {
        match location {
            Location::Port { element_id, port } => {
                let element = self.element(element_id)?;
                Ok(Anchor::Port(element, self.port(element, port)?))
            }
            Location::Span { span_id, pos_ft } => {
                let edge = self.edge(span_id)?;
                let len_ft =
                    edge_len_ft(edge).ok_or_else(|| QueryError::NoCable(span_id.clone()))?;
                if !(0.0..=len_ft).contains(pos_ft) {
                    return Err(QueryError::PositionOffEdge {
                        edge: span_id.clone(),
                        pos_ft: *pos_ft,
                        len_ft,
                    });
                }
                Ok(Anchor::Point(edge, *pos_ft))
            }
        }
    }
}

/// The cable length of an edge, or `None` for one with no cable.
pub(crate) fn edge_len_ft(edge: &Edge) -> Option<f64> {
    edge.kind.cable().map(|(_, len_ft)| len_ft)
}

pub(crate) fn is_active(kind: &ElementKind) -> bool {
    matches!(
        kind,
        ElementKind::Node { .. } | ElementKind::Rpd { .. } | ElementKind::Amp { .. }
    )
}
