//! Legacy, then upgraded (`docs/tasks/23-as-built-designer.md`): a region designed under a legacy rules file and a legacy
//! profile is upgraded in place to the target profile under the target rules' upgrade moves. Sites stay: every element
//! keeps its id, its structure and its spans. Modules and faceplates change where the part no longer qualifies; plug-ins
//! and conditioners are chosen again for the new band; premises parts that no longer fit come out; the node's optics
//! change with its kind, so the fiber plant's equipment and links are chosen again over the same route. What still fails
//! is reported, not hidden: that's the marginal top of the band an upgraded estate has (design-practice.md §10).
//!
//! `design::migrate`'s version 1 conversion never picks a part with a version 2 field (`docs/as-built-plan.md`, rule 7),
//! so it can't make these moves; this is its version 2 counterpart.

use std::collections::BTreeMap;

use schema::common::{ComponentId, EdgeId, ElementId, ServiceGroupId};
use schema::component::{Component, ComponentSpec, ModemGeneration, qualifies};
use schema::design_rules::UpgradeMove;
use schema::plant::{ElementKind, Fitted, Plant, RecordStatus, SetPoint};
use serde::{Deserialize, Serialize};
use sim::align::{align, aligned};

use super::DesignError;
use super::kit::Kit;

/// One part an upgrade changed or took out.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Change {
    /// The element.
    pub element: ElementId,
    /// The part it had.
    pub from: ComponentId,
    /// The part it has now, or `None` where it came out or nothing qualifies.
    pub to: Option<ComponentId>,
    /// The move, or `None` where none the rules allow covers it.
    pub by: Option<UpgradeMove>,
    /// Why.
    pub reason: String,
}

/// What an upgrade did, node by node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UpgradeReport {
    /// The legacy rules.
    pub legacy_rules_id: String,
    /// The legacy profile.
    pub legacy_profile: String,
    /// The moves the target rules allow.
    pub moves: Vec<UpgradeMove>,
    /// Per node, what changed.
    pub changes: BTreeMap<ServiceGroupId, Vec<Change>>,
    /// Parts that no longer qualify and that no allowed move replaces, by node.
    pub stuck: BTreeMap<ServiceGroupId, Vec<Change>>,
}

/// A part's same-shaped replacement from the target rules' parts: an amp by its outputs, a tap by its ports, value and
/// termination, a splitter by its legs, a coupler by its value; or the target root.
fn replacement<'a>(kit: &Kit<'a>, from: &Component) -> Option<&'a Component> {
    let profile = kit.profile;
    match &from.spec {
        ComponentSpec::Node(_) | ComponentSpec::Rpd(_) => Some(kit.root),
        ComponentSpec::Amp(a) => {
            let outputs = a.outputs.len();
            [&kit.express_amp, &kit.feeder_amp]
                .into_iter()
                .find(|x| x.spec.outputs.len() == outputs)
                .map(|x| x.part)
        }
        ComponentSpec::Tap(t) => kit
            .taps
            .iter()
            .filter(|f| f.ports == t.ports)
            .flat_map(|f| f.through.iter().chain(f.terminating.iter()))
            .find(|x| {
                x.spec.terminating == t.terminating
                    && (x.spec.tap_value_db - t.tap_value_db).abs() < 1e-9
            })
            .map(|x| x.part),
        _ => kit
            .library
            .components
            .iter()
            .filter(|c| c.class() == from.class() && qualifies(c, profile))
            .find(|c| match (&c.spec, &from.spec) {
                (ComponentSpec::Splitter(a), ComponentSpec::Splitter(b)) => a.legs == b.legs,
                (ComponentSpec::DirectionalCoupler(a), ComponentSpec::DirectionalCoupler(b)) => {
                    (a.coupled_value_db - b.coupled_value_db).abs() < 1e-9
                }
                (ComponentSpec::LineEqualizer(a), ComponentSpec::LineEqualizer(b)) => {
                    (a.value_db - b.value_db).abs() < 1e-9
                }
                _ => true,
            }),
    }
}

/// Whether a filter stops part of the profile's downstream.
fn blocks(kit: &Kit<'_>, part: &Component) -> bool {
    let ComponentSpec::Filter(f) = &part.spec else {
        return false;
    };
    let (lo, hi) = sim::walk::ds_band(kit.profile);
    f.stop_bands.iter().any(|b| b.lo_mhz < hi && b.hi_mhz > lo)
}

/// The bottom of the tuner band, MHz: a return band reaching above it transmits into a set-top's or a television's tuner
/// (TR-MIDSPLIT §6.2; `check_physics`'s transmit-through-the-premises check, physics §19).
const TUNER_LO_MHZ: f64 = 54.0;

/// Takes every RF set-top and television off the coax, and, with `bypass`, every in-home splitter then left with one leg.
fn replace_set_tops(
    kit: &Kit<'_>,
    plant: &mut Plant,
    changes: &mut Vec<Change>,
    bypass: bool,
) -> Result<(), DesignError> {
    use schema::component::CpeDevice;
    use schema::plant::EdgeKind;
    let tuners: Vec<(ElementId, ComponentId)> = plant
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Cpe))
        .filter(|e| {
            kit.library.get(&e.component_ref).is_some_and(|c| {
                matches!(&c.spec, ComponentSpec::Cpe(s) if matches!(s.device, CpeDevice::SetTop | CpeDevice::Television))
            })
        })
        .map(|e| (e.id.clone(), e.component_ref.clone()))
        .collect();
    for (id, part) in &tuners {
        plant.elements.retain(|e| &e.id != id);
        plant.edges.retain(|e| &e.to.element_id != id);
        changes.push(Change {
            element: id.clone(),
            from: part.clone(),
            to: None,
            by: Some(UpgradeMove::ReplaceSetTops),
            reason:
                "the return band reaches its tuner; a device on the home's own network replaces it"
                    .into(),
        });
    }
    if !bypass {
        return Ok(());
    }
    let splitters: Vec<(ElementId, ComponentId)> = plant
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Splitter) && e.structure.is_none())
        .map(|e| (e.id.clone(), e.component_ref.clone()))
        .collect();
    for (id, part) in splitters {
        let legs: Vec<usize> = (0..plant.edges.len())
            .filter(|i| plant.edges[*i].from.element_id == id)
            .collect();
        let Some(into) = plant.edges.iter().position(|e| e.to.element_id == id) else {
            continue;
        };
        if legs.len() != 1 {
            continue;
        }
        let leg = legs[0];
        let feed = plant.edges[into].from.clone();
        // One cable from what fed the splitter to what its leg fed: the two lengths of house cable, end to end.
        let kind = match (&plant.edges[into].kind, &plant.edges[leg].kind) {
            (
                EdgeKind::House {
                    cable_ref,
                    len_ft: a,
                    fittings: fa,
                },
                EdgeKind::House {
                    len_ft: b,
                    fittings: fb,
                    ..
                },
            ) => EdgeKind::House {
                cable_ref: cable_ref.clone(),
                len_ft: a + b,
                fittings: fa.iter().chain(fb).cloned().collect(),
            },
            (k, EdgeKind::Direct) => k.clone(),
            (_, k) => k.clone(),
        };
        plant.edges[leg].from = feed;
        plant.edges[leg].kind = kind;
        let into_id = plant.edges[into].id.clone();
        plant.edges.retain(|e| e.id != into_id);
        plant.elements.retain(|e| e.id != id);
        changes.push(Change {
            element: id.clone(),
            from: part,
            to: None,
            by: Some(UpgradeMove::RemovePremisesParts),
            reason: "left feeding one device once the set-tops came off the coax".into(),
        });
    }
    Ok(())
}

/// Upgrades one node's plant in place. Returns the plant on the target profile, aligned, and what changed.
pub fn upgrade_plant(
    kit: &Kit<'_>,
    plant: &Plant,
    allowed: &[UpgradeMove],
) -> Result<(Plant, Vec<Change>, Vec<Change>), DesignError> {
    let mut out = plant.clone();
    out.profile_ref = kit.profile.id.clone();
    let mut changes = Vec::new();
    let mut stuck = Vec::new();
    let may = |m: UpgradeMove| allowed.contains(&m);
    let premises_parts = &kit.complete.premises_parts;
    let mut removed: Vec<ElementId> = Vec::new();
    for element in &mut out.elements {
        let Some(part) = kit.library.get(&element.component_ref) else {
            return Err(DesignError::MissingPart(element.component_ref.to_string()));
        };
        let is_root = matches!(
            element.kind,
            ElementKind::Node { .. } | ElementKind::Rpd { .. }
        );
        let filter_blocks = matches!(element.kind, ElementKind::Filter) && blocks(kit, part);
        let fits = qualifies(part, kit.profile) && (!is_root || part.class() == kit.root.class());
        if fits && !filter_blocks {
            continue;
        }
        let (mv, reason) = match element.kind {
            ElementKind::Node { .. } | ElementKind::Rpd { .. } | ElementKind::Amp { .. } => (
                UpgradeMove::SwapActiveModules,
                "the module isn't rated to the target",
            ),
            ElementKind::Tap => (
                UpgradeMove::SwapTapFaceplates,
                "the faceplate isn't rated to the target",
            ),
            ElementKind::Splitter
            | ElementKind::DirectionalCoupler
            | ElementKind::LineEqualizer
            | ElementKind::PowerInserter
            | ElementKind::Terminator
                if element.structure.is_some() =>
            {
                (
                    UpgradeMove::SwapPassives,
                    "the passive isn't rated to the target",
                )
            }
            ElementKind::Modem => (
                UpgradeMove::SwapModems,
                "the modem isn't rated to the target",
            ),
            _ => (
                UpgradeMove::RemovePremisesParts,
                if filter_blocks {
                    "the filter stops part of the target's downstream"
                } else {
                    "it isn't rated to the target"
                },
            ),
        };
        if !may(mv) {
            stuck.push(Change {
                element: element.id.clone(),
                from: part.id.clone(),
                to: None,
                by: None,
                reason: reason.into(),
            });
            continue;
        }
        if mv == UpgradeMove::RemovePremisesParts {
            removed.push(element.id.clone());
            changes.push(Change {
                element: element.id.clone(),
                from: part.id.clone(),
                to: None,
                by: Some(mv),
                reason: reason.into(),
            });
            continue;
        }
        let to = if mv == UpgradeMove::SwapModems {
            let generation = match &part.spec {
                ComponentSpec::Modem(m) => m.generation,
                _ => ModemGeneration::D31,
            };
            let id = match generation {
                ModemGeneration::D30 => &premises_parts.modem_d30_ref.value,
                ModemGeneration::D31 => &premises_parts.modem_d31_ref.value,
            };
            kit.library
                .get(id)
                .filter(|c| qualifies(c, kit.profile))
                .or_else(|| kit.library.get(&premises_parts.modem_d31_ref.value))
        } else {
            replacement(kit, part)
        };
        match to {
            Some(new) => {
                changes.push(Change {
                    element: element.id.clone(),
                    from: part.id.clone(),
                    to: Some(new.id.clone()),
                    by: Some(mv),
                    reason: reason.into(),
                });
                element.component_ref = new.id.clone();
                element.record_status = RecordStatus::Designed;
                if is_root {
                    element.kind = match kit.profile.node_kind {
                        schema::profile::NodeKind::Analog => ElementKind::Node {
                            return_atten_db: 0.0,
                        },
                        schema::profile::NodeKind::Rpd => ElementKind::Rpd {
                            return_atten_db: 0.0,
                        },
                    };
                }
            }
            None => stuck.push(Change {
                element: element.id.clone(),
                from: part.id.clone(),
                to: None,
                by: Some(mv),
                reason: format!("{reason}; the target rules name nothing like it"),
            }),
        }
    }
    // Premises parts out: what fed each now feeds what it fed.
    for id in &removed {
        let into: Option<EdgeId> = out
            .edges
            .iter()
            .find(|e| &e.to.element_id == id)
            .map(|e| e.id.clone());
        let onward: Vec<usize> = out
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| &e.from.element_id == id)
            .map(|(i, _)| i)
            .collect();
        let Some(into) = into else {
            continue;
        };
        let feed = out
            .edges
            .iter()
            .find(|e| e.id == into)
            .map(|e| e.from.clone());
        let Some(feed) = feed else {
            continue;
        };
        // A splitter: only its first leg's device stays connected; the rest come out with it.
        let keep = onward.first().copied();
        if let Some(k) = keep {
            out.edges[k].from = feed;
        }
        let drop_edges: Vec<EdgeId> = onward
            .iter()
            .skip(1)
            .map(|i| out.edges[*i].id.clone())
            .collect();
        let orphans: Vec<ElementId> = onward
            .iter()
            .skip(1)
            .map(|i| out.edges[*i].to.element_id.clone())
            .collect();
        out.edges
            .retain(|e| e.id != into && !drop_edges.contains(&e.id));
        out.elements
            .retain(|e| &e.id != id && !orphans.contains(&e.id));
        for o in orphans {
            changes.push(Change {
                element: o,
                from: ComponentId::new("-"),
                to: None,
                by: Some(UpgradeMove::RemovePremisesParts),
                reason: "on a removed splitter's leg".into(),
            });
        }
    }
    // Set-tops and televisions on the coax, where the return band now reaches their tuners: replaced by devices off the
    // coax, and a splitter left feeding the modem alone taken out, its two cables joined.
    if may(UpgradeMove::ReplaceSetTops) && sim::walk::us_band(kit.profile).1 > TUNER_LO_MHZ {
        replace_set_tops(
            kit,
            &mut out,
            &mut changes,
            may(UpgradeMove::RemovePremisesParts),
        )?;
    }
    // Every station to the target's design level, its plug-ins chosen again for the new band, and conditioners too.
    if may(UpgradeMove::ChangePlugIns) {
        let st = kit.station;
        for element in &mut out.elements {
            if let Some(Fitted::Active(f)) = &mut element.fitted {
                f.set_point = Some(SetPoint {
                    out_dbmv_at_hi: st.out_dbmv_at_hi.value,
                    tilt_db: st.tilt_db.value,
                });
                f.forward_rx = None;
                f.return_tx = None;
                f.transceivers.clear();
            }
        }
        let fitted = align(&out, kit.library, kit.profile)?;
        out = aligned(&out, &fitted);
        // Each station set where its own taps sit nearest the target, as the walk sets a new one, then the faceplates
        // re-valued for the new band from the sim's levels: an upgraded run's near taps are often at their highest value
        // already, and only the station can bring them down.
        for _ in 0..2 {
            let moved = super::feeder::center_stations(kit, &mut out)?;
            let fitted = align(&out, kit.library, kit.profile)?;
            out = aligned(&out, &fitted);
            let revalued = super::feeder::retune(kit, &mut out);
            let fitted = align(&out, kit.library, kit.profile)?;
            out = aligned(&out, &fitted);
            if moved == 0 && revalued == 0 {
                break;
            }
        }
        // The faceplates re-valued for the new band, on the record as their swap.
        for change in &mut changes {
            let now = out
                .elements
                .iter()
                .find(|e| e.id == change.element)
                .map(|e| e.component_ref.clone());
            if let (Some(to), Some(now)) = (&change.to, now)
                && to != &now
            {
                change.reason = format!("{}; re-valued for the new band", change.reason);
                change.to = Some(now);
            }
        }
    }
    Ok((out, changes, stuck))
}
