//! What each power supply reaches (§6.2). Topology only: the sim solves current and voltage
//! on these segments.

use std::collections::{BTreeMap, BTreeSet};

use schema::common::{EdgeId, ElementId};
use schema::component::ComponentSpec;
use schema::plant::{Edge, EdgeKind, Element, ElementKind, PowerSupply};

use crate::graph::{PlantGraph, is_active};

/// The powering of a whole plant.
#[derive(Debug, Clone, PartialEq)]
pub struct PowerOverlay<'a> {
    /// One per supply, by supply id.
    pub feeds: Vec<PowerFeed<'a>>,
    /// Actives no supply reaches, by id.
    pub unpowered: Vec<ElementId>,
    /// Actives more than one supply reaches, by id.
    pub contested: Vec<ElementId>,
}

/// What one supply reaches.
#[derive(Debug, Clone, PartialEq)]
pub struct PowerFeed<'a> {
    /// The supply.
    pub supply: &'a PowerSupply,
    /// Every active it reaches, by id.
    pub actives: Vec<ElementId>,
    /// The edges that carry its current, by edge id. An edge with no active beyond it carries
    /// none, so it isn't listed.
    pub segments: Vec<PowerSegment<'a>>,
}

/// One edge that carries a supply's current.
#[derive(Debug, Clone, PartialEq)]
pub struct PowerSegment<'a> {
    /// The edge.
    pub edge: &'a Edge,
    /// The actives whose current crosses it, by id.
    pub actives_beyond: Vec<ElementId>,
}

impl<'a> PlantGraph<'a> {
    /// What each supply reaches, walking outward both ways from where it feeds, since a supply
    /// can sit mid-cascade. Spans with `power_passing` and parts that pass power carry it, and
    /// anything else is a cut (§6.2).
    pub fn power(&self) -> PowerOverlay<'a> {
        let mut supplies: Vec<&PowerSupply> = self.plant().power_supplies.iter().collect();
        supplies.sort_by_key(|supply| &supply.id);
        let feeds: Vec<_> = supplies
            .into_iter()
            .map(|supply| self.power_feed(supply))
            .collect();

        let mut reached: BTreeMap<&ElementId, usize> = BTreeMap::new();
        for active in feeds.iter().flat_map(|feed| &feed.actives) {
            *reached.entry(active).or_default() += 1;
        }
        let actives = self.actives();
        let with_count = |keep: fn(usize) -> bool| {
            let ids = actives.iter().map(|e| &e.id);
            ids.filter(|id| keep(reached.get(id).copied().unwrap_or(0)))
                .cloned()
                .collect()
        };
        PowerOverlay {
            unpowered: with_count(|n| n == 0),
            contested: with_count(|n| n > 1),
            feeds,
        }
    }

    fn power_feed(&self, supply: &'a PowerSupply) -> PowerFeed<'a> {
        let Ok(start) = self.element(&supply.feeds) else {
            return PowerFeed {
                supply,
                actives: Vec::new(),
                segments: Vec::new(),
            };
        };
        // Visit outward, then total from the far ends back so each edge knows what's beyond it.
        let mut visits: Vec<(&'a Element, Option<&'a Edge>)> = vec![(start, None)];
        let mut next = 0;
        while let Some(&(element, came_by)) = visits.get(next) {
            next += 1;
            if !self.passes_power(element) {
                continue;
            }
            for edge in self.power_edges(element) {
                if came_by.is_some_and(|c| c.id == edge.id) {
                    continue;
                }
                let far = if edge.from.element_id == element.id {
                    &edge.to.element_id
                } else {
                    &edge.from.element_id
                };
                if let Ok(far) = self.element(far) {
                    visits.push((far, Some(edge)));
                }
            }
        }

        let mut beyond: BTreeMap<&ElementId, BTreeSet<ElementId>> = BTreeMap::new();
        let mut segments = BTreeMap::new();
        for &(element, came_by) in visits.iter().rev() {
            let mut actives = beyond.remove(&element.id).unwrap_or_default();
            if is_active(&element.kind) {
                actives.insert(element.id.clone());
            }
            let Some(edge) = came_by else {
                let segments: Vec<_> = segments.into_values().collect();
                return PowerFeed {
                    supply,
                    actives: actives.into_iter().collect(),
                    segments,
                };
            };
            if !actives.is_empty() {
                let actives_beyond: Vec<_> = actives.iter().cloned().collect();
                segments.insert(
                    &edge.id,
                    PowerSegment {
                        edge,
                        actives_beyond,
                    },
                );
            }
            let near = if edge.from.element_id == element.id {
                &edge.to.element_id
            } else {
                &edge.from.element_id
            };
            beyond.entry(near).or_default().append(&mut actives);
        }
        PowerFeed {
            supply,
            actives: Vec::new(),
            segments: Vec::new(),
        }
    }

    /// The power-passing edges from where `supply`'s current enters to `edge`, in order, `edge` last, and whether `edge` is
    /// reached from its `from` end; `None` if the supply's current doesn't reach it (§6.2, §20.4).
    pub fn power_path(
        &self,
        supply: &'a PowerSupply,
        edge: &EdgeId,
    ) -> Option<(Vec<&'a Edge>, bool)> {
        let start = self.element(&supply.feeds).ok()?;
        let mut came: BTreeMap<&ElementId, &'a Edge> = BTreeMap::new();
        let mut queue: Vec<&'a Element> = vec![start];
        let mut seen: BTreeSet<&ElementId> = [&start.id].into();
        let mut next = 0;
        while let Some(&element) = queue.get(next) {
            next += 1;
            if !self.passes_power(element) {
                continue;
            }
            for e in self.power_edges(element) {
                let from_here = e.from.element_id == element.id;
                if &e.id == edge {
                    let mut path = vec![e];
                    let mut at = &element.id;
                    while let Some(back) = came.get(at) {
                        path.push(back);
                        at = if &back.to.element_id == at {
                            &back.from.element_id
                        } else {
                            &back.to.element_id
                        };
                    }
                    path.reverse();
                    return Some((path, from_here));
                }
                let far = if from_here {
                    &e.to.element_id
                } else {
                    &e.from.element_id
                };
                if seen.insert(far)
                    && let Ok(far_element) = self.element(far)
                {
                    came.insert(&far_element.id, e);
                    queue.push(far_element);
                }
            }
        }
        None
    }

    /// The edges at an element that can carry AC: only spans marked to.
    fn power_edges(&self, element: &'a Element) -> Vec<&'a Edge> {
        let parent = self
            .parent(&element.id)
            .ok()
            .flatten()
            .map(|(_, edge)| edge);
        let edges = parent
            .into_iter()
            .chain(self.child_edges(&element.id).iter().copied());
        edges
            .filter(|edge| {
                matches!(
                    edge.kind,
                    EdgeKind::Span {
                        power_passing: true,
                        ..
                    }
                )
            })
            .collect()
    }

    /// Whether AC gets across this element. Actives are assumed to pass it between all ports,
    /// because `ActiveSpec` has no field saying otherwise.
    fn passes_power(&self, element: &Element) -> bool {
        if is_active(&element.kind) {
            return true;
        }
        if matches!(element.kind, ElementKind::Terminator | ElementKind::Modem) {
            return false;
        }
        match self.component(&element.component_ref).map(|c| &c.spec) {
            Some(ComponentSpec::Tap(spec)) => spec.power_passing,
            Some(ComponentSpec::Splitter(spec)) => spec.power_passing,
            Some(ComponentSpec::DirectionalCoupler(spec)) => spec.power_passing,
            Some(ComponentSpec::PowerInserter(spec)) => spec.power_passing,
            Some(ComponentSpec::LineEqualizer(spec)) => spec.power_passing,
            _ => false,
        }
    }
}
