//! Stage `cost`: prices a design from the rules' table. Parts each, or by the foot where the part is cable, strand or
//! conduit; plug-ins by family; construction by the foot, once per run whatever is lashed to it or pulled through it, at
//! the coax rate where coax runs and the fiber rate where only fiber does; drops by the foot as aerial or buried. A
//! structure is priced once, however many plants use it. Modems and customers' devices aren't plant a design builds and
//! aren't priced. Every cost the generic table holds is `Generic`, and says so wherever it's shown.
//!
//! Each node carries its own plant, an even share of each run's construction and structure among the nodes that use
//! them, its share of each fiber sheath by the fibers it takes, its own optics, and an even share of the hub's common
//! equipment. Hub equipment with ports (a CMTS or core, a switch, a patch panel) serves more than one region, so a
//! design is charged for the share of its ports it uses.

use std::collections::{BTreeMap, BTreeSet};

use schema::common::{ComponentId, ServiceGroupId};
use schema::component::{ComponentClass, ComponentSpec};
use schema::design_rules::{Basis, ConstructionKind};
use schema::fiber::{FiberElementKind, FiberPlant};
use schema::plant::{EdgeKind, Installation, Plant};
use serde::{Deserialize, Serialize};

use super::kit::Kit;
use super::structures::Structures;

/// One line of a cost.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostLine {
    /// A part's id, a plug-in family, or a kind of construction.
    pub item: String,
    /// How many, or feet.
    pub quantity: f64,
    /// `each` or `ft`.
    pub unit: String,
    /// The price each, US dollars.
    pub usd_each: f64,
    /// The line's total, US dollars.
    pub usd: f64,
    /// `Generic`, or the note that sources the price.
    pub basis: String,
}

/// One node's share.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeCost {
    /// The node.
    pub service_group: ServiceGroupId,
    /// Homes it passes.
    pub homes_passed: u32,
    /// Its cost, US dollars.
    pub usd: f64,
    /// Per home passed.
    pub usd_per_home: f64,
}

/// A design's cost.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CostReport {
    /// Every line, parts then construction.
    pub lines: Vec<CostLine>,
    /// Each node's share.
    pub nodes: Vec<NodeCost>,
    /// The whole design, US dollars.
    pub total_usd: f64,
    /// Homes passed.
    pub homes_passed: u32,
    /// Per home passed.
    pub usd_per_home: f64,
    /// Whether every price is `Generic`.
    pub all_generic: bool,
    /// Parts placed that the table has no price for.
    pub unpriced: Vec<String>,
}

fn basis_text(basis: &Basis) -> String {
    match basis {
        Basis::Generic => "Generic".into(),
        Basis::Sourced { note } => note.clone(),
    }
}

/// The table, as lookups.
struct Prices<'k> {
    parts: BTreeMap<&'k ComponentId, (f64, String)>,
    families: BTreeMap<&'k str, (f64, String)>,
    construction: BTreeMap<ConstructionKind, (f64, String)>,
}

impl<'k> Prices<'k> {
    fn new(kit: &'k Kit<'_>) -> Prices<'k> {
        let costs = &kit.rules.costs;
        Prices {
            parts: costs
                .parts
                .iter()
                .map(|c| (&c.component_ref, (c.usd.value, basis_text(&c.usd.basis))))
                .collect(),
            families: costs
                .plug_in_families
                .iter()
                .flatten()
                .map(|f| (f.family.as_str(), (f.usd.value, basis_text(&f.usd.basis))))
                .collect(),
            construction: costs
                .construction
                .iter()
                .map(|c| {
                    (
                        c.kind,
                        (c.usd_per_ft.value, basis_text(&c.usd_per_ft.basis)),
                    )
                })
                .collect(),
        }
    }
}

/// Accumulates priced lines by item.
#[derive(Default)]
struct Ledger {
    lines: BTreeMap<String, CostLine>,
    unpriced: BTreeSet<String>,
}

impl Ledger {
    fn add(&mut self, item: &str, quantity: f64, unit: &str, price: Option<&(f64, String)>) -> f64 {
        let Some((each, basis)) = price else {
            self.unpriced.insert(item.to_string());
            return 0.0;
        };
        let line = self.lines.entry(item.to_string()).or_insert(CostLine {
            item: item.to_string(),
            quantity: 0.0,
            unit: unit.to_string(),
            usd_each: *each,
            usd: 0.0,
            basis: basis.clone(),
        });
        line.quantity += quantity;
        line.usd += quantity * each;
        quantity * each
    }
}

/// Prices one part placed `quantity` times or for `len_ft` feet, by its class: a plug-in by family, a modem or customer
/// device not at all.
fn price_part(
    kit: &Kit<'_>,
    prices: &Prices<'_>,
    ledger: &mut Ledger,
    id: &ComponentId,
    quantity: f64,
    len_ft: Option<f64>,
) -> f64 {
    let Some(part) = kit.library.get(id) else {
        ledger.unpriced.insert(id.to_string());
        return 0.0;
    };
    match (&part.spec, part.class()) {
        (_, ComponentClass::Modem | ComponentClass::Cpe | ComponentClass::Structure) => 0.0,
        (ComponentSpec::PlugIn(p), _) => ledger.add(
            &format!("family {}", p.family),
            quantity,
            "each",
            prices.families.get(p.family.as_str()),
        ),
        _ => match len_ft {
            Some(ft) => ledger.add(id.as_str(), ft, "ft", prices.parts.get(id)),
            None => ledger.add(id.as_str(), quantity, "each", prices.parts.get(id)),
        },
    }
}

/// Prices a design: each node's plant and the runs its coax follows, the fiber plant and the runs its fiber follows, and
/// the structures any of them use.
pub fn cost(
    kit: &Kit<'_>,
    s: &Structures,
    nodes: &[(&Plant, u32, &BTreeSet<usize>)],
    fiber: &FiberPlant,
    fiber_runs: &BTreeMap<ServiceGroupId, BTreeSet<usize>>,
) -> CostReport {
    let prices = Prices::new(kit);
    let mut ledger = Ledger::default();
    let mut share: BTreeMap<ServiceGroupId, f64> = BTreeMap::new();
    // Each node's own plant.
    for (plant, _, _) in nodes {
        let mut usd = 0.0;
        for line in plant.parts_list() {
            usd += price_part(
                kit,
                &prices,
                &mut ledger,
                &line.component_ref,
                line.quantity as f64,
                line.total_len_ft,
            );
        }
        // Drops by the foot, as they're installed.
        for edge in &plant.edges {
            if let EdgeKind::Drop {
                len_ft,
                installation,
                ..
            } = &edge.kind
            {
                let kind = match installation {
                    Some(Installation::Underground) => ConstructionKind::BuriedDrop,
                    _ => ConstructionKind::AerialDrop,
                };
                usd += ledger.add(
                    &format!("{kind:?}"),
                    *len_ft,
                    "ft",
                    prices.construction.get(&kind),
                );
            }
        }
        *share.entry(plant.service_group.clone()).or_default() += usd;
    }
    // Construction, once per run: coax where any node's coax runs, fiber where only fiber does; an even share each.
    let mut users: BTreeMap<usize, (BTreeSet<&ServiceGroupId>, bool)> = BTreeMap::new();
    for (plant, _, runs) in nodes {
        for r in runs.iter() {
            let entry = users.entry(*r).or_default();
            entry.0.insert(&plant.service_group);
            entry.1 = true;
        }
    }
    for (sg, runs) in fiber_runs {
        for r in runs {
            users.entry(*r).or_default().0.insert(sg);
        }
    }
    for (r, (who, coax)) in &users {
        let run = &s.runs[*r];
        let kind = match (coax, run.installation) {
            (true, Installation::Aerial) => ConstructionKind::AerialCoax,
            (true, Installation::Underground) => ConstructionKind::UndergroundCoax,
            (false, Installation::Aerial) => ConstructionKind::AerialFiber,
            (false, Installation::Underground) => ConstructionKind::UndergroundFiber,
        };
        let usd = ledger.add(
            &format!("{kind:?}"),
            run.len_ft,
            "ft",
            prices.construction.get(&kind),
        );
        for sg in who {
            *share.entry((*sg).clone()).or_default() += usd / who.len() as f64;
        }
    }
    // Structures, once each, shared by the nodes whose plant or fiber uses them.
    let mut structure_users: BTreeMap<&str, BTreeSet<&ServiceGroupId>> = BTreeMap::new();
    for (plant, _, _) in nodes {
        for st in &plant.structures {
            structure_users
                .entry(st.id.as_str())
                .or_default()
                .insert(&plant.service_group);
        }
    }
    let fiber_groups: Vec<&ServiceGroupId> = fiber_runs.keys().collect();
    for st in &fiber.structures {
        structure_users.entry(st.id.as_str()).or_default();
    }
    for (id, who) in &structure_users {
        let Some(site) = s.sites.iter().find(|x| x.id.as_str() == *id) else {
            continue;
        };
        let usd = ledger.add(
            site.component_ref.as_str(),
            1.0,
            "each",
            prices.parts.get(&site.component_ref),
        );
        let payers: Vec<&ServiceGroupId> = if who.is_empty() {
            fiber_groups.clone()
        } else {
            who.iter().copied().collect()
        };
        for sg in &payers {
            *share.entry((*sg).clone()).or_default() += usd / payers.len().max(1) as f64;
        }
    }
    // The fiber plant: a node's own optics and tails are its own; a sheath is shared by the fibers each node takes; the
    // hub's common equipment evenly.
    let groups: Vec<ServiceGroupId> = nodes
        .iter()
        .map(|(p, _, _)| p.service_group.clone())
        .collect();
    let mut on_cable: BTreeMap<&str, BTreeSet<&ServiceGroupId>> = BTreeMap::new();
    for link in &fiber.links {
        for hop in &link.hops {
            if let schema::fiber::Hop::Fiber { cable, .. } = hop {
                on_cable
                    .entry(cable.as_str())
                    .or_default()
                    .insert(&link.service_group);
            }
        }
    }
    for cable in &fiber.cables {
        let usd = price_part(
            kit,
            &prices,
            &mut ledger,
            &cable.component_ref,
            1.0,
            Some(cable.len_ft),
        );
        let who: Vec<&ServiceGroupId> = on_cable
            .get(cable.id.as_str())
            .map(|w| w.iter().copied().collect())
            .unwrap_or_default();
        for sg in &who {
            *share.entry((*sg).clone()).or_default() += usd / who.len() as f64;
        }
        if let Some(support) = &cable.support {
            let usd = price_part(kit, &prices, &mut ledger, support, 1.0, Some(cable.len_ft));
            for sg in &who {
                *share.entry((*sg).clone()).or_default() += usd / who.len() as f64;
            }
        }
    }
    for element in &fiber.elements {
        if matches!(element.kind, FiberElementKind::NodeEnd { .. }) {
            continue;
        }
        // Hub equipment with ports serves more than one region: a design pays for the ports it uses.
        let ports = match kit.library.get(&element.component_ref).map(|c| &c.spec) {
            Some(ComponentSpec::Cmts(c)) => Some(c.ports),
            Some(ComponentSpec::Switch(c)) => Some(c.ports),
            Some(ComponentSpec::PatchPanel(c)) => Some(c.ports),
            _ => None,
        };
        let share_of_part = match ports {
            Some(n) if n > 0 => (groups.len() as f64
                * if matches!(element.kind, FiberElementKind::PatchPanel) {
                    2.0
                } else {
                    1.0
                }
                / n as f64)
                .min(1.0),
            _ => 1.0,
        };
        let usd = price_part(
            kit,
            &prices,
            &mut ledger,
            &element.component_ref,
            share_of_part,
            None,
        );
        let own = groups
            .iter()
            .find(|g| element.id.as_str().ends_with(&format!("-{g}")));
        match own {
            Some(g) => *share.entry(g.clone()).or_default() += usd,
            None => {
                for g in &groups {
                    *share.entry(g.clone()).or_default() += usd / groups.len().max(1) as f64;
                }
            }
        }
    }
    // Connectors on every mated pair the links name, once each, shared by the links' nodes.
    let mut pairs: BTreeMap<(String, String), BTreeSet<&ServiceGroupId>> = BTreeMap::new();
    for link in &fiber.links {
        for (i, hop) in link.hops.iter().enumerate() {
            if let schema::fiber::Hop::Connector { at, component_ref } = hop {
                let strand = link
                    .hops
                    .get(i + 1)
                    .or(i.checked_sub(1).and_then(|j| link.hops.get(j)));
                pairs
                    .entry((format!("{at}:{strand:?}"), component_ref.to_string()))
                    .or_default()
                    .insert(&link.service_group);
            }
        }
    }
    for ((_, part), who) in &pairs {
        let usd = price_part(
            kit,
            &prices,
            &mut ledger,
            &ComponentId::new(part),
            1.0,
            None,
        );
        for sg in who {
            *share.entry((*sg).clone()).or_default() += usd / who.len() as f64;
        }
    }
    let mut lines: Vec<CostLine> = ledger.lines.into_values().collect();
    lines.sort_by(|a, b| {
        (a.unit != "each")
            .cmp(&(b.unit != "each"))
            .then(a.item.cmp(&b.item))
    });
    let total_usd: f64 = lines.iter().map(|l| l.usd).sum();
    let homes_passed: u32 = nodes.iter().map(|(_, h, _)| *h).sum();
    let node_costs = nodes
        .iter()
        .map(|(plant, homes, _)| {
            let usd = share.get(&plant.service_group).copied().unwrap_or(0.0);
            NodeCost {
                service_group: plant.service_group.clone(),
                homes_passed: *homes,
                usd,
                usd_per_home: usd / (*homes).max(1) as f64,
            }
        })
        .collect();
    CostReport {
        all_generic: lines.iter().all(|l| l.basis == "Generic"),
        lines,
        nodes: node_costs,
        total_usd,
        homes_passed,
        usd_per_home: total_usd / homes_passed.max(1) as f64,
        unpriced: ledger.unpriced.into_iter().collect(),
    }
}

/// The runs a sequence of structures passes, neighbour to neighbour.
fn runs_through(s: &Structures, ids: &[&str]) -> Vec<usize> {
    let index: BTreeMap<&str, usize> = s
        .sites
        .iter()
        .enumerate()
        .map(|(i, x)| (x.id.as_str(), i))
        .collect();
    let at: Vec<usize> = ids.iter().filter_map(|id| index.get(id).copied()).collect();
    at.windows(2)
        .filter_map(|w| s.run_between(w[0], w[1]))
        .collect()
}

/// A design's cost from its records alone: each node's plant, the runs its spans' paths pass, the fiber plant and the
/// runs its sheaths' paths pass. What `get_design_cost` reads, so the cost of what's recorded is never stored.
pub fn cost_of_records(
    kit: &Kit<'_>,
    s: &Structures,
    plants: &[(&Plant, u32)],
    fiber: &FiberPlant,
) -> CostReport {
    let mut coax: Vec<BTreeSet<usize>> = Vec::new();
    for (plant, _) in plants {
        let structure_of = |id: &schema::common::ElementId| {
            plant
                .elements
                .iter()
                .find(|e| &e.id == id)
                .and_then(|e| e.structure.as_ref())
                .map(|s| s.as_str())
        };
        let mut runs = BTreeSet::new();
        for edge in &plant.edges {
            if !matches!(edge.kind, EdgeKind::Span { .. }) {
                continue;
            }
            let mut ids: Vec<&str> = Vec::new();
            ids.extend(structure_of(&edge.from.element_id));
            ids.extend(
                edge.kind
                    .path()
                    .iter()
                    .filter_map(|p| p.structure.as_ref().map(|s| s.as_str())),
            );
            ids.extend(structure_of(&edge.to.element_id));
            runs.extend(runs_through(s, &ids));
        }
        coax.push(runs);
    }
    let mut fiber_runs: BTreeMap<ServiceGroupId, BTreeSet<usize>> = BTreeMap::new();
    for link in &fiber.links {
        for hop in &link.hops {
            let schema::fiber::Hop::Fiber { cable, .. } = hop else {
                continue;
            };
            let Some(c) = fiber.cable(cable) else {
                continue;
            };
            let structure_of = |id: &schema::common::FiberElementId| {
                fiber
                    .element(id)
                    .and_then(|e| e.structure.as_ref())
                    .map(|s| s.as_str())
            };
            let mut ids: Vec<&str> = Vec::new();
            ids.extend(structure_of(&c.from));
            ids.extend(
                c.path
                    .iter()
                    .filter_map(|p| p.structure.as_ref().map(|s| s.as_str())),
            );
            ids.extend(structure_of(&c.to));
            fiber_runs
                .entry(link.service_group.clone())
                .or_default()
                .extend(runs_through(s, &ids));
        }
    }
    let rows: Vec<(&Plant, u32, &BTreeSet<usize>)> = plants
        .iter()
        .zip(&coax)
        .map(|((p, h), r)| (*p, *h, r))
        .collect();
    cost(kit, s, &rows, fiber, &fiber_runs)
}
