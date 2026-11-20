//! AC powering (§6). Actives draw constant power, so the current depends on the voltage it causes.

use std::collections::BTreeMap;

use plant_graph::PlantGraph;
use schema::common::ElementId;
use schema::component::ComponentSpec;
use schema::plant::EdgeKind;

use schema::environment::EnvironmentSample;

use crate::cable::{Exposure, exposure_temp_c, r_loop_ohm_per_kft};
use crate::error::PhysicsError;
use crate::overlay::Overlay;

/// One cable segment of a feed: its resistance and which loads' current crosses it.
#[derive(Debug, Clone)]
pub struct FeedSegment {
    /// `R_loop · len / 1000`, ohms.
    pub r_ohm: f64,
    /// Indexes of the loads beyond it.
    pub beyond: Vec<usize>,
}

/// §6.1, §6.2: voltage at each load of a feed. `passes` is how many times to solve; 2–3 converge.
pub fn solve_feed(
    v_supply: f64,
    loads_w: &[f64],
    segments: &[FeedSegment],
    passes: usize,
) -> Vec<f64> {
    let mut v = vec![v_supply; loads_w.len()];
    for _ in 0..passes {
        let currents_a: Vec<f64> = loads_w
            .iter()
            .zip(&v)
            .map(|(p_w, v_in)| if *v_in > 0.0 { p_w / v_in } else { 0.0 })
            .collect();
        let mut next = vec![v_supply; loads_w.len()];
        for segment in segments {
            let i_seg: f64 = segment.beyond.iter().map(|&j| currents_a[j]).sum();
            for &j in &segment.beyond {
                next[j] -= i_seg * segment.r_ohm;
            }
        }
        v = next;
    }
    v
}

/// One active's power state.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivePower {
    /// Input voltage, volts.
    pub v_in: f64,
    /// Current drawn, amps.
    pub i_draw: f64,
    /// The supply feeding it.
    pub supply: ElementId,
    /// §6.3: its minimum input voltage.
    pub v_min: f64,
}

/// One supply's load.
#[derive(Debug, Clone, PartialEq)]
pub struct SupplyLoad {
    /// Output voltage, after any sag, volts.
    pub v_out: f64,
    /// Total current, amps.
    pub i_total: f64,
    /// Its rating, amps.
    pub i_rated: f64,
}

/// The solved powering of a plant.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AcSolution {
    /// Per active, in canonical order.
    pub actives: BTreeMap<ElementId, ActivePower>,
    /// Per supply, in canonical order.
    pub supplies: BTreeMap<ElementId, SupplyLoad>,
}

impl AcSolution {
    /// §6.3: actives below their minimum voltage.
    pub fn browned_out(&self) -> Vec<ElementId> {
        self.actives
            .iter()
            .filter(|(_, p)| p.v_in < p.v_min)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

const PASSES: usize = 6;

/// §20.6: the share of a battery string's stored energy its inverter delivers to the load (approx).
pub const STANDBY_EFFICIENCY: f64 = 0.8;

/// §20.6: how long a supply's batteries carry `load_w` once its AC input fails, seconds: the string's energy to its
/// end-of-discharge voltage taken whole, through the inverter, at constant power (approx: no Peukert). None without
/// batteries.
pub fn standby_s(
    graph: &PlantGraph<'_>,
    supply: &schema::plant::PowerSupply,
    load_w: f64,
) -> Option<f64> {
    let bank = supply.batteries.as_ref()?;
    let ComponentSpec::Battery(battery) = &graph.component(&bank.component_ref)?.spec else {
        return None;
    };
    let wh = f64::from(bank.strings)
        * f64::from(bank.per_string)
        * battery.v_nominal
        * battery.capacity_ah
        * STANDBY_EFFICIENCY;
    Some(if load_w > 0.0 {
        wh / load_w * 3600.0
    } else {
        f64::INFINITY
    })
}

/// §20.4: the loop resistance from where a supply's current enters to a point on a span, ohms, at a sample's
/// temperatures; None where its current doesn't reach the span.
pub fn path_ohm(
    graph: &PlantGraph<'_>,
    supply: &schema::plant::PowerSupply,
    span_id: &schema::common::EdgeId,
    pos_ft: f64,
    overlay: &Overlay,
    sample: &EnvironmentSample,
) -> Option<f64> {
    let (path, from_end) = graph.power_path(supply, span_id)?;
    let mut r = 0.0;
    for (k, edge) in path.iter().enumerate() {
        let EdgeKind::Span {
            cable_ref,
            len_ft,
            installation,
            ..
        } = &edge.kind
        else {
            continue;
        };
        let temp_c = exposure_temp_c(sample, Exposure::of(Some(*installation)));
        let ComponentSpec::Cable(cable) = &graph.component(cable_ref)?.spec else {
            continue;
        };
        let per_ft = r_loop_ohm_per_kft(cable, temp_c) / 1000.0 * overlay.r_loop_factor;
        let ft = if k + 1 == path.len() {
            if from_end { pos_ft } else { len_ft - pos_ft }
        } else {
            *len_ft
        };
        r += per_ft * ft.max(0.0);
    }
    Some(r)
}

/// §6.2: walk each supply's feed from `PlantGraph::power` and solve it. An active more than one
/// supply reaches is solved on the first, in canonical order; `check_design` flags it.
pub fn solve_ac(graph: &PlantGraph<'_>, overlay: &Overlay) -> Result<AcSolution, PhysicsError> {
    let reference = EnvironmentSample {
        t: schema::common::Timestamp(0),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        wind_m_per_s: None,
        solar_w_per_m2: None,
    };
    solve_ac_at(graph, overlay, &reference)
}

/// [`solve_ac`] at one environment sample: each span's loop resistance at its own temperature (§19).
pub fn solve_ac_at(
    graph: &PlantGraph<'_>,
    overlay: &Overlay,
    sample: &EnvironmentSample,
) -> Result<AcSolution, PhysicsError> {
    crate::support::check_supported(graph)?;
    let power = graph.power();
    let mut solution = AcSolution::default();
    for feed in &power.feeds {
        let supply_spec = graph
            .component(&feed.supply.component_ref)
            .and_then(|c| match &c.spec {
                ComponentSpec::PowerSupply(spec) => Some(spec),
                _ => None,
            })
            .ok_or_else(|| PhysicsError::BadComponent(feed.supply.id.clone()))?;
        let mut v_out =
            supply_spec.v_out + overlay.psu_v_delta.get(&feed.supply.id).unwrap_or(&0.0);

        let mut index = BTreeMap::new();
        let mut loads_w = Vec::new();
        let mut v_mins = Vec::new();
        for id in &feed.actives {
            let active = graph
                .component(&graph.element(id)?.component_ref)
                .and_then(|c| c.spec.active())
                .ok_or_else(|| PhysicsError::BadComponent(id.clone()))?;
            index.insert(id.clone(), loads_w.len());
            loads_w.push(active.power_w);
            v_mins.push(active.v_min);
        }
        let mut segments = Vec::new();
        for segment in &feed.segments {
            let EdgeKind::Span {
                cable_ref,
                len_ft,
                installation,
                ..
            } = &segment.edge.kind
            else {
                continue;
            };
            let temp_c = exposure_temp_c(sample, Exposure::of(Some(*installation)));
            let r_ohm_per_kft = graph
                .component(cable_ref)
                .and_then(|c| match &c.spec {
                    ComponentSpec::Cable(cable) => Some(r_loop_ohm_per_kft(cable, temp_c)),
                    _ => None,
                })
                .ok_or_else(|| PhysicsError::BadComponent(segment.edge.from.element_id.clone()))?;
            let beyond = segment
                .actives_beyond
                .iter()
                .filter_map(|id| index.get(id).copied())
                .collect();
            segments.push(FeedSegment {
                r_ohm: r_ohm_per_kft * len_ft / 1000.0 * overlay.r_loop_factor,
                beyond,
            });
        }
        // §20.1: actives past a cut get nothing, and draw nothing.
        let mut cut: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        for segment in &feed.segments {
            if overlay.ac_open.contains(&segment.edge.id) {
                cut.extend(
                    segment
                        .actives_beyond
                        .iter()
                        .filter_map(|id| index.get(id).copied()),
                );
            }
        }
        for &j in &cut {
            loads_w[j] = 0.0;
        }
        // §20.6: with its AC input failed, a supply runs on its batteries, then stops.
        if let Some(elapsed_s) = overlay.supplies_out.get(&feed.supply.id) {
            let load_w: f64 = loads_w.iter().sum();
            if standby_s(graph, feed.supply, load_w).is_none_or(|t| *elapsed_s >= t) {
                v_out = 0.0;
            }
        }
        // §20.4: a short it reaches draws `V / (R_path + R_fault)`.
        let i_short: f64 = overlay
            .shorts
            .iter()
            .filter_map(|short| {
                let r = path_ohm(
                    graph,
                    feed.supply,
                    &short.span_id,
                    short.pos_ft,
                    overlay,
                    sample,
                )?;
                Some(v_out / (r + short.r_ohm.max(1e-6)))
            })
            .sum();
        let mut v = solve_feed(v_out, &loads_w, &segments, PASSES);
        let i_loads: f64 = loads_w
            .iter()
            .zip(&v)
            .map(|(p, vin)| if *vin > 0.0 { p / vin } else { 0.0 })
            .sum();
        // §20.4: over its rating, it trips, and everything it feeds stops (approx: a current-limited supply folds back).
        let tripped = i_short > 0.0 && i_loads + i_short > supply_spec.i_rated_a;
        if tripped || v_out <= 0.0 {
            v.iter_mut().for_each(|x| *x = 0.0);
        }
        for &j in &cut {
            v[j] = 0.0;
        }
        let mut i_total = if tripped { 0.0 } else { i_short };
        for (id, &j) in &index {
            let i_draw = if v[j] > 0.0 { loads_w[j] / v[j] } else { 0.0 };
            i_total += i_draw;
            solution
                .actives
                .entry(id.clone())
                .or_insert_with(|| ActivePower {
                    v_in: v[j],
                    i_draw,
                    supply: feed.supply.id.clone(),
                    v_min: v_mins[j],
                });
        }
        solution.supplies.insert(
            feed.supply.id.clone(),
            SupplyLoad {
                v_out,
                i_total,
                i_rated: supply_spec.i_rated_a,
            },
        );
    }
    Ok(solution)
}
