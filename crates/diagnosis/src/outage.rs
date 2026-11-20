//! Reading a hard break from which modems vanished (task 19): a modem that's gone has no curve, so the spectrum model can't
//! help, and the reading is the plant tree's. No ML. From the modems' registration states and, where the plant reports
//! them, the node's optics, it names the highest point in the tree whose subtree holds the modems that aren't operational,
//! with the modems that contradict it, or says the losses fit no one point. It's keyed by plant location (invariant 10) and
//! a pure function of its inputs (invariant 1).

use std::collections::BTreeMap;

use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId};
use schema::observation::{ModemObservation, OpticsObservation, RegState};
use schema::plant::ElementKind;
use serde::{Deserialize, Serialize};

use crate::fiber::LinkReading;

/// Light this far under the design's, dB, marks a direction of the node's link as lost: a loss past a link's margin is
/// several dB, and temperature moves a link by a fraction of one (§16, §19).
const LOST_DB: f64 = 3.0;
/// The share of the vanished modems that may sit outside the point named before no one point explains them: room for a
/// modem that was already down, or one an echo takes out above a cut (§20.1).
const TOLERANCE_SHARE: f64 = 0.1;

/// How one modem stands at the step read (§7.4).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModemStanding {
    /// Registered and passing traffic.
    Up,
    /// No downstream: nothing reaches it, or it has no power.
    Dark,
    /// Downstream acquired but not registered: it hears the CMTS and isn't heard, or is still coming back.
    Deaf,
}

impl ModemStanding {
    /// From a modem's registration state.
    pub fn of(state: RegState) -> ModemStanding {
        match state {
            RegState::Operational => ModemStanding::Up,
            RegState::NotSynchronized => ModemStanding::Dark,
            _ => ModemStanding::Deaf,
        }
    }
}

/// Which way the service is lost.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LostWay {
    /// The node's optics show the forward light gone, the return lit.
    ForwardOnly,
    /// The optics show the return gone and the forward lit, or, with no optics, the modems keep downstream and can't range.
    ReturnOnly,
    /// The optics show both ways gone.
    Both,
    /// With no optics: the modems have no downstream, so the forward is lost and the return can't be seen.
    Downstream,
}

/// What the reading concludes.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum OutageVerdict {
    /// Every modem is operational.
    Normal,
    /// One point in the plant explains it: the element at the top of the subtree that went, and the span that feeds it.
    PlantBreak {
        /// The element at the top of the subtree.
        element: ElementId,
        /// The span into it, or `None` at the node.
        span: Option<EdgeId>,
        /// Which way service is lost.
        lost: LostWay,
    },
    /// Everything one supply feeds is out: the supply, or a short somewhere on its run, trips or stops it (§20.4, §20.6).
    SupplyRun {
        /// The supply.
        supply: ElementId,
    },
    /// The modems that went are inside one home or one apartment group's premises: a drop, the home, or its power.
    Premises {
        /// The highest premises element that explains them.
        element: ElementId,
    },
    /// No one point explains the modems that went, within tolerance: not one plant break. A power outage over homes
    /// looks like this, and so do several faults at once. It's "don't know" about where.
    NoOnePoint {
        /// How many separate subtrees, each wholly gone, cover the modems that went.
        groups: usize,
    },
}

/// The reading, with what it rests on.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OutageReading {
    /// The conclusion.
    pub verdict: OutageVerdict,
    /// Modems read.
    pub modems: usize,
    /// Modems not operational.
    pub vanished: usize,
    /// Of those, with no downstream.
    pub dark: usize,
    /// Modems not operational outside the point named, which contradict its being the only cause: an echo above a cut takes
    /// some out (§20.1), and some were down already. Nothing below a break can be up, so no modem up inside it is allowed.
    pub contradicting: Vec<ElementId>,
}

impl OutageReading {
    /// Whether it calls a plant break.
    pub fn is_plant_break(&self) -> bool {
        matches!(
            self.verdict,
            OutageVerdict::PlantBreak { .. } | OutageVerdict::SupplyRun { .. }
        )
    }
}

#[derive(Default, Clone, Copy)]
struct Below {
    up: usize,
    gone: usize,
    deaf: usize,
}

/// Reads one step. `healthy` is the node's link as designed, which the optics are compared with; without it, or without
/// optics, the way service is lost comes from the modems alone.
pub fn read_outage<'g>(
    graph: &PlantGraph<'g>,
    modems: &[ModemObservation],
    optics: Option<&OpticsObservation>,
    healthy: Option<&LinkReading>,
) -> OutageReading {
    let standing: BTreeMap<&ElementId, ModemStanding> = modems
        .iter()
        .map(|m| (&m.element_id, ModemStanding::of(m.reg_state)))
        .collect();
    let gone: Vec<&ElementId> = standing
        .iter()
        .filter(|(_, s)| **s != ModemStanding::Up)
        .map(|(id, _)| *id)
        .collect();
    let dark = standing
        .values()
        .filter(|s| **s == ModemStanding::Dark)
        .count();
    let mut reading = OutageReading {
        verdict: OutageVerdict::Normal,
        modems: standing.len(),
        vanished: gone.len(),
        dark,
        contradicting: Vec::new(),
    };
    if gone.is_empty() {
        return reading;
    }

    // Every element's subtree, counted from the leaves: an element is listed after its parent, so the reverse sums up.
    let mut order: Vec<(&'g ElementId, usize)> = Vec::new();
    let mut stack = vec![(&graph.root().id, 0usize)];
    while let Some((id, depth)) = stack.pop() {
        order.push((id, depth));
        for edge in graph.child_edges(id).iter().rev() {
            stack.push((&edge.to.element_id, depth + 1));
        }
    }
    let mut below: BTreeMap<&ElementId, Below> = BTreeMap::new();
    for (id, _) in order.iter().rev() {
        let mut here = Below::default();
        match standing.get(id) {
            Some(ModemStanding::Up) => here.up += 1,
            Some(ModemStanding::Dark) => here.gone += 1,
            Some(ModemStanding::Deaf) => {
                here.gone += 1;
                here.deaf += 1;
            }
            None => {}
        }
        for edge in graph.child_edges(id) {
            if let Some(child) = below.get(&edge.to.element_id) {
                here.up += child.up;
                here.gone += child.gone;
                here.deaf += child.deaf;
            }
        }
        below.insert(id, here);
    }

    // The candidates: an element's subtree; a powered span's, with every active whose current crosses it, since a cut opens
    // the AC too and a supply can feed amps on either side of where it enters (§6.2, §20.1); and a supply's whole run, which
    // a short trips or an outage stops (§20.4, §20.6). Each is the tops of the subtrees it takes out.
    let depth: BTreeMap<&ElementId, usize> = order.iter().map(|(id, d)| (*id, *d)).collect();
    let under = |x: &ElementId, y: &ElementId| {
        graph
            .ancestors(x)
            .is_ok_and(|up| up.iter().any(|e| &e.id == y))
    };
    let tops_of = |mut ids: Vec<&'g ElementId>| -> Vec<&'g ElementId> {
        ids.sort();
        ids.dedup();
        let all = ids.clone();
        ids.retain(|x| !all.iter().any(|y| y != x && under(x, y)));
        ids
    };
    // Higher in the tree ranks first: the node, then a supply's run, then a span above the element it feeds.
    let rank_of = |id: &ElementId| {
        depth
            .get(id)
            .map_or(usize::MAX, |d| if *d == 0 { 0 } else { 2 * d + 1 })
    };
    let mut candidates: Vec<(Point<'g>, Vec<&'g ElementId>, usize)> = order
        .iter()
        .map(|(id, _)| {
            let element = graph.element(id).ok();
            (
                element.map_or(Point::None, Point::Element),
                vec![*id],
                rank_of(id),
            )
        })
        .collect();
    let overlay = graph.power();
    for feed in &overlay.feeds {
        for segment in &feed.segments {
            let mut ids: Vec<&'g ElementId> = vec![&segment.edge.to.element_id];
            for active in &segment.actives_beyond {
                if let Ok(e) = graph.element(active) {
                    ids.push(&e.id);
                }
            }
            let rank = rank_of(&segment.edge.to.element_id).saturating_sub(1);
            candidates.push((Point::Span(segment.edge), tops_of(ids), rank));
        }
        let ids: Vec<&'g ElementId> = feed
            .actives
            .iter()
            .filter_map(|a| graph.element(a).ok().map(|e| &e.id))
            .collect();
        if !ids.is_empty() {
            candidates.push((Point::Supply(feed.supply), tops_of(ids), 1));
        }
    }

    // The one that leaves the fewest modems unexplained or contradicting, and of those the highest; ties in order.
    let total = gone.len();
    let count = |tops: &[&ElementId]| {
        tops.iter().fold(Below::default(), |mut acc, t| {
            if let Some(b) = below.get(t) {
                acc.up += b.up;
                acc.gone += b.gone;
                acc.deaf += b.deaf;
            }
            acc
        })
    };
    // Where several points explain the same modems, as a run of taps with no homes on them does, the break is on one of the
    // spans between, and cable is cut by the foot: the longest is the likeliest [inferred]. With every modem gone the node
    // comes first, since one thing at the head explains everything.
    let feet = |point: &Point<'_>| -> f64 {
        let span = match point {
            Point::Span(edge) => Some(*edge),
            Point::Element(e) => graph.parent(&e.id).ok().flatten().map(|(_, edge)| edge),
            _ => None,
        };
        span.map_or(0.0, |edge| match &edge.kind {
            schema::plant::EdgeKind::Span { len_ft, .. }
            | schema::plant::EdgeKind::Drop { len_ft, .. } => *len_ft,
            _ => 0.0,
        })
    };
    let everything = total == standing.len();
    let mut best: Option<(usize, usize, usize, f64)> = None;
    for (i, (point, tops, rank)) in candidates.iter().enumerate() {
        let b = count(tops);
        if b.gone == 0 {
            continue;
        }
        // Nothing below a break can be up, so one modem up inside rules the point out; the tolerance is for modems gone
        // outside it, which an echo above a cut takes or which were down already.
        if b.up > 0 {
            continue;
        }
        let cost = total - b.gone;
        let ft = feet(point);
        let better = |(_, c, r, f): (usize, usize, usize, f64)| {
            cost < c
                || (cost == c && everything && *rank < r)
                || (cost == c && !everything && (ft > f || (ft == f && *rank < r)))
        };
        if best.is_none_or(better) {
            best = Some((i, cost, *rank, ft));
        }
    }
    let Some((at, cost, _, _)) = best else {
        return reading;
    };
    let tolerance = ((total as f64 * TOLERANCE_SHARE).floor() as usize).max(1);
    if cost > tolerance {
        reading.verdict = OutageVerdict::NoOnePoint {
            groups: pure_groups(graph, &below),
        };
        return reading;
    }

    let (point, tops, _) = &candidates[at];
    let inside: Vec<&ElementId> = tops.iter().flat_map(|t| modems_under(graph, t)).collect();
    reading.contradicting = gone
        .iter()
        .filter(|id| !inside.contains(id))
        .map(|id| (*id).clone())
        .collect();
    let b = count(tops);
    let lost = lost_way(optics, healthy).unwrap_or(if 2 * b.deaf > b.gone {
        LostWay::ReturnOnly
    } else {
        LostWay::Downstream
    });
    reading.verdict = match point {
        Point::Element(e) if in_premises(graph, &e.id) => OutageVerdict::Premises {
            element: e.id.clone(),
        },
        Point::Element(e) => OutageVerdict::PlantBreak {
            element: e.id.clone(),
            span: graph
                .parent(&e.id)
                .ok()
                .flatten()
                .map(|(_, edge)| edge.id.clone()),
            lost,
        },
        Point::Span(edge) => OutageVerdict::PlantBreak {
            element: edge.to.element_id.clone(),
            span: Some(edge.id.clone()),
            lost,
        },
        Point::Supply(supply) => OutageVerdict::SupplyRun {
            supply: supply.id.clone(),
        },
        Point::None => OutageVerdict::NoOnePoint { groups: 0 },
    };
    reading
}

/// A candidate point.
enum Point<'g> {
    Element(&'g schema::plant::Element),
    Span(&'g schema::plant::Edge),
    Supply(&'g schema::plant::PowerSupply),
    None,
}

/// Which way the node's optics say the link is lost, against its design, if they say either.
fn lost_way(optics: Option<&OpticsObservation>, healthy: Option<&LinkReading>) -> Option<LostWay> {
    let (now, design) = (LinkReading::from_observation(optics?), healthy?);
    let lost = |now: Option<f64>, design: Option<f64>| match (now, design) {
        (None, Some(_)) => true,
        (Some(n), Some(d)) => d - n > LOST_DB,
        _ => false,
    };
    match (
        lost(now.forward_dbm, design.forward_dbm),
        lost(now.return_dbm, design.return_dbm),
    ) {
        (true, true) => Some(LostWay::Both),
        (true, false) => Some(LostWay::ForwardOnly),
        (false, true) => Some(LostWay::ReturnOnly),
        (false, false) => None,
    }
}

/// How many separate subtrees, each with every modem in it gone, cover the modems that went.
fn pure_groups(graph: &PlantGraph<'_>, below: &BTreeMap<&ElementId, Below>) -> usize {
    let pure = |id: &ElementId| below.get(id).is_some_and(|b| b.gone > 0 && b.up == 0);
    below
        .keys()
        .filter(|id| pure(id))
        .filter(|id| {
            !graph
                .parent(id)
                .ok()
                .flatten()
                .is_some_and(|(parent, _)| pure(&parent.id))
        })
        .count()
}

fn modems_under<'g>(graph: &PlantGraph<'g>, point: &ElementId) -> Vec<&'g ElementId> {
    let mut out = Vec::new();
    let mut stack = vec![point.clone()];
    while let Some(id) = stack.pop() {
        if let Ok(element) = graph.element(&id)
            && element.kind == ElementKind::Modem
        {
            out.push(&element.id);
        }
        for edge in graph.child_edges(&id) {
            stack.push(edge.to.element_id.clone());
        }
    }
    out
}

/// Whether an element is inside a home or an apartment group: a modem, or a ground block or anything below one.
fn in_premises(graph: &PlantGraph<'_>, id: &ElementId) -> bool {
    let premises = |kind: &ElementKind| {
        matches!(
            kind,
            ElementKind::Modem | ElementKind::GroundBlock | ElementKind::Cpe
        )
    };
    let Ok(element) = graph.element(id) else {
        return false;
    };
    premises(&element.kind)
        || graph
            .ancestors(id)
            .is_ok_and(|up| up.iter().any(|e| e.kind == ElementKind::GroundBlock))
}

/// Hops in the tree between two points, elements and spans counted alike: a span sits between the two elements it joins.
pub fn hops(graph: &PlantGraph<'_>, a: &ElementId, b: &ElementId) -> Option<usize> {
    let path = |id: &ElementId| -> Option<Vec<ElementId>> {
        let mut out = vec![id.clone()];
        out.extend(graph.ancestors(id).ok()?.into_iter().map(|e| e.id.clone()));
        Some(out)
    };
    let (pa, pb) = (path(a)?, path(b)?);
    let (ia, ib) = pa
        .iter()
        .enumerate()
        .find_map(|(i, x)| pb.iter().position(|y| y == x).map(|j| (i, j)))?;
    // Each step between elements crosses one span: two hops.
    Some(2 * (ia + ib))
}

/// Hops from a named element to a span: the span sits one hop below its upper element and one above its lower one.
pub fn hops_to_span(graph: &PlantGraph<'_>, named: &ElementId, span: &EdgeId) -> Option<usize> {
    let edge = graph.edge(span).ok()?;
    let below = hops(graph, named, &edge.to.element_id)?;
    let above = hops(graph, named, &edge.from.element_id)?;
    Some(below.min(above) + 1)
}
