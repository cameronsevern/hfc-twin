//! One candidate design, stage after stage: structures, nodes, the RF walk, drops, power, alignment. An area whose plant
//! breaks the rules' cascade or active limit splits in two and is designed again.

use schema::common::{ServiceGroupId, StructureId};
use schema::plant::Plant;
use schema::region::{DesignControls, StreetMap};
use sim::align::{align, aligned};

use super::DesignError;
use super::assemble::node_plant;
use super::backbone::Overflow;
use super::kit::Kit;
use super::nodes::{Area, NodeInputs, areas, split};
use super::power::power;
use super::structures::{StructureRules, Structures, structures};

/// One node of a candidate.
#[derive(Debug, Clone)]
pub struct NodePlan {
    /// The plant, powered and aligned; optics are the fiber stage's.
    pub plant: Plant,
    /// Its area.
    pub area: Area,
    /// Where a stage couldn't hold a rule, in words.
    pub notes: Vec<String>,
    /// The template each home got, by home id.
    pub templates: Vec<(String, String)>,
    /// Runs its coax follows.
    pub runs: std::collections::BTreeSet<usize>,
    /// Structures it uses.
    pub sites: std::collections::BTreeSet<usize>,
}

/// What `structures` reads, from the rules and controls.
pub fn structure_rules(
    kit: &Kit<'_>,
    controls: &DesignControls,
) -> Result<StructureRules, DesignError> {
    let st = &kit.rules.structures;
    let need = |what: &str| DesignError::Rules(format!("structures.{what}"));
    let longest_drop = kit
        .rules
        .drops
        .iter()
        .map(|d| d.max_len_ft.value)
        .fold(0.0, f64::max);
    Ok(StructureRules {
        pole_spacing_ft: st.pole_spacing_ft.value,
        pedestal_spacing_ft: st.pedestal_spacing_ft.value,
        side: st.side.as_ref().ok_or_else(|| need("side"))?.value,
        setback_ft: st
            .setback_ft
            .as_ref()
            .ok_or_else(|| need("setback_ft"))?
            .value,
        pole_ref: st
            .pole_ref
            .as_ref()
            .ok_or_else(|| need("pole_ref"))?
            .value
            .clone(),
        pedestal_ref: st
            .pedestal_ref
            .as_ref()
            .ok_or_else(|| need("pedestal_ref"))?
            .value
            .clone(),
        max_drop_ft: controls.max_drop_ft.min(longest_drop),
        block_units: kit
            .templates
            .iter()
            .filter(|t| t.modems > 1)
            .map(|t| t.modems as usize)
            .min()
            .unwrap_or(0),
        group_units: kit
            .templates
            .iter()
            .filter(|t| t.modems > 1)
            .map(|t| t.modems as usize)
            .max()
            .unwrap_or(0),
    })
}

/// The structures of a map under the rules.
pub fn place_structures(
    kit: &Kit<'_>,
    map: &StreetMap,
    controls: &DesignControls,
) -> Result<Structures, DesignError> {
    Ok(structures(map, &structure_rules(kit, controls)?))
}

/// What `nodes` reads.
pub fn node_inputs(kit: &Kit<'_>, controls: &DesignControls, step: i32) -> NodeInputs {
    let price = |id: &schema::common::ComponentId| {
        kit.rules
            .costs
            .parts
            .iter()
            .find(|c| &c.component_ref == id)
            .map_or(0.0, |c| c.usd.value)
    };
    let fiber_usd_per_ft = kit
        .rules
        .fiber
        .cable_refs
        .iter()
        .flat_map(|r| &r.value)
        .map(price)
        .filter(|p| *p > 0.0)
        .fold(f64::INFINITY, f64::min);
    let max_homes = (kit.rules.node.max_homes.value).min(kit.profile.max_homes_passed) as usize;
    NodeInputs {
        target_homes: (controls.homes_per_node as usize).min(max_homes).max(1),
        step,
        pins: controls.pinned_nodes.iter().map(|p| p.geo).collect(),
        fiber_usd_per_ft: if fiber_usd_per_ft.is_finite() {
            fiber_usd_per_ft
        } else {
            0.0
        },
        amp_usd: price(&kit.express_amp.part.id),
    }
}

/// Every node of one candidate, and the homes no node serves.
pub fn plan_nodes(
    kit: &Kit<'_>,
    s: &Structures,
    map: &StreetMap,
    controls: &DesignControls,
    step: i32,
) -> Result<(Vec<NodePlan>, Vec<usize>), DesignError> {
    let hub = s.nearest(map.headend).ok_or(DesignError::NoStreets)?;
    let inputs = node_inputs(kit, controls, step);
    let (found, mut unserved) = areas(s, hub, kit, &inputs);
    let mut work: Vec<Area> = found.into_iter().rev().collect();
    let mut out = Vec::new();
    let mut number = 0;
    while let Some(area) = work.pop() {
        let sg = ServiceGroupId::new(format!("sg-{number}"));
        match node_plant(kit, s, map, &area, controls, &sg) {
            Ok(built) => {
                let (powered, power_notes) = power(kit, &built.plant)?;
                let fitted = align(&powered, kit.library, kit.profile)?;
                let mut plant = aligned(&powered, &fitted);
                // Taps the walk's estimate left outside the port target once alignment's steps are in, re-valued from
                // the sim's own levels, and the amps aligned again; a few rounds at most, since each change is local.
                for _ in 0..4 {
                    if super::feeder::retune(kit, &mut plant) == 0 {
                        break;
                    }
                    let fitted = align(&plant, kit.library, kit.profile)?;
                    plant = aligned(&plant, &fitted);
                }
                // A station whose taps alignment left outside the window, moved inside the rules' levels to where they
                // all fit if any level does, and the taps re-valued from there.
                if super::feeder::center_stations(kit, &mut plant)? > 0 {
                    let fitted = align(&plant, kit.library, kit.profile)?;
                    plant = aligned(&plant, &fitted);
                    if super::feeder::retune(kit, &mut plant) > 0 {
                        let fitted = align(&plant, kit.library, kit.profile)?;
                        plant = aligned(&plant, &fitted);
                    }
                }
                // Homes the sim still flags, drawn again against their ports as built.
                let mut home_templates = built.premises.templates.clone();
                // Homes the sim flags, drawn again; a home drawn again reads the plant as it then is, so a few rounds,
                // while any changes.
                let mut refit = |plant: &mut Plant| {
                    for _ in 0..3 {
                        if super::drops::refit(
                            kit,
                            plant,
                            s,
                            &map.homes,
                            &built.rf.taps,
                            controls,
                            &mut home_templates,
                        ) == 0
                        {
                            break;
                        }
                    }
                };
                refit(&mut plant);
                // Amps the pilots left outside N/10 + 3 dB, swept across the band; the homes below re-read after.
                let (swept, changed, held) = super::feeder::sweep(kit, &plant)?;
                if changed > 0 {
                    plant = swept;
                    // The taps below a swept amp, re-valued from the sim's levels, and the amps aligned again with the
                    // equalizers the sweep chose held.
                    for _ in 0..4 {
                        if super::feeder::retune(kit, &mut plant) == 0 {
                            break;
                        }
                        let fitted =
                            sim::align::align_holding(&plant, kit.library, kit.profile, &held)?;
                        plant = aligned(&plant, &fitted);
                    }
                    refit(&mut plant);
                }
                // What the sim still flags, settled on the sim's own reading: twice at most, since a tap changed for one home
                // moves the others on it.
                for _ in 0..2 {
                    if super::drops::settle(
                        kit,
                        &mut plant,
                        s,
                        &map.homes,
                        &built.rf.taps,
                        controls,
                        &mut home_templates,
                    ) == 0
                    {
                        break;
                    }
                }
                let mut notes = built.rf.notes.clone();
                notes.extend(built.premises.notes.iter().cloned());
                notes.extend(power_notes);
                for &r in &s.joins {
                    if built.rf.runs.contains(&r) {
                        let run = &s.runs[r];
                        notes.push(format!(
                            "joined across a {:.0} ft crossing from {} to {}, where no street in the map meets",
                            run.len_ft,
                            s.sites[run.a].id,
                            s.sites[run.b].id
                        ));
                    }
                }
                let templates = home_templates
                    .iter()
                    .map(|(h, t)| (map.homes[*h].id.clone(), t.clone()))
                    .collect();
                out.push(NodePlan {
                    plant,
                    area,
                    notes,
                    templates,
                    runs: built.rf.runs.clone(),
                    sites: built.rf.sites.clone(),
                });
                number += 1;
            }
            Err(overflow) => {
                let parts = if area.homes.len() > 1 {
                    split(s, hub, kit, &inputs, &area)
                } else {
                    Vec::new()
                };
                if parts.len() > 1 {
                    for part in parts.into_iter().rev() {
                        work.push(part);
                    }
                } else {
                    // Can't split further: its homes go unserved, and it's said so.
                    let why = match overflow {
                        Overflow::Cascade => "cascade",
                        Overflow::Actives => "actives",
                    };
                    let _ = why;
                    unserved.extend(area.homes.iter().copied());
                }
            }
        }
    }
    unserved.sort_unstable();
    Ok((out, unserved))
}

/// A structure's id.
pub fn site_id(s: &Structures, site: usize) -> StructureId {
    s.sites[site].id.clone()
}
