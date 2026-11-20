//! Per-node migration planning: what the physics can compute about a node before and after moving to another profile
//! (`docs/DESIGN.md`, "Migration comparison"). Any gain from removing analog optics stays a labeled what-if, because it rests on an
//! estimate with no source.

use plant_graph::PlantGraph;
use schema::common::{ComponentId, ElementId, Origin, ServiceGroupId, Timestamp};
use schema::component::{
    Component, ComponentClass, ComponentLibrary, ComponentSpec, ModemGeneration, qualifies,
};
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::FaultSet;
use schema::observation::{ChannelUsability, ModemObservation, RegState};
use schema::plant::{EdgeKind, ElementKind, Plant, RecordStatus};
use schema::profile::{NodeKind, Profile, Qam};
use serde::{Deserialize, Serialize};
use sim::Overlay;
use sim::budget::{check_physics, level_budget};
use sim::fec;
use sim::telemetry::{Detail, Scenario, SimOptions, StepObservation, simulate};
use sim::units::{db, lin};

use crate::optimize::DesignError;

/// A part that has to change, and why.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PartChange {
    /// The part in the plant now.
    pub from: ComponentId,
    /// What can replace it, or `None` if the library has nothing.
    pub to: Option<ComponentId>,
    /// How many elements or spans use it.
    pub count: u32,
    /// Why.
    pub reason: String,
}

/// One side of one modem's comparison.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModemState {
    /// Registered and passing traffic.
    pub operational: bool,
    /// Downstream PHY rate at the modulations it can hold, Mbit/s.
    pub ds_capacity_mbps: f64,
    /// Upstream PHY rate at the modulations it can hold, Mbit/s.
    pub us_capacity_mbps: f64,
    /// Highest OFDM modulation it holds, if it has any.
    pub ds_best_qam: Option<Qam>,
    /// Highest OFDMA modulation it holds, if it has any.
    pub us_best_qam: Option<Qam>,
    /// Mean RxMER over OFDM, dB.
    pub rxmer_mean_db: Option<f64>,
    /// How far under its transmit limit it stays, dB.
    pub tx_headroom_db: f64,
}

/// One modem, before and after.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModemComparison {
    /// The modem.
    pub modem: ElementId,
    /// Its generation.
    pub generation: ModemGeneration,
    /// On the current profile.
    pub before: ModemState,
    /// On the target profile.
    pub after: ModemState,
    /// Whether it has service now and won't after.
    pub loses_service: bool,
}

/// What removing analog optics might add. An estimate, labeled as one, until real measurements calibrate it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpticsWhatIf {
    /// The optics CNR assumed, dB.
    pub assumed_cnr_db: f64,
    /// Mean RxMER gain across D3.1 modems, dB.
    pub mean_gain_db: f64,
    /// Says what this is.
    pub label: String,
}

/// A node's migration plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationPlan {
    /// The node.
    pub service_group: ServiceGroupId,
    /// Always `Synthetic` until real plant is imported (invariant 11).
    pub origin: Origin,
    /// Its profile now.
    pub from_profile: String,
    /// Its profile after.
    pub to_profile: String,
    /// Parts to replace.
    pub parts_must_change: Vec<PartChange>,
    /// Limits met now but not after, or vice versa.
    pub issues_before: Vec<String>,
    /// Limits the converted plant doesn't meet.
    pub issues_after: Vec<String>,
    /// Every modem.
    pub modems: Vec<ModemComparison>,
    /// Modems that lose service.
    pub modems_losing_service: u32,
    /// What the typical modem can reach downstream before, Mbit/s: the median of their own rates. The channels are shared, so this isn't per subscriber, and rates don't add across modems.
    pub ds_capacity_before_mbps: f64,
    /// The same, after.
    pub ds_capacity_after_mbps: f64,
    /// What the typical modem can reach upstream before, Mbit/s.
    pub us_capacity_before_mbps: f64,
    /// The same, after.
    pub us_capacity_after_mbps: f64,
    /// The optics what-if, when leaving an analog node for an RPD.
    pub optics_what_if: Option<OpticsWhatIf>,
    /// The plant as it would be, every changed element `Designed`.
    pub converted_plant: Plant,
}

fn component_tap_value(component: &Component) -> f64 {
    match &component.spec {
        ComponentSpec::Tap(tap) => tap.tap_value_db,
        _ => 0.0,
    }
}

/// The nearest part of the same class that does qualify. For taps that means the same value, or nearest to it.
fn replacement<'a>(
    library: &'a ComponentLibrary,
    from: &Component,
    profile: &Profile,
) -> Option<&'a Component> {
    let mut options: Vec<&Component> = library
        .components
        .iter()
        .filter(|c| c.class() == from.class() && qualifies(c, profile))
        .filter(|c| !matches!(&c.spec, ComponentSpec::Tap(t) if !t.power_passing))
        // Version 1's migration: a replacement keeps the part's shape, so a tap keeps its port count and a coupler its
        // value, and a part with a version 2 field is never its choice (`docs/as-built-plan.md`, rule 7).
        .filter(|c| same_shape(c, from) && c.version_2_fields().is_empty())
        .collect();
    let want = component_tap_value(from);
    options.sort_by(|a, b| {
        (component_tap_value(a) - want)
            .abs()
            .total_cmp(&(component_tap_value(b) - want).abs())
            .then(a.id.cmp(&b.id))
    });
    options.first().copied()
}

/// Whether a replacement keeps what the plant around the part depends on: a tap's ports and termination, a
/// splitter's legs, a coupler's value.
fn same_shape(candidate: &Component, from: &Component) -> bool {
    match (&candidate.spec, &from.spec) {
        (ComponentSpec::Tap(a), ComponentSpec::Tap(b)) => {
            a.ports == b.ports && a.terminating == b.terminating
        }
        (ComponentSpec::Splitter(a), ComponentSpec::Splitter(b)) => a.legs == b.legs,
        (ComponentSpec::DirectionalCoupler(a), ComponentSpec::DirectionalCoupler(b)) => {
            (a.coupled_value_db - b.coupled_value_db).abs() < 1e-9
        }
        _ => true,
    }
}

fn change_part(
    changes: &mut Vec<PartChange>,
    from: &ComponentId,
    to: Option<&ComponentId>,
    reason: &str,
) {
    if let Some(change) = changes.iter_mut().find(|c| &c.from == from) {
        change.count += 1;
    } else {
        changes.push(PartChange {
            from: from.clone(),
            to: to.cloned(),
            count: 1,
            reason: reason.to_string(),
        });
    }
}

/// The plant on the target profile, with the parts that must change changed, and those marked `Designed`.
pub fn convert_plant(
    plant: &Plant,
    library: &ComponentLibrary,
    target: &Profile,
) -> Result<(Plant, Vec<PartChange>), DesignError> {
    let mut out = plant.clone();
    out.profile_ref = target.id.clone();
    let mut changes = Vec::new();
    let root_class = match target.node_kind {
        NodeKind::Analog => ComponentClass::Node,
        NodeKind::Rpd => ComponentClass::Rpd,
    };
    let root_part = library
        .components
        .iter()
        .find(|c| c.class() == root_class && qualifies(c, target))
        .ok_or_else(|| DesignError::MissingPart(format!("{root_class:?}")))?;
    for element in &mut out.elements {
        let is_root = matches!(
            element.kind,
            ElementKind::Node { .. } | ElementKind::Rpd { .. }
        );
        if is_root {
            let atten = match element.kind {
                ElementKind::Node { return_atten_db } | ElementKind::Rpd { return_atten_db } => {
                    return_atten_db
                }
                _ => 0.0,
            };
            element.kind = match target.node_kind {
                NodeKind::Analog => ElementKind::Node {
                    return_atten_db: atten,
                },
                NodeKind::Rpd => ElementKind::Rpd {
                    return_atten_db: atten,
                },
            };
            if element.component_ref != root_part.id {
                change_part(
                    &mut changes,
                    &element.component_ref,
                    Some(&root_part.id),
                    "the node type changes",
                );
                element.component_ref = root_part.id.clone();
                element.record_status = RecordStatus::Designed;
            }
            continue;
        }
        let Some(part) = library.get(&element.component_ref) else {
            return Err(DesignError::MissingPart(element.component_ref.to_string()));
        };
        if !qualifies(part, target) {
            let new = replacement(library, part, target);
            let reason = format!(
                "rated to {} MHz, which doesn't cover this band plan",
                part.rated_hi_mhz.unwrap_or(0.0)
            );
            change_part(
                &mut changes,
                &element.component_ref,
                new.map(|c| &c.id),
                &reason,
            );
            if let Some(new) = new {
                element.component_ref = new.id.clone();
                element.record_status = RecordStatus::Designed;
            }
        }
    }
    for edge in &mut out.edges {
        let cable_ref = match &mut edge.kind {
            EdgeKind::Span { cable_ref, .. }
            | EdgeKind::Drop { cable_ref, .. }
            | EdgeKind::House { cable_ref, .. } => cable_ref,
            EdgeKind::Direct => continue,
        };
        let Some(part) = library.get(cable_ref) else {
            return Err(DesignError::MissingPart(cable_ref.to_string()));
        };
        if !qualifies(part, target) {
            let new = replacement(library, part, target);
            let reason = format!(
                "rated to {} MHz, which doesn't cover this band plan",
                part.rated_hi_mhz.unwrap_or(0.0)
            );
            change_part(&mut changes, cable_ref, new.map(|c| &c.id), &reason);
            if let Some(new) = new {
                *cable_ref = new.id.clone();
            }
        }
    }
    changes.sort_by(|a, b| a.from.cmp(&b.from));
    Ok((out, changes))
}

fn calm() -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(0),
        temp_c: 20.0,
        ground_temp_c: 20.0,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

fn observe(graph: &PlantGraph<'_>) -> Result<StepObservation, DesignError> {
    let environment = Environment {
        schema_version: schema::environment::SCHEMA_VERSION.to_string(),
        samples: vec![calm()],
        ambient_sources: Vec::new(),
    };
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.to_string(),
        seed: 0,
        faults: Vec::new(),
    };
    let steps = simulate(&Scenario {
        graph,
        environment: &environment,
        faults: &faults,
        seed: 0,
        options: SimOptions {
            detail: Detail::Summary { stride: 8 },
            randomize: None,
            measurement_noise_db: 0.0,
            rxmer_scatter_db: 0.0,
        },
        link: None,
    })?;
    steps
        .into_iter()
        .next()
        .ok_or_else(|| DesignError::InvalidPlant("no step".into()))
}

/// 256-QAM, J.83 Annex B, at 5.360537 Msym/s: what an SC-QAM channel carries, Mbit/s, before overhead.
const SC_QAM_DS_MBPS: f64 = 5.360537 * 8.0;

fn usable(
    states: &[schema::observation::ChannelStateRecord],
    id: &schema::common::ChannelId,
) -> bool {
    states
        .iter()
        .any(|s| &s.channel_id == id && s.state == ChannelUsability::Usable)
}

/// A modem's state on a profile, from what it reports.
fn modem_state(modem: &ModemObservation, profile: &Profile, headroom_db: f64) -> ModemState {
    let operational = modem.reg_state == RegState::Operational;
    let plan = &profile.channel_plan;
    let mut ds = 0.0;
    let mut us = 0.0;
    let mut ds_best = None;
    let mut us_best = None;
    if operational {
        for channel in &plan.ds_sc_qam {
            if usable(&modem.ds_channel_state, &channel.channel_id) {
                ds += SC_QAM_DS_MBPS;
            }
        }
        for channel in &plan.ofdm {
            if !usable(&modem.ds_channel_state, &channel.channel_id) {
                continue;
            }
            let failed: Vec<u8> = modem
                .partial_channel
                .iter()
                .flatten()
                .filter(|r| r.channel_id == channel.channel_id)
                .flat_map(|r| r.failed_data_profiles.iter().copied())
                .collect();
            // The highest order the channel's data profiles offer that hasn't failed.
            let best = channel
                .data_profiles
                .iter()
                .filter(|p| !failed.contains(&p.data_profile_id))
                .max_by(|a, b| {
                    fec::bits_per_symbol(a.modulation)
                        .total_cmp(&fec::bits_per_symbol(b.modulation))
                });
            if let Some(best) = best {
                let n = (channel.last_active_mhz - channel.first_active_mhz)
                    / (channel.subcarrier_spacing_khz / 1000.0);
                let t_sym_us = 1000.0 / channel.subcarrier_spacing_khz + channel.cp_us;
                ds += n * fec::bits_per_symbol(best.modulation) / t_sym_us;
                ds_best = Some(best.modulation);
            }
        }
        for channel in &plan.us_sc_qam {
            if usable(&modem.us_channel_state, &channel.channel_id) {
                us += channel.symbol_rate_msym_per_s * fec::bits_per_symbol(channel.modulation);
            }
        }
        if modem.generation == ModemGeneration::D31 {
            for channel in &plan.ofdma {
                if !usable(&modem.us_channel_state, &channel.channel_id) {
                    continue;
                }
                let mean = modem
                    .us_rxmer
                    .iter()
                    .flatten()
                    .find(|r| r.channel_id == channel.channel_id)
                    .and_then(|r| r.us_rxmer_mean_db);
                let best = channel
                    .data_profiles
                    .iter()
                    .filter(|p| {
                        // §4.6: what it needs in AWGN to hold that modulation.
                        let need = profile
                            .fec
                            .iter()
                            .find(|r| r.qam == p.modulation)
                            .map(|r| r.req_cnr_db);
                        mean.zip(need).is_some_and(|(m, n)| m >= n)
                    })
                    .max_by(|a, b| {
                        fec::bits_per_symbol(a.modulation)
                            .total_cmp(&fec::bits_per_symbol(b.modulation))
                    });
                if let Some(best) = best {
                    let n = (channel.last_active_mhz - channel.first_active_mhz)
                        / (channel.subcarrier_spacing_khz / 1000.0);
                    let t_sym_us = 1000.0 / channel.subcarrier_spacing_khz + channel.cp_us;
                    us += n * fec::bits_per_symbol(best.modulation) / t_sym_us;
                    us_best = Some(best.modulation);
                }
            }
        }
    }
    let rxmer: Vec<f64> = modem
        .rxmer
        .iter()
        .flatten()
        .filter_map(|r| r.rxmer_mean_db)
        .collect();
    ModemState {
        operational,
        ds_capacity_mbps: ds,
        us_capacity_mbps: us,
        ds_best_qam: ds_best,
        us_best_qam: us_best,
        rxmer_mean_db: (!rxmer.is_empty()).then(|| rxmer.iter().sum::<f64>() / rxmer.len() as f64),
        tx_headroom_db: headroom_db,
    }
}

fn node_states(graph: &PlantGraph<'_>) -> Result<Vec<(ModemObservation, ModemState)>, DesignError> {
    let step = observe(graph)?;
    let budget = level_budget(graph, &calm(), &Overlay::default())?;
    Ok(step
        .modems
        .into_iter()
        .map(|modem| {
            let headroom = budget.modems.get(&modem.element_id).map_or(0.0, |m| {
                m.tx_limit_dbmv_per_1p6mhz - m.tx_used_max_dbmv_per_1p6mhz
            });
            let state = modem_state(&modem, graph.profile(), headroom);
            (modem, state)
        })
        .collect())
}

fn issues_of(graph: &PlantGraph<'_>) -> Result<Vec<String>, DesignError> {
    let temps = [-20.0, 20.0, 45.0].map(|temp_c| EnvironmentSample {
        temp_c,
        ground_temp_c: temp_c,
        ..calm()
    });
    let mut out: Vec<String> = graph
        .check_design()
        .iter()
        .map(|i| format!("{i:?}"))
        .collect();
    out.extend(
        check_physics(graph, &temps, &Overlay::default())?
            .iter()
            .map(|i| format!("{i:?}")),
    );
    out.sort();
    out.dedup();
    Ok(out)
}

/// A node's plan, from its plant on `from` to `to`. Pure: nothing about the node changes until a user applies it. The optics
/// what-if assumes the node part's constant `CNR_opt`; [`plan_migration_with_link`] reads the node's link instead.
pub fn plan_migration(
    plant: &Plant,
    library: &ComponentLibrary,
    from: &Profile,
    to: &Profile,
) -> Result<MigrationPlan, DesignError> {
    plan_migration_with_link(plant, library, from, to, None)
}

/// [`plan_migration`], with the node's link state from `sim::optics` (§16). An analog node's forward CNR then comes from its
/// link, so the gain from leaving it depends on the link's length and parts [TR-DCA §6.2.3]. `None` is `plan_migration`.
pub fn plan_migration_with_link(
    plant: &Plant,
    library: &ComponentLibrary,
    from: &Profile,
    to: &Profile,
    link: Option<&sim::optics::NodeLink>,
) -> Result<MigrationPlan, DesignError> {
    let bad = |e: plant_graph::GraphError| DesignError::InvalidPlant(format!("{e:?}"));
    let before_graph = PlantGraph::new(plant, library, from).map_err(bad)?;
    let (converted, parts_must_change) = convert_plant(plant, library, to)?;
    let after_graph = PlantGraph::new(&converted, library, to).map_err(bad)?;

    let before = node_states(&before_graph)?;
    let after = node_states(&after_graph)?;
    let mut modems = Vec::new();
    for (observation, state) in &before {
        let Some((_, later)) = after
            .iter()
            .find(|(o, _)| o.element_id == observation.element_id)
        else {
            continue;
        };
        modems.push(ModemComparison {
            modem: observation.element_id.clone(),
            generation: observation.generation,
            before: state.clone(),
            after: later.clone(),
            loses_service: state.operational && !later.operational,
        });
    }
    let median = |of: &dyn Fn(&ModemComparison) -> f64| -> f64 {
        let mut values: Vec<f64> = modems.iter().map(of).collect();
        values.sort_by(f64::total_cmp);
        values.get(values.len() / 2).copied().unwrap_or(0.0)
    };

    // §4.5: what removing `CNR_opt` would add to what the RPD side already shows. Only with an analog node to leave.
    let root_part = before_graph.component(&before_graph.root().component_ref);
    let optics_what_if = match (root_part.map(|c| &c.spec), to.node_kind) {
        (Some(ComponentSpec::Node(node)), NodeKind::Rpd) => {
            let from_link = link.and_then(sim::optics::NodeLink::forward_cnr_db);
            let cnr_db = from_link.unwrap_or(node.optics_cnr_db);
            let gains: Vec<f64> = modems
                .iter()
                .filter_map(|m| m.after.rxmer_mean_db)
                .map(|mer| mer - (-db(lin(-mer) + lin(-cnr_db))))
                .collect();
            let label = if from_link.is_some() {
                "What-if only. The optics figure is computed from the node's forward link (§16), from generic and sourced parts, until real measurements calibrate it."
            } else {
                "What-if only. No source gives the optics figure, so this gain is an estimate until real measurements calibrate it."
            };
            (!gains.is_empty()).then(|| OpticsWhatIf {
                assumed_cnr_db: cnr_db,
                mean_gain_db: gains.iter().sum::<f64>() / gains.len() as f64,
                label: label.to_string(),
            })
        }
        _ => None,
    };

    Ok(MigrationPlan {
        service_group: plant.service_group.clone(),
        origin: plant.origin,
        from_profile: from.id.to_string(),
        to_profile: to.id.to_string(),
        parts_must_change,
        issues_before: issues_of(&before_graph)?,
        issues_after: issues_of(&after_graph)?,
        modems_losing_service: modems.iter().filter(|m| m.loses_service).count() as u32,
        ds_capacity_before_mbps: median(&|m| m.before.ds_capacity_mbps),
        ds_capacity_after_mbps: median(&|m| m.after.ds_capacity_mbps),
        us_capacity_before_mbps: median(&|m| m.before.us_capacity_mbps),
        us_capacity_after_mbps: median(&|m| m.after.us_capacity_mbps),
        modems,
        optics_what_if,
        converted_plant: converted,
    })
}
