//! Walks a routed tree: homes into taps, tap values from the level budget, amps when the budget runs out. Every decision
//! reads the sim's own cable and passive numbers (`docs/physics.md` §1, §2, §3.3), at both ends of the temperature range.

use std::collections::BTreeMap;

use schema::common::{
    ComponentId, EdgeId, ElementId, LonLat, Origin, PortId, ProfileId, ServiceGroupId,
};
use schema::component::{
    ActiveSpec, CableSpec, Component, ComponentLibrary, ComponentSpec, ModemGeneration, TapSpec,
};
use schema::plant::{
    Edge, EdgeKind, Element, ElementKind, Installation, Plant, Port, PortRef, PortRole,
    PowerSupply, RecordStatus,
};
use schema::profile::{NodeKind, Profile, Qam};
use schema::region::{DesignControls, Home};
use sim::cable::cable_loss_db;
use sim::passive::interpolate;
use sim::rng::{Rng, hash_str};
use sim::walk::{ds_band, out_level_dbmv, us_band};

use crate::geo::{dist_ft, lerp};
use crate::route::{StreetGraph, Tree};

/// The temperatures a design has to hold across, °C (§1.2).
const COLD_C: f64 = -20.0;
const HOT_C: f64 = 45.0;
/// Margin on the modem window, dB, for tolerances the design can't see.
const LEVEL_MARGIN_DB: f64 = 1.5;
/// Keeps within this much of the tilt limit, dB.
const SPREAD_MARGIN_DB: f64 = 0.5;
/// Longest run laid on a signal already below an amp's threshold, feet.
const LOW_RUN_MAX_FT: f64 = 100.0;

/// Stands in for an input pad, which the schema has no part for: extra cable, as slack, up to this much per tap.
const SLACK_MAX_FT: f64 = 600.0;
/// A span can't be shorter than this, feet: the plant schema wants a positive length.
const MIN_SPAN_FT: f64 = 3.0;
/// Margin on an amp's minimum input, dB.
const AMP_INPUT_MARGIN_DB: f64 = 0.5;
/// Homes on one tap are within this much of it along the street, feet.
const CLUSTER_FT: f64 = 160.0;

/// Parts the placer picks from, already resolved against the library.
pub struct Parts<'a> {
    /// The root.
    pub root: &'a Component,
    /// The amp.
    pub amp: &'a Component,
    /// Taps, highest value first.
    pub taps: Vec<&'a Component>,
    /// The splitter.
    pub splitter: &'a Component,
    /// The terminator.
    pub terminator: &'a Component,
    /// Hardline.
    pub cable: &'a Component,
    /// Drop cable.
    pub drop: &'a Component,
    /// D3.0 modem.
    pub modem_d30: &'a Component,
    /// D3.1 modem.
    pub modem_d31: &'a Component,
    /// The supply.
    pub supply: &'a Component,
}

/// Why a tree couldn't become a plant.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaceError {
    /// It needs more amps in a row than allowed, so this node has to split.
    CascadeExceeded,
    /// A part doesn't have the spec its class should.
    BadPart(ComponentId),
}

fn cable_spec(component: &Component) -> Result<&CableSpec, PlaceError> {
    match &component.spec {
        ComponentSpec::Cable(spec) | ComponentSpec::Drop(spec) => Ok(spec),
        _ => Err(PlaceError::BadPart(component.id.clone())),
    }
}

fn tap_spec(component: &Component) -> Result<&TapSpec, PlaceError> {
    match &component.spec {
        ComponentSpec::Tap(spec) => Ok(spec),
        _ => Err(PlaceError::BadPart(component.id.clone())),
    }
}

/// Frequencies a design checks across its band: enough to see cable's curve against a straight tilt.
const POINTS: usize = 5;
/// The top of the band in `freqs`, where hardline loses most.
const TOP: usize = POINTS - 1;

/// A signal at the design's check points: each frequency at each temperature.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Levels {
    db: [f64; 2 * POINTS],
}

/// The index of the hot end's top of band in `Levels`.
const HOT_TOP: usize = POINTS + TOP;

const TEMPS: [f64; 2] = [COLD_C, HOT_C];

struct Budget<'a> {
    freqs: [f64; POINTS],
    us_lo_mhz: f64,
    us_hi_mhz: f64,
    target_dbmv: [f64; POINTS],
    cable: &'a CableSpec,
    drop: &'a CableSpec,
    floor_dbmv: f64,
    cap_dbmv: f64,
    spread_max_db: f64,
    amp: &'a ActiveSpec,
    us_floor_loss_db: f64,
    us_ceiling_loss_db: f64,
}

impl Budget<'_> {
    fn fresh(&self) -> Levels {
        let mut db = [0.0; 2 * POINTS];
        for (t, _) in TEMPS.iter().enumerate() {
            for f in 0..POINTS {
                db[t * POINTS + f] = self.target_dbmv[f];
            }
        }
        Levels { db }
    }

    fn through_cable(&self, levels: Levels, len_ft: f64, spec: &CableSpec) -> Levels {
        let mut out = levels;
        for (t, temp_c) in TEMPS.iter().enumerate() {
            for f in 0..POINTS {
                out.db[t * POINTS + f] -= cable_loss_db(spec, self.freqs[f], len_ft, *temp_c);
            }
        }
        out
    }

    fn input_ok(&self, levels: Levels) -> bool {
        // Hottest, at the top of the band: the amp's input is lowest there.
        levels.db[HOT_TOP] >= self.amp_threshold()
    }

    fn amp_threshold(&self) -> f64 {
        (self.target_dbmv[TOP] - self.amp.max_gain_db).max(self.amp.min_in_dbmv)
            + AMP_INPUT_MARGIN_DB
    }

    /// How far along a span the signal can go before an amp must take it, feet.
    fn reach_ft(&self, levels: Levels, max_ft: f64) -> f64 {
        let need = self.amp_threshold();
        let spare = levels.db[HOT_TOP] - need;
        if spare <= 0.0 {
            return 0.0;
        }
        let per_ft = cable_loss_db(self.cable, self.freqs[TOP], 1.0, HOT_C);
        (spare / per_ft).min(max_ft).max(0.0)
    }
}

/// What a tap does to a modem hanging from it.
enum Fit {
    /// This value works.
    Good(usize),
    /// Nothing reaches: the signal's too low, so an amp must come first.
    TooLow,
    /// Too hot even at the highest value, by this many dB at the top of the band.
    TooHot(f64),
}

/// §3.3 and §7.2 for one candidate tap: the modems' window, tilt and transmit power, and enough left for what's next.
#[allow(clippy::too_many_arguments)]
fn choose_tap(
    budget: &Budget<'_>,
    parts: &Parts<'_>,
    level_in: Levels,
    up_loss_hi: f64,
    up_loss_lo: f64,
    drops_ft: &[f64],
    more_after: bool,
) -> Result<Fit, PlaceError> {
    let longest = drops_ft.iter().copied().fold(0.0, f64::max);
    let shortest = drops_ft.iter().copied().fold(f64::INFINITY, f64::min);
    let mut hot_by: f64 = 0.0;
    for (i, component) in parts.taps.iter().enumerate() {
        let tap = tap_spec(component)?;
        let mut ok = true;
        let mut over: f64 = 0.0;
        for (t, temp_c) in TEMPS.iter().enumerate() {
            for drop_ft in [longest, shortest] {
                let (mut lowest, mut highest) = (f64::INFINITY, f64::NEG_INFINITY);
                let (mut at_bottom, mut at_top) = (0.0, 0.0);
                for f in 0..POINTS {
                    let at = level_in.db[t * POINTS + f]
                        - interpolate(&tap.tap_loss, budget.freqs[f])
                        - cable_loss_db(budget.drop, budget.freqs[f], drop_ft, *temp_c);
                    lowest = lowest.min(at);
                    highest = highest.max(at);
                    if f == 0 {
                        at_bottom = at;
                    }
                    if f == TOP {
                        at_top = at;
                    }
                }
                if lowest < budget.floor_dbmv {
                    ok = false;
                }
                // §3.3's limit on tilt. Tilted up, cable hasn't taken enough off the top yet, which slack
                // fixes. Tilted down, it's taken too much, and only an amp resets it.
                let excess = highest - lowest - (budget.spread_max_db - SPREAD_MARGIN_DB);
                if excess > 0.0 {
                    ok = false;
                    if at_top > at_bottom {
                        over = over.max(excess);
                    }
                }
                if highest > budget.cap_dbmv {
                    ok = false;
                    over = over.max(highest - budget.cap_dbmv);
                }
            }
        }
        // §7.2: the loss a modem has to transmit through, at each end of the return band.
        let tap_hi = interpolate(&tap.tap_loss, budget.us_hi_mhz);
        let needed_hi =
            up_loss_hi + tap_hi + cable_loss_db(budget.drop, budget.us_hi_mhz, longest, HOT_C);
        let needed_lo =
            up_loss_lo + tap_hi + cable_loss_db(budget.drop, budget.us_lo_mhz, shortest, COLD_C);
        if needed_hi > budget.us_ceiling_loss_db || needed_lo < budget.us_floor_loss_db {
            ok = false;
        }
        // What leaves through has to feed the next amp.
        let through = level_in.db[HOT_TOP] - interpolate(&tap.insertion_loss, budget.freqs[TOP]);
        if more_after && through < budget.amp_threshold() {
            ok = false;
        }
        if ok {
            return Ok(Fit::Good(i));
        }
        if i == 0 {
            hot_by = over;
        }
    }
    // Too hot at the highest value is what slack fixes. Anything else needs more signal.
    if hot_by > 0.0 {
        Ok(Fit::TooHot(hot_by))
    } else {
        Ok(Fit::TooLow)
    }
}

struct Stop {
    pos_ft: f64,
    homes: Vec<(usize, f64)>,
}

/// One unbranched stretch of the tree.
struct Chain {
    vertices: Vec<usize>,
    /// Feet from the chain's start to each vertex.
    at_ft: Vec<f64>,
    installations: Vec<Installation>,
    end: usize,
}

struct Builder<'a> {
    graph: &'a StreetGraph,
    tree: &'a Tree,
    homes: &'a [Home],
    homes_at: BTreeMap<usize, Vec<usize>>,
    budget: Budget<'a>,
    parts: &'a Parts<'a>,
    controls: &'a DesignControls,
    cascade_max: usize,
    prefix: String,
    elements: Vec<Element>,
    edges: Vec<Edge>,
    counter: usize,
}

fn port(id: &str, role: PortRole) -> Port {
    Port {
        id: PortId::new(id),
        role,
    }
}

#[derive(Clone)]
struct Cursor {
    from: PortRef,
    geo: LonLat,
    levels: Levels,
    /// Loss back to the last active, at each end of the return band: unity beyond it (§7.1).
    up_hi: f64,
    up_lo: f64,
    cascade: usize,
    /// Set after an amp, so an impossible stop can't insert amps forever.
    fresh: bool,
}

impl Builder<'_> {
    fn next_id(&mut self, kind: &str) -> ElementId {
        self.counter += 1;
        ElementId::new(format!("{}-{kind}-{}", self.prefix, self.counter))
    }

    fn push_span(&mut self, cursor: &Cursor, to: PortRef, len_ft: f64, installation: Installation) {
        let id = EdgeId::new(format!("{}-span-{}", self.prefix, self.edges.len()));
        self.edges.push(Edge {
            id,
            from: cursor.from.clone(),
            to,
            kind: EdgeKind::Span {
                cable_ref: self.parts.cable.id.clone(),
                len_ft: len_ft.max(MIN_SPAN_FT),
                installation,
                power_passing: true,
                fittings: Vec::new(),
                path: Vec::new(),
                support: None,
            },
            record_status: None,
        });
    }

    fn add_amp(
        &mut self,
        cursor: &mut Cursor,
        len_ft: f64,
        installation: Installation,
        geo: LonLat,
    ) -> Result<(), PlaceError> {
        if cursor.cascade + 1 > self.cascade_max {
            return Err(PlaceError::CascadeExceeded);
        }
        let id = self.next_id("amp");
        self.elements.push(Element {
            id: id.clone(),
            kind: ElementKind::Amp {
                return_atten_db: 0.0,
            },
            component_ref: self.parts.amp.id.clone(),
            geo: Some(geo),
            ports: vec![port("in", PortRole::Input), port("out", PortRole::Output)],
            record_status: RecordStatus::Designed,
            fitted: None,
            structure: None,
        });
        self.push_span(
            cursor,
            PortRef {
                element_id: id.clone(),
                port: PortId::new("in"),
            },
            len_ft,
            installation,
        );
        cursor.from = PortRef {
            element_id: id,
            port: PortId::new("out"),
        };
        cursor.geo = geo;
        cursor.levels = self.budget.fresh();
        cursor.up_hi = 0.0;
        cursor.up_lo = 0.0;
        cursor.cascade += 1;
        cursor.fresh = true;
        Ok(())
    }

    fn chain_from(&self, start: usize, first: usize) -> Chain {
        let mut vertices = vec![start, first];
        let mut at_ft = vec![0.0];
        let mut installations = Vec::new();
        let step = self.tree.parent.get(&first).copied();
        at_ft.push(step.map_or(0.0, |s| s.1));
        installations.push(step.map_or(Installation::Aerial, |s| s.2));
        let mut current = first;
        loop {
            let children = self.tree.children.get(&current);
            match children.map(|c| c.len()) {
                Some(1) => {
                    let next = children.and_then(|c| c.first().copied()).unwrap_or(current);
                    let step = self.tree.parent.get(&next).copied();
                    let previous = at_ft.last().copied().unwrap_or(0.0);
                    at_ft.push(previous + step.map_or(0.0, |s| s.1));
                    installations.push(step.map_or(Installation::Aerial, |s| s.2));
                    vertices.push(next);
                    current = next;
                }
                _ => break,
            }
        }
        Chain {
            vertices,
            at_ft,
            installations,
            end: current,
        }
    }

    fn point_on(&self, chain: &Chain, pos_ft: f64) -> LonLat {
        for i in 1..chain.vertices.len() {
            if pos_ft <= chain.at_ft[i] {
                let span = (chain.at_ft[i] - chain.at_ft[i - 1]).max(1e-9);
                let share = ((pos_ft - chain.at_ft[i - 1]) / span).clamp(0.0, 1.0);
                return lerp(
                    self.graph.vertices[chain.vertices[i - 1]].geo,
                    self.graph.vertices[chain.vertices[i]].geo,
                    share,
                );
            }
        }
        self.graph.vertices[chain.end].geo
    }

    fn installation_on(&self, chain: &Chain, pos_ft: f64) -> Installation {
        for i in 1..chain.vertices.len() {
            if pos_ft <= chain.at_ft[i] {
                return chain.installations[i - 1];
            }
        }
        chain
            .installations
            .last()
            .copied()
            .unwrap_or(Installation::Aerial)
    }

    fn stops_on(&self, chain: &Chain, extra_at_start: &[usize]) -> Vec<Stop> {
        // Homes in order along the chain, skipping its first vertex, which the chain above owns.
        let mut along: Vec<(f64, usize)> = extra_at_start.iter().map(|h| (0.0, *h)).collect();
        for i in 1..chain.vertices.len() {
            if let Some(list) = self.homes_at.get(&chain.vertices[i]) {
                for &h in list {
                    along.push((chain.at_ft[i], h));
                }
            }
        }
        along.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut stops = Vec::new();
        let mut group: Vec<(f64, usize)> = Vec::new();
        let flush = |group: &mut Vec<(f64, usize)>, stops: &mut Vec<Stop>, this: &Self| {
            if group.is_empty() {
                return;
            }
            let pos_ft = (group[0].0 + group[group.len() - 1].0) / 2.0;
            let geo = this.point_on(chain, pos_ft);
            let homes = group
                .iter()
                .map(|(_, h)| (*h, dist_ft(this.homes[*h].geo, geo)))
                .collect();
            stops.push(Stop { pos_ft, homes });
            group.clear();
        };
        for item in along {
            let too_far = group
                .first()
                .is_some_and(|first| item.0 - first.0 > CLUSTER_FT);
            if group.len() >= 4 || too_far {
                flush(&mut group, &mut stops, self);
            }
            group.push(item);
        }
        flush(&mut group, &mut stops, self);
        stops
    }

    /// Lays a chain from `cursor`, then whatever hangs off its end.
    fn lay_chain(
        &mut self,
        mut cursor: Cursor,
        start: usize,
        first: usize,
        extra: &[usize],
        rng_seed: u64,
    ) -> Result<(), PlaceError> {
        let chain = self.chain_from(start, first);
        let stops = self.stops_on(&chain, extra);
        let mut at_pos = 0.0;
        let children_at_end = self
            .tree
            .children
            .get(&chain.end)
            .is_some_and(|c| !c.is_empty());
        for (n, stop) in stops.iter().enumerate() {
            let more_after = children_at_end || n + 1 < stops.len();
            let mut slack_ft = 0.0;
            let mut guard = 0;
            loop {
                guard += 1;
                let remaining = (stop.pos_ft - at_pos).max(0.0);
                let span = remaining + slack_ft;
                let level_in = self
                    .budget
                    .through_cable(cursor.levels, span, self.budget.cable);
                // The signal has to reach here at a level an amp could still take.
                if !self.budget.input_ok(level_in) && guard <= 40 {
                    let reach = self.budget.reach_ft(cursor.levels, remaining);
                    if reach > 1.0 {
                        let pos = at_pos + reach;
                        let geo = self.point_on(&chain, pos);
                        let installation = self.installation_on(&chain, pos);
                        self.add_amp(&mut cursor, reach, installation, geo)?;
                        at_pos = pos;
                        slack_ft = 0.0;
                        continue;
                    }
                }
                let up_hi = cursor.up_hi
                    + cable_loss_db(self.budget.cable, self.budget.us_hi_mhz, span, HOT_C);
                let up_lo = cursor.up_lo
                    + cable_loss_db(self.budget.cable, self.budget.us_lo_mhz, span, COLD_C);
                let drops: Vec<f64> = stop.homes.iter().map(|h| h.1).collect();
                let fit = choose_tap(
                    &self.budget,
                    self.parts,
                    level_in,
                    up_hi,
                    up_lo,
                    &drops,
                    more_after,
                )?;
                match fit {
                    Fit::Good(index) => {
                        self.place_tap(
                            &mut cursor,
                            &chain,
                            stop,
                            span,
                            index,
                            up_hi,
                            up_lo,
                            level_in,
                            rng_seed,
                        )?;
                        at_pos = stop.pos_ft;
                        break;
                    }
                    _ if guard > 40 => {
                        // Nothing fits. Take a middle value, and let the checks report it plainly.
                        let index = self.parts.taps.len() / 2;
                        self.place_tap(
                            &mut cursor,
                            &chain,
                            stop,
                            span,
                            index,
                            up_hi,
                            up_lo,
                            level_in,
                            rng_seed,
                        )?;
                        at_pos = stop.pos_ft;
                        break;
                    }
                    Fit::TooHot(by_db) => {
                        let per_ft =
                            cable_loss_db(self.budget.cable, self.budget.freqs[TOP], 1.0, COLD_C);
                        slack_ft = (slack_ft + (by_db / per_ft).max(10.0)).min(SLACK_MAX_FT);
                        if slack_ft >= SLACK_MAX_FT {
                            guard = 41;
                        }
                    }
                    Fit::TooLow => {
                        if cursor.fresh && remaining <= 100.0 {
                            guard = 41;
                            continue;
                        }
                        // An amp here, as far along as the signal allows. Right after another amp, what's
                        // in the way is a long span's tilt, so split the span.
                        let reach = self.budget.reach_ft(cursor.levels, remaining);
                        let reach = if cursor.fresh {
                            reach.min(remaining / 2.0)
                        } else {
                            reach
                        };
                        let pos = at_pos + reach;
                        let geo = self.point_on(&chain, pos);
                        let installation = self.installation_on(&chain, pos);
                        self.add_amp(&mut cursor, reach, installation, geo)?;
                        at_pos = pos;
                        slack_ft = 0.0;
                    }
                }
            }
        }

        // A run's end: a terminator, or what the tree does next.
        let end_ft = chain.at_ft.last().copied().unwrap_or(0.0);
        let children = self
            .tree
            .children
            .get(&chain.end)
            .cloned()
            .unwrap_or_default();
        if children.is_empty() {
            self.terminate(&cursor);
            return Ok(());
        }
        // Reach the branch with enough left to feed what's after its legs.
        let mut position = at_pos;
        let ComponentSpec::Splitter(split) = &self.parts.splitter.spec else {
            return Err(PlaceError::BadPart(self.parts.splitter.id.clone()));
        };
        // Each split past the first costs a leg again.
        let legs = children.len().saturating_sub(1) as f64;
        let split_db = legs * interpolate(&split.leg_loss, self.budget.freqs[TOP]);
        loop {
            let remaining = end_ft - position;
            let mut after = self
                .budget
                .through_cable(cursor.levels, remaining, self.budget.cable);
            after.db[HOT_TOP] -= split_db;
            if self.budget.input_ok(after) {
                break;
            }
            let reach = self.budget.reach_ft(cursor.levels, remaining);
            // A tap can leave the signal below what an amp wants and still be fine for its own modems. On a short run
            // that's harmless, but cable carries on losing, and a splitter takes more, so before either the amp goes in
            // at once. §3.2.
            let amp_now =
                reach <= 1.0 && !cursor.fresh && (remaining > LOW_RUN_MAX_FT || children.len() > 1);
            let reach = if amp_now {
                MIN_SPAN_FT.min(remaining).max(0.0)
            } else {
                reach
            };
            if reach <= 1.0 && !amp_now {
                break;
            }
            let pos = position + reach;
            let geo = self.point_on(&chain, pos);
            let installation = self.installation_on(&chain, pos);
            self.add_amp(&mut cursor, reach, installation, geo)?;
            position = pos;
        }
        let remaining = (end_ft - position).max(0.0);
        cursor.levels = self
            .budget
            .through_cable(cursor.levels, remaining, self.budget.cable);
        cursor.up_hi += cable_loss_db(self.budget.cable, self.budget.us_hi_mhz, remaining, HOT_C);
        cursor.up_lo += cable_loss_db(self.budget.cable, self.budget.us_lo_mhz, remaining, COLD_C);
        let geo = self.graph.vertices[chain.end].geo;
        let installation = chain
            .installations
            .last()
            .copied()
            .unwrap_or(Installation::Aerial);
        self.branch(
            cursor,
            chain.end,
            &children,
            remaining,
            geo,
            installation,
            rng_seed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn branch(
        &mut self,
        mut cursor: Cursor,
        vertex: usize,
        children: &[usize],
        lead_ft: f64,
        geo: LonLat,
        installation: Installation,
        seed: u64,
    ) -> Result<(), PlaceError> {
        let Some((&first, rest)) = children.split_first() else {
            self.terminate(&cursor);
            return Ok(());
        };
        if rest.is_empty() {
            // The last leg: nothing left to split, so it carries on.
            cursor.fresh = false;
            return self.lay_chain(cursor, vertex, first, &[], seed);
        }
        let id = self.next_id("split");
        self.elements.push(Element {
            id: id.clone(),
            kind: ElementKind::Splitter,
            component_ref: self.parts.splitter.id.clone(),
            geo: Some(geo),
            ports: vec![
                port("in", PortRole::Input),
                port("leg-1", PortRole::Leg),
                port("leg-2", PortRole::Leg),
            ],
            record_status: RecordStatus::Designed,
            fitted: None,
            structure: None,
        });
        self.push_span(
            &cursor,
            PortRef {
                element_id: id.clone(),
                port: PortId::new("in"),
            },
            lead_ft,
            installation,
        );
        let ComponentSpec::Splitter(spec) = &self.parts.splitter.spec else {
            return Err(PlaceError::BadPart(self.parts.splitter.id.clone()));
        };
        let mut leg = cursor.clone();
        leg.fresh = false;
        for t in 0..2 {
            for f in 0..POINTS {
                leg.levels.db[t * POINTS + f] -= interpolate(&spec.leg_loss, self.budget.freqs[f]);
            }
        }
        leg.up_hi += interpolate(&spec.leg_loss, self.budget.us_hi_mhz);
        leg.up_lo += interpolate(&spec.leg_loss, self.budget.us_lo_mhz);

        let mut a = leg.clone();
        a.from = PortRef {
            element_id: id.clone(),
            port: PortId::new("leg-1"),
        };
        a.geo = geo;
        self.lay_chain(a, vertex, first, &[], seed)?;

        let mut b = leg;
        b.from = PortRef {
            element_id: id,
            port: PortId::new("leg-2"),
        };
        b.geo = geo;
        self.branch(b, vertex, rest, 0.0, geo, installation, seed)
    }

    #[allow(clippy::too_many_arguments)]
    fn place_tap(
        &mut self,
        cursor: &mut Cursor,
        chain: &Chain,
        stop: &Stop,
        span_ft: f64,
        tap_index: usize,
        up_hi: f64,
        up_lo: f64,
        level_in: Levels,
        seed: u64,
    ) -> Result<(), PlaceError> {
        let component = self.parts.taps[tap_index];
        let tap = tap_spec(component)?;
        let id = self.next_id("tap");
        let geo = self.point_on(chain, stop.pos_ft);
        let mut ports = vec![
            port("in", PortRole::Input),
            port("through", PortRole::Through),
        ];
        for p in 0..tap.ports {
            ports.push(port(&format!("tap-{}", p + 1), PortRole::TapPort));
        }
        self.elements.push(Element {
            id: id.clone(),
            kind: ElementKind::Tap,
            component_ref: component.id.clone(),
            geo: Some(geo),
            ports,
            record_status: RecordStatus::Designed,
            fitted: None,
            structure: None,
        });
        let installation = self.installation_on(chain, stop.pos_ft);
        self.push_span(
            cursor,
            PortRef {
                element_id: id.clone(),
                port: PortId::new("in"),
            },
            span_ft,
            installation,
        );

        // The homes on this tap that have service.
        for (n, (home, drop_ft)) in stop.homes.iter().enumerate() {
            let home_id = &self.homes[*home].id;
            let mut rng = Rng::derive(seed, &[hash_str(home_id)]);
            if rng.uniform() >= self.controls.take_rate {
                continue;
            }
            let generation = if rng.uniform() < self.controls.d30_share {
                ModemGeneration::D30
            } else {
                ModemGeneration::D31
            };
            let part = match generation {
                ModemGeneration::D30 => self.parts.modem_d30,
                ModemGeneration::D31 => self.parts.modem_d31,
            };
            let modem_id = ElementId::new(format!("{}-cm-{home_id}", self.prefix));
            self.elements.push(Element {
                id: modem_id.clone(),
                kind: ElementKind::Modem,
                component_ref: part.id.clone(),
                geo: Some(self.homes[*home].geo),
                ports: vec![port("rf", PortRole::Input)],
                record_status: RecordStatus::Designed,
                fitted: None,
                structure: None,
            });
            self.edges.push(Edge {
                id: EdgeId::new(format!("{}-drop-{home_id}", self.prefix)),
                from: PortRef {
                    element_id: id.clone(),
                    port: PortId::new(format!("tap-{}", n + 1)),
                },
                to: PortRef {
                    element_id: modem_id,
                    port: PortId::new("rf"),
                },
                kind: EdgeKind::Drop {
                    cable_ref: self.parts.drop.id.clone(),
                    len_ft: drop_ft.max(MIN_SPAN_FT),
                    fittings: Vec::new(),
                    installation: None,
                    path: Vec::new(),
                },
                record_status: None,
            });
        }

        let mut levels = level_in;
        for t in 0..2 {
            for f in 0..POINTS {
                levels.db[t * POINTS + f] -= interpolate(&tap.insertion_loss, self.budget.freqs[f]);
            }
        }
        cursor.levels = levels;
        cursor.up_hi = up_hi + interpolate(&tap.insertion_loss, self.budget.us_hi_mhz);
        cursor.up_lo = up_lo + interpolate(&tap.insertion_loss, self.budget.us_lo_mhz);
        cursor.from = PortRef {
            element_id: id,
            port: PortId::new("through"),
        };
        cursor.geo = geo;
        cursor.fresh = false;
        Ok(())
    }

    fn terminate(&mut self, cursor: &Cursor) {
        let id = self.next_id("term");
        self.elements.push(Element {
            id: id.clone(),
            kind: ElementKind::Terminator,
            component_ref: self.parts.terminator.id.clone(),
            geo: Some(cursor.geo),
            ports: vec![port("in", PortRole::Input)],
            record_status: RecordStatus::Designed,
            fitted: None,
            structure: None,
        });
        self.edges.push(Edge {
            id: EdgeId::new(format!("{}-direct-{}", self.prefix, self.edges.len())),
            from: cursor.from.clone(),
            to: PortRef {
                element_id: id,
                port: PortId::new("in"),
            },
            kind: EdgeKind::Direct,
            record_status: None,
        });
    }
}

/// One node's plant from its routed tree.
#[allow(clippy::too_many_arguments)]
pub fn place(
    service_group: &ServiceGroupId,
    profile: &Profile,
    library: &ComponentLibrary,
    parts: &Parts<'_>,
    graph: &StreetGraph,
    tree: &Tree,
    root_vertex: usize,
    homes: &[Home],
    group: &[usize],
    controls: &DesignControls,
) -> Result<Plant, PlaceError> {
    let _ = library;
    let (ds_lo, ds_hi) = ds_band(profile);
    let (us_lo, us_hi) = us_band(profile);
    let freqs: [f64; POINTS] =
        std::array::from_fn(|i| ds_lo + (ds_hi - ds_lo) * i as f64 / (POINTS - 1) as f64);
    let target: [f64; POINTS] = std::array::from_fn(|i| out_level_dbmv(profile, freqs[i]));
    let levels = &profile.design_levels;
    let floor = levels
        .modem_in_min_dbmv_by_qam
        .iter()
        .find(|f| f.qam == Qam::Qam4096)
        .map_or(-6.0, |f| f.min_dbmv)
        + LEVEL_MARGIN_DB;
    let active = parts
        .amp
        .spec
        .active()
        .ok_or_else(|| PlaceError::BadPart(parts.amp.id.clone()))?;
    let us_target = levels.us_node_in_target_dbmv;
    let per_1p6 = sim::upstream::per_1p6_from_per_6p4_db();
    let limit = sim::upstream::p1p6hi_dbmv(65.0, 0.0, us_hi - us_lo);
    let budget = Budget {
        freqs,
        us_lo_mhz: us_lo,
        us_hi_mhz: us_hi,
        target_dbmv: target,
        cable: cable_spec(parts.cable)?,
        drop: cable_spec(parts.drop)?,
        floor_dbmv: floor,
        cap_dbmv: levels.modem_in_max_dbmv - LEVEL_MARGIN_DB,
        spread_max_db: levels.psd_spread_max_db,
        amp: active,
        // §7.2: at least the reportable 17 dBmV/1.6 MHz, at most P1.6hi, each with a little margin.
        us_floor_loss_db: 17.0 + per_1p6 - us_target + 0.5,
        us_ceiling_loss_db: limit + per_1p6 - us_target - 1.0,
    };

    let mut homes_at: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &h in group {
        if let Some(v) = graph.home_vertex[h] {
            homes_at.entry(v).or_default().push(h);
        }
    }

    let cascade_max = controls
        .cascade_limit
        .map_or(profile.max_cascade, |limit| limit.min(profile.max_cascade))
        as usize;
    let prefix = service_group.as_str().to_string();
    let mut builder = Builder {
        graph,
        tree,
        homes,
        homes_at,
        budget,
        parts,
        controls,
        cascade_max,
        prefix: prefix.clone(),
        elements: Vec::new(),
        edges: Vec::new(),
        counter: 0,
    };

    let root_id = ElementId::new(format!("{prefix}-root"));
    let geo = graph.vertices[root_vertex].geo;
    let kind = match profile.node_kind {
        NodeKind::Analog => ElementKind::Node {
            return_atten_db: 0.0,
        },
        NodeKind::Rpd => ElementKind::Rpd {
            return_atten_db: 0.0,
        },
    };
    let children = tree.children.get(&root_vertex).cloned().unwrap_or_default();
    // Up to four RF ports directly; more share the last through splitters.
    let ports_used = children.len().clamp(1, 4);
    let root_ports: Vec<Port> = (0..ports_used)
        .map(|i| port(&format!("rf-{}", i + 1), PortRole::Output))
        .collect();
    builder.elements.push(Element {
        id: root_id.clone(),
        kind,
        component_ref: parts.root.id.clone(),
        geo: Some(geo),
        ports: root_ports,
        record_status: RecordStatus::Designed,
        fitted: None,
        structure: None,
    });
    let seed = controls.seed ^ hash_str(service_group.as_str());
    let at_root = builder
        .homes_at
        .get(&root_vertex)
        .cloned()
        .unwrap_or_default();
    for (i, child) in children.iter().enumerate() {
        let index = i.min(3);
        if i > 3 {
            break;
        }
        let cursor = Cursor {
            from: PortRef {
                element_id: root_id.clone(),
                port: PortId::new(format!("rf-{}", index + 1)),
            },
            geo,
            levels: builder.budget.fresh(),
            up_hi: 0.0,
            up_lo: 0.0,
            cascade: 0,
            fresh: false,
        };
        let extra: &[usize] = if i == 0 { &at_root } else { &[] };
        if i == 3 && children.len() > 4 {
            let rest = &children[3..];
            builder.branch(
                cursor,
                root_vertex,
                rest,
                MIN_SPAN_FT,
                geo,
                Installation::Aerial,
                seed,
            )?;
        } else {
            builder.lay_chain(cursor, root_vertex, *child, extra, seed)?;
        }
    }

    Ok(Plant {
        schema_version: schema::plant::SCHEMA_VERSION.to_string(),
        service_group: service_group.clone(),
        profile_ref: ProfileId::new(profile.id.as_str()),
        origin: Origin::Synthetic,
        elements: builder.elements,
        edges: builder.edges,
        power_supplies: vec![PowerSupply {
            id: ElementId::new(format!("{prefix}-psu-1")),
            component_ref: parts.supply.id.clone(),
            feeds: root_id,
            geo: Some(geo),
            batteries: None,
            record_status: None,
            structure: None,
        }],
        modem_bindings: Vec::new(),
        structures: Vec::new(),
    })
}
