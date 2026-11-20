//! Homes and fittings: version 2's queries on one plant (`docs/tasks/21-as-built-schemas.md`, "Readers"). A home is
//! derived from the tree, never stored: everything below one drop.

use std::collections::BTreeMap;

use schema::common::{EdgeId, ElementId};
use schema::plant::{Edge, EdgeKind, Element, ElementKind, Fitting};

use crate::graph::{PlantGraph, QueryError};

/// One home: a drop and everything below it.
#[derive(Debug, Clone)]
pub struct Premises<'a> {
    /// The drop that feeds it.
    pub drop: &'a Edge,
    /// The element the drop leaves: a tap, or a filter on a tap port.
    pub tap: &'a Element,
    /// Its ground block, if it has one.
    pub ground_block: Option<&'a Element>,
    /// Everything below the drop, by id.
    pub elements: Vec<&'a Element>,
    /// The modems and customer devices among them, by id.
    pub devices: Vec<&'a Element>,
    /// Its in-home cable, by id.
    pub house_edges: Vec<&'a Edge>,
}

impl<'a> PlantGraph<'a> {
    /// The home an element is in, or `None` if it isn't below a drop.
    pub fn premises_of(&self, id: &ElementId) -> Result<Option<Premises<'a>>, QueryError> {
        let mut current = self.element(id)?;
        loop {
            let Some((parent, edge)) = self.parent(&current.id)? else {
                return Ok(None);
            };
            if matches!(edge.kind, EdgeKind::Drop { .. }) {
                return self.home(edge).map(Some);
            }
            current = parent;
        }
    }

    /// Every home, by the id of its drop.
    pub fn premises(&self) -> Result<Vec<Premises<'a>>, QueryError> {
        let drops: BTreeMap<&EdgeId, &Edge> = self
            .plant()
            .edges
            .iter()
            .filter(|e| matches!(e.kind, EdgeKind::Drop { .. }))
            .map(|e| (&e.id, e))
            .collect();
        drops.into_values().map(|drop| self.home(drop)).collect()
    }

    fn home(&self, drop: &'a Edge) -> Result<Premises<'a>, QueryError> {
        let tap = self.element(&drop.from.element_id)?;
        let mut elements: BTreeMap<&ElementId, &'a Element> = BTreeMap::new();
        let mut house: BTreeMap<&EdgeId, &'a Edge> = BTreeMap::new();
        let mut stack = vec![self.element(&drop.to.element_id)?];
        while let Some(element) = stack.pop() {
            elements.insert(&element.id, element);
            for edge in self.child_edges(&element.id) {
                if matches!(edge.kind, EdgeKind::House { .. }) {
                    house.insert(&edge.id, edge);
                }
                stack.push(self.element(&edge.to.element_id)?);
            }
        }
        let elements: Vec<&'a Element> = elements.into_values().collect();
        let ground_block = elements
            .iter()
            .copied()
            .find(|e| e.kind == ElementKind::GroundBlock);
        let devices = elements
            .iter()
            .copied()
            .filter(|e| matches!(e.kind, ElementKind::Modem | ElementKind::Cpe))
            .collect();
        Ok(Premises {
            drop,
            tap,
            ground_block,
            elements,
            devices,
            house_edges: house.into_values().collect(),
        })
    }

    /// An edge's fittings, from its upstream end, then by part id where two share a position.
    pub fn fittings(&self, id: &EdgeId) -> Result<Vec<&'a Fitting>, QueryError> {
        let mut fittings: Vec<&'a Fitting> = self.edge(id)?.kind.fittings().iter().collect();
        fittings.sort_by(|a, b| {
            a.pos_ft
                .total_cmp(&b.pos_ft)
                .then(a.component_ref.cmp(&b.component_ref))
        });
        Ok(fittings)
    }
}
