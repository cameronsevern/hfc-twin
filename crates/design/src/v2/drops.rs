//! Stage `drops`: a drop from the tap port to each home that takes service, sized by its length, aerial or buried as its
//! street's plant; every other tap port gets the rules' state. Each home's premises are drawn from the templates the rules
//! allow on the profile, by their shares, from the seed and the home's id, and the first drawn that fits the modem's
//! window is the one built. A template with a drop amp is drawn only where the modem would otherwise sit below the
//! rules' level for one (§13).

use std::collections::BTreeMap;

use schema::common::{ComponentId, EdgeId, ElementId, LonLat, PortId};
use schema::component::{Component, ComponentSpec, CpeDevice, ModemGeneration};
use schema::design_rules::{PremisesTemplate, UnusedPort};
use schema::plant::{
    Edge, EdgeKind, Element, ElementKind, Fitted, Installation, PassiveFitted, Port, PortRef,
    PortRole, PortState, PortStateEntry, RecordStatus,
};
use schema::profile::Qam;
use schema::region::{DesignControls, Home};
use sim::passive::{filter_loss_db, filter_shape};
use sim::rng::{Rng, hash_str};

use super::backbone::TapHomes;
use super::fittings::drop_fittings;
use super::kit::Kit;
use super::levels::{Band, Levels, Up, cable, through};
use super::structures::Structures;
use crate::geo::offset;

/// What the drops stage built.
#[derive(Debug, Clone, Default)]
pub struct Premises {
    /// Elements in homes, and the modems.
    pub elements: Vec<Element>,
    /// Drops and in-home cable.
    pub edges: Vec<Edge>,
    /// Per tap, the states of the ports with no drop.
    pub port_states: BTreeMap<ElementId, Vec<PortStateEntry>>,
    /// The template each served home got, by home index.
    pub templates: BTreeMap<usize, String>,
    /// Homes whose premises fit no template, for the record.
    pub notes: Vec<String>,
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

/// The templates in the order a home draws them: by share, without replacement, from its own stream.
fn draw_order<'a>(templates: &[&'a PremisesTemplate], rng: &mut Rng) -> Vec<&'a PremisesTemplate> {
    let mut left: Vec<&PremisesTemplate> = templates.to_vec();
    let mut out = Vec::new();
    while !left.is_empty() {
        let total: f64 = left.iter().map(|t| t.share.value.max(0.0)).sum();
        let mut x = rng.uniform() * total;
        // Templates with no share come last, in the rules' order: they're a remedy, not a draw.
        let mut pick = if total > 0.0 { left.len() - 1 } else { 0 };
        for (i, t) in left.iter().enumerate() {
            x -= t.share.value.max(0.0);
            if x < 0.0 {
                pick = i;
                break;
            }
        }
        out.push(left.remove(pick));
    }
    out
}

/// The loss a template puts between the drop's end and its modem, less the gain of its drop amp, at a frequency: forward,
/// or on the return, where a drop amp's return is its passive loss or its return gain.
fn premises_loss(kit: &Kit<'_>, template: &PremisesTemplate, f: f64, temp: f64) -> f64 {
    premises_loss_dir(kit, template, f, temp, true)
}

pub(super) fn premises_loss_dir(
    kit: &Kit<'_>,
    template: &PremisesTemplate,
    f: f64,
    temp: f64,
    forward: bool,
) -> f64 {
    let get = |id: &Option<ComponentId>| id.as_ref().and_then(|i| kit.library.get(i));
    let mut loss = 0.0;
    if let Some(gb) = get(&template.ground_block_ref) {
        loss += through(gb, f);
    }
    if let Some(filter) = get(&template.filter_ref)
        && let ComponentSpec::Filter(spec) = &filter.spec
    {
        loss += filter_loss_db(spec, &filter_shape(spec), f);
    }
    if let Some(splitter) = get(&template.splitter_ref) {
        loss += through(splitter, f);
    }
    if let Some(amp) = get(&template.drop_amp_ref)
        && let ComponentSpec::DropAmp(spec) = &amp.spec
    {
        if forward {
            loss -= spec.gain_db;
        } else if let Some(gain) = spec.return_gain_db {
            loss -= gain;
        } else {
            loss += spec.return_passive_loss_db.unwrap_or(0.0);
        }
    }
    if template.ground_block_ref.is_some() && template.house_cable_ft > 0.0 {
        let house = &kit.complete.premises_parts.house_cable_ref.value;
        if let Some(ComponentSpec::Drop(spec)) = kit.library.get(house).map(|c| &c.spec) {
            loss += cable(spec, template.house_cable_ft)(f, temp) + 2.0 * through(kit.drop_f, f);
        }
    }
    loss
}

/// The lowest transmit power a DOCSIS 3.1 modem reports, dBmV per 1.6 MHz (§7.2).
const TX_FLOOR_DBMV: f64 = 17.0;

/// §7.2: a modem part's transmit limit per 1.6 MHz on this profile's return band, as `check_physics` reads it.
pub(super) fn tx_limit(kit: &Kit<'_>, modem: &Component) -> f64 {
    let ComponentSpec::Modem(spec) = &modem.spec else {
        return f64::INFINITY;
    };
    let (us_lo, us_hi) = sim::walk::us_band(kit.profile);
    match spec.generation {
        ModemGeneration::D31 => {
            sim::upstream::p1p6hi_dbmv(spec.tx_max_total_dbmv, 0.0, us_hi - us_lo)
        }
        ModemGeneration::D30 => {
            let channels = kit.profile.channel_plan.us_sc_qam.len().max(1) as f64;
            spec.tx_max_total_dbmv
                - sim::units::db(channels)
                - sim::upstream::per_1p6_from_per_6p4_db()
        }
    }
}

/// The window a modem's level must sit in: the profile's floor for its highest modulation, and its ceiling.
fn window(kit: &Kit<'_>) -> (f64, f64) {
    let levels = &kit.profile.design_levels;
    let top = kit
        .profile
        .channel_plan
        .ofdm
        .iter()
        .flat_map(|o| o.data_profiles.iter().map(|p| p.modulation))
        .chain(
            kit.profile
                .channel_plan
                .ds_sc_qam
                .iter()
                .map(|c| c.modulation),
        )
        .max_by_key(|q| *q as u32)
        .unwrap_or(Qam::Qam256);
    let floor = levels
        .modem_in_min_dbmv_by_qam
        .iter()
        .find(|f| f.qam == top)
        .map_or(-6.0, |f| f.min_dbmv);
    (floor, levels.modem_in_max_dbmv)
}

/// The modem's level, top and bottom of the band at each temperature, from a tap port through a drop and a template.
fn modem_levels(
    kit: &Kit<'_>,
    band: &Band,
    port: &Levels,
    drop: &Component,
    drop_ft: f64,
    template: &PremisesTemplate,
) -> Levels {
    let ComponentSpec::Drop(spec) = &drop.spec else {
        return *port;
    };
    port.less(band, |f, t| {
        cable(spec, drop_ft)(f, t)
            + 2.0 * through(kit.drop_f, f)
            + premises_loss(kit, template, f, t)
    })
}

/// The drop cable the rules give a drop of this length.
pub(super) fn drop_cable<'a>(kit: &Kit<'a>, drop_ft: f64) -> Option<&'a Component> {
    kit.rules
        .drops
        .iter()
        .find(|d| d.max_len_ft.value >= drop_ft)
        .or(kit.rules.drops.last())
        .and_then(|d| kit.library.get(&d.cable_ref.value))
}

/// Whether a template puts a modem behind `port` on a drop of `drop_ft` inside the level window and the spread limit
/// at the modem (physics §3.3, PHY Tables 45 and 46), and, given the return loss from the port back to its active,
/// transmitting inside its limit and, for DOCSIS 3.1, above the reportable floor (§7.2). A drop amp only where the
/// level without one is under the rules'. Every limit is held `margin_db` inside itself: the drops stage reads five check
/// points and the sim the whole band, so a home is first fitted with room for the difference (task 25).
#[allow(clippy::too_many_arguments)]
fn template_fits(
    kit: &Kit<'_>,
    band: &Band,
    port: &Levels,
    port_up: Option<&Up>,
    drop: &Component,
    drop_ft: f64,
    t: &PremisesTemplate,
    d31: bool,
    tx_limit: f64,
    margin_db: f64,
) -> bool {
    let (floor, cap) = window(kit);
    let (floor, cap) = (floor + margin_db, cap - margin_db);
    let spread_max = kit.profile.design_levels.psd_spread_max_db - margin_db;
    let tx_limit = tx_limit - margin_db;
    let m = modem_levels(kit, band, port, drop, drop_ft, t);
    let inside = (0..3).all(|i| {
        let row = &m.0[i];
        let hi = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let lo = row.iter().copied().fold(f64::INFINITY, f64::min);
        lo >= floor && hi <= cap && hi - lo <= spread_max
    });
    if !inside {
        return false;
    }
    if let Some(port_up) = port_up {
        let ComponentSpec::Drop(dspec) = &drop.spec else {
            return false;
        };
        let up = port_up.plus(band, |f, temp| {
            cable(dspec, drop_ft)(f, temp)
                + 2.0 * through(kit.drop_f, f)
                + premises_loss_dir(kit, t, f, temp, false)
        });
        let target = kit.profile.design_levels.us_node_in_target_dbmv;
        let per_1p6 = sim::upstream::per_1p6_from_per_6p4_db();
        let tx_ok = (0..3).all(|i| {
            let most = up.0[i][0].max(up.0[i][1]);
            let least = up.0[i][0].min(up.0[i][1]);
            target + most - per_1p6 <= tx_limit
                && (!d31 || target + least - per_1p6 >= TX_FLOOR_DBMV + margin_db)
        });
        if !tx_ok {
            return false;
        }
    }
    // A drop amp with several outputs distributes, as a splitter would; one with one output boosts, and only where
    // the level without it is under the rules'.
    let booster = t
        .drop_amp_ref
        .as_ref()
        .and_then(|id| kit.library.get(id))
        .is_some_and(|c| matches!(&c.spec, ComponentSpec::DropAmp(a) if a.outputs <= 1));
    if booster {
        let bare = PremisesTemplate {
            drop_amp_ref: None,
            ..t.clone()
        };
        let without = modem_levels(kit, band, port, drop, drop_ft, &bare);
        let below = kit.complete.premises_parts.drop_amp_below_dbmv.value;
        return (0..3).any(|i| without.hi(i) < below);
    }
    true
}

/// Whether a modem behind `port`, on each of these drops, can be served by one of the templates the profile allows, as
/// [`template_fits`] judges, for either generation of modem: the drops stage draws among the templates and takes the
/// first that fits. The limits are the modem's, behind its premises, not the port's: an in-home split tilts the signal
/// down by 8 dB at 1 GHz, and that's where a run's negative tilt binds (plant-and-powering.md §5). `port_up`, the
/// return loss from the port to its active, adds the transmit checks.
pub fn serves(
    kit: &Kit<'_>,
    band: &Band,
    port: &Levels,
    port_up: Option<&Up>,
    drops_ft: &[f64],
    group_drops_ft: &[f64],
) -> bool {
    let parts = &kit.complete.premises_parts;
    let modems: Vec<(bool, f64)> = [
        (false, &parts.modem_d30_ref.value),
        (true, &parts.modem_d31_ref.value),
    ]
    .into_iter()
    .filter_map(|(d31, id)| kit.library.get(id).map(|m| (d31, tx_limit(kit, m))))
    .collect();
    let fits_on = |drops: &[f64], group: bool| {
        drops.iter().all(|&ft| {
            let Some(drop) = drop_cable(kit, ft) else {
                return true;
            };
            modems.iter().all(|&(d31, limit)| {
                kit.templates
                    .iter()
                    .filter(|t| (t.modems > 1) == group)
                    .any(|t| template_fits(kit, band, port, port_up, drop, ft, t, d31, limit, 0.0))
            })
        })
    };
    // An apartment group behind the port is judged on the templates for a group, a home on those for a home (task 25).
    fits_on(drops_ft, false) && fits_on(group_drops_ft, true)
}

/// Builds every drop and home behind `taps`.
pub fn drops(
    kit: &Kit<'_>,
    structures: &Structures,
    homes: &[Home],
    taps: &[TapHomes],
    controls: &DesignControls,
    service_group: &str,
) -> Premises {
    drops_with(kit, structures, homes, taps, controls, service_group, None)
}

/// [`drops`], with `forced` built for every home it suits (one modem for a home, more for an apartment group) in place
/// of the draw: how [`settle`] tries a template.
fn drops_with(
    kit: &Kit<'_>,
    structures: &Structures,
    homes: &[Home],
    taps: &[TapHomes],
    controls: &DesignControls,
    service_group: &str,
    forced: Option<&PremisesTemplate>,
) -> Premises {
    let band = Band::new(kit.profile);
    let parts = &kit.complete.premises_parts;
    let mut out = Premises::default();
    let unused = |port: &PortId| PortStateEntry {
        port: port.clone(),
        state: match kit.unused_port {
            UnusedPort::Terminated => PortState::Terminated {
                component_ref: kit.terminator.id.clone(),
            },
            UnusedPort::Open => PortState::Open,
        },
    };
    let mut drop_n = 0;
    let mut house_n = 0;
    let mut direct_n = 0;
    let singles: Vec<&PremisesTemplate> = kit
        .templates
        .iter()
        .copied()
        .filter(|t| t.modems <= 1)
        .collect();
    for tap in taps {
        for (port_id, h) in &tap.ports {
            if let Some(units) = structures.groups.get(h) {
                let from = PortRef {
                    element_id: tap.tap.clone(),
                    port: port_id.clone(),
                };
                if !group(
                    kit,
                    &mut out,
                    &band,
                    tap,
                    from,
                    structures,
                    homes,
                    units,
                    controls,
                    (&mut drop_n, &mut house_n, &mut direct_n),
                    service_group,
                    forced,
                ) {
                    out.port_states
                        .entry(tap.tap.clone())
                        .or_default()
                        .push(unused(port_id));
                }
                continue;
            }
            let home = &homes[*h];
            let mut rng = Rng::derive(controls.seed, &[hash_str(&home.id)]);
            if rng.uniform() >= controls.take_rate {
                out.port_states
                    .entry(tap.tap.clone())
                    .or_default()
                    .push(unused(port_id));
                continue;
            }
            let generation = if rng.uniform() < controls.d30_share {
                ModemGeneration::D30
            } else {
                ModemGeneration::D31
            };
            let Some(at) = structures.homes[*h] else {
                continue;
            };
            let drop_rule = kit
                .rules
                .drops
                .iter()
                .find(|d| d.max_len_ft.value >= at.drop_ft)
                .or(kit.rules.drops.last());
            let Some(drop) = drop_rule.and_then(|d| kit.library.get(&d.cable_ref.value)) else {
                continue;
            };
            // Draw, then take the first that fits: a drop amp only where the level without one is under the rules'.
            let order = draw_order(&singles, &mut rng);
            let modem = kit.library.get(match generation {
                ModemGeneration::D30 => &parts.modem_d30_ref.value,
                ModemGeneration::D31 => &parts.modem_d31_ref.value,
            });
            let tx_limit = modem.map_or(f64::INFINITY, |m| tx_limit(kit, m));
            let fits = |t: &PremisesTemplate, margin_db: f64| -> bool {
                template_fits(
                    kit,
                    &band,
                    &tap.port_levels,
                    Some(&tap.up),
                    drop,
                    at.drop_ft,
                    t,
                    generation == ModemGeneration::D31,
                    tx_limit,
                    margin_db,
                )
            };
            let template = match forced.filter(|t| t.modems <= 1).or_else(|| {
                order
                    .iter()
                    .find(|t| fits(t, super::feeder::RESIDUAL_DB))
                    .or_else(|| order.iter().find(|t| fits(t, 0.0)))
                    .copied()
            }) {
                Some(t) => t,
                None => {
                    out.notes.push(format!("no template fits home {}", home.id));
                    match order.first() {
                        Some(t) => *t,
                        None => continue,
                    }
                }
            };
            out.templates.insert(*h, template.id.clone());
            let modem_part = match generation {
                ModemGeneration::D30 => &parts.modem_d30_ref.value,
                ModemGeneration::D31 => &parts.modem_d31_ref.value,
            };
            let base = format!("{service_group}-h{h}");
            build_home(
                kit,
                &mut out,
                template,
                std::slice::from_ref(modem_part),
                &base,
                home.geo,
                PortRef {
                    element_id: tap.tap.clone(),
                    port: port_id.clone(),
                },
                drop,
                at.drop_ft,
                at.installation,
                (&mut drop_n, &mut house_n, &mut direct_n),
                service_group,
            );
        }
    }
    out
}

/// One apartment group on one tap port (`docs/tasks/25-design-any-map.md`): a drop from the street to a box in the
/// building, its ground block, a drop amp where the level calls for one, and a splitter with a home run to each unit that
/// takes service, as a building fed from the tap and split before its crossconnect is [CS-DROP-MANUAL §10] (design-practice.md
/// §9). Each unit takes service and its modem's generation from its own stream, as a home does. The templates tried are
/// the rules' templates for more than one modem that hold the group, the fewest legs first, in the rules' order.
/// Returns whether any unit takes service.
#[allow(clippy::too_many_arguments)]
fn group(
    kit: &Kit<'_>,
    out: &mut Premises,
    band: &Band,
    tap: &TapHomes,
    from: PortRef,
    structures: &Structures,
    homes: &[Home],
    units: &[usize],
    controls: &DesignControls,
    counters: (&mut usize, &mut usize, &mut usize),
    service_group: &str,
    forced: Option<&PremisesTemplate>,
) -> bool {
    let parts = &kit.complete.premises_parts;
    let mut served: Vec<(usize, ModemGeneration)> = Vec::new();
    for &u in units {
        let mut rng = Rng::derive(controls.seed, &[hash_str(&homes[u].id)]);
        if rng.uniform() >= controls.take_rate {
            continue;
        }
        let generation = if rng.uniform() < controls.d30_share {
            ModemGeneration::D30
        } else {
            ModemGeneration::D31
        };
        served.push((u, generation));
    }
    let (Some(&first), false) = (units.first(), served.is_empty()) else {
        return false;
    };
    let Some(at) = structures.homes[first] else {
        return false;
    };
    let Some(drop) = drop_cable(kit, at.drop_ft) else {
        return false;
    };
    let part_of = |g: ModemGeneration| match g {
        ModemGeneration::D30 => &parts.modem_d30_ref.value,
        ModemGeneration::D31 => &parts.modem_d31_ref.value,
    };
    let mut candidates: Vec<&PremisesTemplate> = kit
        .templates
        .iter()
        .copied()
        .filter(|t| t.modems > 1 && t.modems as usize >= served.len())
        .collect();
    candidates.sort_by_key(|t| t.modems);
    let fits = |t: &PremisesTemplate, margin_db: f64| {
        served.iter().all(|&(_, g)| {
            let limit = kit
                .library
                .get(part_of(g))
                .map_or(f64::INFINITY, |m| tx_limit(kit, m));
            template_fits(
                kit,
                band,
                &tap.port_levels,
                Some(&tap.up),
                drop,
                at.drop_ft,
                t,
                g == ModemGeneration::D31,
                limit,
                margin_db,
            )
        })
    };
    let forced = forced.filter(|t| t.modems > 1 && t.modems as usize >= served.len());
    let Some(template) = forced
        .or_else(|| {
            candidates
                .iter()
                .find(|t| fits(t, super::feeder::RESIDUAL_DB))
                .or_else(|| candidates.iter().find(|t| fits(t, 0.0)))
                .copied()
        })
        .or_else(|| {
            out.notes.push(format!(
                "no template fits the group at home {}",
                homes[first].id
            ));
            candidates.last().copied()
        })
    else {
        out.notes.push(format!(
            "no template holds the {} units of the group at home {}",
            served.len(),
            homes[first].id
        ));
        return false;
    };
    for &(u, _) in &served {
        out.templates.insert(u, template.id.clone());
    }
    let built = PremisesTemplate {
        modems: served.len() as u8,
        ..template.clone()
    };
    let modem_parts: Vec<ComponentId> = served.iter().map(|&(_, g)| part_of(g).clone()).collect();
    build_home(
        kit,
        out,
        &built,
        &modem_parts,
        &format!("{service_group}-h{first}"),
        homes[first].geo,
        from,
        drop,
        at.drop_ft,
        at.installation,
        counters,
        service_group,
    );
    true
}

/// One home: its drop, ground block, filter, drop amp or splitter, in-home cable and devices.
#[allow(clippy::too_many_arguments)]
fn build_home(
    kit: &Kit<'_>,
    out: &mut Premises,
    template: &PremisesTemplate,
    modem_parts: &[ComponentId],
    base: &str,
    geo: LonLat,
    from: PortRef,
    drop: &Component,
    drop_ft: f64,
    installation: Installation,
    counters: (&mut usize, &mut usize, &mut usize),
    service_group: &str,
) {
    let (drop_n, house_n, direct_n) = counters;
    let house_cable = kit.complete.premises_parts.house_cable_ref.value.clone();
    let element = |id: &str,
                   kind: ElementKind,
                   part: &ComponentId,
                   at: LonLat,
                   ports: Vec<Port>,
                   fitted: Option<Fitted>| Element {
        id: ElementId::new(id),
        kind,
        component_ref: part.clone(),
        geo: Some(at),
        ports,
        record_status: RecordStatus::Designed,
        fitted,
        structure: None,
    };
    let mut edge = |out: &mut Premises, from: PortRef, to: PortRef, kind: EdgeKind| {
        let id = match &kind {
            EdgeKind::Drop { .. } => {
                *drop_n += 1;
                format!("{service_group}-drop-{drop_n}")
            }
            EdgeKind::House { .. } => {
                *house_n += 1;
                format!("{service_group}-house-{house_n}")
            }
            _ => {
                *direct_n += 1;
                format!("{service_group}-hdirect-{direct_n}")
            }
        };
        out.edges.push(Edge {
            id: EdgeId::new(id),
            from,
            to,
            kind,
            record_status: None,
        });
    };
    let house = |len_ft: f64| EdgeKind::House {
        cable_ref: house_cable.clone(),
        len_ft,
        fittings: drop_fittings(kit, len_ft),
    };
    let drop_kind = EdgeKind::Drop {
        cable_ref: drop.id.clone(),
        len_ft: drop_ft,
        fittings: drop_fittings(kit, drop_ft),
        path: Vec::new(),
        installation: Some(installation),
    };
    // The devices: modems first, then the rest, each a few feet apart inside the home.
    let mut devices: Vec<(String, ElementKind, ComponentId)> = Vec::new();
    for m in 0..template.modems.max(1) as usize {
        let Some(part) = modem_parts.get(m).or(modem_parts.last()) else {
            continue;
        };
        devices.push((
            format!("{base}-modem-{}", m + 1),
            ElementKind::Modem,
            part.clone(),
        ));
    }
    for (i, device) in template.cpe.iter().enumerate() {
        if let Some(part) = kit.cpe(*device) {
            let tag = match device {
                CpeDevice::SetTop => "stb",
                CpeDevice::Television => "tv",
                CpeDevice::MocaAdapter => "moca",
                CpeDevice::Other => "cpe",
            };
            devices.push((
                format!("{base}-{tag}-{}", i + 1),
                ElementKind::Cpe,
                part.id.clone(),
            ));
        }
    }
    let spot = |i: usize| offset(geo, 6.0 * (i as f64 - 1.0), 8.0);
    let Some(gb) = &template.ground_block_ref else {
        // A modem straight on the drop.
        let (id, kind, part) = &devices[0];
        out.elements.push(element(
            id,
            kind.clone(),
            part,
            geo,
            vec![port("rf", PortRole::Input)],
            None,
        ));
        edge(out, from, port_ref(&ElementId::new(id), "rf"), drop_kind);
        return;
    };
    let gb_id = format!("{base}-gb");
    out.elements.push(element(
        &gb_id,
        ElementKind::GroundBlock,
        gb,
        geo,
        vec![port("in", PortRole::Input), port("out", PortRole::Through)],
        None,
    ));
    edge(
        out,
        from,
        port_ref(&ElementId::new(&gb_id), "in"),
        drop_kind,
    );
    let mut feed = port_ref(&ElementId::new(&gb_id), "out");
    if let Some(filter) = &template.filter_ref {
        let id = format!("{base}-filter");
        out.elements.push(element(
            &id,
            ElementKind::Filter,
            filter,
            geo,
            vec![port("in", PortRole::Input), port("out", PortRole::Through)],
            None,
        ));
        edge(
            out,
            feed,
            port_ref(&ElementId::new(&id), "in"),
            EdgeKind::Direct,
        );
        feed = port_ref(&ElementId::new(&id), "out");
    }
    if let Some(amp) = &template.drop_amp_ref {
        let id = format!("{base}-damp");
        out.elements.push(element(
            &id,
            ElementKind::DropAmp,
            amp,
            geo,
            vec![port("in", PortRole::Input), port("out-1", PortRole::Output)],
            None,
        ));
        edge(
            out,
            feed,
            port_ref(&ElementId::new(&id), "in"),
            EdgeKind::Direct,
        );
        let outputs = match kit.library.get(amp).map(|c| &c.spec) {
            Some(ComponentSpec::DropAmp(a)) => a.outputs as usize,
            _ => 1,
        };
        if outputs > 1 && template.splitter_ref.is_none() {
            // A distribution amp: a device on each output it uses, by its own home run (task 25).
            let used = devices.len().min(outputs);
            if let Some(e) = out.elements.iter_mut().find(|e| e.id.as_str() == id) {
                e.ports = std::iter::once(port("in", PortRole::Input))
                    .chain((1..=used).map(|k| port(&format!("out-{k}"), PortRole::Output)))
                    .collect();
            }
            for (i, (device_id, kind, part)) in devices.iter().enumerate().take(used) {
                out.elements.push(element(
                    device_id,
                    kind.clone(),
                    part,
                    spot(i),
                    vec![port("rf", PortRole::Input)],
                    None,
                ));
                edge(
                    out,
                    port_ref(&ElementId::new(&id), &format!("out-{}", i + 1)),
                    port_ref(&ElementId::new(device_id), "rf"),
                    house(template.house_cable_ft),
                );
            }
            return;
        }
        feed = port_ref(&ElementId::new(&id), "out-1");
    }
    match &template.splitter_ref {
        Some(splitter) => {
            let legs = match kit.library.get(splitter).map(|c| &c.spec) {
                Some(ComponentSpec::Splitter(s)) => s.legs as usize,
                _ => 2,
            };
            let id = format!("{base}-hsp");
            let mut ports = vec![port("in", PortRole::Input)];
            let mut states = Vec::new();
            for leg in 1..=legs {
                let name = format!("leg-{leg}");
                ports.push(port(&name, PortRole::Leg));
                if leg > devices.len() {
                    states.push(PortStateEntry {
                        port: PortId::new(&name),
                        state: PortState::Terminated {
                            component_ref: kit.terminator.id.clone(),
                        },
                    });
                }
            }
            let fitted = (!states.is_empty()).then_some(Fitted::Passive(PassiveFitted {
                conditioner: None,
                ports: states,
            }));
            out.elements.push(element(
                &id,
                ElementKind::Splitter,
                splitter,
                geo,
                ports,
                fitted,
            ));
            edge(
                out,
                feed,
                port_ref(&ElementId::new(&id), "in"),
                EdgeKind::Direct,
            );
            for (i, (device_id, kind, part)) in devices.iter().enumerate().take(legs) {
                out.elements.push(element(
                    device_id,
                    kind.clone(),
                    part,
                    spot(i),
                    vec![port("rf", PortRole::Input)],
                    None,
                ));
                edge(
                    out,
                    port_ref(&ElementId::new(&id), &format!("leg-{}", i + 1)),
                    port_ref(&ElementId::new(device_id), "rf"),
                    house(template.house_cable_ft),
                );
            }
        }
        None => {
            let (device_id, kind, part) = &devices[0];
            out.elements.push(element(
                device_id,
                kind.clone(),
                part,
                spot(0),
                vec![port("rf", PortRole::Input)],
                None,
            ));
            let kind_edge = if template.house_cable_ft > 0.0 {
                house(template.house_cable_ft)
            } else {
                EdgeKind::Direct
            };
            edge(
                out,
                feed,
                port_ref(&ElementId::new(device_id), "rf"),
                kind_edge,
            );
        }
    }
}

/// The level at a tap's ports as the sim reads it, at the check points and all three temperatures: the output of the
/// active feeding it less `sim::transfer` to the tap port.
pub fn port_levels_now(
    kit: &Kit<'_>,
    graph: &plant_graph::PlantGraph<'_>,
    tap: &ElementId,
    port: &PortId,
) -> Option<Levels> {
    use schema::fault::Location;
    use sim::walk::{Band as SimBand, Grid, Walker};
    let band = Band::new(kit.profile);
    let mut at = tap.clone();
    let mut feed = None;
    while let Ok(Some((parent, edge))) = graph.parent(&at) {
        if matches!(
            parent.kind,
            ElementKind::Node { .. } | ElementKind::Rpd { .. } | ElementKind::Amp { .. }
        ) {
            feed = Some((parent.id.clone(), edge.from.port.clone()));
            break;
        }
        at = parent.id.clone();
    }
    let (active, active_port) = feed?;
    let overlay = sim::Overlay::default();
    let mut out = Levels([[0.0; super::levels::NF]; 3]);
    for (t, temp) in super::levels::TEMPS.iter().enumerate() {
        let s = super::levels::sample(*temp);
        let walker = Walker::new(graph, &s, &overlay).ok()?;
        let capture = walker
            .capture(&Grid::new(band.f.to_vec()), SimBand::Down)
            .ok()?;
        let levels = capture
            .outputs
            .get(&(active.clone(), active_port.clone()))?;
        let a = Location::Port {
            element_id: active.clone(),
            port: active_port.clone(),
        };
        let b = Location::Port {
            element_id: tap.clone(),
            port: port.clone(),
        };
        for (i, f) in band.f.iter().enumerate() {
            out.0[t][i] = levels[i] - sim::transfer::transfer(graph, &s, &a, &b, *f).ok()?.loss_db;
        }
    }
    Some(out)
}

/// The issues `check_physics` finds that name a modem's level, spread or transmit power, by modem.
fn modem_issue(issue: &sim::budget::PhysicsIssue) -> Option<&ElementId> {
    use sim::budget::PhysicsIssue as I;
    match issue {
        I::LevelTooHigh { modem, .. }
        | I::LevelTooLow { modem, .. }
        | I::PsdSpread { modem, .. }
        | I::TotalInputTooHigh { modem, .. }
        | I::TxLimit { modem, .. }
        | I::TxBelowFloor { modem, .. } => Some(modem),
        _ => None,
    }
}

/// The sim's last word on the homes (task 25). The drops stage judges a template on five check points and its own sum
/// of losses; the sim reads the whole band, with every reflection. So each home the sim still flags has every template
/// the profile allows for it (one modem for a home, more for an apartment group) built in turn and the node read again,
/// at both ends of the temperature range, and keeps the one that leaves the node the fewest issues, if that's fewer
/// than now. Returns how many homes changed.
pub fn settle(
    kit: &Kit<'_>,
    plant: &mut schema::plant::Plant,
    structures: &Structures,
    homes: &[Home],
    taps: &[TapHomes],
    controls: &DesignControls,
    templates: &mut BTreeMap<usize, String>,
) -> usize {
    let samples = [
        super::levels::sample(super::levels::TEMPS[1]),
        super::levels::sample(super::levels::TEMPS[2]),
    ];
    let issues_of = |plant: &schema::plant::Plant| -> Option<Vec<sim::budget::PhysicsIssue>> {
        let graph = plant_graph::PlantGraph::new(plant, kit.library, kit.profile).ok()?;
        sim::budget::check_physics(&graph, &samples, &sim::Overlay::default()).ok()
    };
    let Some(mut now) = issues_of(plant) else {
        return 0;
    };
    let sg = plant.service_group.to_string();
    // The homes flagged, by tap, port and home, in order.
    let mut flagged: std::collections::BTreeSet<(ElementId, PortId, usize)> = Default::default();
    {
        let Ok(graph) = plant_graph::PlantGraph::new(plant, kit.library, kit.profile) else {
            return 0;
        };
        for modem in now.iter().filter_map(modem_issue) {
            let Ok(Some(home)) = graph.premises_of(modem) else {
                continue;
            };
            let port = home.drop.from.port.clone();
            if let Some(&(_, h)) = taps
                .iter()
                .find(|t| t.tap == home.tap.id)
                .and_then(|t| t.ports.iter().find(|(p, _)| *p == port))
            {
                flagged.insert((home.tap.id.clone(), port, h));
            }
        }
    }
    let mut changed = 0;
    // First the homes: every template each flagged home may take, the node read again each time.
    for (tap_id, port, h) in flagged.clone() {
        let prefix = format!("{sg}-h{h}-");
        let mine = |issues: &[sim::budget::PhysicsIssue]| {
            issues
                .iter()
                .filter_map(modem_issue)
                .filter(|m| m.as_str().starts_with(&prefix))
                .count()
        };
        if mine(&now) == 0 {
            continue;
        }
        let Some(tap) = taps.iter().find(|t| t.tap == tap_id) else {
            continue;
        };
        let levels = {
            let Ok(graph) = plant_graph::PlantGraph::new(plant, kit.library, kit.profile) else {
                continue;
            };
            port_levels_now(kit, &graph, &tap_id, &port)
        };
        let Some(levels) = levels else {
            continue;
        };
        let fixed = TapHomes {
            port_levels: levels,
            ports: vec![(port.clone(), h)],
            ..tap.clone()
        };
        let group = structures.groups.contains_key(&h);
        let mut best: Option<(
            usize,
            schema::plant::Plant,
            Vec<sim::budget::PhysicsIssue>,
            Premises,
        )> = None;
        for t in kit.templates.iter().filter(|t| (t.modems > 1) == group) {
            let mut built = drops_with(
                kit,
                structures,
                homes,
                std::slice::from_ref(&fixed),
                controls,
                &sg,
                Some(t),
            );
            if built.elements.is_empty() {
                continue;
            }
            built.port_states.clear();
            let mut trial = plant.clone();
            trial
                .elements
                .retain(|e| !e.id.as_str().starts_with(&prefix));
            trial.edges.retain(|e| {
                !e.to.element_id.as_str().starts_with(&prefix)
                    && !e.from.element_id.as_str().starts_with(&prefix)
            });
            let mut edges = built.edges.clone();
            for (i, e) in edges.iter_mut().enumerate() {
                e.id = EdgeId::new(format!("{}-settle-{}", prefix.trim_end_matches('-'), i + 1));
            }
            trial.elements.extend(built.elements.clone());
            trial.edges.extend(edges);
            let Some(issues) = issues_of(&trial) else {
                continue;
            };
            let total = issues.len();
            if total < now.len() && best.as_ref().is_none_or(|b| total < b.0) {
                best = Some((total, trial, issues, built));
            }
        }
        if let Some((_, trial, issues, built)) = best {
            *plant = trial;
            now = issues;
            for (u, t) in built.templates {
                templates.insert(u, t);
            }
            changed += 1;
        }
    } // Then the taps of the homes still flagged: every conditioner its family takes, or none, the node read again. One that
    // removes issues is kept even if it takes a port off its target, since an issue is a modem out of its limits and a
    // port off target only a design practice; among those that remove as many, the one with fewer ports off is kept.
    let tap_ids: std::collections::BTreeSet<ElementId> =
        flagged.iter().map(|(t, _, _)| t.clone()).collect();
    for tap_id in &tap_ids {
        let Some(element) = plant.elements.iter().find(|e| &e.id == tap_id) else {
            continue;
        };
        let takes = kit
            .library
            .get(&element.component_ref)
            .is_some_and(|c| matches!(&c.spec, ComponentSpec::Tap(t) if t.plugin_family.is_some()));
        if !takes {
            continue;
        }
        let current = match &element.fitted {
            Some(Fitted::Passive(p)) => p.conditioner.clone(),
            _ => None,
        };
        // Its ports' level against the rules' target at both ends of the temperature range: a conditioner that takes a port
        // outside the tolerance isn't kept, whatever it does for the modems ("Tap-port level", task 23).
        let off_target = |plant: &schema::plant::Plant| -> usize {
            [super::levels::TEMPS[1], super::levels::TEMPS[2]]
                .iter()
                .filter(|t| {
                    super::bars::tap_port_levels(kit, plant, **t)
                        .get(tap_id)
                        .is_some_and(|[lo, hi]| {
                            let e_lo = kit.port_lo_dbmv.map_or(0.0, |target| lo - target);
                            let e_hi = hi - kit.port_hi_dbmv;
                            e_lo.abs() > kit.port_tolerance_db + 1e-9
                                || e_hi.abs() > kit.port_tolerance_db + 1e-9
                        })
                })
                .count()
        };
        let off_now = off_target(plant);
        let mut best: Option<(
            (usize, usize),
            schema::plant::Plant,
            Vec<sim::budget::PhysicsIssue>,
        )> = None;
        for option in
            std::iter::once(None).chain(kit.conditioners.iter().map(|c| Some(c.part.id.clone())))
        {
            if option == current {
                continue;
            }
            let mut trial = plant.clone();
            if let Some(e) = trial.elements.iter_mut().find(|e| &e.id == tap_id) {
                let mut fitted = match &e.fitted {
                    Some(Fitted::Passive(p)) => p.clone(),
                    _ => PassiveFitted::default(),
                };
                fitted.conditioner = option.clone();
                e.fitted = (fitted.conditioner.is_some() || !fitted.ports.is_empty())
                    .then_some(Fitted::Passive(fitted));
            }
            let Some(issues) = issues_of(&trial) else {
                continue;
            };
            let total = (issues.len(), off_target(&trial).saturating_sub(off_now));
            if total.0 < now.len() && best.as_ref().is_none_or(|b| total < b.0) {
                best = Some((total, trial, issues));
            }
        }
        if let Some((_, trial, issues)) = best {
            *plant = trial;
            now = issues;
            changed += 1;
        }
    }

    changed
}

/// §7.1: each of `modems`' return loss up to its first active as the sim reads it now (the loss below the first active
/// plus the excess above it), at the return band's edges and each of the design temperatures.
fn loss_up_now(
    kit: &Kit<'_>,
    graph: &plant_graph::PlantGraph<'_>,
    modems: &std::collections::BTreeSet<ElementId>,
) -> BTreeMap<ElementId, Up> {
    use sim::walk::{Band as SimBand, Grid, ModemView, Walker};
    let band = Band::new(kit.profile);
    let overlay = sim::Overlay::default();
    let mut out: BTreeMap<ElementId, Up> = BTreeMap::new();
    if modems.is_empty() {
        return out;
    }
    for (t, temp) in super::levels::TEMPS.iter().enumerate() {
        let s = super::levels::sample(*temp);
        let Ok(walker) = Walker::new(graph, &s, &overlay) else {
            return BTreeMap::new();
        };
        let grid = Grid::new(band.us.to_vec());
        let mut seen = |view: &ModemView<'_>| -> Result<(), sim::PhysicsError> {
            if modems.contains(&view.modem.id) {
                let entry = out
                    .entry(view.modem.id.clone())
                    .or_insert(Up([[0.0; 2]; 3]));
                entry.0[t] = [view.loss_up_db[0], view.loss_up_db[1]];
            }
            Ok(())
        };
        if walker.run(&grid, SimBand::Up, &mut seen).is_err() {
            return BTreeMap::new();
        }
    }
    out
}

/// After the last alignment: each home whose modem the sim flags for its level, spread or transmit power gets its
/// template drawn again, in its own order, against its tap port as the sim reads it; the first that fits is built in
/// place of the old. Returns how many homes changed.
pub fn refit(
    kit: &Kit<'_>,
    plant: &mut schema::plant::Plant,
    structures: &Structures,
    homes: &[Home],
    taps: &[TapHomes],
    controls: &DesignControls,
    templates: &mut BTreeMap<usize, String>,
) -> usize {
    use sim::budget::PhysicsIssue as I;
    let Ok(graph) = plant_graph::PlantGraph::new(plant, kit.library, kit.profile) else {
        return 0;
    };
    let samples = [
        super::levels::sample(super::levels::TEMPS[1]),
        super::levels::sample(super::levels::TEMPS[2]),
    ];
    let Ok(issues) = sim::budget::check_physics(&graph, &samples, &sim::Overlay::default()) else {
        return 0;
    };
    let mut flagged: std::collections::BTreeSet<ElementId> = Default::default();
    for issue in &issues {
        let modem = match issue {
            I::LevelTooHigh { modem, .. }
            | I::LevelTooLow { modem, .. }
            | I::PsdSpread { modem, .. }
            | I::TotalInputTooHigh { modem, .. }
            | I::TxLimit { modem, .. }
            | I::TxBelowFloor { modem, .. } => modem,
            _ => continue,
        };
        flagged.insert(modem.clone());
    }
    let sg = plant.service_group.to_string();
    let up_now = loss_up_now(kit, &graph, &flagged);
    let band = Band::new(kit.profile);
    let mut rebuilt: Vec<(String, usize, Premises)> = Vec::new();
    for modem in &flagged {
        let Ok(Some(home)) = graph.premises_of(modem) else {
            continue;
        };
        let tap_id = home.tap.id.clone();
        let port = home.drop.from.port.clone();
        let Some(tap) = taps.iter().find(|t| t.tap == tap_id) else {
            continue;
        };
        let Some(&(_, h)) = tap.ports.iter().find(|(p, _)| *p == port) else {
            continue;
        };
        let Some(now) = port_levels_now(kit, &graph, &tap_id, &port) else {
            continue;
        };
        // §7.1: the return path as aligned. The sim's loss from this modem to its first active, less what the drop and
        // its premises put below the tap port, is the port's return loss now; alignment's return pads and equalizers
        // moved it from the walk's estimate.
        // A group's template is any of its served units'; its first unit may not take service.
        let template_id = templates.get(&h).or_else(|| {
            structures
                .groups
                .get(&h)
                .and_then(|units| units.iter().find_map(|u| templates.get(u)))
        });
        let up = template_id
            .and_then(|id| kit.templates.iter().find(|t| &t.id == id))
            .zip(structures.homes[h])
            .zip(up_now.get(modem))
            .and_then(|((t, at), sim_up)| {
                let ComponentSpec::Drop(dspec) = &drop_cable(kit, at.drop_ft)?.spec else {
                    return None;
                };
                let mut port_up = *sim_up;
                for (ti, temp) in super::levels::TEMPS.iter().enumerate() {
                    for (i, f) in band.us.iter().enumerate() {
                        port_up.0[ti][i] -= cable(dspec, at.drop_ft)(*f, *temp)
                            + 2.0 * through(kit.drop_f, *f)
                            + premises_loss_dir(kit, t, *f, *temp, false);
                    }
                }
                Some(port_up)
            })
            .unwrap_or(tap.up);
        let fixed = TapHomes {
            port_levels: now,
            up,
            ports: vec![(port.clone(), h)],
            ..tap.clone()
        };
        let mut built = drops(kit, structures, homes, &[fixed], controls, &sg);
        if built.notes.is_empty() && !built.elements.is_empty() {
            built.port_states.clear();
            rebuilt.push((format!("{sg}-h{h}-"), h, built));
        }
    }
    drop(graph);
    let n = rebuilt.len();
    for (prefix, h, built) in rebuilt {
        if let Some(t) = built.templates.get(&h) {
            templates.insert(h, t.clone());
        }
        plant
            .elements
            .retain(|e| !e.id.as_str().starts_with(&prefix));
        plant.edges.retain(|e| {
            !e.to.element_id.as_str().starts_with(&prefix)
                && !e.from.element_id.as_str().starts_with(&prefix)
        });
        // Edge ids: number the new ones after the plant's own.
        let mut edges = built.edges;
        for (i, e) in edges.iter_mut().enumerate() {
            e.id = EdgeId::new(format!("{}-refit-{}", prefix.trim_end_matches('-'), i + 1));
        }
        plant.elements.extend(built.elements);
        plant.edges.extend(edges);
    }
    n
}
