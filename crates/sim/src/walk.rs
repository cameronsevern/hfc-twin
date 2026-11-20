//! One pass from the root over a grid of frequencies, branching at each element, so the cost is
//! elements × frequencies. Every level, response and noise term a modem sees comes from here; a
//! scalar question is a grid of one. See `docs/tasks/03-physics-sim.md`.
// Parallel per-frequency arrays read more plainly indexed.
#![allow(clippy::needless_range_loop)]

use std::collections::BTreeMap;
use std::f64::consts::PI;

use plant_graph::PlantGraph;
use schema::common::{ComponentId, ElementId, PortId};
use schema::component::{ActiveSpec, Component, ComponentSpec, GroupDelayPoint};
use schema::environment::EnvironmentSample;
use schema::plant::{Edge, EdgeKind, Element, ElementKind, Fitted, Port, PortRole};
use schema::profile::{NodeKind, Profile};

use crate::cable::{delay_us as cable_delay_us, exposure_temp_c, temp_factor};
use crate::echo;
use crate::error::PhysicsError;
use crate::noise::{THERMAL_DBMV_PER_HZ, cin_db, composite_dbmv, mer_src_db, n_floor_6mhz_dbmv};
use crate::overlay::{AmpOverride, Ingress, Overlay, PointImpairment, ReturnLoss, table_at};
use crate::passive::{
    active_gain_db, active_split, check_rated, coupler_through_db, filter_loss_db,
    filter_return_loss, filter_shape, interpolate, line_eq_loss_db, loss_points_to,
    part_return_loss, plugin_loss_db, plugin_value_shape,
};
use crate::transfer::edge_cable;
use crate::units::{C64, amp, db, lin};

/// Echoes between ordinary parts below this level are too small to matter, and are skipped, dB.
const HEALTHY_ECHO_FLOOR_DB: f64 = -50.0;
/// Echoes involving a fault are kept down to here, dB.
const FAULT_ECHO_FLOOR_DB: f64 = -80.0;
/// Where those floors are read, and where a passive inside a cavity is priced, MHz. Fixed, so a
/// walk's answer at a frequency doesn't depend on what else is on its grid.
/// §8: how fast an amp's CIN worsens per dB of local resonant rise, dB per dB (approx, §4.4).
const RESONANCE_CIN_DB_PER_DB: f64 = 2.0;
const CAVITY_REF_MHZ: f64 = 100.0;
/// §14: an open port reflects totally, dB (approx).
const OPEN_PORT_RL_DB: f64 = 0.0;
const CAVITY_PASSIVE_REF_MHZ: f64 = 500.0;

/// The frequencies a walk answers for, MHz.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    /// The frequencies.
    pub f_mhz: Vec<f64>,
    sqrt_f: Vec<f64>,
    /// Stretches of evenly spaced frequencies at least `CHUNK` long, as (first index, length, spacing MHz).
    runs: Vec<(usize, usize, f64)>,
}

/// Points per chunk where a home's echoes are synthesized by chunks (see `Walker::add_reflector`).
const CHUNK: usize = 32;

impl Grid {
    /// A grid from any list of frequencies.
    pub fn new(f_mhz: Vec<f64>) -> Grid {
        let sqrt_f = f_mhz.iter().map(|f| f.sqrt()).collect();
        let mut runs = Vec::new();
        let mut start = 0;
        while start + 1 < f_mhz.len() {
            let step = f_mhz[start + 1] - f_mhz[start];
            let mut end = start + 1;
            while end + 1 < f_mhz.len()
                && step > 0.0
                && ((f_mhz[end + 1] - f_mhz[end]) - step).abs() <= 1e-9 * step.abs().max(1.0)
            {
                end += 1;
            }
            let len = end - start + 1;
            if step > 0.0 && len >= 2 * CHUNK {
                runs.push((start, len, step));
            }
            start = end;
        }
        Grid {
            f_mhz,
            sqrt_f,
            runs,
        }
    }

    /// `n` frequencies from `first_mhz`, `spacing_mhz` apart.
    pub fn uniform(first_mhz: f64, spacing_mhz: f64, n: usize) -> Grid {
        Grid::new((0..n).map(|i| first_mhz + i as f64 * spacing_mhz).collect())
    }

    fn range(&self) -> (f64, f64) {
        let lo = self.f_mhz.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = self.f_mhz.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (lo, hi)
    }

    fn mid(&self) -> f64 {
        let (lo, hi) = self.range();
        (lo + hi) / 2.0
    }

    fn len(&self) -> usize {
        self.f_mhz.len()
    }
}

/// Which way a walk's noise and gain are being asked about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    /// Forward: levels and forward noise.
    Down,
    /// Return: path loss and return noise.
    Up,
}

/// One echo in a modem's path, for `MER_isi` (§4.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Echo {
    /// Delay, µs.
    pub tau_us: f64,
    /// Linear amplitude at the grid's middle.
    pub rho: f64,
    /// Whether a fault is one of its two reflectors.
    pub fault: bool,
}

/// What a modem's path looks like at each grid frequency.
pub struct ModemView<'v> {
    /// The modem.
    pub modem: &'v Element,
    /// Its part.
    pub component: &'v Component,
    /// The root's RF port it hangs from.
    pub root_port: &'v PortId,
    /// The grid.
    pub grid: &'v Grid,
    /// §3.2: level at the modem, dBmV per 6 MHz, without echoes. Meaningless on `Band::Up`.
    pub level_dbmv: &'v [f64],
    /// §5.1, §5.6: the path's complex response from echoes and active group delay.
    pub h: &'v [C64],
    /// §4.5: the linear noise terms from everything above the modem. Meaningless on `Band::Up`.
    pub inv_plant: &'v [f64],
    /// §7.1: loss below the first active, plus excess above it. Meaningless on `Band::Down`.
    pub loss_up_db: &'v [f64],
    /// The echoes in the path.
    pub echoes: &'v [Echo],
    /// §14: the summed power of the echoes the per-pair floor dropped on this path, linear, relative to the main signal at
    /// §5.1's reference frequency.
    pub discarded_echo_power: f64,
}

/// Edges below which nothing is read (see `Walker::inert`), worked out from the leaves up.
fn inert_edges<'a>(
    graph: &PlantGraph<'a>,
    children: &BTreeMap<ElementId, Vec<&'a Edge>>,
    overlay: &Overlay,
) -> std::collections::BTreeSet<schema::common::EdgeId> {
    use std::collections::BTreeSet;
    // Elements in an order where every child comes after its parent: then read it backwards.
    let mut order: Vec<&ElementId> = vec![&graph.root().id];
    let mut k = 0;
    while k < order.len() {
        if let Some(edges) = children.get(order[k]) {
            order.extend(edges.iter().map(|e| &e.to.element_id));
        }
        k += 1;
    }
    let faulted_ports: BTreeSet<&ElementId> =
        overlay.port_points.keys().map(|(id, _)| id).collect();
    let mut live: BTreeSet<&ElementId> = BTreeSet::new();
    let mut inert = BTreeSet::new();
    for id in order.iter().rev() {
        let Ok(element) = graph.element(id) else {
            continue;
        };
        let reads = matches!(
            element.kind,
            ElementKind::Modem
                | ElementKind::Amp { .. }
                | ElementKind::Node { .. }
                | ElementKind::Rpd { .. }
                | ElementKind::DropAmp
        ) || matches!(element.fitted, Some(Fitted::GroundBlock(_)))
            || faulted_ports.contains(id);
        let mut any_live_child = false;
        for edge in children.get(*id).into_iter().flatten() {
            let edge_live = live.contains(&edge.to.element_id)
                || overlay.span_points.contains_key(&edge.id)
                || overlay.wet.contains_key(&edge.id);
            if edge_live {
                any_live_child = true;
            } else {
                inert.insert(edge.id.clone());
            }
        }
        if reads || any_live_child {
            live.insert(id);
        }
    }
    inert
}

/// `v[i]`, or 0 where `v` holds nothing: a per-frequency term that's 0 everywhere needs no buffer.
fn zero_past(v: &[f64], i: usize) -> f64 {
    v.get(i).copied().unwrap_or(0.0)
}

/// A source MER table as points, by frequency (§17).
fn mer_points(points: &[schema::component::MerPoint]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = points.iter().map(|p| (p.f_mhz, p.mer_db)).collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// Receives each modem's view from a walk, in canonical order.
pub trait ModemSink {
    /// One modem.
    fn modem(&mut self, view: &ModemView<'_>) -> Result<(), PhysicsError>;
}

impl<F: FnMut(&ModemView<'_>) -> Result<(), PhysicsError>> ModemSink for F {
    fn modem(&mut self, view: &ModemView<'_>) -> Result<(), PhysicsError> {
        self(view)
    }
}

/// §7.3: noise at the node per root port, from one `Band::Up` walk, as linear power per Hz.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReturnNoise {
    /// Time-average, per port, per grid frequency.
    pub avg_per_hz: BTreeMap<PortId, Vec<f64>>,
    /// During a burst, per port, per grid frequency.
    pub peak_per_hz: BTreeMap<PortId, Vec<f64>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AgcEntry {
    correction_db: f64,
    cable_ref_pilot_db: f64,
    out_dev_pilot_db: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Facing {
    Down,
    Up,
    Both,
}

#[derive(Debug, Clone)]
struct Reflector {
    /// What it reflects: a wave going down (an input) or up (an output). An echo goes down, back up
    /// from a `Down` reflector, and down again from an `Up` one above it, so only that pair forms a
    /// cavity (§5.1). A fault in the line reflects both.
    facing: Facing,
    rl: ReturnLoss,
    alpha: f64,
    beta: f64,
    gamma_db: f64,
    delay_us: f64,
    fault: bool,
    /// In a home, or a fitting: a version 2 reflector, whose echoes are synthesized by chunks.
    home: bool,
    /// §20.1: −1 where it reflects inverted, as a short; 1 otherwise.
    sign: f64,
}

/// The state of the signal at one point, per grid frequency. Copied with `copy`, which reuses dropped frames' buffers.
struct Frame<'a> {
    out_dev_db: Vec<f64>,
    /// §15: what the deviation is at alignment, 20 °C and healthy: a fitted amp's residual, else 0.
    out_dev_ref_db: Vec<f64>,
    loss_db: Vec<f64>,
    ref_loss_db: Vec<f64>,
    /// This section healthy at this temperature: what a return amp's thermal control holds unity against.
    thermal_loss_db: Vec<f64>,
    cable_ref_db: Vec<f64>,
    h: Vec<C64>,
    inv_plant: Vec<f64>,
    excess_up_db: Vec<f64>,
    reflectors: Vec<Reflector>,
    echoes: Vec<Echo>,
    discarded: f64,
    alpha: f64,
    beta: f64,
    gamma_db: f64,
    delay_us: f64,
    root_port: Option<&'a PortId>,
    /// Below a home's first part, or on house cable (§13): its reflectors are version 2's.
    in_home: bool,
}

/// A frame's buffers, kept for the next frame once one is dropped. Every branch the walk takes copies its parent's frame, and a pass
/// ends by dropping them all; taking buffers back from here instead of the allocator keeps a pass from asking the system for memory
/// it gave back at the end of the last one (task 22's latency bar). The values are copied, so nothing a walk computes changes.
#[derive(Default)]
struct Spare {
    out_dev_db: Vec<f64>,
    out_dev_ref_db: Vec<f64>,
    loss_db: Vec<f64>,
    ref_loss_db: Vec<f64>,
    thermal_loss_db: Vec<f64>,
    cable_ref_db: Vec<f64>,
    h: Vec<C64>,
    inv_plant: Vec<f64>,
    excess_up_db: Vec<f64>,
    reflectors: Vec<Reflector>,
    echoes: Vec<Echo>,
}

/// Spares kept per thread: about as many frames as a walk holds at once on a deep plant.
const SPARES_KEPT: usize = 32;

thread_local! {
    static SPARES: std::cell::RefCell<Vec<Spare>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn take_spare() -> Spare {
    SPARES
        .try_with(|s| s.borrow_mut().pop())
        .ok()
        .flatten()
        .unwrap_or_default()
}

impl<'a> Frame<'a> {
    /// The frame a pass starts from at the root: nothing lost, nothing deviating, a flat response.
    fn fresh(n: usize) -> Frame<'a> {
        let spare = take_spare();
        let zeros = |mut v: Vec<f64>| {
            v.clear();
            v.resize(n, 0.0);
            v
        };
        let mut h = spare.h;
        h.clear();
        h.resize(n, C64::ONE);
        let mut reflectors = spare.reflectors;
        reflectors.clear();
        let mut echoes = spare.echoes;
        echoes.clear();
        Frame {
            out_dev_db: zeros(spare.out_dev_db),
            out_dev_ref_db: zeros(spare.out_dev_ref_db),
            loss_db: zeros(spare.loss_db),
            ref_loss_db: zeros(spare.ref_loss_db),
            thermal_loss_db: zeros(spare.thermal_loss_db),
            cable_ref_db: zeros(spare.cable_ref_db),
            h,
            inv_plant: zeros(spare.inv_plant),
            excess_up_db: zeros(spare.excess_up_db),
            reflectors,
            echoes,
            discarded: 0.0,
            alpha: 0.0,
            beta: 0.0,
            gamma_db: 0.0,
            delay_us: 0.0,
            root_port: None,
            in_home: false,
        }
    }

    /// An exact copy, in a spare's buffers where there is one.
    fn copy(&self) -> Frame<'a> {
        let spare = take_spare();
        let copied = |mut v: Vec<f64>, from: &Vec<f64>| {
            v.clone_from(from);
            v
        };
        let mut h = spare.h;
        h.clone_from(&self.h);
        let mut reflectors = spare.reflectors;
        reflectors.clone_from(&self.reflectors);
        let mut echoes = spare.echoes;
        echoes.clone_from(&self.echoes);
        Frame {
            out_dev_db: copied(spare.out_dev_db, &self.out_dev_db),
            out_dev_ref_db: copied(spare.out_dev_ref_db, &self.out_dev_ref_db),
            loss_db: copied(spare.loss_db, &self.loss_db),
            ref_loss_db: copied(spare.ref_loss_db, &self.ref_loss_db),
            thermal_loss_db: copied(spare.thermal_loss_db, &self.thermal_loss_db),
            cable_ref_db: copied(spare.cable_ref_db, &self.cable_ref_db),
            h,
            inv_plant: copied(spare.inv_plant, &self.inv_plant),
            excess_up_db: copied(spare.excess_up_db, &self.excess_up_db),
            reflectors,
            echoes,
            discarded: self.discarded,
            alpha: self.alpha,
            beta: self.beta,
            gamma_db: self.gamma_db,
            delay_us: self.delay_us,
            root_port: self.root_port,
            in_home: self.in_home,
        }
    }
}

impl Drop for Frame<'_> {
    fn drop(&mut self) {
        let spare = Spare {
            out_dev_db: std::mem::take(&mut self.out_dev_db),
            out_dev_ref_db: std::mem::take(&mut self.out_dev_ref_db),
            loss_db: std::mem::take(&mut self.loss_db),
            ref_loss_db: std::mem::take(&mut self.ref_loss_db),
            thermal_loss_db: std::mem::take(&mut self.thermal_loss_db),
            cable_ref_db: std::mem::take(&mut self.cable_ref_db),
            h: std::mem::take(&mut self.h),
            inv_plant: std::mem::take(&mut self.inv_plant),
            excess_up_db: std::mem::take(&mut self.excess_up_db),
            reflectors: std::mem::take(&mut self.reflectors),
            echoes: std::mem::take(&mut self.echoes),
        };
        let _ = SPARES.try_with(|s| {
            let mut spares = s.borrow_mut();
            if spares.len() < SPARES_KEPT {
                spares.push(spare);
            }
        });
    }
}

/// §15: what a reference walk records for `sim::align` and the level budget.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Capture {
    /// Per active: forward, its input at alignment, dBmV per 6 MHz; return, the section above's loss at reference, dB.
    pub inputs: BTreeMap<ElementId, Vec<f64>>,
    /// Per active and output port, forward: its output level, dBmV per 6 MHz.
    pub outputs: BTreeMap<(ElementId, PortId), Vec<f64>>,
}

struct Run<'s, S> {
    sink: &'s mut S,
    capture: Option<Capture>,
    agc_out: Option<BTreeMap<ElementId, AgcEntry>>,
    noise: ReturnNoise,
    grid: &'s Grid,
    band: Band,
    targets: Vec<f64>,
    phases: BTreeMap<ComponentId, Vec<C64>>,
}

/// One forward or return pass at one environment sample with one overlay.
pub struct Walker<'a> {
    graph: &'a PlantGraph<'a>,
    sample: &'a EnvironmentSample,
    overlay: &'a Overlay,
    children: BTreeMap<ElementId, Vec<&'a Edge>>,
    agc: BTreeMap<ElementId, AgcEntry>,
    off_path: BTreeMap<ElementId, Vec<OffPath<'a>>>,
    us_hi_mhz: f64,
    ds_lo_mhz: f64,
    ds_hi_mhz: f64,
    /// Edges whose whole subtree nothing reads: no modem, no active, no ground block where ambient enters (§18), and no fault on
    /// it. Walking one changes no view, capture or noise, so the walk doesn't.
    inert: std::collections::BTreeSet<schema::common::EdgeId>,
}

/// A fault's mismatch seen from a tap it lies below (§5.1).
#[derive(Clone)]
struct OffPath<'a> {
    /// The tap's port the reflector hangs from.
    via_port: &'a Port,
    /// The drop edge it's on, if it's on one of this tap's drops.
    rl: ReturnLoss,
    alpha: f64,
    beta: f64,
    gamma_db: f64,
    delay_us: f64,
    /// §20.1: −1 where it reflects inverted.
    sign: f64,
}

/// §3.1: `L_out(f)`, the output a healthy active holds, dBmV per 6 MHz.
pub fn out_level_dbmv(profile: &Profile, f_mhz: f64) -> f64 {
    let levels = &profile.design_levels;
    let (ds_lo_mhz, ds_hi_mhz) = ds_band(profile);
    levels.out_dbmv_at_hi - levels.tilt_db * (ds_hi_mhz - f_mhz) / (ds_hi_mhz - ds_lo_mhz)
}

/// The downstream band's edges, MHz.
pub fn ds_band(profile: &Profile) -> (f64, f64) {
    let ds = profile
        .band_plan
        .iter()
        .filter(|s| s.carries(schema::profile::Direction::Ds));
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for segment in ds {
        lo = lo.min(segment.lo_mhz);
        hi = hi.max(segment.hi_mhz);
    }
    (lo, hi)
}

/// The upstream band's edges, MHz.
pub fn us_band(profile: &Profile) -> (f64, f64) {
    let us = profile
        .band_plan
        .iter()
        .filter(|s| s.carries(schema::profile::Direction::Us));
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for segment in us {
        lo = lo.min(segment.lo_mhz);
        hi = hi.max(segment.hi_mhz);
    }
    (lo, hi)
}

fn input_port(element: &Element) -> Option<&Port> {
    element.ports.iter().find(|p| p.role == PortRole::Input)
}

fn phase_from_group_delay(table: &[GroupDelayPoint], grid: &Grid) -> Vec<C64> {
    // §5.6: φ(f) = −2π∫τ df. Piecewise linear τ integrates in closed form; below the table τ holds.
    let integral_ns_mhz = |f: f64| -> f64 {
        let Some(first) = table.first() else {
            return 0.0;
        };
        let mut area = first.delay_ns * f.min(first.f_mhz);
        for pair in table.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if f <= a.f_mhz {
                break;
            }
            let end = f.min(b.f_mhz);
            let width = end - a.f_mhz;
            let share = width / (b.f_mhz - a.f_mhz);
            let end_delay = a.delay_ns + share * (b.delay_ns - a.delay_ns);
            area += width * (a.delay_ns + end_delay) / 2.0;
        }
        if let Some(last) = table.last()
            && f > last.f_mhz
        {
            area += (f - last.f_mhz) * last.delay_ns;
        }
        area
    };
    grid.f_mhz
        .iter()
        // ns·MHz is 1e-3 cycles.
        .map(|&f| C64::from_polar(1.0, -2.0 * PI * integral_ns_mhz(f) * 1e-3))
        .collect()
}

impl<'a> Walker<'a> {
    /// Prepares a walk, which includes one pass at the AGC pilot.
    pub fn new(
        graph: &'a PlantGraph<'a>,
        sample: &'a EnvironmentSample,
        overlay: &'a Overlay,
    ) -> Result<Walker<'a>, PhysicsError> {
        let mut children: BTreeMap<ElementId, Vec<&'a Edge>> = BTreeMap::new();
        for edge in &graph.plant().edges {
            children
                .entry(edge.from.element_id.clone())
                .or_default()
                .push(edge);
        }
        for edges in children.values_mut() {
            edges.sort_by(|a, b| a.from.port.cmp(&b.from.port));
        }
        let profile = graph.profile();
        let (us_hi_mhz, ds_lo_mhz) = profile
            .split_edges()
            .ok_or_else(|| PhysicsError::BadComponent(graph.root().id.clone()))?;
        let (_, ds_hi_mhz) = ds_band(profile);
        crate::support::check_supported(graph)?;
        let inert = inert_edges(graph, &children, overlay);
        let mut walker = Walker {
            graph,
            sample,
            overlay,
            children,
            agc: BTreeMap::new(),
            off_path: BTreeMap::new(),
            us_hi_mhz,
            ds_lo_mhz,
            ds_hi_mhz,
            inert,
        };
        walker.off_path = walker.build_off_path()?;
        // §15: the band's top, and any pilot a part states, so each AGC reads where it reads.
        let mut pilots = vec![ds_hi_mhz];
        for active in graph.actives() {
            if let Some(spec) = graph
                .component(&active.component_ref)
                .and_then(|c| c.spec.active())
            {
                for f in &spec.agc_pilots_mhz {
                    if !pilots.contains(f) {
                        pilots.push(*f);
                    }
                }
            }
        }
        let pilot = Grid::new(pilots);
        let mut null = |_: &ModemView<'_>| Ok(());
        let agc = walker.pass(&pilot, Band::Down, &mut null, true)?.0;
        walker.agc = agc;
        Ok(walker)
    }

    /// An active's output level deviation at the AGC pilot, dB. `None` if the walk didn't reach it.
    pub fn out_dev_at_pilot_db(&self, id: &ElementId) -> Option<f64> {
        self.agc.get(id).map(|entry| entry.out_dev_pilot_db)
    }

    /// One pass over `grid`. Modems reach `sink` in canonical order. Returns the return noise, which
    /// is empty on `Band::Down`.
    pub fn run<S: ModemSink>(
        &self,
        grid: &Grid,
        band: Band,
        sink: &mut S,
    ) -> Result<ReturnNoise, PhysicsError> {
        Ok(self.pass(grid, band, sink, false)?.1)
    }

    fn pass<S: ModemSink>(
        &self,
        grid: &Grid,
        band: Band,
        sink: &mut S,
        pilot: bool,
    ) -> Result<(BTreeMap<ElementId, AgcEntry>, ReturnNoise), PhysicsError> {
        let profile = self.graph.profile();
        let targets = match band {
            Band::Down => grid
                .f_mhz
                .iter()
                .map(|&f| out_level_dbmv(profile, f))
                .collect(),
            Band::Up => vec![0.0; grid.len()],
        };
        let mut run = Run {
            sink,
            capture: None,
            agc_out: pilot.then(BTreeMap::new),
            noise: ReturnNoise::default(),
            grid,
            band,
            targets,
            phases: BTreeMap::new(),
        };
        let n = grid.len();
        let frame = Frame::fresh(n);
        self.descend(&mut run, self.graph.root(), frame)?;
        Ok((run.agc_out.unwrap_or_default(), run.noise))
    }

    /// §15: one pass over `grid` that records each active's input and each output port's level, and nothing else.
    pub fn capture(&self, grid: &Grid, band: Band) -> Result<Capture, PhysicsError> {
        let profile = self.graph.profile();
        let targets = match band {
            Band::Down => grid
                .f_mhz
                .iter()
                .map(|&f| out_level_dbmv(profile, f))
                .collect(),
            Band::Up => vec![0.0; grid.len()],
        };
        let mut null = |_: &ModemView<'_>| Ok(());
        let mut run = Run {
            sink: &mut null,
            capture: Some(Capture::default()),
            agc_out: None,
            noise: ReturnNoise::default(),
            grid,
            band,
            targets,
            phases: BTreeMap::new(),
        };
        let n = grid.len();
        let frame = Frame::fresh(n);
        self.descend(&mut run, self.graph.root(), frame)?;
        Ok(run.capture.unwrap_or_default())
    }

    fn component(&self, element: &Element) -> Result<&'a Component, PhysicsError> {
        self.graph
            .component(&element.component_ref)
            .ok_or_else(|| PhysicsError::BadComponent(element.id.clone()))
    }

    fn check_grid(&self, component: &Component, grid: &Grid) -> Result<(), PhysicsError> {
        let (lo, hi) = grid.range();
        check_rated(component, lo)?;
        check_rated(component, hi)
    }

    fn descend<S: ModemSink>(
        &self,
        run: &mut Run<'_, S>,
        element: &'a Element,
        mut frame: Frame<'a>,
    ) -> Result<(), PhysicsError> {
        let component = self.component(element)?;
        if matches!(
            element.kind,
            ElementKind::GroundBlock
                | ElementKind::Filter
                | ElementKind::DropAmp
                | ElementKind::Cpe
        ) {
            frame.in_home = true;
        }
        if let Some(port) = input_port(element) {
            self.apply_port_points(run, element, port, &mut frame);
        }
        match element.kind {
            ElementKind::Modem => {
                // §20.6: a modem with no house power hears nothing and sends nothing.
                if self.overlay.modems_off.contains(&element.id) {
                    for x in frame.loss_db.iter_mut() {
                        *x += crate::faults::CUT_LOSS_DB;
                    }
                }
                let rl = part_return_loss(component, PortRole::Input).unwrap_or(ReturnLoss::Flat {
                    rl_db: f64::INFINITY,
                });
                self.add_reflector(run.grid, &mut frame, rl, Facing::Down, false);
                let Some(root_port) = frame.root_port else {
                    return Ok(());
                };
                let level_dbmv: Vec<f64> = (0..run.grid.len())
                    .map(|i| run.targets[i] + frame.out_dev_db[i] - frame.loss_db[i])
                    .collect();
                let loss_up_db: Vec<f64> = (0..run.grid.len())
                    .map(|i| frame.loss_db[i] + frame.excess_up_db[i])
                    .collect();
                let view = ModemView {
                    modem: element,
                    component,
                    root_port,
                    grid: run.grid,
                    level_dbmv: &level_dbmv,
                    h: &frame.h,
                    inv_plant: &frame.inv_plant,
                    loss_up_db: &loss_up_db,
                    echoes: &frame.echoes,
                    discarded_echo_power: frame.discarded,
                };
                run.sink.modem(&view)
            }
            ElementKind::Terminator => Ok(()),
            // §13: customer equipment reflects, and reports nothing.
            ElementKind::Cpe => {
                if let Some(rl) = part_return_loss(component, PortRole::Input) {
                    self.add_reflector(run.grid, &mut frame, rl, Facing::Down, false);
                }
                Ok(())
            }
            ElementKind::DropAmp => self.cross_drop_amp(run, element, component, frame),
            ElementKind::Node { .. } | ElementKind::Rpd { .. } | ElementKind::Amp { .. } => {
                self.cross_active(run, element, component, frame)
            }
            _ => self.cross_passive(run, element, component, frame),
        }
    }

    fn cross_passive<S: ModemSink>(
        &self,
        run: &mut Run<'_, S>,
        element: &'a Element,
        component: &'a Component,
        mut frame: Frame<'a>,
    ) -> Result<(), PhysicsError> {
        self.check_grid(component, run.grid)?;
        let offset_db = self
            .overlay
            .element_offset_db
            .get(&element.id)
            .copied()
            .unwrap_or(0.0);
        // §13: a filter's loss and mismatch have a shape; every other passive's return loss is flat.
        let filter = match &component.spec {
            ComponentSpec::Filter(spec) => Some((spec, filter_shape(spec))),
            _ => None,
        };
        let rl_at = |role: PortRole| match &filter {
            Some((spec, shape)) => Some(filter_return_loss(spec, shape)),
            None => part_return_loss(component, role),
        };
        if let Some(rl) = rl_at(PortRole::Input) {
            self.add_reflector(run.grid, &mut frame, rl, Facing::Down, false);
        }
        // §14: each stated port reflects at the input, behind the passive's loss to it twice over.
        if let Some(schema::plant::Fitted::Passive(passive)) = &element.fitted {
            for entry in &passive.ports {
                let Some(port) = element.ports.iter().find(|p| p.id == entry.port) else {
                    continue;
                };
                let state_rl = match &entry.state {
                    schema::plant::PortState::Open => ReturnLoss::Flat {
                        rl_db: OPEN_PORT_RL_DB,
                    },
                    schema::plant::PortState::Terminated { component_ref } => {
                        let terminator = self
                            .graph
                            .component(component_ref)
                            .ok_or_else(|| PhysicsError::BadComponent(element.id.clone()))?;
                        part_return_loss(terminator, PortRole::Input)
                            .ok_or_else(|| PhysicsError::BadComponent(element.id.clone()))?
                    }
                };
                let rl = ReturnLoss::Behind {
                    rl: Box::new(state_rl),
                    loss: loss_points_to(component, port.role),
                };
                self.add_reflector(run.grid, &mut frame, rl, Facing::Down, false);
            }
        }
        let Some(edges) = self.children.get(&element.id) else {
            return Ok(());
        };
        let leg_isolation_db = match &component.spec {
            ComponentSpec::Splitter(spec) => spec.leg_isolation_db,
            _ => None,
        };
        // §15: an in-line equalizer's shape, forward only where it's diplexed; a tap's faceplate conditioner on its ports.
        let line_eq = match &component.spec {
            ComponentSpec::LineEqualizer(spec) => Some(spec),
            _ => None,
        };
        let conditioner = match (&element.fitted, &component.spec) {
            (Some(schema::plant::Fitted::Passive(passive)), ComponentSpec::Tap(_)) => passive
                .conditioner
                .as_ref()
                .and_then(|id| self.graph.component(id))
                .and_then(|c| match &c.spec {
                    ComponentSpec::PlugIn(spec) => Some(spec),
                    _ => None,
                }),
            _ => None,
        };
        // The last branch read takes the frame itself; the others take a copy. A part with one way through copies nothing.
        let last = edges
            .iter()
            .rposition(|e| self.inert.is_empty() || !self.inert.contains(&e.id));
        let mut frame = Some(frame);
        for (k, edge) in edges.iter().enumerate() {
            if !self.inert.is_empty() && self.inert.contains(&edge.id) {
                continue;
            }
            let port = element
                .ports
                .iter()
                .find(|p| p.id == edge.from.port)
                .ok_or_else(|| PhysicsError::BadComponent(element.id.clone()))?;
            let mut branch = if Some(k) == last {
                frame
                    .take()
                    .ok_or_else(|| PhysicsError::BadComponent(element.id.clone()))?
            } else {
                frame
                    .as_ref()
                    .map(Frame::copy)
                    .ok_or_else(|| PhysicsError::BadComponent(element.id.clone()))?
            };
            for i in 0..run.grid.len() {
                let f = run.grid.f_mhz[i];
                let loss = match &filter {
                    Some((spec, shape)) => {
                        branch.h[i] = branch.h[i] * C64::from_polar(1.0, shape.phase_rad(f));
                        filter_loss_db(spec, shape, f)
                    }
                    None => match line_eq {
                        Some(spec) if run.band == Band::Up && spec.duplex.is_some() => {
                            spec.insertion_loss_db
                        }
                        Some(spec) => line_eq_loss_db(spec, f),
                        None => passive_loss_fast(component, port.role, f),
                    },
                } + offset_db
                    + match conditioner {
                        Some(spec) if port.role == PortRole::TapPort => plugin_loss_db(spec, f),
                        _ => 0.0,
                    };
                branch.loss_db[i] += loss;
                branch.ref_loss_db[i] += loss;
                branch.thermal_loss_db[i] += loss;
            }
            branch.gamma_db += match &filter {
                Some((spec, shape)) => filter_loss_db(spec, shape, CAVITY_PASSIVE_REF_MHZ),
                None => match line_eq {
                    Some(spec) => line_eq_loss_db(spec, CAVITY_PASSIVE_REF_MHZ),
                    None => passive_loss_fast(component, port.role, CAVITY_PASSIVE_REF_MHZ),
                },
            } + offset_db
                + match conditioner {
                    Some(spec) if port.role == PortRole::TapPort => {
                        plugin_loss_db(spec, CAVITY_PASSIVE_REF_MHZ)
                    }
                    _ => 0.0,
                };
            self.apply_port_points(run, element, port, &mut branch);
            // §18: ambient enters at a ground block, on the home's side of it, its coupling below the source.
            if run.band == Band::Up
                && let Some(schema::plant::Fitted::GroundBlock(block)) = &element.fitted
            {
                for source in &self.overlay.ambient {
                    add_us_ingress(run, source, block.coupling_db, &mut branch);
                }
            }
            if let Some(rl) = rl_at(port.role) {
                self.add_reflector(run.grid, &mut branch, rl, Facing::Up, false);
            }
            if port.role == PortRole::TapPort {
                self.apply_off_path(run.grid, element, component, port, &mut branch);
            }
            if let (Some(iso_db), PortRole::Leg) = (leg_isolation_db, port.role) {
                self.apply_leg_isolation(run.grid, element, edges, edge, iso_db, &mut branch)?;
            }
            self.cross_edge(run, edge, &mut branch)?;
            let child = self.graph.element(&edge.to.element_id)?;
            self.descend(run, child, branch)?;
        }
        Ok(())
    }

    /// §13: a drop amp. Gain and noise each way, then a new cavity section, since it's directional.
    fn cross_drop_amp<S: ModemSink>(
        &self,
        run: &mut Run<'_, S>,
        element: &'a Element,
        component: &'a Component,
        mut frame: Frame<'a>,
    ) -> Result<(), PhysicsError> {
        let ComponentSpec::DropAmp(amp) = &component.spec else {
            return Err(PhysicsError::BadComponent(element.id.clone()));
        };
        self.check_grid(component, run.grid)?;
        let n = run.grid.len();
        self.add_reflector(
            run.grid,
            &mut frame,
            ReturnLoss::Flat {
                rl_db: amp.return_loss_db,
            },
            Facing::Down,
            false,
        );
        let gain_db = match run.band {
            Band::Down => {
                // Its noise enters §4.5 from its own input level.
                let floor_dbmv = n_floor_6mhz_dbmv();
                for i in 0..n {
                    let level_dbmv = run.targets[i] + frame.out_dev_db[i] - frame.loss_db[i];
                    let cnr = level_dbmv + db(frame.h[i].norm_sqr()) - amp.nf_db - floor_dbmv;
                    frame.inv_plant[i] += lin(-cnr);
                }
                amp.gain_db
            }
            Band::Up => match amp.return_gain_db {
                Some(gain_db) => {
                    // An amplified return adds its noise at its place, reaching the node as ingress does (§7.3).
                    if let Some(port) = frame.root_port {
                        let nf_db = amp.return_nf_db.unwrap_or(amp.nf_db);
                        let per_hz = lin(THERMAL_DBMV_PER_HZ + nf_db + gain_db);
                        let (avg, peak) = port_noise(&mut run.noise, port, n);
                        for i in 0..n {
                            let reach = lin(-(frame.loss_db[i] + frame.excess_up_db[i]));
                            avg[i] += per_hz * reach;
                            peak[i] += per_hz * reach;
                        }
                    }
                    gain_db
                }
                None => -amp.return_passive_loss_db.unwrap_or(0.0),
            },
        };
        for i in 0..n {
            frame.loss_db[i] -= gain_db;
            frame.ref_loss_db[i] -= gain_db;
            frame.thermal_loss_db[i] -= gain_db;
        }
        if let Some(split) = active_split(&amp.duplex, self.us_hi_mhz, self.ds_lo_mhz)
            && !split.group_delay_ns.is_empty()
        {
            let phase = run
                .phases
                .entry(component.id.clone())
                .or_insert_with(|| phase_from_group_delay(&split.group_delay_ns, run.grid));
            for i in 0..n {
                frame.h[i] = frame.h[i] * phase[i];
            }
        }
        frame.reflectors.clear();
        frame.alpha = 0.0;
        frame.beta = 0.0;
        frame.gamma_db = 0.0;
        frame.delay_us = 0.0;
        self.add_reflector(
            run.grid,
            &mut frame,
            ReturnLoss::Flat {
                rl_db: amp.return_loss_db,
            },
            Facing::Up,
            false,
        );
        let Some(edges) = self.children.get(&element.id) else {
            return Ok(());
        };
        for edge in edges {
            let port = element
                .ports
                .iter()
                .find(|p| p.id == edge.from.port)
                .ok_or_else(|| PhysicsError::BadComponent(element.id.clone()))?;
            let mut branch = frame.copy();
            self.apply_port_points(run, element, port, &mut branch);
            self.cross_edge(run, edge, &mut branch)?;
            let child = self.graph.element(&edge.to.element_id)?;
            self.descend(run, child, branch)?;
        }
        Ok(())
    }

    /// §13: a device behind a splitter leg sees what hangs on each other leg, through the splitter's isolation. The leg
    /// losses cancel, since every leg shares one table.
    fn apply_leg_isolation(
        &self,
        grid: &Grid,
        element: &Element,
        edges: &[&'a Edge],
        this: &Edge,
        iso_db: f64,
        frame: &mut Frame<'a>,
    ) -> Result<(), PhysicsError> {
        for other in edges {
            if other.id == this.id {
                continue;
            }
            let is_leg = element
                .ports
                .iter()
                .any(|p| p.id == other.from.port && p.role == PortRole::Leg);
            if !is_leg {
                continue;
            }
            let child = self.graph.element(&other.to.element_id)?;
            let Some(rl) = part_return_loss(self.component(child)?, PortRole::Input) else {
                continue;
            };
            let (Some((cable, exposure)), Some(len_ft)) =
                (edge_cable(self.graph, other)?, edge_len_ft(other))
            else {
                continue;
            };
            let (ComponentSpec::Cable(spec) | ComponentSpec::Drop(spec)) = &cable.spec else {
                continue;
            };
            let factor = temp_factor(spec, exposure_temp_c(self.sample, exposure));
            let alpha = spec.atten_a * factor * len_ft / 100.0;
            let beta = spec.atten_b * factor * len_ft / 100.0;
            let tau_us = 2.0 * cable_delay_us(spec, len_ft);
            let level =
                |f: f64, sqrt_f: f64| -(iso_db + rl.at(f) + 2.0 * (alpha * sqrt_f + beta * f));
            let best = -(iso_db
                + rl.lowest_db()
                + 2.0 * (alpha * CAVITY_REF_MHZ.sqrt() + beta * CAVITY_REF_MHZ));
            if tau_us <= 0.0 {
                continue;
            }
            if best < HEALTHY_ECHO_FLOOR_DB {
                frame.discarded += lin(best);
                continue;
            }
            for i in 0..grid.len() {
                let f = grid.f_mhz[i];
                let echo = C64::from_polar(amp(level(f, grid.sqrt_f[i])), -2.0 * PI * f * tau_us);
                frame.h[i] = frame.h[i] * (C64::ONE + echo);
            }
            let mid = grid.mid();
            frame.echoes.push(Echo {
                tau_us,
                rho: amp(level(mid, mid.sqrt())),
                fault: false,
            });
        }
        Ok(())
    }

    fn cross_active<S: ModemSink>(
        &self,
        run: &mut Run<'_, S>,
        element: &'a Element,
        component: &'a Component,
        mut frame: Frame<'a>,
    ) -> Result<(), PhysicsError> {
        let active = component
            .spec
            .active()
            .ok_or_else(|| PhysicsError::BadComponent(element.id.clone()))?;
        self.check_grid(component, run.grid)?;
        let n = run.grid.len();
        let is_root = matches!(
            element.kind,
            ElementKind::Node { .. } | ElementKind::Rpd { .. }
        );
        let overrides = self.overlay.amps.get(&element.id);
        let failed = overrides.is_some_and(|o| o.failed)
            || self.overlay.down.contains(&element.id)
            || self.overlay.rf_off.contains(&element.id);
        let return_atten_db = match element.kind {
            ElementKind::Node { return_atten_db }
            | ElementKind::Rpd { return_atten_db }
            | ElementKind::Amp { return_atten_db } => return_atten_db,
            _ => 0.0,
        };

        // §15: what's fitted. An amp's plug-ins shape its output; a root holds its set point.
        let fitted = match &element.fitted {
            Some(Fitted::Active(fitted)) => Some(fitted),
            _ => None,
        };
        let aligned = if is_root { None } else { fitted };
        let (ds_lo_mhz, ds_hi_mhz) = (self.ds_lo_mhz, self.ds_hi_mhz);
        let set_point_dev: Vec<f64> = match fitted.and_then(|f| f.set_point.as_ref()) {
            Some(sp) if run.band == Band::Down => (0..n)
                .map(|i| {
                    let f = run.grid.f_mhz[i];
                    sp.out_dbmv_at_hi
                        - sp.tilt_db * (ds_hi_mhz - f) / (ds_hi_mhz - ds_lo_mhz)
                        - run.targets[i]
                })
                .collect(),
            // None: read as 0 at every frequency, without a buffer to allocate.
            _ => Vec::new(),
        };
        // §8: what this active's plug-ins drew within their tolerance, if any.
        let no_draws = crate::overlay::PlugInDraws::default();
        let draws = self
            .overlay
            .plugin_draws
            .get(&element.id)
            .unwrap_or(&no_draws);
        // §8, §15: a wrong plug-in's extra loss at the input: flat for the pad, the equalizer's √f shape across its slope band
        // (the forward band without one), most at the bottom.
        let misaligned_db: Vec<f64> = match overrides.and_then(|o| o.misalignment) {
            Some((pad_db, eq_db, _)) if run.band == Band::Down => {
                let band = aligned
                    .and_then(|fitted| fitted.input_eq.as_ref())
                    .and_then(|id| self.graph.component(id))
                    .and_then(|c| match &c.spec {
                        ComponentSpec::PlugIn(spec) => spec.slope_band,
                        _ => None,
                    })
                    .map_or((ds_lo_mhz, ds_hi_mhz), |b| (b.lo_mhz, b.hi_mhz));
                (0..n)
                    .map(|i| {
                        let share = ((band.1.sqrt() - run.grid.sqrt_f[i])
                            / (band.1.sqrt() - band.0.sqrt()))
                        .clamp(0.0, 1.0);
                        pad_db + eq_db * share
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        let (pre_db, gain_db) = match aligned {
            Some(fitted) if run.band == Band::Down => {
                let pre = (0..n)
                    .map(|i| {
                        let f = run.grid.f_mhz[i];
                        self.plugin_drawn_db(&fitted.input_pad, draws.input_pad_db, f)
                            + self.plugin_drawn_db(&fitted.input_eq, draws.input_eq_db, f)
                            + misaligned_db.get(i).copied().unwrap_or(0.0)
                    })
                    .collect::<Vec<f64>>();
                let gain = (0..n)
                    .map(|i| active_gain_db(active, run.grid.f_mhz[i]))
                    .collect::<Vec<f64>>();
                (pre, gain)
            }
            // Unfitted: none, read as 0.
            _ => (Vec::new(), Vec::new()),
        };
        let c_max_db = active
            .outputs
            .iter()
            .map(|o| o.return_combining_loss_db)
            .fold(0.0, f64::max);

        let agc_range_db = if is_root { 0.0 } else { active.agc_range_db };
        // §3.4: a pilot reads the deviation at the input from what alignment left there; the correction is shaped like
        // the section's cable, which is what temperature looks like (§1.2). §15: the part's own pilot where it has one.
        let entry = match run.agc_out.as_ref() {
            Some(_) => {
                let p = active
                    .agc_pilots_mhz
                    .iter()
                    .copied()
                    .fold(None, |best: Option<f64>, f| {
                        Some(best.map_or(f, |b| b.max(f)))
                    })
                    .and_then(|f| run.grid.f_mhz.iter().position(|g| *g == f))
                    .unwrap_or(0);
                let in_dev_db = frame.out_dev_db[p]
                    - (frame.loss_db[p] - frame.ref_loss_db[p])
                    - frame.out_dev_ref_db[p];
                let correction_db = (-in_dev_db).clamp(-agc_range_db, agc_range_db);
                let gain_delta_db = overrides.map_or(0.0, |o| o.gain_delta_db);
                let out_dev_pilot_db = if failed {
                    f64::NEG_INFINITY
                } else if aligned.is_some() {
                    frame.out_dev_db[p] - frame.loss_db[p] - zero_past(&pre_db, p)
                        + zero_past(&gain_db, p)
                        + correction_db
                        + gain_delta_db
                } else {
                    in_dev_db + correction_db + gain_delta_db + zero_past(&set_point_dev, p)
                };
                AgcEntry {
                    correction_db,
                    cable_ref_pilot_db: frame.cable_ref_db[p],
                    out_dev_pilot_db,
                }
            }
            None => self
                .agc
                .get(&element.id)
                .copied()
                .ok_or_else(|| PhysicsError::BadComponent(element.id.clone()))?,
        };
        if let Some(agc_out) = run.agc_out.as_mut() {
            agc_out.insert(element.id.clone(), entry);
        }
        // §15: what `sim::align` reads: the input at reference forward, the section above at reference return.
        if let Some(capture) = run.capture.as_mut() {
            let levels = match run.band {
                Band::Down => (0..n)
                    .map(|i| run.targets[i] + frame.out_dev_ref_db[i] - frame.ref_loss_db[i])
                    .collect(),
                Band::Up => frame.ref_loss_db.clone(),
            };
            capture.inputs.insert(element.id.clone(), levels);
        }

        // An amp's input closes any cavity above it (§5.1), before its noise reads the signal.
        let active_rl = |role| {
            part_return_loss(component, role).unwrap_or(ReturnLoss::Flat {
                rl_db: active.return_loss_db,
            })
        };
        if !is_root {
            self.add_reflector(
                run.grid,
                &mut frame,
                active_rl(PortRole::Input),
                Facing::Down,
                false,
            );
        }

        if run.band == Band::Down && !is_root {
            self.add_amp_noise(run, active, overrides, &entry, &pre_db, &mut frame);
        }

        // §8: a wrong return pad adds to the return path below (§15).
        let misaligned_return_db = match overrides.and_then(|o| o.misalignment) {
            Some((_, _, return_pad_db)) if run.band == Band::Up => return_pad_db,
            _ => 0.0,
        };
        // Output: §3.1 target, plus whatever deviation got this far. §15: a fitted amp's from its own level.
        for i in 0..n {
            let shape = if entry.cable_ref_pilot_db > 0.0 {
                frame.cable_ref_db[i] / entry.cable_ref_pilot_db
            } else {
                1.0
            };
            let drift_db = overrides.map_or(0.0, |o| {
                o.gain_delta_db
                    - o.tilt_delta_db * (ds_hi_mhz - run.grid.f_mhz[i]) / (ds_hi_mhz - ds_lo_mhz)
            });
            let f = run.grid.f_mhz[i];
            // §8: gain at one frequency, and a block off at the source. Neither is what AGC reads.
            let resonance_db: f64 = overrides.map_or(0.0, |o| {
                o.resonance.iter().map(|r| -r.loss_db(f)).sum::<f64>()
            });
            let source_db: f64 = if is_root {
                self.overlay
                    .source_offsets
                    .iter()
                    .filter(|(lo, hi, _)| f >= *lo && f <= *hi)
                    .map(|(_, _, offset)| *offset)
                    .sum()
            } else {
                0.0
            };
            if aligned.is_some() {
                let out_ref =
                    frame.out_dev_ref_db[i] - frame.ref_loss_db[i] - zero_past(&pre_db, i)
                        + zero_past(&gain_db, i);
                frame.out_dev_db[i] = if failed {
                    f64::NEG_INFINITY
                } else {
                    frame.out_dev_db[i] - frame.loss_db[i] - zero_past(&pre_db, i)
                        + zero_past(&gain_db, i)
                        + entry.correction_db * shape
                        + drift_db
                        + resonance_db
                };
                frame.out_dev_ref_db[i] = out_ref;
            } else {
                let in_dev_db = frame.out_dev_db[i]
                    - (frame.loss_db[i] - frame.ref_loss_db[i])
                    - frame.out_dev_ref_db[i];
                frame.out_dev_db[i] = if failed {
                    f64::NEG_INFINITY
                } else {
                    in_dev_db + entry.correction_db * shape + drift_db + resonance_db + source_db
                        - misaligned_db.get(i).copied().unwrap_or(0.0)
                        + zero_past(&set_point_dev, i)
                };
                frame.out_dev_ref_db[i] = zero_past(&set_point_dev, i);
            }
            if let Some(o) = overrides {
                for r in &o.resonance {
                    frame.h[i] = frame.h[i] * C64::from_polar(1.0, r.phase_rad(f));
                }
            }
            // §7.1: unity back to the node, except what a fault or switch has changed. Return amps have
            // thermal level control [SCTE 283 §7.2.1.4.11], so temperature alone doesn't add up.
            let added = (frame.loss_db[i] - frame.thermal_loss_db[i])
                + return_atten_db
                + misaligned_return_db;
            frame.excess_up_db[i] += if failed { f64::INFINITY } else { added };
        }

        if is_root {
            // §4.5: MER_src, and CNR_opt on an analog node: its link's (§16), or its part's constant without one. An
            // RPD has digital optics.
            let optics_inv = match (&component.spec, self.graph.profile().node_kind) {
                (ComponentSpec::Node(node), NodeKind::Analog) => {
                    let cnr_db = self.overlay.optics_cnr_db.unwrap_or(node.optics_cnr_db);
                    lin(-(cnr_db - self.overlay.optics_cnr_loss_db))
                }
                _ => 0.0,
            };
            // §17: the source's own table, the RPD's or, behind an analog node, the bound CMTS's; PHY Table 41's floors
            // without one. An analog node's own RF stages are one more term where it states them.
            let (source, node_rf) = match &component.spec {
                ComponentSpec::Rpd(rpd) => (mer_points(&rpd.source_mer), Vec::new()),
                ComponentSpec::Node(node) => (
                    mer_points(&self.overlay.cmts_source_mer),
                    mer_points(&node.source_mer),
                ),
                _ => (Vec::new(), Vec::new()),
            };
            for i in 0..n {
                let f = run.grid.f_mhz[i];
                let src_db = if source.is_empty() {
                    mer_src_db(f)
                } else {
                    table_at(&source, f)
                };
                frame.inv_plant[i] = lin(-src_db) + optics_inv;
                if !node_rf.is_empty() {
                    frame.inv_plant[i] += lin(-table_at(&node_rf, f));
                }
            }
        }

        // §15: a fitted amp's return gain against the section above, before the port's combining loss. Unfitted, it's
        // aligned for its largest combining loss by assumption.
        let ret_gain_db: Vec<f64> = match aligned {
            Some(fitted) if run.band == Band::Up => (0..n)
                .map(|i| {
                    let f = run.grid.f_mhz[i];
                    active.return_gain_db
                        - self.plugin_drawn_db(&fitted.return_pad, draws.return_pad_db, f)
                        - self.plugin_drawn_db(&fitted.return_eq, draws.return_eq_db, f)
                        - frame.ref_loss_db[i]
                })
                .collect(),
            // Unfitted: `c_max_db` at every frequency, without a buffer.
            _ => Vec::new(),
        };
        let ret_gain_at = |i: usize| ret_gain_db.get(i).copied().unwrap_or(c_max_db);

        // §7.3: each return amp adds its own noise at the node, whatever its depth, at its gain to there (§15).
        if run.band == Band::Up
            && !is_root
            && let Some(port) = frame.root_port
        {
            let (avg, peak) = port_noise(&mut run.noise, port, n);
            for i in 0..n {
                let noise = lin(THERMAL_DBMV_PER_HZ + active.return_nf_db + ret_gain_at(i));
                let reach = lin(-frame.excess_up_db[i]);
                avg[i] += noise * reach;
                peak[i] += noise * reach;
            }
        }

        // §5.6: band-edge group delay.
        if let Some(split) = active_split(&active.duplex, self.us_hi_mhz, self.ds_lo_mhz) {
            let phase = run
                .phases
                .entry(component.id.clone())
                .or_insert_with(|| phase_from_group_delay(&split.group_delay_ns, run.grid));
            for i in 0..n {
                frame.h[i] = frame.h[i] * phase[i];
            }
        }

        // A new section: an amp is directional, so no echo crosses it.
        frame.loss_db.iter_mut().for_each(|x| *x = 0.0);
        frame.ref_loss_db.iter_mut().for_each(|x| *x = 0.0);
        frame.thermal_loss_db.iter_mut().for_each(|x| *x = 0.0);
        frame.cable_ref_db.iter_mut().for_each(|x| *x = 0.0);
        frame.reflectors.clear();
        frame.alpha = 0.0;
        frame.beta = 0.0;
        frame.gamma_db = 0.0;
        frame.delay_us = 0.0;
        self.add_reflector(
            run.grid,
            &mut frame,
            active_rl(PortRole::Output),
            Facing::Up,
            false,
        );

        let Some(edges) = self.children.get(&element.id) else {
            return Ok(());
        };
        for edge in edges {
            let port = element
                .ports
                .iter()
                .find(|p| p.id == edge.from.port)
                .ok_or_else(|| PhysicsError::BadComponent(element.id.clone()))?;
            let mut branch = frame.copy();
            if is_root {
                branch.root_port = Some(&port.id);
            }
            // §15: the port's own loss from the main output, and its pad and equalizer; return, its combining loss.
            let output = active.outputs.iter().find(|o| o.port == port.id);
            let c_p_db = output.map_or(c_max_db, |o| o.return_combining_loss_db);
            let out_fitted = aligned.and_then(|f| f.outputs.iter().find(|o| o.port == port.id));
            if !failed && (output.is_some() || out_fitted.is_some()) {
                for i in 0..n {
                    match run.band {
                        Band::Down => {
                            let f = run.grid.f_mhz[i];
                            let port_db = output.map_or(0.0, |o| o.loss_from_main_db)
                                + out_fitted.map_or(0.0, |o| {
                                    let (pad_db, eq_db) =
                                        draws.outputs.get(&o.port).copied().unwrap_or((0.0, 0.0));
                                    self.plugin_drawn_db(&o.pad, pad_db, f)
                                        + self.plugin_drawn_db(&o.eq, eq_db, f)
                                });
                            branch.out_dev_db[i] -= port_db;
                            branch.out_dev_ref_db[i] -= port_db;
                        }
                        Band::Up => branch.excess_up_db[i] += c_p_db - ret_gain_at(i),
                    }
                }
            } else if !failed && aligned.is_some() && run.band == Band::Up {
                for i in 0..n {
                    branch.excess_up_db[i] += c_p_db - ret_gain_at(i);
                }
            }
            if let Some(capture) = run.capture.as_mut()
                && run.band == Band::Down
            {
                let levels = (0..n)
                    .map(|i| run.targets[i] + branch.out_dev_db[i])
                    .collect();
                capture
                    .outputs
                    .insert((element.id.clone(), port.id.clone()), levels);
            }
            self.apply_port_points(run, element, port, &mut branch);
            self.cross_edge(run, edge, &mut branch)?;
            let child = self.graph.element(&edge.to.element_id)?;
            self.descend(run, child, branch)?;
        }
        Ok(())
    }

    /// §15: a fitted plug-in's loss at `f_mhz`, dB, or 0 for an empty slot.
    /// A fitted plug-in's loss with its value `drawn_db` off its stated one (§8), shaped as its value is.
    fn plugin_drawn_db(&self, id: &Option<ComponentId>, drawn_db: f64, f_mhz: f64) -> f64 {
        let Some(id) = id else {
            return 0.0;
        };
        match self.graph.component(id).map(|c| &c.spec) {
            Some(ComponentSpec::PlugIn(spec)) if drawn_db != 0.0 => {
                plugin_loss_db(spec, f_mhz) + drawn_db * plugin_value_shape(spec, f_mhz)
            }
            Some(ComponentSpec::PlugIn(spec)) => plugin_loss_db(spec, f_mhz),
            _ => 0.0,
        }
    }

    /// §4.2, §4.4: an amp's own noise and distortion, from the signal at its input, echoes included
    /// (§4.5).
    fn add_amp_noise<S: ModemSink>(
        &self,
        run: &Run<'_, S>,
        active: &ActiveSpec,
        overrides: Option<&AmpOverride>,
        entry: &AgcEntry,
        pre_db: &[f64],
        frame: &mut Frame<'a>,
    ) {
        let floor_dbmv = n_floor_6mhz_dbmv();
        let profile = self.graph.profile();
        let out_at_hi_dbmv = profile.design_levels.out_dbmv_at_hi + entry.out_dev_pilot_db;
        let onset = overrides.and_then(|o| o.compression_onset_dbmv);
        let cin = cin_db(
            active.cin_ref_db,
            out_at_hi_dbmv,
            active.ref_out_dbmv,
            onset,
        );
        let cin_inv = lin(-cin);
        // §17: CIN and noise figure by frequency, and CIN against the composite the plan loads, where the part states them.
        let sorted = |mut points: Vec<(f64, f64)>| {
            points.sort_by(|a, b| a.0.total_cmp(&b.0));
            points
        };
        let nf_table = sorted(active.nf_table.iter().map(|p| (p.f_mhz, p.nf_db)).collect());
        let cin_table = sorted(
            active
                .cin_table
                .iter()
                .map(|p| (p.f_mhz, p.cin_db))
                .collect(),
        );
        let by_frequency = !cin_table.is_empty() || active.ref_composite_dbmv.is_some();
        let cin_shift_db = match active.ref_composite_dbmv {
            Some(ref_composite_dbmv) => {
                let (lo, hi) = ds_band(profile);
                composite_dbmv(
                    &profile.channel_plan,
                    out_at_hi_dbmv,
                    profile.design_levels.tilt_db,
                    lo,
                    hi,
                ) - ref_composite_dbmv
            }
            None => out_at_hi_dbmv - active.ref_out_dbmv,
        };
        let over_db = onset.map_or(0.0, |o| (out_at_hi_dbmv - o).max(0.0));
        for i in 0..run.grid.len() {
            let f = run.grid.f_mhz[i];
            // §15: a fitted amp's input pad and equalizer come before its first stage.
            let level_dbmv =
                run.targets[i] + frame.out_dev_db[i] - frame.loss_db[i] - zero_past(pre_db, i);
            let ripple_db = db(frame.h[i].norm_sqr());
            let nf_db = if nf_table.is_empty() {
                active.nf_db
            } else {
                table_at(&nf_table, f)
            };
            let cnr = level_dbmv + ripple_db - nf_db - floor_dbmv;
            let cin_inv = if by_frequency {
                let cin_at_db = if cin_table.is_empty() {
                    active.cin_ref_db
                } else {
                    table_at(&cin_table, f)
                };
                lin(-(cin_at_db - cin_shift_db - 2.0 * over_db))
            } else {
                cin_inv
            };
            // §8: a resonance drives this amp harder at its peak, at the third-order slope.
            let rise_db: f64 = overrides.map_or(0.0, |o| {
                o.resonance
                    .iter()
                    .map(|r| -r.loss_db(run.grid.f_mhz[i]))
                    .sum()
            });
            frame.inv_plant[i] += lin(-cnr) + cin_inv * lin(RESONANCE_CIN_DB_PER_DB * rise_db);
        }
    }

    fn apply_port_points<S: ModemSink>(
        &self,
        run: &mut Run<'_, S>,
        element: &Element,
        port: &Port,
        frame: &mut Frame<'a>,
    ) {
        let key = (element.id.clone(), port.id.clone());
        if let Some(points) = self.overlay.port_points.get(&key) {
            for point in points {
                self.apply_point(run, point, frame);
            }
        }
    }

    fn apply_point<S: ModemSink>(
        &self,
        run: &mut Run<'_, S>,
        point: &PointImpairment,
        frame: &mut Frame<'a>,
    ) {
        let n = run.grid.len();
        if let Some(loss) = &point.loss {
            for i in 0..n {
                let f = run.grid.f_mhz[i];
                frame.loss_db[i] += loss.loss_db(f);
                frame.h[i] = frame.h[i] * C64::from_polar(1.0, loss.phase_rad(f));
            }
            frame.gamma_db += loss.loss_db(CAVITY_PASSIVE_REF_MHZ);
        }
        if let Some(rl) = &point.reflector {
            self.add_reflector_signed(
                run.grid,
                frame,
                rl.clone(),
                Facing::Both,
                true,
                point.inverted,
            );
        }
        match run.band {
            Band::Down => {
                if let Some(ingress) = &point.ds_ingress {
                    add_ds_ingress(run, ingress, frame);
                }
            }
            Band::Up => {
                if let Some(coupling_db) = point.ambient_coupling_db {
                    for source in &self.overlay.ambient {
                        add_us_ingress(run, source, coupling_db, frame);
                    }
                }
                if let Some(ingress) = &point.us_ingress {
                    add_us_ingress(run, ingress, 0.0, frame);
                }
            }
        }
    }

    /// §14: a connector, splice or adapter: a small loss and a healthy reflector facing both ways.
    fn apply_fitting<S: ModemSink>(
        &self,
        run: &mut Run<'_, S>,
        fitting: &schema::plant::Fitting,
        drawn_rl_db: f64,
        frame: &mut Frame<'a>,
    ) -> Result<(), PhysicsError> {
        let bad = || PhysicsError::BadComponent(ElementId::new(fitting.component_ref.as_str()));
        let component = self
            .graph
            .component(&fitting.component_ref)
            .ok_or_else(bad)?;
        let ComponentSpec::Connector(connector) = &component.spec else {
            return Err(bad());
        };
        self.check_grid(component, run.grid)?;
        let il = connector.insertion_loss_db;
        for i in 0..run.grid.len() {
            frame.loss_db[i] += il;
            frame.ref_loss_db[i] += il;
            frame.thermal_loss_db[i] += il;
        }
        frame.gamma_db += il;
        if let Some(rl) = part_return_loss(component, PortRole::Input) {
            // §8: its return loss drawn within its tolerance, as a mismatch behind no loss.
            let rl = if drawn_rl_db == 0.0 {
                rl
            } else {
                ReturnLoss::Behind {
                    rl: Box::new(rl),
                    loss: vec![(0.0, drawn_rl_db / 2.0)],
                }
            };
            // A fitting is a version 2 reflector wherever it is.
            let was = frame.in_home;
            frame.in_home = true;
            self.add_reflector(run.grid, frame, rl, Facing::Both, false);
            frame.in_home = was;
        }
        Ok(())
    }

    /// §8: a fitting's drawn return loss offset, dB, or 0.
    fn fitting_draw(&self, edge: &Edge, k: usize) -> f64 {
        if self.overlay.fitting_rl_draws.is_empty() {
            return 0.0;
        }
        self.overlay
            .fitting_rl_draws
            .get(&(edge.id.clone(), k))
            .copied()
            .unwrap_or(0.0)
    }

    /// §5.1: every earlier reflector in this section makes a cavity with a new one.
    fn add_reflector(
        &self,
        grid: &Grid,
        frame: &mut Frame<'a>,
        rl: ReturnLoss,
        facing: Facing,
        fault: bool,
    ) {
        self.add_reflector_signed(grid, frame, rl, facing, fault, false);
    }

    /// [`Self::add_reflector`], for a mismatch that may reflect inverted (§20.1): an echo between two reflectors carries
    /// the product of their signs, so a short and an open at one point give echoes of opposite sign.
    fn add_reflector_signed(
        &self,
        grid: &Grid,
        frame: &mut Frame<'a>,
        rl: ReturnLoss,
        facing: Facing,
        fault: bool,
        inverted: bool,
    ) {
        let new = Reflector {
            facing,
            rl,
            alpha: frame.alpha,
            beta: frame.beta,
            gamma_db: frame.gamma_db,
            delay_us: frame.delay_us,
            fault,
            home: frame.in_home,
            sign: if inverted { -1.0 } else { 1.0 },
        };
        for prev in &frame.reflectors {
            if new.facing == Facing::Up || prev.facing == Facing::Down {
                continue;
            }
            let (da, db_, dg) = (
                new.alpha - prev.alpha,
                new.beta - prev.beta,
                new.gamma_db - prev.gamma_db,
            );
            let either_fault = new.fault || prev.fault;
            let floor_db = if either_fault {
                FAULT_ECHO_FLOOR_DB
            } else {
                HEALTHY_ECHO_FLOOR_DB
            };
            let e_best_db = -(new.rl.lowest_db()
                + prev.rl.lowest_db()
                + 2.0 * (da * CAVITY_REF_MHZ.sqrt() + db_ * CAVITY_REF_MHZ + dg));
            let tau_us = 2.0 * (new.delay_us - prev.delay_us);
            if tau_us <= 0.0 {
                continue;
            }
            if e_best_db < floor_db {
                // §14: what the floor drops, so a section can say how much.
                frame.discarded += lin(e_best_db);
                continue;
            }
            let sign = new.sign * prev.sign;
            let e_db_at = |i: usize| {
                let f = grid.f_mhz[i];
                -(new.rl.at(f) + prev.rl.at(f) + 2.0 * (da * grid.sqrt_f[i] + db_ * f + dg))
            };
            if new.home || prev.home {
                // A home's echoes: exact every `CHUNK` points of an evenly spaced stretch, amplitude linear and phase rotated
                // between (approx, numerical: under 1e-6 of the echo; `walk::tests`). Version 1's echoes stay exact everywhere.
                let mut done = vec![false; grid.len()];
                for &(start, len, step) in &grid.runs {
                    let rotation = C64::from_polar(1.0, -2.0 * PI * step * tau_us);
                    let end = start + len - 1;
                    let mut i = start;
                    while i < end {
                        let j = (i + CHUNK).min(end);
                        let (a_i, a_j) = (amp(e_db_at(i)), amp(e_db_at(j)));
                        let mut phasor = C64::from_polar(1.0, -2.0 * PI * grid.f_mhz[i] * tau_us);
                        for m in i..j {
                            let share = (m - i) as f64 / (j - i) as f64;
                            let echo = phasor.scale(sign * (a_i + (a_j - a_i) * share));
                            frame.h[m] = frame.h[m] * (C64::ONE + echo);
                            done[m] = true;
                            phasor = phasor * rotation;
                        }
                        i = j;
                    }
                }
                for (i, finished) in done.iter().enumerate() {
                    if !finished {
                        let echo =
                            C64::from_polar(amp(e_db_at(i)), -2.0 * PI * grid.f_mhz[i] * tau_us)
                                .scale(sign);
                        frame.h[i] = frame.h[i] * (C64::ONE + echo);
                    }
                }
            } else {
                for i in 0..grid.len() {
                    let f = grid.f_mhz[i];
                    let e_db = -(new.rl.at(f)
                        + prev.rl.at(f)
                        + 2.0 * (da * grid.sqrt_f[i] + db_ * f + dg));
                    let echo = C64::from_polar(amp(e_db), -2.0 * PI * f * tau_us).scale(sign);
                    frame.h[i] = frame.h[i] * (C64::ONE + echo);
                }
            }
            let mid = grid.mid();
            let e_mid_db =
                -(new.rl.at(mid) + prev.rl.at(mid) + 2.0 * (da * mid.sqrt() + db_ * mid + dg));
            frame.echoes.push(Echo {
                tau_us,
                rho: amp(e_mid_db),
                fault: either_fault,
            });
        }
        frame.reflectors.push(new);
    }

    fn cross_edge<S: ModemSink>(
        &self,
        run: &mut Run<'_, S>,
        edge: &'a Edge,
        frame: &mut Frame<'a>,
    ) -> Result<(), PhysicsError> {
        if matches!(edge.kind, EdgeKind::House { .. }) {
            frame.in_home = true;
        }
        let (len_ft, cable) = match &edge.kind {
            EdgeKind::Span { len_ft, .. }
            | EdgeKind::Drop { len_ft, .. }
            | EdgeKind::House { len_ft, .. } => {
                let Some(cable) = edge_cable(self.graph, edge)? else {
                    return Ok(());
                };
                (*len_ft, cable)
            }
            EdgeKind::Direct => return Ok(()),
        };
        let (component, installation) = cable;
        self.check_grid(component, run.grid)?;
        let spec = match &component.spec {
            ComponentSpec::Cable(spec) | ComponentSpec::Drop(spec) => spec,
            _ => return Err(PhysicsError::BadComponent(edge.from.element_id.clone())),
        };
        let factor = temp_factor(spec, exposure_temp_c(self.sample, installation));
        let jitter = self
            .overlay
            .edge_loss_factor
            .get(&edge.id)
            .copied()
            .unwrap_or(1.0);
        let wet = self.overlay.wet.get(&edge.id);

        let mut stops: Vec<f64> = vec![0.0, len_ft];
        let empty = Vec::new();
        let points = self.overlay.span_points.get(&edge.id).unwrap_or(&empty);
        stops.extend(points.iter().map(|(pos, _)| *pos));
        // §14: fittings are points too, healthy ones.
        let fittings = edge.kind.fittings();
        stops.extend(fittings.iter().map(|f| f.pos_ft));
        if let Some(sections) = wet {
            for section in sections {
                stops.push(section.pos_ft);
                stops.push((section.pos_ft + section.len_ft).min(len_ft));
            }
        }
        stops.retain(|pos| (0.0..=len_ft).contains(pos));
        stops.sort_by(f64::total_cmp);
        stops.dedup();

        for (j, pair) in stops.windows(2).enumerate() {
            let piece_ft = pair[1] - pair[0];
            let mid_ft = (pair[0] + pair[1]) / 2.0;
            if j == 0 {
                for (k, fitting) in fittings.iter().enumerate() {
                    if fitting.pos_ft <= 0.0 {
                        let drawn = self.fitting_draw(edge, k);
                        self.apply_fitting(run, fitting, drawn, frame)?;
                    }
                }
                for (pos, point) in points.iter() {
                    if *pos <= 0.0 {
                        self.apply_point(run, point, frame);
                    }
                }
            }
            let k_w = wet.map_or(0.0, |sections| {
                sections
                    .iter()
                    .filter(|s| mid_ft >= s.pos_ft && mid_ft <= s.pos_ft + s.len_ft)
                    .map(|s| s.k_w_eff_db_per_100ft)
                    .sum()
            });
            for i in 0..run.grid.len() {
                let f = run.grid.f_mhz[i];
                let atten = (spec.atten_a * run.grid.sqrt_f[i] + spec.atten_b * f) * jitter;
                let piece_ref = atten * piece_ft / 100.0;
                let wet_db = k_w * (f / 1000.0).sqrt() * piece_ft / 100.0;
                frame.loss_db[i] += piece_ref * factor + wet_db;
                frame.ref_loss_db[i] += piece_ref;
                frame.thermal_loss_db[i] += piece_ref * factor;
                frame.cable_ref_db[i] += piece_ref;
            }
            frame.alpha += spec.atten_a * factor * jitter * piece_ft / 100.0;
            frame.beta += spec.atten_b * factor * jitter * piece_ft / 100.0;
            frame.delay_us += cable_delay_us(spec, piece_ft);
            for (k, fitting) in fittings.iter().enumerate() {
                if fitting.pos_ft > pair[0] && fitting.pos_ft <= pair[1] {
                    let drawn = self.fitting_draw(edge, k);
                    self.apply_fitting(run, fitting, drawn, frame)?;
                }
            }
            for (pos, point) in points.iter() {
                if *pos > pair[0] && *pos <= pair[1] {
                    self.apply_point(run, point, frame);
                }
            }
        }
        Ok(())
    }

    /// §5.1: for each fault mismatch, the taps above it in its section see an echo on their other
    /// ports, by leaking through isolation. These reach modems whose path never crosses the fault.
    fn build_off_path(&self) -> Result<BTreeMap<ElementId, Vec<OffPath<'a>>>, PhysicsError> {
        let mut out: BTreeMap<ElementId, Vec<OffPath<'a>>> = BTreeMap::new();
        let mut consider = |start: &'a Element,
                            mut alpha: f64,
                            mut beta: f64,
                            mut delay: f64,
                            rl: &ReturnLoss,
                            sign: f64|
         -> Result<(), PhysicsError> {
            let mut gamma = 0.0;
            let mut current = start;
            while let Some((parent, edge)) = self.graph.parent(&current.id)? {
                if let Some((component, installation)) = edge_cable(self.graph, edge)?
                    && let (ComponentSpec::Cable(spec) | ComponentSpec::Drop(spec), Some(len_ft)) =
                        (&component.spec, edge_len_ft(edge))
                {
                    let factor = temp_factor(spec, exposure_temp_c(self.sample, installation));
                    alpha += spec.atten_a * factor * len_ft / 100.0;
                    beta += spec.atten_b * factor * len_ft / 100.0;
                    delay += cable_delay_us(spec, len_ft);
                }
                let parent_component = self.component(parent)?;
                if parent_component.spec.active().is_some() {
                    break;
                }
                if matches!(parent.kind, ElementKind::Tap)
                    && let Some(via_port) = parent.ports.iter().find(|p| p.id == edge.from.port)
                {
                    out.entry(parent.id.clone()).or_default().push(OffPath {
                        via_port,
                        rl: rl.clone(),
                        alpha,
                        beta,
                        gamma_db: gamma,
                        delay_us: delay,
                        sign,
                    });
                    gamma +=
                        passive_loss_fast(parent_component, via_port.role, CAVITY_PASSIVE_REF_MHZ);
                }
                current = parent;
            }
            Ok(())
        };

        for (edge_id, points) in &self.overlay.span_points {
            for (pos_ft, point) in points {
                let Some(rl) = &point.reflector else {
                    continue;
                };
                let edge = self.graph.edge(edge_id)?;
                let (Some((component, installation)), Some(len_ft)) =
                    (edge_cable(self.graph, edge)?, edge_len_ft(edge))
                else {
                    continue;
                };
                let (ComponentSpec::Cable(spec) | ComponentSpec::Drop(spec)) = &component.spec
                else {
                    continue;
                };
                let factor = temp_factor(spec, exposure_temp_c(self.sample, installation));
                // Start the climb as if from the edge's far end, less the part of the edge past
                // the point, which the signal doesn't cross.
                let beyond = len_ft - pos_ft;
                let child = self.graph.element(&edge.to.element_id)?;
                consider(
                    child,
                    -spec.atten_a * factor * beyond / 100.0,
                    -spec.atten_b * factor * beyond / 100.0,
                    -cable_delay_us(spec, beyond),
                    rl,
                    if point.inverted { -1.0 } else { 1.0 },
                )?;
            }
        }
        for ((element_id, _), points) in &self.overlay.port_points {
            for point in points {
                if let Some(rl) = &point.reflector {
                    let element = self.graph.element(element_id)?;
                    consider(
                        element,
                        0.0,
                        0.0,
                        0.0,
                        rl,
                        if point.inverted { -1.0 } else { 1.0 },
                    )?;
                }
            }
        }
        Ok(out)
    }

    fn apply_off_path(
        &self,
        grid: &Grid,
        element: &Element,
        component: &Component,
        port: &Port,
        frame: &mut Frame<'a>,
    ) {
        let (Some(reflectors), ComponentSpec::Tap(tap)) =
            (self.off_path.get(&element.id), &component.spec)
        else {
            return;
        };
        for reflector in reflectors {
            if reflector.via_port.id == port.id {
                continue;
            }
            let leak_db = match reflector.via_port.role {
                PortRole::TapPort => tap.port_isolation_db,
                _ => tap.port_isolation_db - tap.tap_value_db,
            };
            for i in 0..grid.len() {
                let f = grid.f_mhz[i];
                let e_db = -(leak_db
                    + reflector.rl.at(f)
                    + reflector.gamma_db
                    + 2.0 * (reflector.alpha * grid.sqrt_f[i] + reflector.beta * f));
                let tau_us = 2.0 * reflector.delay_us;
                let echo = C64::from_polar(amp(e_db), -2.0 * PI * f * tau_us).scale(reflector.sign);
                frame.h[i] = frame.h[i] * (C64::ONE + echo);
            }
            let mid = grid.mid();
            frame.echoes.push(Echo {
                tau_us: 2.0 * reflector.delay_us,
                rho: amp(-(leak_db
                    + reflector.rl.at(mid)
                    + reflector.gamma_db
                    + 2.0 * (reflector.alpha * mid.sqrt() + reflector.beta * mid))),
                fault: true,
            });
        }
    }
}

fn edge_len_ft(edge: &Edge) -> Option<f64> {
    edge.kind.cable().map(|(_, len_ft)| len_ft)
}

fn port_noise<'n>(
    noise: &'n mut ReturnNoise,
    port: &PortId,
    n: usize,
) -> (&'n mut Vec<f64>, &'n mut Vec<f64>) {
    let avg = noise
        .avg_per_hz
        .entry(port.clone())
        .or_insert_with(|| vec![0.0; n]);
    let peak = noise
        .peak_per_hz
        .entry(port.clone())
        .or_insert_with(|| vec![0.0; n]);
    (avg, peak)
}

fn add_ds_ingress<S: ModemSink>(run: &mut Run<'_, S>, ingress: &Ingress, frame: &mut Frame<'_>) {
    for i in 0..run.grid.len() {
        let f = run.grid.f_mhz[i];
        if ingress.covers(f) {
            let level_dbmv = run.targets[i] + frame.out_dev_db[i] - frame.loss_db[i];
            let mer = crate::noise::mer_ingress_db(level_dbmv, ingress.power_dbmv, ingress.bw_mhz);
            frame.inv_plant[i] += lin(-mer);
        }
    }
}

/// Noise entering the return path at the frame's place, `below_db` under `ingress`'s power (§7.3, §18).
fn add_us_ingress<S: ModemSink>(
    run: &mut Run<'_, S>,
    ingress: &Ingress,
    below_db: f64,
    frame: &mut Frame<'_>,
) {
    let Some(port) = frame.root_port else {
        return;
    };
    let n = run.grid.len();
    let per_hz_dbmv = ingress.power_dbmv - below_db - db(ingress.bw_mhz * 1e6);
    let (avg, peak) = port_noise(&mut run.noise, port, n);
    for i in 0..n {
        if ingress.covers(run.grid.f_mhz[i]) {
            let reach = lin(-(frame.loss_db[i] + frame.excess_up_db[i]));
            avg[i] += lin(per_hz_dbmv) * reach;
            peak[i] += lin(per_hz_dbmv + ingress.peak_over_avg_db) * reach;
        }
    }
}

/// Loss through a passive from its input to a port, for the walk's inner loop. The rating was
/// checked already, so this can't fail.
fn passive_loss_fast(component: &Component, out_role: PortRole, f_mhz: f64) -> f64 {
    match &component.spec {
        ComponentSpec::Tap(tap) => match out_role {
            PortRole::Through => interpolate(&tap.insertion_loss, f_mhz),
            _ => interpolate(&tap.tap_loss, f_mhz),
        },
        ComponentSpec::Splitter(splitter) => interpolate(&splitter.leg_loss, f_mhz),
        ComponentSpec::GroundBlock(block) => interpolate(&block.insertion_loss, f_mhz),
        // §14: what an inserter states, nothing for one that states nothing.
        ComponentSpec::PowerInserter(inserter) => interpolate(&inserter.insertion_loss, f_mhz),
        ComponentSpec::DirectionalCoupler(coupler) => match out_role {
            PortRole::Through => {
                coupler_through_db(coupler.coupled_value_db, coupler.excess_loss_db)
            }
            _ => coupler.coupled_value_db + coupler.excess_loss_db,
        },
        _ => 0.0,
    }
}

/// §5.1: re-export for readers of the walk.
pub use echo::cavity_level_db;
