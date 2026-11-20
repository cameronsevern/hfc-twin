//! The fiber plant as a graph, and the region-level check that ties it to each node's plant
//! (`docs/tasks/21-as-built-schemas.md`, "Readers"). Like the plant graph, it never computes a dB.

use std::collections::{BTreeMap, BTreeSet};

use schema::common::{FiberCableId, LinkId, ServiceGroupId};
use schema::component::{ComponentLibrary, OpticalDirection};
use schema::fiber::{FiberError, FiberPlant, Hop, OpticalLink};
use schema::plant::Plant;
use schema::region::Region;
use thiserror::Error;

/// A fiber query named something the fiber plant doesn't have.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum FiberQueryError {
    /// No link has this id.
    #[error("unknown link {0}")]
    UnknownLink(LinkId),
    /// No cable has this id.
    #[error("unknown cable {0}")]
    UnknownCable(FiberCableId),
}

/// A read-only index over one valid fiber plant. Results come in id order, whatever order the file lists things in.
#[derive(Debug, Clone)]
pub struct FiberGraph<'a> {
    fiber: &'a FiberPlant,
    links: BTreeMap<&'a LinkId, &'a OpticalLink>,
    on_cable: BTreeMap<&'a FiberCableId, BTreeSet<&'a LinkId>>,
}

impl<'a> FiberGraph<'a> {
    /// Validates the fiber plant against the library and indexes it.
    pub fn new(fiber: &'a FiberPlant, library: &ComponentLibrary) -> Result<Self, Vec<FiberError>> {
        fiber.validate(library)?;
        let mut on_cable: BTreeMap<&FiberCableId, BTreeSet<&LinkId>> = fiber
            .cables
            .iter()
            .map(|c| (&c.id, BTreeSet::new()))
            .collect();
        for link in &fiber.links {
            for hop in &link.hops {
                if let Hop::Fiber { cable, .. } = hop {
                    on_cable.entry(cable).or_default().insert(&link.id);
                }
            }
        }
        Ok(FiberGraph {
            fiber,
            links: fiber.links.iter().map(|l| (&l.id, l)).collect(),
            on_cable,
        })
    }

    /// The fiber plant this indexes.
    pub fn fiber(&self) -> &'a FiberPlant {
        self.fiber
    }

    /// A link by id.
    pub fn link(&self, id: &LinkId) -> Result<&'a OpticalLink, FiberQueryError> {
        let found = self.links.get(id).copied();
        found.ok_or_else(|| FiberQueryError::UnknownLink(id.clone()))
    }

    /// A link's hops, in order from its transmitting end.
    pub fn hops(&self, id: &LinkId) -> Result<&'a [Hop], FiberQueryError> {
        Ok(&self.link(id)?.hops)
    }

    /// Every link with a hop on this cable, by id: what a cut on it reaches.
    pub fn links_on_cable(
        &self,
        id: &FiberCableId,
    ) -> Result<Vec<&'a OpticalLink>, FiberQueryError> {
        let ids = self
            .on_cable
            .get(id)
            .ok_or_else(|| FiberQueryError::UnknownCable(id.clone()))?;
        Ok(ids.iter().map(|id| self.links[id]).collect())
    }

    /// A service group's links, by id.
    pub fn links_for(&self, service_group: &ServiceGroupId) -> Vec<&'a OpticalLink> {
        self.links
            .values()
            .copied()
            .filter(|l| &l.service_group == service_group)
            .collect()
    }
}

/// What a region-level check finds: a node its fiber plant doesn't serve as built.
#[derive(Error, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RegionIssue {
    /// The region names another fiber plant, or none.
    #[error("region {region} names fiber plant {named:?}, checked against {checked}")]
    WrongFiberPlant {
        /// The region.
        region: String,
        /// What it names.
        named: Option<String>,
        /// What it was checked against.
        checked: String,
    },
    /// No node end for a service group.
    #[error("service group {0} has no node end")]
    NoNodeEnd(ServiceGroupId),
    /// No CMTS port bound to a service group.
    #[error("service group {0} isn't bound to a CMTS port")]
    NoBinding(ServiceGroupId),
    /// No link carries a service group's downstream.
    #[error("service group {0} has no forward link")]
    NoForwardLink(ServiceGroupId),
    /// No link carries a service group's upstream.
    #[error("service group {0} has no return link")]
    NoReturnLink(ServiceGroupId),
    /// A node end whose part isn't its plant's root.
    #[error("service group {0}: the node end's part isn't the plant root's")]
    NodeEndPart(ServiceGroupId),
}

/// Whether every service group in a region has a node end, a CMTS port and a link each way, and whether each node
/// end is its plant's root. `plants` are the region's nodes' plants, in any order; a node with no plant here is
/// checked on the fiber side only. Issues come sorted.
pub fn check_region(
    region: &Region,
    fiber: &FiberGraph<'_>,
    plants: &[&Plant],
) -> Vec<RegionIssue> {
    let mut issues = BTreeSet::new();
    let plant = fiber.fiber();
    if region.fiber_ref.as_deref() != Some(plant.id.as_str()) {
        issues.insert(RegionIssue::WrongFiberPlant {
            region: region.id.clone(),
            named: region.fiber_ref.clone(),
            checked: plant.id.clone(),
        });
    }
    for node in &region.nodes {
        let group = &node.service_group;
        let node_end = plant.node_end(group);
        match node_end {
            None => {
                issues.insert(RegionIssue::NoNodeEnd(group.clone()));
            }
            Some(end) => {
                let root = plants
                    .iter()
                    .find(|p| &p.service_group == group)
                    .and_then(|p| p.root());
                if root.is_some_and(|root| root.component_ref != end.component_ref) {
                    issues.insert(RegionIssue::NodeEndPart(group.clone()));
                }
            }
        }
        if !plant.bindings.iter().any(|b| &b.service_group == group) {
            issues.insert(RegionIssue::NoBinding(group.clone()));
        }
        let links = fiber.links_for(group);
        let carries = |direction: OpticalDirection| {
            links
                .iter()
                .any(|l| l.direction == direction || l.direction == OpticalDirection::Both)
        };
        if !carries(OpticalDirection::Forward) {
            issues.insert(RegionIssue::NoForwardLink(group.clone()));
        }
        if !carries(OpticalDirection::Return) {
            issues.insert(RegionIssue::NoReturnLink(group.clone()));
        }
    }
    issues.into_iter().collect()
}
