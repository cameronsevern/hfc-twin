//! §15: the pass a technician makes. Working out from the root, depth by depth, at 20 °C with no fault, each fitted amp
//! takes the plug-ins of its family that bring its output to its set point at its pilots, and its return to unity at the
//! return band's edges. Pure and deterministic: it returns fitted values and changes no plant.

use std::collections::BTreeMap;

use plant_graph::PlantGraph;
use schema::common::{ComponentId, ElementId, Timestamp};
use schema::component::{Component, ComponentLibrary, ComponentSpec, PlugInKind, PlugInSpec};
use schema::environment::EnvironmentSample;
use schema::plant::{ActiveFitted, Fitted, Plant};
use schema::profile::Profile;

use crate::error::PhysicsError;
use crate::overlay::Overlay;
use crate::passive::{active_gain_db, plugin_loss_db};
use crate::walk::{Band, Grid, Walker, ds_band, out_level_dbmv, us_band};

/// Alignment's conditions: 20 °C air and ground, dry.
fn reference_sample() -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(0),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

/// The plug-ins of one family and kind, smallest value first, then by id.
fn family<'l>(
    library: &'l ComponentLibrary,
    family: &str,
    kinds: &[PlugInKind],
) -> Vec<(&'l Component, &'l PlugInSpec)> {
    let mut found: Vec<(&Component, &PlugInSpec)> = library
        .components
        .iter()
        .filter_map(|c| match &c.spec {
            ComponentSpec::PlugIn(spec) if spec.family == family && kinds.contains(&spec.kind) => {
                Some((c, spec))
            }
            _ => None,
        })
        .collect();
    found.sort_by(|a, b| {
        a.1.value_db
            .total_cmp(&b.1.value_db)
            .then(a.0.id.cmp(&b.0.id))
    });
    found
}

/// The pair from `pads` × `eqs` with the least summed squared error; the first found wins a tie, so the smaller values do.
fn best_pair<'l>(
    pads: &[(&'l Component, &'l PlugInSpec)],
    eqs: &[(&'l Component, &'l PlugInSpec)],
    error: impl Fn(&PlugInSpec, &PlugInSpec) -> f64,
) -> Option<(&'l ComponentId, &'l ComponentId)> {
    let mut best: Option<(f64, &ComponentId, &ComponentId)> = None;
    for (eq_part, eq) in eqs {
        for (pad_part, pad) in pads {
            let cost = error(pad, eq);
            if best.is_none_or(|(b, _, _)| cost < b - 1e-12) {
                best = Some((cost, &pad_part.id, &eq_part.id));
            }
        }
    }
    best.map(|(_, pad, eq)| (pad, eq))
}

/// `plant` with the root's optics left out: alignment reads RF levels, and optics only change noise (§16).
fn without_root_optics(plant: &Plant) -> Plant {
    let mut working = plant.clone();
    if let Some(root) = plant.root().map(|r| r.id.clone())
        && let Some(element) = working.elements.iter_mut().find(|e| e.id == root)
        && let Some(Fitted::Active(fitted)) = &mut element.fitted
    {
        fitted.forward_rx = None;
        fitted.return_tx = None;
        fitted.transceivers.clear();
    }
    working
}

/// §15: the fitted values a technician would choose for every fitted amp in `plant`, keyed by element. Slots the part's
/// family has no plug-in for keep what they had, and so does an amp whose part names no family. Set points and anything
/// else fitted are kept as they are.
pub fn align(
    plant: &Plant,
    library: &ComponentLibrary,
    profile: &Profile,
) -> Result<BTreeMap<ElementId, ActiveFitted>, PhysicsError> {
    align_holding(plant, library, profile, &BTreeMap::new())
}

/// [`align`], with the input equalizer of each amp in `held` given rather than chosen: that amp takes only the pad that
/// best brings it to its set point at its pilots with that equalizer, and everything below it is aligned to what it
/// then puts out. A sweep across the band, not the two pilots, is how a technician picks between equalizers whose
/// pilot levels are alike (`docs/tasks/25-design-any-map.md`). An equalizer that isn't of the amp's family is ignored.
pub fn align_holding(
    plant: &Plant,
    library: &ComponentLibrary,
    profile: &Profile,
    held: &BTreeMap<ElementId, ComponentId>,
) -> Result<BTreeMap<ElementId, ActiveFitted>, PhysicsError> {
    let mut working = without_root_optics(plant);
    let root = plant
        .root()
        .map(|r| r.id.clone())
        .ok_or_else(|| PhysicsError::InvalidPlant("no root".into()))?;
    // Fitted amps by depth from the root, then by id.
    let mut amps: Vec<(usize, ElementId)> = Vec::new();
    {
        let graph = PlantGraph::new(&working, library, profile)
            .map_err(|e| PhysicsError::InvalidPlant(format!("{e:?}")))?;
        for element in &working.elements {
            if element.id != root && matches!(element.fitted, Some(Fitted::Active(_))) {
                amps.push((graph.cascade(&element.id)?.len(), element.id.clone()));
            }
        }
    }
    amps.sort();
    let (ds_lo_mhz, ds_hi_mhz) = ds_band(profile);
    let (us_lo_mhz, us_hi_mhz) = us_band(profile);
    let sample = reference_sample();
    let overlay = Overlay::default();
    let mut out: BTreeMap<ElementId, ActiveFitted> = BTreeMap::new();

    let mut depth_start = 0;
    while depth_start < amps.len() {
        let depth = amps[depth_start].0;
        let depth_end = amps[depth_start..]
            .iter()
            .position(|(d, _)| *d != depth)
            .map_or(amps.len(), |k| depth_start + k);
        let graph = PlantGraph::new(&working, library, profile)
            .map_err(|e| PhysicsError::InvalidPlant(format!("{e:?}")))?;
        let walker = Walker::new(&graph, &sample, &overlay)?;
        for (_, id) in &amps[depth_start..depth_end] {
            let element = graph.element(id)?;
            let component = graph
                .component(&element.component_ref)
                .ok_or_else(|| PhysicsError::BadComponent(id.clone()))?;
            let active = component
                .spec
                .active()
                .ok_or_else(|| PhysicsError::BadComponent(id.clone()))?;
            let Some(Fitted::Active(current)) = &element.fitted else {
                continue;
            };
            let mut chosen = current.clone();
            let Some(family_name) = active.plugin_family.as_deref() else {
                out.insert(id.clone(), chosen);
                continue;
            };
            // Forward pilots: its two, or the band's edges.
            let mut pilots: Vec<f64> = active.agc_pilots_mhz.clone();
            pilots.sort_by(f64::total_cmp);
            let (lo, hi) = match pilots.as_slice() {
                [.., a, b] => (*a, *b),
                _ => (ds_lo_mhz, ds_hi_mhz),
            };
            let forward = Grid::new(vec![lo, hi]);
            let inputs = walker.capture(&forward, Band::Down)?;
            let l_in = inputs
                .inputs
                .get(id)
                .ok_or_else(|| PhysicsError::BadComponent(id.clone()))?;
            let target = |f: f64| match &current.set_point {
                Some(sp) => {
                    sp.out_dbmv_at_hi - sp.tilt_db * (ds_hi_mhz - f) / (ds_hi_mhz - ds_lo_mhz)
                }
                None => out_level_dbmv(profile, f),
            };
            let pads = family(library, family_name, &[PlugInKind::Pad]);
            let mut eqs = family(
                library,
                family_name,
                &[PlugInKind::Equalizer, PlugInKind::CableSimulator],
            );
            // Two pilots can't tell shapes apart, so the pass picks among the equalizers a value and a shape state; one
            // read from a measured table is chosen only by a sweep across the band, which holds it here.
            match held.get(id) {
                Some(eq) if eqs.iter().any(|(c, _)| &c.id == eq) => {
                    eqs.retain(|(c, _)| &c.id == eq);
                }
                _ => eqs.retain(|(_, spec)| spec.loss_table.is_empty()),
            }
            let main_loss_db = active
                .outputs
                .iter()
                .map(|o| o.loss_from_main_db)
                .fold(f64::INFINITY, f64::min);
            let main_loss_db = if main_loss_db.is_finite() {
                main_loss_db
            } else {
                0.0
            };
            // Every output takes at least the family's smallest pad.
            let least_pad_db = |f: f64| pads.first().map_or(0.0, |(_, p)| plugin_loss_db(p, f));
            let error = |pad: &PlugInSpec, eq: &PlugInSpec| {
                forward
                    .f_mhz
                    .iter()
                    .enumerate()
                    .map(|(i, &f)| {
                        let out = l_in[i] - plugin_loss_db(pad, f) - plugin_loss_db(eq, f)
                            + active_gain_db(active, f)
                            - main_loss_db
                            - least_pad_db(f);
                        (out - target(f)).powi(2)
                    })
                    .sum()
            };
            if let Some((pad, eq)) = best_pair(&pads, &eqs, error) {
                chosen.input_pad = Some(pad.clone());
                chosen.input_eq = Some(eq.clone());
            }
            // Each output's pad, from the level its input choice leaves.
            let mid = |i: usize, f: f64| {
                let slot = |slot: &Option<ComponentId>| {
                    slot.as_ref()
                        .and_then(|s| library.get(s))
                        .and_then(|c| match &c.spec {
                            ComponentSpec::PlugIn(spec) => Some(plugin_loss_db(spec, f)),
                            _ => None,
                        })
                        .unwrap_or(0.0)
                };
                l_in[i] - slot(&chosen.input_pad) - slot(&chosen.input_eq)
                    + active_gain_db(active, f)
            };
            let ports: Vec<_> = element
                .ports
                .iter()
                .filter(|p| p.role == schema::plant::PortRole::Output)
                .collect();
            let mut outputs = Vec::new();
            for port in ports {
                let loss_db = active
                    .outputs
                    .iter()
                    .find(|o| o.port == port.id)
                    .map_or(0.0, |o| o.loss_from_main_db);
                let mut best: Option<(f64, &ComponentId)> = None;
                for (part, pad) in &pads {
                    let cost: f64 = forward
                        .f_mhz
                        .iter()
                        .enumerate()
                        .map(|(i, &f)| {
                            (mid(i, f) - loss_db - plugin_loss_db(pad, f) - target(f)).powi(2)
                        })
                        .sum();
                    if best.is_none_or(|(b, _)| cost < b - 1e-12) {
                        best = Some((cost, &part.id));
                    }
                }
                let eq = current
                    .outputs
                    .iter()
                    .find(|o| o.port == port.id)
                    .and_then(|o| o.eq.clone());
                outputs.push(schema::plant::OutputFitted {
                    port: port.id.clone(),
                    pad: best.map(|(_, id)| id.clone()),
                    eq,
                });
            }
            if !outputs.is_empty() {
                chosen.outputs = outputs;
            }
            // Return: unity for the port with the largest combining loss, at the return band's edges.
            let ret_pads = family(library, family_name, &[PlugInKind::ReturnPad]);
            let ret_eqs = family(library, family_name, &[PlugInKind::ReturnEqualizer]);
            let ret = Grid::new(vec![us_lo_mhz, us_hi_mhz]);
            let above = walker.capture(&ret, Band::Up)?;
            let l_above = above
                .inputs
                .get(id)
                .ok_or_else(|| PhysicsError::BadComponent(id.clone()))?;
            let c_max_db = active
                .outputs
                .iter()
                .map(|o| o.return_combining_loss_db)
                .fold(0.0, f64::max);
            let ret_error = |pad: &PlugInSpec, eq: &PlugInSpec| {
                ret.f_mhz
                    .iter()
                    .enumerate()
                    .map(|(i, &f)| {
                        let residual =
                            l_above[i] + plugin_loss_db(pad, f) + plugin_loss_db(eq, f) + c_max_db
                                - active.return_gain_db;
                        residual.powi(2)
                    })
                    .sum()
            };
            if let Some((pad, eq)) = best_pair(&ret_pads, &ret_eqs, ret_error) {
                chosen.return_pad = Some(pad.clone());
                chosen.return_eq = Some(eq.clone());
            }
            out.insert(id.clone(), chosen);
        }
        // The next depth reads inputs these choices make.
        for (_, id) in &amps[depth_start..depth_end] {
            if let (Some(element), Some(chosen)) = (
                working.elements.iter_mut().find(|e| &e.id == id),
                out.get(id),
            ) {
                element.fitted = Some(Fitted::Active(chosen.clone()));
            }
        }
        depth_start = depth_end;
    }
    Ok(out)
}

/// `plant` with `align`'s values fitted. The root's optics are kept.
pub fn aligned(plant: &Plant, fitted: &BTreeMap<ElementId, ActiveFitted>) -> Plant {
    let mut out = plant.clone();
    for element in &mut out.elements {
        if let Some(chosen) = fitted.get(&element.id) {
            element.fitted = Some(Fitted::Active(chosen.clone()));
        }
    }
    out
}
