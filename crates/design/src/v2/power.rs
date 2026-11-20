//! Stage `power`: supplies at structures near the centre of their load, each feeding a power inserter, checked at both
//! ends of the temperature range against the supply's 80 % [SCTE 183 §16.12], each inserter port's rating and each
//! active's minimum input with the rules' margin (§6). Start with one supply; while a zone fails, cut its power where the
//! actives on either side balance and give the far side a supply of its own.
//!
//! An inserter goes in at an amp's input, where alignment takes up its loss; with no amp, at the node's first port.

use std::collections::{BTreeMap, BTreeSet};

use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId, PortId};
use schema::component::ComponentSpec;
use schema::plant::{
    BatteryBank, Edge, EdgeKind, Element, ElementKind, Plant, Port, PortRef, PortRole, PowerSupply,
    RecordStatus,
};
use sim::Overlay;
use sim::ac::solve_ac_at;

use super::DesignError;
use super::kit::Kit;
use super::levels::{TEMPS, sample};

/// Most supplies one node gets before the stage gives up and reports what's left.
const MAX_SUPPLIES: usize = 24;

fn is_active(e: &Element) -> bool {
    matches!(
        e.kind,
        ElementKind::Node { .. } | ElementKind::Rpd { .. } | ElementKind::Amp { .. }
    )
}

fn carries_power(edge: &Edge) -> bool {
    match &edge.kind {
        EdgeKind::Span { power_passing, .. } => *power_passing,
        EdgeKind::Direct => true,
        EdgeKind::Drop { .. } | EdgeKind::House { .. } => false,
    }
}

/// The plant's power zones: elements joined by edges that carry power.
fn zones(plant: &Plant, cuts: &BTreeSet<EdgeId>) -> Vec<BTreeSet<ElementId>> {
    let mut adjacent: BTreeMap<&ElementId, Vec<&ElementId>> = BTreeMap::new();
    for edge in &plant.edges {
        if carries_power(edge) && !cuts.contains(&edge.id) {
            adjacent
                .entry(&edge.from.element_id)
                .or_default()
                .push(&edge.to.element_id);
            adjacent
                .entry(&edge.to.element_id)
                .or_default()
                .push(&edge.from.element_id);
        }
    }
    let mut seen: BTreeSet<ElementId> = BTreeSet::new();
    let mut out = Vec::new();
    for e in plant.elements.iter().filter(|e| is_active(e)) {
        if seen.contains(&e.id) {
            continue;
        }
        let mut zone = BTreeSet::new();
        let mut stack = vec![&e.id];
        while let Some(id) = stack.pop() {
            if !zone.insert(id.clone()) {
                continue;
            }
            for n in adjacent.get(id).into_iter().flatten() {
                stack.push(n);
            }
        }
        seen.extend(zone.iter().cloned());
        out.push(zone);
    }
    out
}

/// Feet along the plant from `from` to every element in its zone.
fn distances(
    plant: &Plant,
    zone: &BTreeSet<ElementId>,
    from: &ElementId,
    cuts: &BTreeSet<EdgeId>,
) -> BTreeMap<ElementId, f64> {
    let mut adjacent: BTreeMap<&ElementId, Vec<(&ElementId, f64)>> = BTreeMap::new();
    for edge in &plant.edges {
        if !carries_power(edge) || cuts.contains(&edge.id) || !zone.contains(&edge.from.element_id)
        {
            continue;
        }
        let len = edge.kind.cable().map_or(0.0, |(_, l)| l);
        adjacent
            .entry(&edge.from.element_id)
            .or_default()
            .push((&edge.to.element_id, len));
        adjacent
            .entry(&edge.to.element_id)
            .or_default()
            .push((&edge.from.element_id, len));
    }
    let mut dist: BTreeMap<ElementId, f64> = BTreeMap::new();
    let mut stack = vec![(from.clone(), 0.0)];
    while let Some((id, d)) = stack.pop() {
        if dist.contains_key(&id) {
            continue;
        }
        dist.insert(id.clone(), d);
        for (n, len) in adjacent.get(&id).into_iter().flatten() {
            if !dist.contains_key(*n) {
                stack.push(((*n).clone(), d + len));
            }
        }
    }
    dist
}

fn power_w(kit: &Kit<'_>, e: &Element) -> f64 {
    kit.library
        .get(&e.component_ref)
        .and_then(|c| c.spec.active())
        .map_or(0.0, |a| a.power_w)
}

/// The active a zone's supply goes at: an amp, of those whose input feeds from the plant, nearest its load weighted by
/// draw; the node if the zone has no amp.
fn centre(
    kit: &Kit<'_>,
    plant: &Plant,
    zone: &BTreeSet<ElementId>,
    cuts: &BTreeSet<EdgeId>,
) -> Option<ElementId> {
    let actives: Vec<&Element> = plant
        .elements
        .iter()
        .filter(|e| is_active(e) && zone.contains(&e.id))
        .collect();
    let amps: Vec<&&Element> = actives
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Amp { .. }))
        .collect();
    let candidates: Vec<&Element> = if amps.is_empty() {
        actives.clone()
    } else {
        amps.into_iter().copied().collect()
    };
    let mut best: Option<(f64, ElementId)> = None;
    for c in candidates {
        let dist = distances(plant, zone, &c.id, cuts);
        let cost: f64 = actives
            .iter()
            .map(|a| power_w(kit, a) * dist.get(&a.id).copied().unwrap_or(0.0))
            .sum();
        if best.as_ref().is_none_or(|b| cost < b.0 - 1e-9) {
            best = Some((cost, c.id.clone()));
        }
    }
    best.map(|b| b.1)
}

/// `plant` with an inserter and supply for each zone.
fn powered(kit: &Kit<'_>, plant: &Plant, cuts: &BTreeSet<EdgeId>) -> Plant {
    let mut out = plant.clone();
    for edge in &mut out.edges {
        if let EdgeKind::Span { power_passing, .. } = &mut edge.kind {
            *power_passing = !cuts.contains(&edge.id);
        }
    }
    let sg = plant.service_group.to_string();
    let rules = &kit.rules.powering;
    for (k, zone) in zones(plant, cuts).iter().enumerate() {
        let Some(at) = centre(kit, plant, zone, cuts) else {
            continue;
        };
        // An inserter the walk already put in this zone, at the node, takes its supply; otherwise one goes in at the
        // centre.
        let existing = plant
            .elements
            .iter()
            .find(|e| matches!(e.kind, ElementKind::PowerInserter) && zone.contains(&e.id))
            .cloned();
        if let Some(pi) = existing {
            out.power_supplies.push(PowerSupply {
                id: ElementId::new(format!("{sg}-psu-{}", k + 1)),
                component_ref: rules.supply_ref.value.clone(),
                feeds: pi.id.clone(),
                geo: pi.geo,
                batteries: Some(BatteryBank {
                    component_ref: rules.battery_ref.value.clone(),
                    per_string: rules.batteries_per_string.value,
                    strings: rules.battery_strings.value,
                }),
                structure: pi.structure.clone(),
                record_status: None,
            });
            continue;
        }
        let Some(active) = plant.elements.iter().find(|e| e.id == at).cloned() else {
            continue;
        };
        let pi = ElementId::new(format!("{sg}-pi-{}", k + 2));
        if matches!(active.kind, ElementKind::Amp { .. }) {
            let mut cable = kit.feeder.part.id.clone();
            if let Some(edge) = out
                .edges
                .iter_mut()
                .find(|e| e.to.element_id == at && e.to.port.as_str() == "in")
            {
                edge.to = PortRef {
                    element_id: pi.clone(),
                    port: PortId::new("in"),
                };
                if let Some((c, _)) = edge.kind.cable() {
                    cable = c.clone();
                }
            }
            // The rules' jumper from the inserter into the amp, of the cable that comes in.
            let hardline = if cable == kit.express.part.id {
                &kit.express
            } else {
                &kit.feeder
            };
            let installation = match plant
                .structures
                .iter()
                .find(|s| Some(&s.id) == active.structure.as_ref())
                .map(|s| s.kind)
            {
                Some(schema::component::StructureKind::Pole) => schema::plant::Installation::Aerial,
                _ => schema::plant::Installation::Underground,
            };
            out.edges.push(Edge {
                id: EdgeId::new(format!("{sg}-pijumper-{}", k + 1)),
                from: PortRef {
                    element_id: pi.clone(),
                    port: PortId::new("out"),
                },
                to: PortRef {
                    element_id: at.clone(),
                    port: PortId::new("in"),
                },
                kind: EdgeKind::Span {
                    cable_ref: hardline.part.id.clone(),
                    len_ft: kit.jumper_len_ft,
                    installation,
                    power_passing: true,
                    fittings: super::fittings::span_fittings(
                        hardline,
                        kit.jumper_len_ft,
                        kit.reel_len_ft,
                    ),
                    support: None,
                    path: Vec::new(),
                },
                record_status: None,
            });
        } else {
            // The node: on its first port's line.
            let Some(edge) = out.edges.iter_mut().find(|e| e.from.element_id == at) else {
                continue;
            };
            let port = edge.from.port.clone();
            edge.from = PortRef {
                element_id: pi.clone(),
                port: PortId::new("out"),
            };
            out.edges.push(Edge {
                id: EdgeId::new(format!("{sg}-pidirect-{}", k + 1)),
                from: PortRef {
                    element_id: at.clone(),
                    port,
                },
                to: PortRef {
                    element_id: pi.clone(),
                    port: PortId::new("in"),
                },
                kind: EdgeKind::Direct,
                record_status: None,
            });
        }
        out.elements.push(Element {
            id: pi.clone(),
            kind: ElementKind::PowerInserter,
            component_ref: kit.inserter.id.clone(),
            geo: active.geo,
            ports: vec![
                Port {
                    id: PortId::new("in"),
                    role: PortRole::Input,
                },
                Port {
                    id: PortId::new("out"),
                    role: PortRole::Output,
                },
            ],
            record_status: RecordStatus::Designed,
            fitted: None,
            structure: active.structure.clone(),
        });
        out.power_supplies.push(PowerSupply {
            id: ElementId::new(format!("{sg}-psu-{}", k + 1)),
            component_ref: rules.supply_ref.value.clone(),
            feeds: pi,
            geo: active.geo,
            batteries: Some(BatteryBank {
                component_ref: rules.battery_ref.value.clone(),
                per_string: rules.batteries_per_string.value,
                strings: rules.battery_strings.value,
            }),
            structure: active.structure.clone(),
            record_status: None,
        });
    }
    out
}

/// What's wrong with a powered plant at both temperatures, worst first, each with an element in the zone it's in.
fn violations(kit: &Kit<'_>, plant: &Plant) -> Result<Vec<(f64, ElementId, String)>, DesignError> {
    let graph = PlantGraph::new(plant, kit.library, kit.profile)
        .map_err(|e| DesignError::InvalidPlant(format!("{e:?}")))?;
    let rules = &kit.rules.powering;
    let margin = rules.min_margin_v.as_ref().map_or(0.0, |m| m.value);
    let share = rules.design_load_pct.value / 100.0;
    let port_limit =
        |part: Option<f64>| part.map_or(rules.port_max_a.value, |p| p.min(rules.port_max_a.value));
    let mut out = Vec::new();
    for temp in [TEMPS[1], TEMPS[2]] {
        let solved = solve_ac_at(&graph, &Overlay::default(), &sample(temp))?;
        for (id, s) in &solved.supplies {
            let over = s.i_total - share * s.i_rated;
            if over > 0.0 {
                let fed = plant
                    .power_supplies
                    .iter()
                    .find(|p| &p.id == id)
                    .map(|p| p.feeds.clone())
                    .unwrap_or_else(|| id.clone());
                out.push((
                    over * 10.0,
                    fed,
                    format!("{id} at {:.1} A of {:.1} A", s.i_total, s.i_rated),
                ));
            }
        }
        for (id, a) in &solved.actives {
            let short = a.v_min + margin - a.v_in;
            if short > 0.0 {
                out.push((
                    short,
                    id.clone(),
                    format!("{id} at {:.1} V, needs {:.1} V", a.v_in, a.v_min + margin),
                ));
            }
        }
        for element in &plant.elements {
            let Some(ComponentSpec::PowerInserter(spec)) =
                kit.library.get(&element.component_ref).map(|c| &c.spec)
            else {
                continue;
            };
            let rated = port_limit(spec.port_i_rated_a);
            for feed in &graph.power().feeds {
                for segment in &feed.segments {
                    let edge = segment.edge;
                    if edge.from.element_id != element.id && edge.to.element_id != element.id {
                        continue;
                    }
                    let i_a: f64 = segment
                        .actives_beyond
                        .iter()
                        .filter_map(|id| solved.actives.get(id))
                        .map(|a| a.i_draw)
                        .sum();
                    if i_a > rated {
                        out.push((
                            (i_a - rated) * 10.0,
                            element.id.clone(),
                            format!("{} port at {i_a:.1} A of {rated:.1} A", element.id),
                        ));
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    Ok(out)
}

/// The span in `zone` whose cut leaves the actives' draw most even on its two sides, each side with an active.
fn best_cut(
    kit: &Kit<'_>,
    plant: &Plant,
    zone: &BTreeSet<ElementId>,
    cuts: &BTreeSet<EdgeId>,
) -> Option<EdgeId> {
    let total: f64 = plant
        .elements
        .iter()
        .filter(|e| is_active(e) && zone.contains(&e.id))
        .map(|e| power_w(kit, e))
        .sum();
    let mut best: Option<(f64, EdgeId)> = None;
    for edge in &plant.edges {
        if !matches!(edge.kind, EdgeKind::Span { .. })
            || cuts.contains(&edge.id)
            || !zone.contains(&edge.from.element_id)
        {
            continue;
        }
        let mut trial = cuts.clone();
        trial.insert(edge.id.clone());
        let parts = zones(plant, &trial);
        let side = parts.iter().find(|z| z.contains(&edge.to.element_id));
        let Some(side) = side else {
            continue;
        };
        let w: f64 = plant
            .elements
            .iter()
            .filter(|e| is_active(e) && side.contains(&e.id))
            .map(|e| power_w(kit, e))
            .sum();
        if w <= 0.0 || w >= total {
            continue;
        }
        let imbalance = (total - 2.0 * w).abs();
        if best.as_ref().is_none_or(|b| imbalance < b.0 - 1e-9) {
            best = Some((imbalance, edge.id.clone()));
        }
    }
    best.map(|b| b.1)
}

/// Powers an area's plant. Returns the plant with its inserters and supplies, and what it couldn't fix.
pub fn power(kit: &Kit<'_>, plant: &Plant) -> Result<(Plant, Vec<String>), DesignError> {
    let mut cuts: BTreeSet<EdgeId> = BTreeSet::new();
    loop {
        let candidate = powered(kit, plant, &cuts);
        let wrong = violations(kit, &candidate)?;
        let Some((_, at, _)) = wrong.first() else {
            return Ok((candidate, Vec::new()));
        };
        if candidate.power_supplies.len() >= MAX_SUPPLIES {
            return Ok((candidate, wrong.into_iter().map(|w| w.2).collect()));
        }
        // The zone the worst problem is in, as the unpowered plant has it.
        let in_zone = zones(plant, &cuts).into_iter().find(|z| {
            z.contains(at)
                || candidate
                    .power_supplies
                    .iter()
                    .any(|p| &p.feeds == at && z.iter().any(|e| e == at))
        });
        let zone = match in_zone {
            Some(z) => z,
            None => {
                // An inserter's own zone: find the active it was put beside.
                let beside = candidate
                    .edges
                    .iter()
                    .find(|e| &e.from.element_id == at)
                    .map(|e| e.to.element_id.clone());
                match beside.and_then(|b| zones(plant, &cuts).into_iter().find(|z| z.contains(&b)))
                {
                    Some(z) => z,
                    None => return Ok((candidate, wrong.into_iter().map(|w| w.2).collect())),
                }
            }
        };
        match best_cut(kit, plant, &zone, &cuts) {
            Some(cut) => {
                cuts.insert(cut);
            }
            None => return Ok((candidate, wrong.into_iter().map(|w| w.2).collect())),
        }
    }
}
