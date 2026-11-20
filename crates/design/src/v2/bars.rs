//! The bars of `docs/tasks/23-as-built-designer.md`, fixed before any design was run, measured on a design as the sim
//! and the plant graph read it. Each says whether it's met, how many things it checked, and the first few that failed.
//! `realism_survey` reports them and `tests/design_v2.rs` holds them.

use std::collections::{BTreeMap, BTreeSet};

use plant_graph::{FiberGraph, PlantGraph, check_region};
use schema::common::{ElementId, PortId, ServiceGroupId};
use schema::component::{ComponentSpec, ConnectorKind};
use schema::fault::Location;
use schema::fiber::{FiberElementKind, FiberPlant};
use schema::plant::{EdgeKind, ElementKind, Fitted, Plant, PortRole, PortState};
use schema::region::Region;
use serde::{Deserialize, Serialize};
use sim::Overlay;
use sim::ac::solve_ac_at;
use sim::budget::check_physics;
use sim::optics::{link_issues, node_link};
use sim::walk::{Band, Grid, Walker};

use super::fittings::path_len_ft;
use super::kit::Kit;
use super::levels::{TEMPS, sample};

/// One bar, as it fell.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bar {
    /// Its name, as the spec's table has it.
    pub name: String,
    /// Whether it's met.
    pub met: bool,
    /// How many things it checked.
    pub checked: usize,
    /// How many failed.
    pub failed: usize,
    /// The first few failures, in words.
    pub detail: Vec<String>,
}

const DETAIL: usize = 8;

fn bar(name: &str, checked: usize, failures: Vec<String>) -> Bar {
    Bar {
        name: name.to_string(),
        met: failures.is_empty(),
        checked,
        failed: failures.len(),
        detail: failures.into_iter().take(DETAIL).collect(),
    }
}

/// What the bars read.
pub struct Design<'a> {
    /// The parts and rules.
    pub kit: &'a Kit<'a>,
    /// The region.
    pub region: &'a Region,
    /// Each node's plant, optics fitted.
    pub plants: &'a [Plant],
    /// The fiber plant.
    pub fiber: &'a FiberPlant,
}

fn graph<'a>(kit: &'a Kit<'_>, plant: &'a Plant) -> Option<PlantGraph<'a>> {
    PlantGraph::new(plant, kit.library, kit.profile).ok()
}

/// "No issues": `check_design`, `check_physics` at both temperatures with task 22's additions, `check_region` and each
/// node's `link_issues`, by node; region-wide issues under the region's id.
pub fn issues(d: &Design<'_>) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let plan = &d.kit.profile.channel_plan;
    for plant in d.plants {
        let key = plant.service_group.to_string();
        let list = out.entry(key).or_default();
        let Some(g) = graph(d.kit, plant) else {
            list.push("the plant doesn't validate".into());
            continue;
        };
        list.extend(g.check_design().iter().map(|i| format!("{i:?}")));
        match check_physics(
            &g,
            &[sample(TEMPS[1]), sample(TEMPS[2])],
            &Overlay::default(),
        ) {
            Ok(issues) => list.extend(issues.iter().map(|i| format!("{i:?}"))),
            Err(e) => list.push(format!("the sim refused it: {e}")),
        }
        match node_link(d.fiber, plant, d.kit.library, plan) {
            Ok(Some(link)) => list.extend(
                link_issues(&link, d.kit.library, plan)
                    .iter()
                    .map(|i| format!("{i:?}")),
            ),
            Ok(None) => list.push("no optical link".into()),
            Err(e) => list.push(format!("the links don't read: {e}")),
        }
    }
    match FiberGraph::new(d.fiber, d.kit.library) {
        Ok(fg) => {
            let refs: Vec<&Plant> = d.plants.iter().collect();
            let region_issues = check_region(d.region, &fg, &refs);
            if !region_issues.is_empty() {
                out.entry(d.region.id.clone())
                    .or_default()
                    .extend(region_issues.iter().map(|i| format!("{i:?}")));
            }
        }
        Err(e) => out
            .entry(d.region.id.clone())
            .or_default()
            .push(format!("the fiber plant doesn't validate: {e:?}")),
    }
    out.retain(|_, v| !v.is_empty());
    out
}

/// "No stand-ins": every span is its path's length within 1 ft, and nothing is shorter than a run the walk laid.
pub fn no_stand_ins(d: &Design<'_>) -> Bar {
    let mut checked = 0;
    let mut failures = Vec::new();
    for plant in d.plants {
        let geo = |id: &ElementId| {
            plant
                .elements
                .iter()
                .find(|e| &e.id == id)
                .and_then(|e| e.geo)
        };
        for edge in &plant.edges {
            let EdgeKind::Span { len_ft, path, .. } = &edge.kind else {
                continue;
            };
            checked += 1;
            // The rules' jumper between two housings at one structure is the one span shorter than a run.
            let structure = |id: &ElementId| {
                plant
                    .elements
                    .iter()
                    .find(|e| &e.id == id)
                    .and_then(|e| e.structure.clone())
            };
            let same_place = structure(&edge.from.element_id).is_some()
                && structure(&edge.from.element_id) == structure(&edge.to.element_id);
            if path.is_empty() && same_place && (len_ft - d.kit.jumper_len_ft).abs() < 1e-9 {
                continue;
            }
            let (Some(a), Some(b)) = (geo(&edge.from.element_id), geo(&edge.to.element_id)) else {
                failures.push(format!("{}: an end has no position", edge.id));
                continue;
            };
            let along = path_len_ft(a, path, b);
            if (along - len_ft).abs() > 1.0 {
                failures.push(format!(
                    "{}: {len_ft:.1} ft, its path {along:.1} ft",
                    edge.id
                ));
            }
        }
    }
    bar("No stand-ins", checked, failures)
}

/// The sim's level at every tap's port, at the forward band's two edges, at a temperature: the output of the active
/// feeding it less what lies between, by `sim::transfer`.
pub fn tap_port_levels(kit: &Kit<'_>, plant: &Plant, temp_c: f64) -> BTreeMap<ElementId, [f64; 2]> {
    let mut out = BTreeMap::new();
    let Some(g) = graph(kit, plant) else {
        return out;
    };
    let (lo, hi) = sim::walk::ds_band(kit.profile);
    let s = sample(temp_c);
    let overlay = Overlay::default();
    let Ok(walker) = Walker::new(&g, &s, &overlay) else {
        return out;
    };
    let Ok(capture) = walker.capture(&Grid::new(vec![lo, hi]), Band::Down) else {
        return out;
    };
    for element in &plant.elements {
        if !matches!(element.kind, ElementKind::Tap) {
            continue;
        }
        let Some(port) = element.ports.iter().find(|p| p.role == PortRole::TapPort) else {
            continue;
        };
        // Up to the active feeding it.
        let mut at = element.id.clone();
        let mut feed: Option<(ElementId, PortId)> = None;
        while let Ok(Some((parent, edge))) = g.parent(&at) {
            if matches!(
                parent.kind,
                ElementKind::Node { .. } | ElementKind::Rpd { .. } | ElementKind::Amp { .. }
            ) {
                feed = Some((parent.id.clone(), edge.from.port.clone()));
                break;
            }
            at = parent.id.clone();
        }
        let Some((active, active_port)) = feed else {
            continue;
        };
        let Some(levels) = capture.outputs.get(&(active.clone(), active_port.clone())) else {
            continue;
        };
        let a = Location::Port {
            element_id: active,
            port: active_port,
        };
        let b = Location::Port {
            element_id: element.id.clone(),
            port: port.id.clone(),
        };
        let loss = |f: f64| sim::transfer::transfer(&g, &s, &a, &b, f).map(|t| t.loss_db);
        if let (Ok(l_lo), Ok(l_hi)) = (loss(lo), loss(hi)) {
            out.insert(element.id.clone(), [levels[0] - l_lo, levels[1] - l_hi]);
        }
    }
    out
}

/// "Tap-port level": every tap's port within the rules' tolerance of their target at both design frequencies and both
/// temperatures.
pub fn tap_port_level(d: &Design<'_>) -> Bar {
    let kit = d.kit;
    let tol = kit.port_tolerance_db;
    let mut checked = 0;
    let mut failures = Vec::new();
    for plant in d.plants {
        for temp in [TEMPS[1], TEMPS[2]] {
            for (tap, [lo, hi]) in tap_port_levels(kit, plant, temp) {
                checked += 1;
                // At the bottom of the band only where the rules state a level there.
                let e_lo = kit.port_lo_dbmv.map_or(0.0, |t| lo - t);
                let e_hi = hi - kit.port_hi_dbmv;
                if e_lo.abs() > tol + 1e-9 || e_hi.abs() > tol + 1e-9 {
                    let at_lo = kit
                        .port_lo_dbmv
                        .map_or("none".to_string(), |t| format!("{t:.1}"));
                    failures.push(format!(
                        "{tap} at {temp} °C: {hi:.2} / {lo:.2} dBmV against {:.1} / {at_lo} ± {tol}",
                        kit.port_hi_dbmv
                    ));
                }
            }
        }
    }
    bar("Tap-port level", checked, failures)
}

/// "End of line": every tap's through port goes on, or the tap is terminating, or the port is terminated; and no other
/// line device has an output left open.
pub fn end_of_line(d: &Design<'_>) -> Bar {
    let mut checked = 0;
    let mut failures = Vec::new();
    for plant in d.plants {
        let used: BTreeSet<(&ElementId, &PortId)> = plant
            .edges
            .iter()
            .map(|e| (&e.from.element_id, &e.from.port))
            .collect();
        for element in &plant.elements {
            if !matches!(
                element.kind,
                ElementKind::Tap
                    | ElementKind::Splitter
                    | ElementKind::DirectionalCoupler
                    | ElementKind::LineEqualizer
                    | ElementKind::PowerInserter
            ) {
                continue;
            }
            if element.structure.is_none() {
                continue; // in a home
            }
            let states: BTreeMap<&PortId, &PortState> = match &element.fitted {
                Some(Fitted::Passive(p)) => p.ports.iter().map(|s| (&s.port, &s.state)).collect(),
                _ => BTreeMap::new(),
            };
            for port in &element.ports {
                if !matches!(
                    port.role,
                    PortRole::Through | PortRole::Leg | PortRole::Coupled | PortRole::Output
                ) {
                    continue;
                }
                checked += 1;
                let goes_on = used.contains(&(&element.id, &port.id));
                let terminated = matches!(states.get(&port.id), Some(PortState::Terminated { .. }));
                if !goes_on && !terminated {
                    failures.push(format!("{} {} ends open", element.id, port.id));
                }
            }
        }
    }
    bar("End of line", checked, failures)
}

/// "Every port has a state": every tap port has a drop, a terminator, or is open where the rules allow it.
pub fn every_port(d: &Design<'_>) -> Bar {
    let open_ok = d.kit.unused_port == schema::design_rules::UnusedPort::Open;
    let mut checked = 0;
    let mut failures = Vec::new();
    for plant in d.plants {
        let used: BTreeSet<(&ElementId, &PortId)> = plant
            .edges
            .iter()
            .map(|e| (&e.from.element_id, &e.from.port))
            .collect();
        for element in plant
            .elements
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Tap))
        {
            let states: BTreeMap<&PortId, &PortState> = match &element.fitted {
                Some(Fitted::Passive(p)) => p.ports.iter().map(|s| (&s.port, &s.state)).collect(),
                _ => BTreeMap::new(),
            };
            for port in element.ports.iter().filter(|p| p.role == PortRole::TapPort) {
                checked += 1;
                let ok = used.contains(&(&element.id, &port.id))
                    || matches!(states.get(&port.id), Some(PortState::Terminated { .. }))
                    || (open_ok && matches!(states.get(&port.id), Some(PortState::Open)));
                if !ok {
                    failures.push(format!("{} {} has no state", element.id, port.id));
                }
            }
        }
    }
    bar("Every port has a state", checked, failures)
}

/// "Alignment": every active's plug-ins are its family's (`check_physics`' `PlugInOutsideFamily`), and at each amp's
/// output the residual against its set point, across the forward band, peaks to valleys inside N/10 + 3 dB for its
/// cascade of N [SCTE 183 §6.2].
pub fn alignment(d: &Design<'_>) -> Bar {
    let (checked, failures) = alignment_check(d);
    bar("Alignment", checked, failures)
}

/// Every cascade response outside N/10 + 3 dB, as [`alignment`] reads it.
pub fn alignment_failures(d: &Design<'_>) -> Vec<String> {
    alignment_check(d).1
}

fn alignment_check(d: &Design<'_>) -> (usize, Vec<String>) {
    let mut checked = 0;
    let mut failures = Vec::new();
    for plant in d.plants {
        let (n, found) = plant_alignment(d.kit, plant);
        checked += n;
        failures.extend(found.into_iter().filter_map(|a| a.failure()));
    }
    (checked, failures)
}

/// One amp output's response against its set point, as the "Alignment" bar reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct AmpResponse {
    /// The amp.
    pub amp: ElementId,
    /// Its output.
    pub port: PortId,
    /// Its depth in the cascade, the root's first amp 1.
    pub n: usize,
    /// Peak to valley of the output less the set point's line across the forward band at 20 °C, dB.
    pub peak_to_valley_db: f64,
    /// N/10 + 3 dB [SCTE 183 §6.2].
    pub limit_db: f64,
    /// A plug-in outside its family, which the bar counts as a failure on its own.
    pub outside_family: Option<String>,
}

impl AmpResponse {
    /// The bar's line for it, if it fails.
    pub fn failure(&self) -> Option<String> {
        if let Some(text) = &self.outside_family {
            return Some(text.clone());
        }
        (self.peak_to_valley_db > self.limit_db).then(|| {
            format!(
                "{} {}: {:.2} dB peak to valley, N = {}, limit {:.1}",
                self.amp, self.port, self.peak_to_valley_db, self.n, self.limit_db
            )
        })
    }
}

/// Every fitted amp output of `plant` with a set point, and how many were checked: the "Alignment" bar on one plant.
pub fn plant_alignment(kit: &Kit<'_>, plant: &Plant) -> (usize, Vec<AmpResponse>) {
    let (lo, hi) = sim::walk::ds_band(kit.profile);
    let grid: Vec<f64> = (0..=40).map(|i| lo + (hi - lo) * i as f64 / 40.0).collect();
    let mut checked = 0;
    let mut out = Vec::new();
    let Some(g) = graph(kit, plant) else {
        return (0, out);
    };
    if let Ok(issues) = check_physics(&g, &[sample(TEMPS[0])], &Overlay::default()) {
        for i in issues {
            if let sim::budget::PhysicsIssue::PlugInOutsideFamily { element, .. } = &i {
                out.push(AmpResponse {
                    amp: element.clone(),
                    port: PortId::new("in"),
                    n: 0,
                    peak_to_valley_db: 0.0,
                    limit_db: 0.0,
                    outside_family: Some(format!("{i:?}")),
                });
            }
        }
    }
    let s = sample(TEMPS[0]);
    let overlay = Overlay::default();
    let Ok(walker) = Walker::new(&g, &s, &overlay) else {
        return (checked, out);
    };
    let Ok(capture) = walker.capture(&Grid::new(grid.clone()), Band::Down) else {
        return (checked, out);
    };
    for element in plant
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Amp { .. }))
    {
        let Some(Fitted::Active(f)) = &element.fitted else {
            continue;
        };
        let Some(sp) = &f.set_point else {
            continue;
        };
        let n = g.cascade(&element.id).map_or(1, |c| c.len().max(1));
        for port in element.ports.iter().filter(|p| p.role == PortRole::Output) {
            let Some(levels) = capture.outputs.get(&(element.id.clone(), port.id.clone())) else {
                continue;
            };
            checked += 1;
            let residual: Vec<f64> = grid
                .iter()
                .zip(levels)
                .map(|(f, l)| l - (sp.out_dbmv_at_hi - sp.tilt_db * (hi - f) / (hi - lo)))
                .collect();
            let pv = residual.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - residual.iter().copied().fold(f64::INFINITY, f64::min);
            out.push(AmpResponse {
                amp: element.id.clone(),
                port: port.id.clone(),
                n,
                peak_to_valley_db: pv,
                limit_db: n as f64 / 10.0 + 3.0,
                outside_family: None,
            });
        }
    }
    (checked, out)
}

/// "Branches": no splitter or coupler is the first device after an active that has an output unused.
pub fn branches(d: &Design<'_>) -> Bar {
    let mut checked = 0;
    let mut failures = Vec::new();
    for plant in d.plants {
        let by_id: BTreeMap<&ElementId, &schema::plant::Element> =
            plant.elements.iter().map(|e| (&e.id, e)).collect();
        for element in plant.elements.iter().filter(|e| {
            matches!(
                e.kind,
                ElementKind::Node { .. } | ElementKind::Rpd { .. } | ElementKind::Amp { .. }
            )
        }) {
            let outputs: Vec<&PortId> = element
                .ports
                .iter()
                .filter(|p| p.role == PortRole::Output)
                .map(|p| &p.id)
                .collect();
            let leaving: Vec<&schema::plant::Edge> = plant
                .edges
                .iter()
                .filter(|e| e.from.element_id == element.id)
                .collect();
            let unused = outputs
                .iter()
                .any(|o| !leaving.iter().any(|e| &e.from.port == *o));
            checked += 1;
            if !unused {
                continue;
            }
            for edge in leaving {
                // The first device on this output: past any inserter, mounted at the same place.
                let mut next = &edge.to.element_id;
                while let Some(e) = by_id.get(next) {
                    if !matches!(e.kind, ElementKind::PowerInserter) {
                        break;
                    }
                    match plant.edges.iter().find(|x| &x.from.element_id == next) {
                        Some(x) => next = &x.to.element_id,
                        None => break,
                    }
                }
                if let Some(first) = by_id.get(next)
                    && matches!(
                        first.kind,
                        ElementKind::Splitter | ElementKind::DirectionalCoupler
                    )
                {
                    failures.push(format!(
                        "{} feeds {} with an output spare",
                        element.id, first.id
                    ));
                }
            }
        }
    }
    bar("Branches", checked, failures)
}

/// "Powering": every supply feeds an inserter; at both temperatures no supply over the rules' share of its rating, no
/// inserter port over its rating, every active at or above its minimum.
pub fn powering(d: &Design<'_>) -> Bar {
    let kit = d.kit;
    let rules = &kit.rules.powering;
    let share = rules.design_load_pct.value / 100.0;
    let mut checked = 0;
    let mut failures = Vec::new();
    for plant in d.plants {
        for supply in &plant.power_supplies {
            checked += 1;
            let feeds = plant.elements.iter().find(|e| e.id == supply.feeds);
            if !feeds.is_some_and(|e| matches!(e.kind, ElementKind::PowerInserter)) {
                failures.push(format!(
                    "{} feeds {}, not an inserter",
                    supply.id, supply.feeds
                ));
            }
        }
        let Some(g) = graph(kit, plant) else {
            continue;
        };
        for temp in [TEMPS[1], TEMPS[2]] {
            let Ok(solved) = solve_ac_at(&g, &Overlay::default(), &sample(temp)) else {
                failures.push(format!("{}: no AC solution", plant.service_group));
                continue;
            };
            for (id, s) in &solved.supplies {
                checked += 1;
                if s.i_total > share * s.i_rated + 1e-9 {
                    failures.push(format!(
                        "{id} at {temp} °C: {:.2} A of {:.1} A",
                        s.i_total, s.i_rated
                    ));
                }
            }
            for (id, a) in &solved.actives {
                checked += 1;
                if a.v_in < a.v_min {
                    failures.push(format!(
                        "{id} at {temp} °C: {:.1} V under {:.1} V",
                        a.v_in, a.v_min
                    ));
                }
            }
            if let Ok(issues) = check_physics(&g, &[sample(temp)], &Overlay::default()) {
                for i in issues {
                    if matches!(i, sim::budget::PhysicsIssue::InserterPortOverloaded { .. }) {
                        failures.push(format!("{i:?}"));
                    }
                }
            }
        }
    }
    bar("Powering", checked, failures)
}

/// "Fittings": every hardline end has a pin of its cable's size family, and every drop and in-home cable end an F
/// connector.
pub fn fittings(d: &Design<'_>) -> Bar {
    let lib = d.kit.library;
    let mut checked = 0;
    let mut failures = Vec::new();
    for plant in d.plants {
        for edge in &plant.edges {
            let Some((cable, len)) = edge.kind.cable() else {
                continue;
            };
            let family = match lib.get(cable).map(|c| &c.spec) {
                Some(ComponentSpec::Cable(c)) | Some(ComponentSpec::Drop(c)) => c.family.clone(),
                _ => None,
            };
            let hardline = matches!(edge.kind, EdgeKind::Span { .. });
            for end in [0.0, len] {
                checked += 1;
                let fits = edge.kind.fittings().iter().any(|f| {
                    (f.pos_ft - end).abs() < 1e-6
                        && match lib.get(&f.component_ref).map(|c| &c.spec) {
                            Some(ComponentSpec::Connector(c)) if hardline => {
                                c.kind == ConnectorKind::HardlinePin
                                    && c.cable_family.is_some()
                                    && c.cable_family == family
                            }
                            Some(ComponentSpec::Connector(c)) => c.kind == ConnectorKind::DropF,
                            _ => false,
                        }
                });
                if !fits {
                    failures.push(format!(
                        "{} at {end:.0} ft has no connector that fits {cable}",
                        edge.id
                    ));
                }
            }
        }
    }
    bar("Fittings", checked, failures)
}

/// "Fiber": every node has a link each way with at least the rules' margin over its receiver's minimum; no strand is
/// used twice (the fiber plant validates); every sheath is one of the rules' counts; a structure the coax and fiber
/// plants share has one id and one position in both.
pub fn fiber(d: &Design<'_>) -> Bar {
    let kit = d.kit;
    let margin = kit.rules.fiber.link_margin_db.value;
    let plan = &kit.profile.channel_plan;
    let mut checked = 0;
    let mut failures = Vec::new();
    if let Err(e) = d.fiber.validate(kit.library) {
        failures.push(format!("the fiber plant doesn't validate: {e:?}"));
    }
    for plant in d.plants {
        checked += 1;
        let link = match node_link(d.fiber, plant, kit.library, plan) {
            Ok(Some(l)) => l,
            _ => {
                failures.push(format!("{}: no link", plant.service_group));
                continue;
            }
        };
        for (dir, one) in [("forward", &link.forward), ("return", &link.ret)] {
            let Some(one) = one else {
                failures.push(format!("{}: no {dir} link", plant.service_group));
                continue;
            };
            let min = match kit.library.get(&one.receiver).map(|c| &c.spec) {
                Some(ComponentSpec::OpticalRx(rx)) => rx.in_min_dbm,
                Some(ComponentSpec::Transceiver(rx)) => rx.rx_sensitivity_dbm,
                _ => f64::INFINITY,
            };
            match one.received_dbm {
                Some(p) if p - min >= margin - 1e-9 => {}
                p => failures.push(format!(
                    "{}: {dir} at {p:?} dBm, minimum {min} + {margin} dB",
                    one.link
                )),
            }
        }
    }
    let counts = &kit.rules.fiber.cable_counts.value;
    let jumpers: BTreeSet<&schema::common::ComponentId> = [
        &kit.rules.fiber.jumper_ref,
        &kit.rules.fiber.duplex_jumper_ref,
    ]
    .into_iter()
    .flatten()
    .map(|r| &r.value)
    .collect();
    for cable in &d.fiber.cables {
        if jumpers.contains(&cable.component_ref) {
            continue;
        }
        checked += 1;
        match kit.library.get(&cable.component_ref).map(|c| &c.spec) {
            Some(ComponentSpec::FiberCable(c)) if counts.contains(&c.fiber_count) => {}
            _ => failures.push(format!("{} isn't one of the counts", cable.id)),
        }
    }
    // Structures: one id, one position, wherever they're recorded.
    let mut seen: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    let all = d
        .plants
        .iter()
        .flat_map(|p| p.structures.iter())
        .chain(d.fiber.structures.iter());
    for st in all {
        checked += 1;
        let Some(geo) = st.geo else {
            failures.push(format!("{} has no position", st.id));
            continue;
        };
        let at = (geo.lon_deg, geo.lat_deg);
        match seen.get(st.id.as_str()) {
            Some(other) if (other.0 - at.0).abs() > 1e-12 || (other.1 - at.1).abs() > 1e-12 => {
                failures.push(format!("{} is at two places", st.id));
            }
            _ => {
                seen.insert(st.id.as_str(), at);
            }
        }
    }
    // Every element of the fiber plant on a structure names one the fiber plant records.
    let recorded: BTreeSet<&str> = d.fiber.structures.iter().map(|s| s.id.as_str()).collect();
    for e in &d.fiber.elements {
        if let Some(st) = &e.structure {
            checked += 1;
            if !recorded.contains(st.as_str()) {
                failures.push(format!(
                    "{} on {st}, which the fiber plant doesn't record",
                    e.id
                ));
            }
        }
        if let FiberElementKind::NodeEnd { service_group } = &e.kind {
            let plant = d.plants.iter().find(|p| &p.service_group == service_group);
            let root_structure = plant
                .and_then(|p| p.root())
                .and_then(|r| r.structure.clone());
            if root_structure != e.structure {
                failures.push(format!(
                    "{}: the node's end isn't at its root's structure",
                    e.id
                ));
            }
        }
    }
    bar("Fiber", checked, failures)
}

/// "Parts state their tables": every root a design places states a source MER table, and every amp a noise figure
/// table.
pub fn tables(d: &Design<'_>) -> Bar {
    let mut checked = 0;
    let mut failures = Vec::new();
    for plant in d.plants {
        for element in &plant.elements {
            let Some(part) = d.kit.library.get(&element.component_ref) else {
                continue;
            };
            let ok = match (&element.kind, &part.spec) {
                (ElementKind::Node { .. }, ComponentSpec::Node(n)) => !n.source_mer.is_empty(),
                (ElementKind::Rpd { .. }, ComponentSpec::Rpd(r)) => !r.source_mer.is_empty(),
                (ElementKind::Amp { .. }, ComponentSpec::Amp(a)) => !a.nf_table.is_empty(),
                _ => continue,
            };
            checked += 1;
            if !ok {
                failures.push(format!("{} ({}) states no table", element.id, part.id));
            }
        }
    }
    bar("Parts state their tables", checked, failures)
}

/// Every bar a design can be measured on by itself. "Parts list", "Deterministic" and "The old path" are measured by
/// the tests, which build what they compare against.
pub fn all(d: &Design<'_>, excluded: &BTreeSet<ServiceGroupId>) -> Vec<Bar> {
    let found = issues(d);
    let failures: Vec<String> = found
        .iter()
        .filter(|(k, _)| !excluded.iter().any(|e| e.as_str() == k.as_str()))
        .flat_map(|(k, v)| v.iter().map(move |i| format!("{k}: {i}")))
        .collect();
    vec![
        bar("No issues", d.plants.len() + 1, failures),
        no_stand_ins(d),
        tap_port_level(d),
        end_of_line(d),
        every_port(d),
        alignment(d),
        branches(d),
        powering(d),
        fittings(d),
        fiber(d),
        tables(d),
    ]
}
