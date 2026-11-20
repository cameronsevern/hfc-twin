//! Stage `backbone`, which drives `feeder`: from each node port, along the area's tree, structure by structure. Express
//! runs from an active to the first tap on the rules' express cable, feeder after it on the feeder cable. An amp goes
//! where the budget ends: at the last structure before the signal would arrive too low or too thermally worn for the next
//! taps or the next amp, after that structure's taps. A branch at an active takes its outputs. A branch elsewhere takes
//! a splitter when it's even and a coupler of the value the lighter side needs when it isn't (§2.2). Each active's set
//! point is chosen within the rules' station levels so the first taps on its outputs fit (§15).

use std::collections::{BTreeMap, BTreeSet};

use schema::common::{ComponentId, EdgeId, ElementId, PortId, ServiceGroupId};
use schema::component::{Component, ComponentSpec};
use schema::design_rules::UnusedPort;
use schema::plant::{
    ActiveFitted, Edge, EdgeKind, Element, ElementKind, Fitted, OutputFitted, PassiveFitted, Port,
    PortRef, PortRole, PortState, PortStateEntry, RecordStatus, SetPoint,
};
use schema::profile::NodeKind;
use sim::passive::plugin_loss_db;

use super::feeder::{Misfit, TapChoice, family_for, line_eq_for, split_homes};
use super::fittings::{span_fittings, span_path, support};
use super::kit::{Amp, Hardline, Kit, coupled};
use super::levels::{Band, HI, LO, Levels, REF, Up, cable, coupled_leg, through};
use super::nodes::Area;
use super::structures::Structures;

/// Why an area's plant can't be built under the rules: it has to split.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Overflow {
    /// More amps in cascade than the rules allow.
    Cascade,
    /// More actives than the rules allow.
    Actives,
}

/// A tap and the homes on its ports.
#[derive(Debug, Clone)]
pub struct TapHomes {
    /// The tap.
    pub tap: ElementId,
    /// The structure it's at.
    pub site: usize,
    /// `(port, home)` for each home it serves.
    pub ports: Vec<(PortId, usize)>,
    /// Its ports' level.
    pub port_levels: Levels,
    /// The return path's loss from its ports back to the active feeding it.
    pub up: Up,
}

/// What the walk built for one area.
#[derive(Debug, Clone)]
pub struct Rf {
    /// Elements, every one `Designed`.
    pub elements: Vec<Element>,
    /// Edges.
    pub edges: Vec<Edge>,
    /// Each tap's homes.
    pub taps: Vec<TapHomes>,
    /// Structures any element is at or any span passes.
    pub sites: BTreeSet<usize>,
    /// Where the design couldn't hold a rule, in words, for the record.
    pub notes: Vec<String>,
    /// The root's id.
    pub root: ElementId,
    /// Every run a span follows.
    pub runs: BTreeSet<usize>,
}

/// The cable from a line's last device to the structure being looked at.
#[derive(Debug, Clone)]
struct Line {
    from: PortRef,
    at: usize,
    levels: Levels,
    cascade: usize,
    tapped: bool,
    runs: Vec<usize>,
    /// The return path's loss from here back to the active feeding the line (§7.1).
    up: Up,
}

/// One branch leaving a structure: the runs it follows and the structure they reach.
#[derive(Debug, Clone)]
struct Branch {
    runs: Vec<usize>,
    to: usize,
}

struct Walker<'k, 'a> {
    kit: &'k Kit<'a>,
    band: Band,
    s: &'k Structures,
    area: &'k Area,
    sg: String,
    elements: Vec<Element>,
    edges: Vec<Edge>,
    counters: BTreeMap<&'static str, usize>,
    taps: Vec<TapHomes>,
    sites: BTreeSet<usize>,
    runs: BTreeSet<usize>,
    notes: Vec<String>,
    actives: usize,
    amp_threshold: f64,
    /// Gain less the smallest plug-ins and the largest pad: an amp's output from its input at the least.
    amp_reach_db: f64,
    agc_db: f64,
}

fn port(id: &str, role: PortRole) -> Port {
    Port {
        id: PortId::new(id),
        role,
    }
}

fn port_ref(element: &ElementId, port: &str) -> PortRef {
    PortRef {
        element_id: element.clone(),
        port: PortId::new(port),
    }
}

/// An active's output ports: its part's, or one `out`.
pub fn output_ports(component: &Component) -> Vec<String> {
    match component.spec.active() {
        Some(a) if !a.outputs.is_empty() => a.outputs.iter().map(|o| o.port.to_string()).collect(),
        _ => vec!["out".to_string()],
    }
}

impl<'k, 'a> Walker<'k, 'a> {
    fn id(&mut self, kind: &'static str) -> ElementId {
        let n = self.counters.entry(kind).or_insert(0);
        *n += 1;
        ElementId::new(format!("{}-{kind}-{n}", self.sg))
    }

    fn edge_id(&mut self, kind: &'static str) -> EdgeId {
        let n = self.counters.entry(kind).or_insert(0);
        *n += 1;
        EdgeId::new(format!("{}-{kind}-{n}", self.sg))
    }

    fn cable_for(&self, line: &Line) -> &'k Hardline<'a> {
        if line.tapped {
            &self.kit.feeder
        } else {
            &self.kit.express
        }
    }

    fn runs_len(&self, runs: &[usize]) -> f64 {
        runs.iter().map(|r| self.s.runs[*r].len_ft).sum()
    }

    /// The level a line brings to the end of `runs` more, its cable and fittings counted.
    fn arrive(&self, line: &Line, more: &[usize]) -> Levels {
        let mut len: f64 = self.runs_len(&line.runs) + self.runs_len(more);
        if len <= 0.0 {
            // At the same structure: a jumper between the two housings.
            len = self.kit.jumper_len_ft;
        }
        let hardline = self.cable_for(line);
        let fittings = span_fittings(hardline, len, self.kit.reel_len_ft);
        let fit_loss: f64 = fittings
            .iter()
            .filter_map(|f| self.kit.library.get(&f.component_ref))
            .map(|c| through(c, 0.0))
            .sum();
        line.levels
            .less(&self.band, cable(hardline.spec, len))
            .less(&self.band, |_, _| fit_loss)
    }

    /// The return path's loss a line has at the end of `runs` more, its cable and fittings counted.
    fn arrive_up(&self, line: &Line, more: &[usize]) -> Up {
        let mut len: f64 = self.runs_len(&line.runs) + self.runs_len(more);
        if len <= 0.0 {
            len = self.kit.jumper_len_ft;
        }
        let hardline = self.cable_for(line);
        let fittings = span_fittings(hardline, len, self.kit.reel_len_ft);
        let fit_loss: f64 = fittings
            .iter()
            .filter_map(|f| self.kit.library.get(&f.component_ref))
            .map(|c| through(c, 0.0))
            .sum();
        line.up
            .plus(&self.band, cable(hardline.spec, len))
            .plus(&self.band, |_, _| fit_loss)
    }

    /// Whether an amp could take this as its input at every temperature: enough for its gain and smallest plug-ins to
    /// reach the highest station level, at least its minimum input, and no more thermal swing than its AGC holds.
    /// Whether an amp on `line` could take this input: over its threshold, inside its AGC's swing, and its response
    /// inside N/10 + 3 dB for its place in the cascade [SCTE 183 §6.2]. Below the first amp, less half a pad step for
    /// the edges the plug-ins above land off their set point (physics §15), which the walk, holding each output on its
    /// set point, doesn't see. What the cascade adds up to is read on the design, by the Alignment bar, and a
    /// candidate it fails isn't kept while one passes.
    fn amp_ok(&self, input: &Levels, line: &Line) -> bool {
        let r = input.hi(REF);
        let n = line.cascade + 1;
        let steps = if n > 1 {
            super::feeder::RESIDUAL_DB
        } else {
            0.0
        };
        input.min_hi() >= self.amp_threshold
            && r - input.hi(2) <= self.agc_db + 1e-9
            && input.hi(1) - r <= self.agc_db + 1e-9
            && self.response_pv(input) + steps <= 0.1 * n as f64 + 3.0 - 1e-9
    }

    /// The peak to valley [`Self::response_residual`] leaves.
    fn response_pv(&self, input: &Levels) -> f64 {
        let residual = self.response_residual(input);
        residual.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - residual.iter().copied().fold(f64::INFINITY, f64::min)
    }

    /// §15, SCTE 183 §6.2: what an amp's output keeps from this input at each check point, once its equalizer brings
    /// the band's two edges onto a linear set point: what cable's linear term leaves that an equalizer's √f shape
    /// can't take out. Zero at the edges.
    fn response_residual(&self, input: &Levels) -> [f64; super::levels::NF] {
        let share = |f: f64| {
            let (a, b) = (54.0_f64.sqrt(), 1218.0_f64.sqrt());
            ((f.sqrt() - a) / (b - a)).clamp(0.0, 1.0)
        };
        let row = &input.0[REF];
        // The equalizer's shape, 1 − share, normalised so it adds one dB of tilt between the band's edges.
        let span = share(self.band.f[super::levels::HI]) - share(self.band.f[LO]);
        if span <= 1e-9 {
            return [0.0; super::levels::NF];
        }
        // Output less a straight line through its two edges, with any equalizer value: the equalizer's own part is a
        // multiple of `1 − share`, which a straight line through the edges can't remove either, so it's chosen to cancel
        // the input's best it can at the edges and the rest is what's left.
        let n = row.len();
        let lin = |i: usize| -> f64 {
            let t = (self.band.f[i] - self.band.f[LO]) / (self.band.f[n - 1] - self.band.f[LO]);
            row[LO] + (row[n - 1] - row[LO]) * t
        };
        let eq_shape = |i: usize| -> f64 {
            let t = (self.band.f[i] - self.band.f[LO]) / (self.band.f[n - 1] - self.band.f[LO]);
            let s_lin = share(self.band.f[LO]) + span * t;
            share(self.band.f[i]) - s_lin
        };
        // E·(eq shape) is fixed by the tilt the equalizer has to add; take that value.
        let tilt_needed = self.kit.station.tilt_db.value - (row[n - 1] - row[LO]);
        let e = tilt_needed / span;
        std::array::from_fn(|i| (row[i] - lin(i)) + e * eq_shape(i))
    }

    /// Lays the cable a line has pending into `to`, or a direct mount if it's at the same structure.
    fn connect(&mut self, line: &Line, to: PortRef) {
        let kind = if line.runs.is_empty() {
            // Two housings at one structure: the rules' jumper, of the line's cable, a pin at each end, power passing.
            let hardline = self.cable_for(line);
            let installation = match self.s.sites[line.at].kind {
                schema::component::StructureKind::Pole => schema::plant::Installation::Aerial,
                _ => schema::plant::Installation::Underground,
            };
            EdgeKind::Span {
                cable_ref: hardline.part.id.clone(),
                len_ft: self.kit.jumper_len_ft,
                installation,
                power_passing: true,
                fittings: span_fittings(hardline, self.kit.jumper_len_ft, self.kit.reel_len_ft),
                support: None,
                path: Vec::new(),
            }
        } else {
            let hardline = self.cable_for(line);
            self.runs.extend(line.runs.iter().copied());
            let (path, len_ft, installation) = span_path(self.s, line.at, &line.runs);
            for p in &path {
                if let Some(id) = &p.structure
                    && let Some(i) = self.s.sites.iter().position(|s| &s.id == id)
                {
                    self.sites.insert(i);
                }
            }
            EdgeKind::Span {
                cable_ref: hardline.part.id.clone(),
                len_ft,
                installation,
                power_passing: true,
                fittings: span_fittings(hardline, len_ft, self.kit.reel_len_ft),
                support: Some(support(self.kit, installation)),
                path,
            }
        };
        let id = match &kind {
            EdgeKind::Span { path, .. } if path.is_empty() && line.runs.is_empty() => {
                self.edge_id("jumper")
            }
            _ => self.edge_id("span"),
        };
        self.edges.push(Edge {
            id,
            from: line.from.clone(),
            to,
            kind,
            record_status: None,
        });
    }

    fn element(
        &mut self,
        id: ElementId,
        kind: ElementKind,
        part: &Component,
        at: usize,
        ports: Vec<Port>,
        fitted: Option<Fitted>,
    ) {
        self.sites.insert(at);
        let site = &self.s.sites[at];
        self.elements.push(Element {
            id,
            kind,
            component_ref: part.id.clone(),
            geo: Some(site.geo),
            ports,
            record_status: RecordStatus::Designed,
            fitted,
            structure: Some(site.id.clone()),
        });
    }

    /// The first structure with homes down a branch, and the levels a station's output would reach it with, per volt of
    /// output: the express loss on the way. `None` if the branch forks first.
    fn first_taps(&self, from: usize, branch: &Branch) -> Option<(usize, Vec<usize>)> {
        let mut runs = branch.runs.clone();
        let mut at = branch.to;
        let _ = from;
        loop {
            if self.area.homes_at.contains_key(&at) {
                return Some((at, runs));
            }
            let kids = self.area.children.get(&at).cloned().unwrap_or_default();
            if kids.len() != 1 {
                return None;
            }
            runs.push(self.area.run_to[&kids[0]]);
            at = kids[0];
        }
    }

    /// The set point for a station at `at` feeding `branches` (and taps at `at` itself first when `own_homes`): the
    /// highest output and tilt within the rules' station levels that keep the first taps on every output within the port
    /// target.
    fn station(&self, at: usize, branches: &[Branch], own_homes: bool, lowest: f64) -> (f64, f64) {
        let st = self.kit.station;
        let top = st.out_dbmv_at_hi.value;
        // No lower than the rules allow, nor than the amp's largest pad brings its input down to (§15).
        let floor = st
            .min_out_dbmv_at_hi
            .value
            .max(((lowest * 2.0).ceil() / 2.0).min(top));
        let (tilt_top, tilt_floor) = (st.tilt_db.value, st.min_tilt_db.value);
        let mut firsts: Vec<(usize, Vec<usize>)> = Vec::new();
        if own_homes {
            firsts.push((at, Vec::new()));
        }
        for b in branches {
            if let Some(f) = self.first_taps(at, b) {
                firsts.push(f);
            }
        }
        // Per first tap at this level and tilt: whether it fits, and how far its ports sit from the target at worst.
        let fits = |out: f64, tilt: f64| -> (usize, usize, f64) {
            let mut own = 0;
            let mut n = 0;
            let mut worst: f64 = 0.0;
            for (site, runs) in &firsts {
                let line = Line {
                    from: port_ref(&ElementId::new("probe"), "out"),
                    at,
                    levels: Levels::station(&self.band, out, tilt),
                    cascade: 0,
                    tapped: false,
                    runs: runs.clone(),
                    up: Up::default(),
                };
                let input = self.arrive(&line, &[]);
                let homes = self.area.homes_at.get(site).map_or(0, Vec::len);
                let ends = self.area.children.get(site).is_none_or(Vec::is_empty) && *site != at;
                let _ = homes;
                if let Ok(chain) = self.try_taps(&input, &self.arrive_up(&line, &[]), *site, ends) {
                    n += 1;
                    if runs.is_empty() {
                        own += 1;
                    }
                    for c in &chain {
                        for t in 0..3 {
                            worst = worst.max((c.port.hi(t) - self.kit.port_hi_dbmv).abs()).max(
                                self.kit
                                    .port_lo_dbmv
                                    .map_or(0.0, |lo| (c.port.lo(t) - lo).abs()),
                            );
                        }
                    }
                }
            }
            (own, n, worst)
        };
        // The most first taps fitting; of those, the ports nearest the target, so alignment's residual (§15) has room;
        // then the highest output and the most tilt. A branch whose first taps fit at no station level (a line
        // equalizer's job, further on) doesn't hold the others back.
        // Taps at the station's own structure first: nothing but a jumper lies between them and its output, so no level
        // further on can make up for them.
        let mut best: Option<(usize, usize, f64, f64, f64)> = None;
        let mut out = top;
        while out >= floor - 1e-9 {
            let mut tilt = tilt_top;
            while tilt >= tilt_floor - 1e-9 {
                let (own, n, worst) = fits(out, tilt);
                let worst = (worst * 4.0).round() / 4.0;
                let better = match best {
                    None => true,
                    Some((bo, bn, bw, _, _)) => {
                        own > bo || (own == bo && (n > bn || (n == bn && worst < bw - 1e-9)))
                    }
                };
                if better {
                    best = Some((own, n, worst, out, tilt));
                }
                tilt -= 0.5;
            }
            out -= 0.5;
        }
        best.map_or((top, tilt_top), |b| (b.3, b.4))
    }

    /// Places an amp at `at` on `line`, and returns one line per output it drives into `branches`.
    fn amp(
        &mut self,
        line: &Line,
        at: usize,
        branches: &[Branch],
        role_express: bool,
    ) -> Result<Vec<(Line, Vec<Branch>)>, Overflow> {
        if line.cascade + 1 > self.kit.max_cascade {
            return Err(Overflow::Cascade);
        }
        if self.actives + 1 > self.kit.max_actives {
            return Err(Overflow::Actives);
        }
        self.actives += 1;
        let input = self.arrive(line, &[]);
        let mut line = line.clone();
        // An input tilted further down than the amp's equalizers reach takes a line equalizer first (§15).
        let eq_reach = self.max_eq_tilt();
        let station_tilt = self.kit.station.tilt_db.value;
        if input.tilt(REF) + eq_reach < station_tilt
            && let Some(leq) = line_eq_for(self.kit, &self.band, &input)
        {
            let leq = leq.clone();
            line = self.line_eq(&line, at, &leq);
        }
        let amp: &Amp<'a> = if role_express || branches.len() > 1 {
            &self.kit.express_amp
        } else {
            &self.kit.feeder_amp
        };
        let amp = amp.clone();
        let outputs = output_ports(amp.part);
        let id = self.id("amp");
        let mut ports = vec![port("in", PortRole::Input)];
        ports.extend(outputs.iter().map(|o| port(o, PortRole::Output)));
        // Branches on outputs; any more than outputs share the last.
        let mut per_output: Vec<Vec<Branch>> = vec![Vec::new(); outputs.len()];
        let lifted = self.lift(at, branches, outputs.len());
        for (i, b) in lifted.into_iter().enumerate() {
            per_output[i.min(outputs.len() - 1)].push(b);
        }
        let all: Vec<Branch> = per_output.iter().flatten().cloned().collect();
        let lowest = self.arrive(&line, &[]).hi(REF) + self.amp_reach_db;
        let (out, tilt) = self.station(at, &all, false, lowest);
        let fitted = Fitted::Active(ActiveFitted {
            input_pad: None,
            input_eq: None,
            outputs: outputs
                .iter()
                .map(|o| OutputFitted {
                    port: PortId::new(o),
                    pad: None,
                    eq: None,
                })
                .collect(),
            return_pad: None,
            return_eq: None,
            set_point: Some(SetPoint {
                out_dbmv_at_hi: out,
                tilt_db: tilt,
            }),
            forward_rx: None,
            return_tx: None,
            transceivers: Vec::new(),
        });
        self.connect(&line, port_ref(&id, "in"));
        self.element(
            id.clone(),
            ElementKind::Amp {
                return_atten_db: 0.0,
            },
            amp.part,
            at,
            ports,
            Some(fitted),
        );
        let levels = Levels::station(&self.band, out, tilt);
        // §15: a return signal from an output crosses its combiner, which alignment pads for the largest.
        let combining = |o: &str| {
            amp.spec
                .outputs
                .iter()
                .find(|x| x.port.as_str() == o)
                .map_or(0.0, |x| x.return_combining_loss_db)
        };
        let worst = amp
            .spec
            .outputs
            .iter()
            .map(|x| x.return_combining_loss_db)
            .fold(0.0, f64::max);
        let _ = worst;
        Ok(outputs
            .iter()
            .zip(per_output)
            .filter(|(_, b)| !b.is_empty())
            .map(|(o, b)| {
                let c = combining(o);
                (
                    Line {
                        from: port_ref(&id, o),
                        at,
                        levels,
                        cascade: line.cascade + 1,
                        tapped: false,
                        runs: Vec::new(),
                        up: Up([[c; 2]; 3]),
                    },
                    b,
                )
            })
            .collect())
    }

    /// The most tilt the express amp's input equalizers add across the band.
    fn max_eq_tilt(&self) -> f64 {
        let family = self.kit.express_amp.spec.plugin_family.as_deref();
        self.kit
            .library
            .components
            .iter()
            .filter_map(|c| match &c.spec {
                ComponentSpec::PlugIn(p)
                    if p.kind == schema::component::PlugInKind::Equalizer
                        && Some(p.family.as_str()) == family =>
                {
                    Some(plugin_loss_db(p, self.band.f[LO]) - plugin_loss_db(p, self.band.f[HI]))
                }
                _ => None,
            })
            .fold(0.0, f64::max)
    }

    /// An active's branches, with a fork that comes before any home lifted onto its free outputs: no splitter is then the
    /// first device after an active with an output to spare.
    fn lift(&self, at: usize, branches: &[Branch], outputs: usize) -> Vec<Branch> {
        let mut out: Vec<Branch> = branches.to_vec();
        loop {
            if out.len() >= outputs {
                return out;
            }
            let mut changed = false;
            for i in 0..out.len() {
                let b = &out[i];
                let mut runs = b.runs.clone();
                let mut v = b.to;
                loop {
                    if self.area.homes_at.contains_key(&v) {
                        break;
                    }
                    let kids = self.area.children.get(&v).cloned().unwrap_or_default();
                    if kids.len() == 1 {
                        runs.push(self.area.run_to[&kids[0]]);
                        v = kids[0];
                        continue;
                    }
                    if kids.len() >= 2 && out.len() < outputs {
                        // The bar on branches holds whatever the common path's length: its cables run side by side.
                        let probe = Line {
                            from: port_ref(&ElementId::new("probe"), "out"),
                            at,
                            levels: Levels::station(
                                &self.band,
                                self.kit.station.out_dbmv_at_hi.value,
                                self.kit.station.tilt_db.value,
                            ),
                            cascade: 0,
                            tapped: false,
                            runs: runs.clone(),
                            up: Up::default(),
                        };
                        let _ = &probe;
                        {
                            let replacement: Vec<Branch> = kids
                                .iter()
                                .map(|k| {
                                    let mut r = runs.clone();
                                    r.push(self.area.run_to[k]);
                                    Branch { runs: r, to: *k }
                                })
                                .collect();
                            out.splice(i..=i, replacement);
                            changed = true;
                        }
                    }
                    break;
                }
                if changed {
                    break;
                }
            }
            if !changed {
                return out;
            }
        }
    }

    /// Cuts a line equalizer into a line at `at`.
    fn line_eq(&mut self, line: &Line, at: usize, leq: &super::kit::LineEq<'a>) -> Line {
        let input = self.arrive(line, &[]);
        let id = self.id("leq");
        self.connect(line, port_ref(&id, "in"));
        self.element(
            id.clone(),
            ElementKind::LineEqualizer,
            leq.part,
            at,
            vec![port("in", PortRole::Input), port("out", PortRole::Through)],
            None,
        );
        Line {
            from: port_ref(&id, "out"),
            at,
            levels: input.less(&self.band, |f, _| through(leq.part, f)),
            cascade: line.cascade,
            tapped: line.tapped,
            runs: Vec::new(),
            up: self
                .arrive_up(line, &[])
                .plus(&self.band, |f, _| through(leq.part, f)),
        }
    }

    /// The taps at a structure, chained, from `input`; a terminating one last where the line `ends` and the rules say so.
    /// As few taps as serve the homes first; then, if those don't fit, the homes over more, smaller taps, whose values run
    /// lower.
    fn try_taps(
        &self,
        input: &Levels,
        up: &Up,
        site: usize,
        ends: bool,
    ) -> Result<Vec<TapChoice<'a>>, Misfit> {
        let homes = self.area.homes_at.get(&site).map_or(0, Vec::len);
        let mut splits = vec![split_homes(self.kit, homes)];
        for family in &self.kit.taps {
            let p = family.ports as usize;
            if p >= homes || p == 0 {
                continue;
            }
            let mut sizes = vec![p; homes / p];
            if !homes.is_multiple_of(p) {
                sizes.push(homes % p);
            }
            if !splits.contains(&sizes) {
                splits.push(sizes);
            }
        }
        let mut first_err = None;
        for sizes in splits {
            match self.taps_for(input, up, site, &sizes, ends) {
                Ok(chain) => return Ok(chain),
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        Err(first_err.unwrap_or(Misfit::TooLow))
    }

    fn taps_for(
        &self,
        input: &Levels,
        up: &Up,
        site: usize,
        sizes: &[usize],
        ends: bool,
    ) -> Result<Vec<TapChoice<'a>>, Misfit> {
        let mut out = Vec::new();
        let mut level = *input;
        let mut up = *up;
        let room_base = self.return_room(site);
        let (drops_ft, group_drops_ft) = self.drops_at(site);
        for (i, n) in sizes.iter().enumerate() {
            let last = i + 1 == sizes.len();
            let family = family_for(self.kit, *n);
            let mut room = room_base;
            for t in 0..3 {
                for k in 0..2 {
                    room.0[t][k] -= up.0[t][k];
                }
            }
            let choice = super::feeder::fit_tap_within(
                self.kit,
                &self.band,
                &level,
                family,
                last && ends && self.kit.end_terminating,
                super::feeder::Behind {
                    room: Some(&room),
                    up_in: Some(&up),
                    drops_ft: &drops_ft,
                    group_drops_ft: &group_drops_ft,
                    margin_db: super::feeder::RESIDUAL_DB,
                },
            )?;
            if let Some(next) = choice.out {
                level = next;
            }
            up = up.plus(&self.band, |f, _| through(choice.tap.part, f));
            out.push(choice);
        }
        Ok(out)
    }

    /// The shortest and longest drop at a structure, the two a tap's modems are judged on: to its homes, and to its
    /// apartment groups' boxes (task 25).
    fn drops_at(&self, site: usize) -> (Vec<f64>, Vec<f64>) {
        let mut homes = Vec::new();
        let mut groups = Vec::new();
        for h in self.area.homes_at.get(&site).into_iter().flatten() {
            if let Some(at) = self.s.homes[*h] {
                if self.s.groups.contains_key(h) {
                    groups.push(at.drop_ft);
                } else {
                    homes.push(at.drop_ft);
                }
            }
        }
        let ends = |lens: Vec<f64>| -> Vec<f64> {
            if lens.is_empty() {
                return Vec::new();
            }
            let lo = lens.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = lens.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            vec![lo, hi]
        };
        (ends(homes), ends(groups))
    }

    /// §7.2: the most return loss a structure's tap ports and everything back to the active may add for its homes'
    /// modems to transmit inside the lower of the two modem parts' limits, past the longest drop there and the least
    /// lossy template the profile allows.
    fn return_room(&self, site: usize) -> Up {
        let parts = &self.kit.complete.premises_parts;
        let limit = [&parts.modem_d30_ref.value, &parts.modem_d31_ref.value]
            .iter()
            .filter_map(|id| self.kit.library.get(id))
            .map(|m| super::drops::tx_limit(self.kit, m))
            .fold(f64::INFINITY, f64::min);
        let budget = limit + sim::upstream::per_1p6_from_per_6p4_db()
            - self.kit.profile.design_levels.us_node_in_target_dbmv;
        let longest = self
            .area
            .homes_at
            .get(&site)
            .into_iter()
            .flatten()
            .filter_map(|h| self.s.homes[*h])
            .map(|h| h.drop_ft)
            .fold(0.0, f64::max);
        let drop = self
            .kit
            .rules
            .drops
            .iter()
            .find(|d| d.max_len_ft.value >= longest)
            .or(self.kit.rules.drops.last())
            .and_then(|d| self.kit.library.get(&d.cable_ref.value));
        let mut room = Up([[budget; 2]; 3]);
        for (t, temp) in super::levels::TEMPS.iter().enumerate() {
            for (k, f) in self.band.us.iter().enumerate() {
                let drop_loss = match drop.map(|d| &d.spec) {
                    Some(ComponentSpec::Drop(spec)) => {
                        cable(spec, longest)(*f, *temp) + 2.0 * through(self.kit.drop_f, *f)
                    }
                    _ => 0.0,
                };
                let premises = self
                    .kit
                    .templates
                    .iter()
                    .map(|tpl| super::drops::premises_loss_dir(self.kit, tpl, *f, *temp, false))
                    .fold(f64::INFINITY, f64::min);
                room.0[t][k] -= drop_loss + if premises.is_finite() { premises } else { 0.0 };
            }
        }
        room
    }

    /// Places taps at `at` and returns the line leaving the last one's through port, or `None` where the line ends there.
    fn place_taps(
        &mut self,
        line: &Line,
        at: usize,
        chain: Vec<TapChoice<'a>>,
        ends: bool,
    ) -> Option<Line> {
        let homes: Vec<usize> = self.area.homes_at.get(&at).cloned().unwrap_or_default();
        let mut up = self.arrive_up(line, &[]);
        let mut line = line.clone();
        let mut next_home = 0;
        let count = chain.len();
        // Homes per tap: each takes as many as its ports, in order, the last whatever's left.
        let mut left = homes.len();
        let sizes: Vec<usize> = chain
            .iter()
            .map(|c| {
                let n = (c.tap.spec.ports as usize).min(left);
                left -= n;
                n
            })
            .collect();
        for (i, choice) in chain.into_iter().enumerate() {
            let id = self.id("tap");
            let spec = choice.tap.spec;
            let mut ports = vec![port("in", PortRole::Input)];
            if !spec.terminating {
                ports.push(port("through", PortRole::Through));
            }
            let mut served = Vec::new();
            let mut states = Vec::new();
            for p in 1..=spec.ports {
                let name = format!("tap-{p}");
                ports.push(port(&name, PortRole::TapPort));
                if next_home < homes.len() && served.len() < sizes[i] {
                    served.push((PortId::new(&name), homes[next_home]));
                    next_home += 1;
                } else {
                    states.push(PortStateEntry {
                        port: PortId::new(&name),
                        state: match self.kit.unused_port {
                            UnusedPort::Terminated => PortState::Terminated {
                                component_ref: self.kit.terminator.id.clone(),
                            },
                            UnusedPort::Open => PortState::Open,
                        },
                    });
                }
            }
            let fitted = (choice.conditioner.is_some() || !states.is_empty()).then(|| {
                Fitted::Passive(PassiveFitted {
                    conditioner: choice.conditioner.as_ref().map(|c| c.part.id.clone()),
                    ports: states,
                })
            });
            self.connect(&line, port_ref(&id, "in"));
            self.element(
                id.clone(),
                ElementKind::Tap,
                choice.tap.part,
                at,
                ports,
                fitted,
            );
            let port_up = up.plus(&self.band, |f, _| {
                super::levels::tap_port(choice.tap.part, f)
                    + choice
                        .conditioner
                        .as_ref()
                        .map_or(0.0, |c| through(c.part, f))
            });
            self.taps.push(TapHomes {
                tap: id.clone(),
                site: at,
                ports: served,
                port_levels: choice.port,
                up: port_up,
            });
            up = up.plus(&self.band, |f, _| through(choice.tap.part, f));
            let last = i + 1 == count;
            let out = choice.out?;
            line = Line {
                from: port_ref(&id, "through"),
                at,
                levels: out,
                cascade: line.cascade,
                tapped: true,
                runs: Vec::new(),
                up,
            };
            if last && ends {
                // A through tap at a line's end: its through port terminated.
                let term = self.id("term");
                let id = self.edge_id("direct");
                self.edges.push(Edge {
                    id,
                    from: line.from.clone(),
                    to: port_ref(&term, "in"),
                    kind: EdgeKind::Direct,
                    record_status: None,
                });
                let part = self.kit.terminator;
                self.element(
                    term,
                    ElementKind::Terminator,
                    part,
                    at,
                    vec![port("in", PortRole::Input)],
                    None,
                );
                return None;
            }
        }
        Some(line)
    }

    /// The device that splits the lightest of `branches` off a line at `at`, and the rest: a coupler of the highest value
    /// the lighter side still gets enough from when it has no more than half the others' homes, a splitter otherwise.
    fn choose_device(
        &self,
        line: &Line,
        at: usize,
        mut branches: Vec<Branch>,
    ) -> (&'k Component, bool, Branch, Vec<Branch>) {
        let weight = |b: &Branch| self.area.homes_below(b.to);
        branches.sort_by(|a, b| weight(b).cmp(&weight(a)).then(a.to.cmp(&b.to)));
        let light = branches
            .pop()
            .unwrap_or_else(|| unreachable!("two or more"));
        let heavy_homes: usize = branches.iter().map(&weight).sum();
        let input = self.arrive(line, &[]);
        if weight(&light) * 2 <= heavy_homes {
            for c in self.kit.couplers.iter().rev() {
                let probe = Line {
                    levels: input.less(&self.band, |_, _| coupled_leg(c)),
                    runs: Vec::new(),
                    at,
                    ..line.clone()
                };
                // Fit for an amp there, and for the taps at its first structure.
                if self.reaches(&probe, &light) {
                    return (c, true, light, branches);
                }
            }
        }
        (self.kit.splitter, false, light, branches)
    }

    /// What each branch would get from the devices `branch` would place at `at`, without placing them: the line leaving
    /// each device's leg toward it.
    fn plan_branch(&self, line: &Line, at: usize, branches: Vec<Branch>) -> Vec<(Branch, Line)> {
        if branches.len() <= 1 {
            return branches.into_iter().map(|b| (b, line.clone())).collect();
        }
        let (part, is_coupler, light, rest) = self.choose_device(line, at, branches);
        let input = self.arrive(line, &[]);
        let up_in = self.arrive_up(line, &[]);
        let heavy = Line {
            levels: input.less(&self.band, |f, _| through(part, f)),
            up: up_in.plus(&self.band, |f, _| through(part, f)),
            runs: Vec::new(),
            at,
            ..line.clone()
        };
        let light_line = if is_coupler {
            Line {
                levels: input.less(&self.band, |_, _| coupled_leg(part)),
                up: up_in.plus(&self.band, |_, _| coupled_leg(part)),
                ..heavy.clone()
            }
        } else {
            heavy.clone()
        };
        let mut out = vec![(light, light_line)];
        out.extend(self.plan_branch(&heavy, at, rest));
        out
    }

    /// Splits a line at `at` into `branches` with splitters and couplers, as `choose_device` picks them.
    fn branch(
        &mut self,
        line: &Line,
        at: usize,
        branches: Vec<Branch>,
    ) -> Vec<(Line, Vec<Branch>)> {
        if branches.len() <= 1 {
            return vec![(line.clone(), branches)];
        }
        let input = self.arrive(line, &[]);
        let (part, is_coupler, light, branches) = self.choose_device(line, at, branches);
        let id = if is_coupler {
            self.id("dc")
        } else {
            self.id("sp")
        };
        let (kind, ports, heavy_port, light_port) = if is_coupler {
            (
                ElementKind::DirectionalCoupler,
                vec![
                    port("in", PortRole::Input),
                    port("through", PortRole::Through),
                    port("coupled", PortRole::Coupled),
                ],
                "through",
                "coupled",
            )
        } else {
            (
                ElementKind::Splitter,
                vec![
                    port("in", PortRole::Input),
                    port("leg-1", PortRole::Leg),
                    port("leg-2", PortRole::Leg),
                ],
                "leg-1",
                "leg-2",
            )
        };
        self.connect(line, port_ref(&id, "in"));
        self.element(id.clone(), kind, part, at, ports, None);
        let heavy_levels = input.less(&self.band, |f, _| through(part, f));
        let light_levels = if is_coupler {
            input.less(&self.band, |_, _| coupled_leg(part))
        } else {
            heavy_levels
        };
        let _ = coupled(part);
        let up_in = self.arrive_up(line, &[]);
        let heavy_up = up_in.plus(&self.band, |f, _| through(part, f));
        let light_up = if is_coupler {
            up_in.plus(&self.band, |_, _| coupled_leg(part))
        } else {
            heavy_up
        };
        let make = |port_name: &str, levels: Levels, up: Up| Line {
            from: port_ref(&id, port_name),
            at,
            levels,
            cascade: line.cascade,
            tapped: line.tapped,
            runs: Vec::new(),
            up,
        };
        let mut out = vec![(make(light_port, light_levels, light_up), vec![light])];
        let heavy_line = make(heavy_port, heavy_levels, heavy_up);
        out.extend(self.branch(&heavy_line, at, branches));
        out
    }

    /// Walks every branch from a line at `at`, taps, amps and branches as it goes, until every home in them is served.
    fn walk(&mut self, start: Line, branches: Vec<Branch>) -> Result<(), Overflow> {
        let mut work: Vec<(Line, Vec<Branch>)> = vec![(start, branches)];
        while let Some((line, branches)) = work.pop() {
            if branches.is_empty() {
                continue;
            }
            // Several branches from here need a device; an active's output that should carry them each is arranged by
            // `amp` before they get here.
            if branches.len() > 1 {
                // The device goes where the line's pending cable ends: the fork.
                let fork = self.end_of(&line);
                for item in self.branch(&line, fork, branches).into_iter().rev() {
                    work.push(item);
                }
                continue;
            }
            let b = &branches[0];
            if b.runs.len() > 1 {
                // A branch `lift` gave an output runs past structures with no homes of their own, to the fork it lifted:
                // one run at a time, so each of them is looked at for an amp as any empty structure is.
                let mid = self.s.runs[b.runs[0]].other(self.end_of(&line));
                let rest = Branch {
                    runs: b.runs[1..].to_vec(),
                    to: b.to,
                };
                // Looked at a run ahead, as an empty structure looks at its next one: the last structure before the
                // line can't go on is where the amp goes.
                let step = if rest.runs.len() > 1 {
                    Branch {
                        runs: vec![rest.runs[0]],
                        to: self.s.runs[rest.runs[0]].other(mid),
                    }
                } else {
                    rest.clone()
                };
                let mut line = line.clone();
                line.runs.push(b.runs[0]);
                if self.needs_amp(&line, std::slice::from_ref(&step)) {
                    if !self.amp_ok(&self.arrive(&line, &[]), &line) {
                        self.notes
                            .push(format!("amp input short at {}", self.s.sites[mid].id));
                    }
                    let lines = self.amp(&line, mid, &[rest], !line.tapped)?;
                    work.extend(lines.into_iter().rev());
                } else {
                    work.push((line, vec![rest]));
                }
                continue;
            }
            let mut line = line.clone();
            line.runs.extend(b.runs.iter().copied());
            let v = b.to;
            let homes = self.area.homes_at.get(&v).map_or(0, Vec::len);
            let kids: Vec<usize> = self.area.children.get(&v).cloned().unwrap_or_default();
            let ends = kids.is_empty();
            let next: Vec<Branch> = kids
                .iter()
                .map(|k| Branch {
                    runs: vec![self.area.run_to[k]],
                    to: *k,
                })
                .collect();
            let input = self.arrive(&line, &[]);
            if homes == 0 {
                // An empty structure: an amp here if the next one would be too far, else carry on.
                if self.needs_amp(&line, &next) {
                    if !self.amp_ok(&input, &line) {
                        self.notes
                            .push(format!("amp input short at {}", self.s.sites[v].id));
                    }
                    let lines = self.amp(&line, v, &next, !line.tapped)?;
                    work.extend(lines.into_iter().rev());
                } else {
                    work.push((line, next));
                }
                continue;
            }
            // Taps here: as they are, or behind a line equalizer if the line's gone too flat.
            let up = self.arrive_up(&line, &[]);
            let mut taps = self.try_taps(&input, &up, v, ends);
            let mut fitting = None;
            let mut through_fitting = None;
            if matches!(taps, Err(Misfit::TooFlat)) {
                for leq in &self.kit.line_eqs {
                    let fitted = self.try_taps(
                        &input.less(&self.band, |f, _| through(leq.part, f)),
                        &up.plus(&self.band, |f, _| through(leq.part, f)),
                        v,
                        ends,
                    );
                    if fitted.is_ok() {
                        fitting = Some(leq.clone());
                        through_fitting = Some(fitted);
                        break;
                    }
                }
            }
            // An amp before them if the signal's run out for them, or if it will have run out for what comes after
            // and their own loss leaves too little for an amp after them.
            let short_after = match through_fitting.as_ref().unwrap_or(&taps) {
                Ok(chain) => {
                    let after = chain.last().and_then(|c| c.out).unwrap_or(input);
                    let probe = Line {
                        levels: after,
                        runs: Vec::new(),
                        tapped: true,
                        at: v,
                        ..line.clone()
                    };
                    !ends && self.needs_amp(&probe, &next) && !self.amp_ok(&after, &line)
                }
                Err(Misfit::TooLow) => true,
                Err(_) => false,
            };
            // Where an amp goes in anyway, its own input equalizer does the line equalizer's work: a station takes an
            // input pad and EQ, set from the map (design-practice.md §1, §4). The line equalizer stays only where the
            // amp couldn't take the line without it.
            if short_after && self.amp_ok(&input, &line) {
                fitting = None;
            }
            if let Some(leq) = fitting {
                line = self.line_eq(&line, v, &leq);
                taps = self.try_taps(
                    &self.arrive(&line, &[]),
                    &self.arrive_up(&line, &[]),
                    v,
                    ends,
                );
            }
            if short_after && self.amp_ok(&self.arrive(&line, &[]), &line) {
                let here = Branch {
                    runs: Vec::new(),
                    to: v,
                };
                let mut lines = self.amp(&line, v, &[here], !line.tapped)?;
                if let Some((amp_line, _)) = lines.pop() {
                    line = amp_line;
                    taps = self.try_taps(
                        &self.arrive(&line, &[]),
                        &self.arrive_up(&line, &[]),
                        v,
                        ends,
                    );
                }
            }
            let chain = match taps {
                Ok(chain) => chain,
                Err(misfit) => {
                    self.notes
                        .push(format!("{misfit:?} at {}", self.s.sites[v].id));
                    self.best_effort(&self.arrive(&line, &[]), homes, ends)
                }
            };
            let Some(after) = self.place_taps(&line, v, chain, ends) else {
                continue;
            };
            // After the taps: an amp here if any next structure would arrive too low for its taps or too worn for an amp.
            if self.needs_amp(&after, &next) {
                let lines = self.amp(&after, v, &next, false)?;
                work.extend(lines.into_iter().rev());
            } else {
                work.push((after, next));
            }
        }
        Ok(())
    }

    /// The structure a line's pending cable reaches.
    fn end_of(&self, line: &Line) -> usize {
        let mut at = line.at;
        for r in &line.runs {
            at = self.s.runs[*r].other(at);
        }
        at
    }

    /// Whether a line can't carry on to every one of `next` without an amp. Where it branches, past the devices `branch`
    /// would place there, by their own losses.
    fn needs_amp(&self, line: &Line, next: &[Branch]) -> bool {
        if next.len() <= 1 {
            return next.iter().any(|n| !self.reaches(line, n));
        }
        let fork = self.end_of(line);
        let at_fork = Line {
            levels: self.arrive(line, &[]),
            up: self.arrive_up(line, &[]),
            runs: Vec::new(),
            at: fork,
            ..line.clone()
        };
        // The fork's line has no cable pending: `arrive` from it would count a jumper the device placement also lays.
        self.plan_branch(
            &Line {
                levels: line.levels,
                up: line.up,
                ..line.clone()
            },
            fork,
            next.to_vec(),
        )
        .iter()
        .any(|(n, l)| {
            let _ = &at_fork;
            !self.reaches(l, n)
        })
    }

    /// Whether a line carries on to `next`: it arrives fit for an amp, and fit for the taps there if it has homes.
    fn reaches(&self, line: &Line, next: &Branch) -> bool {
        let arrival = self.arrive(line, &next.runs);
        if !self.amp_ok(&arrival, line) {
            return false;
        }
        let homes = self.area.homes_at.get(&next.to).map_or(0, Vec::len);
        if homes == 0 {
            return true;
        }
        let ends = self.area.children.get(&next.to).is_none_or(Vec::is_empty);
        // A signal run short calls for an amp; so does one run too flat or too worn for the taps there, if an amp here
        // could take what arrives here (its largest pad brings it to a station level). One too hot an amp makes no
        // better.
        let chain = match self.try_taps(&arrival, &self.arrive_up(line, &next.runs), next.to, ends)
        {
            Ok(chain) => chain,
            Err(Misfit::TooLow) => return false,
            Err(Misfit::TooFlat) => {
                let here = self.arrive(line, &[]);
                return here.hi(REF) + self.amp_reach_db
                    > self.kit.station.out_dbmv_at_hi.value + 1e-9;
            }
            Err(_) => return true,
        };
        // And it mustn't leave that structure needing an amp before its own taps: what's left after them takes an amp,
        // or reaches what comes next without one.
        if ends {
            return true;
        }
        let after = chain.last().and_then(|c| c.out).unwrap_or(arrival);
        if self.amp_ok(&after, line) {
            return true;
        }
        let probe = Line {
            levels: after,
            runs: Vec::new(),
            tapped: true,
            at: next.to,
            ..line.clone()
        };
        let kids: Vec<Branch> = self
            .area
            .children
            .get(&next.to)
            .into_iter()
            .flatten()
            .map(|k| Branch {
                runs: vec![self.area.run_to[k]],
                to: *k,
            })
            .collect();
        kids.iter().all(|k| {
            let a = self.arrive(&probe, &k.runs);
            self.amp_ok(&a, &probe)
        })
    }

    /// When nothing fits: the value nearest the target at the top of the band, so the shortfall is on the record.
    fn best_effort(&self, input: &Levels, homes: usize, ends: bool) -> Vec<TapChoice<'a>> {
        let sizes = split_homes(self.kit, homes);
        let mut out = Vec::new();
        let mut level = *input;
        for (i, n) in sizes.iter().enumerate() {
            let family = family_for(self.kit, *n);
            let last = i + 1 == sizes.len();
            let mut list: Vec<&super::kit::TapPart<'a>> = family.through.iter().collect();
            if last && ends && self.kit.end_terminating {
                list.extend(family.terminating.iter());
            }
            let tap = list
                .into_iter()
                .min_by(|a, b| {
                    let e = |t: &super::kit::TapPart<'_>| {
                        (level.hi(REF)
                            - super::levels::tap_port(t.part, self.band.f[HI])
                            - self.kit.port_hi_dbmv)
                            .abs()
                    };
                    e(a).total_cmp(&e(b))
                })
                .cloned()
                .unwrap_or_else(|| family.through[0].clone());
            let port = super::feeder::port_levels(&self.band, &level, &tap, None);
            let out_levels = (!tap.spec.terminating)
                .then(|| level.less(&self.band, |f, _| through(tap.part, f)));
            if let Some(next) = out_levels {
                level = next;
            }
            out.push(TapChoice {
                tap,
                conditioner: None,
                port,
                out: out_levels,
            });
        }
        out
    }
}

/// Builds one area's RF plant: the root at its site, every port's line, taps, amps, branches and ends of line.
pub fn build(
    kit: &Kit<'_>,
    structures: &Structures,
    area: &Area,
    service_group: &ServiceGroupId,
) -> Result<Rf, Overflow> {
    let band = Band::new(kit.profile);
    let amp = kit.express_amp.spec.clone();
    let family_ils = {
        let family = amp.plugin_family.as_deref();
        let min_il = |kind: schema::component::PlugInKind| {
            kit.library
                .components
                .iter()
                .filter_map(|c| match &c.spec {
                    ComponentSpec::PlugIn(p)
                        if p.kind == kind && Some(p.family.as_str()) == family =>
                    {
                        Some(plugin_loss_db(p, band.f[HI]))
                    }
                    _ => None,
                })
                .fold(f64::INFINITY, f64::min)
        };
        let pad = min_il(schema::component::PlugInKind::Pad);
        let eq = min_il(schema::component::PlugInKind::Equalizer);
        (if pad.is_finite() { pad } else { 0.0 }) + (if eq.is_finite() { eq } else { 0.0 })
    };
    let gain = sim::passive::active_gain_db(&amp, band.f[HI]);
    let inserter_il = through(kit.inserter, band.f[HI]);
    // §15: an amp reaches the highest station level from this input with its smallest plug-ins, with room for a power
    // inserter at its input.
    // §15: the largest pad in the family, and the most input it brings to the highest station level.
    let pad_max = {
        let family = amp.plugin_family.as_deref();
        kit.library
            .components
            .iter()
            .filter_map(|c| match &c.spec {
                ComponentSpec::PlugIn(p)
                    if p.kind == schema::component::PlugInKind::Pad
                        && Some(p.family.as_str()) == family =>
                {
                    Some(p.value_db)
                }
                _ => None,
            })
            .fold(0.0, f64::max)
    };
    let leq_il = kit
        .line_eqs
        .iter()
        .map(|e| through(e.part, band.f[HI]))
        .fold(0.0, f64::max);
    // ... and for a line equalizer, if its input is tilted further down than its equalizers reach.
    let amp_threshold = (kit.station.out_dbmv_at_hi.value - gain + family_ils).max(amp.min_in_dbmv)
        + inserter_il
        + leq_il;
    let mut w = Walker {
        kit,
        band,
        s: structures,
        area,
        sg: service_group.to_string(),
        elements: Vec::new(),
        edges: Vec::new(),
        counters: BTreeMap::new(),
        taps: Vec::new(),
        sites: BTreeSet::new(),
        runs: BTreeSet::new(),
        notes: Vec::new(),
        actives: 0,
        amp_threshold,
        amp_reach_db: gain - family_ils - pad_max,
        agc_db: amp.agc_range_db,
    };
    let root = ElementId::new(format!("{service_group}-node"));
    let kind = match kit.profile.node_kind {
        NodeKind::Analog => ElementKind::Node {
            return_atten_db: 0.0,
        },
        NodeKind::Rpd => ElementKind::Rpd {
            return_atten_db: 0.0,
        },
    };
    let site = area.site;
    let own_homes = area.homes_at.contains_key(&site);
    // Ports: each port's branches, lifted onto free ports where a fork comes before any home.
    let mut port_branches: Vec<Vec<Branch>> = area
        .ports
        .iter()
        .map(|kids| {
            kids.iter()
                .map(|k| Branch {
                    runs: vec![area.run_to[k]],
                    to: *k,
                })
                .collect()
        })
        .collect();
    loop {
        let free: Vec<usize> = (0..port_branches.len())
            .filter(|p| port_branches[*p].is_empty() && !(own_homes && *p == area.site_port))
            .collect();
        if free.is_empty() {
            break;
        }
        let mut moved = false;
        for p in 0..port_branches.len() {
            if port_branches[p].len() != 1 {
                continue;
            }
            let lifted = w.lift(site, &port_branches[p], 1 + free.len());
            if lifted.len() > 1 {
                // One branch on each free port; any more than there are ports stay on this one, behind a device.
                let mut lifted = lifted.into_iter();
                let mut keep = vec![lifted.next().unwrap_or_else(|| unreachable!())];
                for q in &free {
                    if let Some(b) = lifted.next() {
                        port_branches[*q] = vec![b];
                    }
                }
                keep.extend(lifted);
                port_branches[p] = keep;
                moved = true;
                break;
            }
        }
        if !moved {
            break;
        }
    }
    let all: Vec<Branch> = port_branches.iter().flatten().cloned().collect();
    let (out, tilt) = w.station(site, &all, own_homes, f64::NEG_INFINITY);
    let used: Vec<usize> = (0..port_branches.len())
        .filter(|p| !port_branches[*p].is_empty() || (own_homes && *p == area.site_port))
        .collect();
    let ports: Vec<Port> = (0..kit.rf_ports)
        .map(|p| port(&format!("rf-{}", p + 1), PortRole::Output))
        .collect();
    let fitted = Fitted::Active(ActiveFitted {
        input_pad: None,
        input_eq: None,
        outputs: used
            .iter()
            .map(|p| OutputFitted {
                port: PortId::new(format!("rf-{}", p + 1)),
                pad: None,
                eq: None,
            })
            .collect(),
        return_pad: None,
        return_eq: None,
        set_point: Some(SetPoint {
            out_dbmv_at_hi: out,
            tilt_db: tilt,
        }),
        forward_rx: None,
        return_tx: None,
        transceivers: Vec::new(),
    });
    w.element(root.clone(), kind, kit.root, site, ports, Some(fitted));
    let levels = Levels::station(&w.band, out, tilt);
    for p in used.iter().rev() {
        let mut line = Line {
            from: port_ref(&root, &format!("rf-{}", p + 1)),
            at: site,
            levels,
            cascade: 0,
            tapped: false,
            runs: Vec::new(),
            up: Up::default(),
        };
        // §6: the node's power comes in on its first port, through an inserter right after it.
        if Some(p) == used.first() {
            let pi = w.id("pi");
            let input = w.arrive(&line, &[]);
            let up = w.arrive_up(&line, &[]);
            w.connect(&line, port_ref(&pi, "in"));
            w.element(
                pi.clone(),
                ElementKind::PowerInserter,
                kit.inserter,
                site,
                vec![port("in", PortRole::Input), port("out", PortRole::Output)],
                None,
            );
            line = Line {
                from: port_ref(&pi, "out"),
                at: site,
                levels: input.less(&w.band, |f, _| through(kit.inserter, f)),
                cascade: 0,
                tapped: false,
                runs: Vec::new(),
                up: up.plus(&w.band, |f, _| through(kit.inserter, f)),
            };
        }
        let mut branches = port_branches[*p].clone();
        if own_homes && *p == area.site_port {
            // The site's own homes first, on this port.
            let homes = area.homes_at[&site].len();
            let ends = branches.is_empty();
            let input = w.arrive(&line, &[]);
            let chain = match w.try_taps(&input, &w.arrive_up(&line, &[]), site, ends) {
                Ok(chain) => chain,
                Err(misfit) => {
                    w.notes
                        .push(format!("{misfit:?} at the node's own structure"));
                    w.best_effort(&input, homes, ends)
                }
            };
            match w.place_taps(&line, site, chain, ends) {
                Some(after) => line = after,
                None => continue,
            }
            if branches.iter().any(|b| !w.reaches(&line, b)) {
                let lines = w.amp(&line, site, &branches, false)?;
                for (l, b) in lines {
                    w.walk(l, b)?;
                }
                continue;
            }
            branches = std::mem::take(&mut branches);
        }
        w.walk(line, branches)?;
    }
    Ok(Rf {
        elements: w.elements,
        edges: w.edges,
        taps: w.taps,
        sites: w.sites,
        notes: w.notes,
        root,
        runs: w.runs,
    })
}

/// A component's id, for a note.
pub fn part_id(component: &Component) -> &ComponentId {
    &component.id
}
