//! Stage `feeder`: taps at structures, sized by the homes there, valued by working out from each active so every port
//! holds the rules' level within tolerance at the top and bottom of the forward band and at both ends of the
//! temperature range. A conditioner where the port's tilt is short of the rules', a line equalizer where a conditioner
//! can't reach. A line ends on a terminating tap, or a through tap whose through port is terminated (§2.1, §15).

use super::kit::{Kit, LineEq, PlugIn, TapFamily, TapPart};
use super::levels::{Band, HI, LO, Levels, tap_port, through};

/// A tap chosen for a structure.
#[derive(Debug, Clone)]
pub struct TapChoice<'a> {
    /// The tap.
    pub tap: TapPart<'a>,
    /// Its conditioner, if it needs one.
    pub conditioner: Option<PlugIn<'a>>,
    /// The level at its ports.
    pub port: Levels,
    /// What leaves its through port, for a through tap.
    pub out: Option<Levels>,
}

/// Why no tap fits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Misfit {
    /// Even the lowest value leaves the port under the target's tolerance: the signal ran out.
    TooLow,
    /// Even the highest leaves it over.
    TooHot,
    /// The level fits but the tilt is short by more than any conditioner adds.
    TooFlat,
    /// The tilt is over the target's at the bottom of the band: the station feeding it is tilted too far.
    TooTilted,
}

/// What alignment leaves at the top of the band, which the walk can't see: half a pad step, pads coming in 1 dB steps
/// (physics §15). The walk keeps a port this far inside the tolerance; the retune, reading the sim, and the bar hold the
/// tolerance itself.
pub const RESIDUAL_DB: f64 = 0.5;

/// How far a port is from the target, the worst over temperatures and the two band edges, and whether it's inside the
/// tolerance; `margin_db` inside it at the bottom of the window at the two temperature extremes, where a run's far
/// taps sit when the station feeding it can rise no further.
fn score(kit: &Kit<'_>, port: &Levels, margin_db: f64) -> (bool, f64) {
    let tol = kit.port_tolerance_db;
    let mut worst: f64 = 0.0;
    let mut sum = 0.0;
    let mut ok = true;
    for t in 0..3 {
        let e_hi = port.hi(t) - kit.port_hi_dbmv;
        // Held at the top alone where the rules state no level at the bottom.
        let e_lo = kit.port_lo_dbmv.map_or(0.0, |lo| port.lo(t) - lo);
        worst = worst.max(e_hi.abs()).max(e_lo.abs());
        sum += e_hi * e_hi + e_lo * e_lo;
        let floor = if t == super::levels::REF {
            tol
        } else {
            tol - margin_db
        };
        if e_hi > tol || e_hi < -floor || e_lo.abs() > tol {
            ok = false;
        }
    }
    // The tilt limit, read where alignment is done, where the rules set one.
    if let (Some(target_tilt), Some(limit)) = (kit.target_tilt(), kit.tilt_tolerance_db)
        && (port.tilt(super::levels::REF) - target_tilt).abs() > limit + 1e-9
    {
        ok = false;
    }
    // And no spread a modem on the shortest drop couldn't take (§3.3).
    let spread_max = kit.profile.design_levels.psd_spread_max_db;
    for row in &port.0 {
        let hi = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let lo = row.iter().copied().fold(f64::INFINITY, f64::min);
        if hi - lo > spread_max {
            ok = false;
        }
    }
    (ok, sum + worst)
}

/// The port levels a tap and conditioner give from an input.
pub fn port_levels(
    band: &Band,
    input: &Levels,
    tap: &TapPart<'_>,
    conditioner: Option<&PlugIn<'_>>,
) -> Levels {
    input.less(band, |f, _| {
        tap_port(tap.part, f) + conditioner.map_or(0.0, |c| through(c.part, f))
    })
}

/// The family that serves `homes` homes on one tap, or the largest when none does.
pub fn family_for<'k, 'a>(kit: &'k Kit<'a>, homes: usize) -> &'k TapFamily<'a> {
    kit.taps
        .iter()
        .find(|f| f.ports as usize >= homes)
        .unwrap_or_else(|| &kit.taps[kit.taps.len() - 1])
}

/// How many homes each tap at a structure serves: as few taps as can, each the smallest family that fits what's left.
pub fn split_homes(kit: &Kit<'_>, homes: usize) -> Vec<usize> {
    let largest = kit.taps.last().map_or(1, |f| f.ports as usize).max(1);
    let mut out = Vec::new();
    let mut left = homes;
    while left > 0 {
        let take = left.min(largest);
        out.push(take);
        left -= take;
    }
    out
}

/// The tap, of `family`, that holds its ports nearest the target from `input`. `terminating` asks for a terminating tap
/// when one fits. Without a conditioner first; with the smallest conditioner that brings the tilt in, if none fits
/// without.
pub fn fit_tap<'a>(
    kit: &Kit<'a>,
    band: &Band,
    input: &Levels,
    family: &TapFamily<'a>,
    terminating: bool,
    drops_ft: &[f64],
    group_drops_ft: &[f64],
) -> Result<TapChoice<'a>, Misfit> {
    fit_tap_within(
        kit,
        band,
        input,
        family,
        terminating,
        Behind {
            drops_ft,
            group_drops_ft,
            ..Behind::default()
        },
    )
}

/// What a tap's fit is judged on beyond its own port: the modems behind it and, in the walk, their return path and
/// alignment's residual.
#[derive(Clone, Copy, Default)]
pub struct Behind<'x> {
    /// The most return loss its ports may add for its modems to transmit inside their limit (§7.2).
    pub room: Option<&'x super::levels::Up>,
    /// The return loss from its input back to its active, for the modems' transmit floor.
    pub up_in: Option<&'x super::levels::Up>,
    /// The drops it feeds, shortest and longest.
    pub drops_ft: &'x [f64],
    /// The drops to apartment groups it feeds, shortest and longest (task 25).
    pub group_drops_ft: &'x [f64],
    /// How far inside the tolerance its ports are kept: [`RESIDUAL_DB`] in the walk.
    pub margin_db: f64,
}

/// [`fit_tap`], with the most return loss its ports may add, at the return band's two edges and each temperature, for
/// the modems behind it to transmit inside their limit (§7.2): a tap value and conditioner that lose more on the return
/// aren't chosen. A tap that can't be fitted for want of it is `TooLow`: an amp before it resets the return.
pub fn fit_tap_within<'a>(
    kit: &Kit<'a>,
    band: &Band,
    input: &Levels,
    family: &TapFamily<'a>,
    terminating: bool,
    behind: Behind<'_>,
) -> Result<TapChoice<'a>, Misfit> {
    let Behind {
        room,
        up_in,
        drops_ft,
        group_drops_ft,
        margin_db,
    } = behind;
    let mut short_return = false;
    let lists: Vec<&Vec<TapPart<'a>>> = if terminating {
        vec![&family.terminating, &family.through]
    } else {
        vec![&family.through]
    };
    let mut misfit = Misfit::TooLow;
    let options: Vec<Option<&PlugIn<'a>>> = std::iter::once(None)
        .chain(kit.conditioners.iter().map(Some))
        .collect();
    for list in lists {
        for conditioner in &options {
            if conditioner.is_some()
                && family
                    .through
                    .first()
                    .and_then(|t| t.spec.plugin_family.as_ref())
                    .is_none()
            {
                break;
            }
            let mut best: Option<(f64, &TapPart<'a>, Levels)> = None;
            for tap in list.iter() {
                if conditioner.is_some() && tap.spec.plugin_family.is_none() {
                    continue;
                }
                let port = port_levels(band, input, tap, *conditioner);
                let (ok, s) = score(kit, &port, margin_db);
                // And the modems behind it, on their drops: inside their window and spread, and, with the return
                // path known, transmitting inside their limit and above their floor.
                let port_up = up_in.map(|u| {
                    u.plus(band, |f, _| {
                        tap_port(tap.part, f) + conditioner.map_or(0.0, |c| through(c.part, f))
                    })
                });
                let ok = ok
                    && super::drops::serves(
                        kit,
                        band,
                        &port,
                        port_up.as_ref(),
                        drops_ft,
                        group_drops_ft,
                    );
                if ok && let Some(room) = room {
                    let loss = super::levels::Up::default().plus(band, |f, _| {
                        tap_port(tap.part, f) + conditioner.map_or(0.0, |c| through(c.part, f))
                    });
                    let over = (0..3).any(|t| (0..2).any(|i| loss.0[t][i] > room.0[t][i] + 1e-9));
                    if over {
                        short_return = true;
                        continue;
                    }
                }
                if ok && best.as_ref().is_none_or(|b| s < b.0) {
                    best = Some((s, tap, port));
                }
            }
            if let Some((_, tap, port)) = best {
                let out =
                    (!tap.spec.terminating).then(|| input.less(band, |f, _| through(tap.part, f)));
                return Ok(TapChoice {
                    tap: tap.clone(),
                    conditioner: conditioner.cloned(),
                    port,
                    out,
                });
            }
        }
        // Why the plain list doesn't fit: the top of the band past either end of its values, or else the tilt, read on
        // the value that puts the top nearest the target.
        if let (Some(high), Some(low)) = (list.first(), list.last()) {
            let hot = port_levels(band, input, high, None);
            let cold = port_levels(band, input, low, None);
            let tol = kit.port_tolerance_db;
            // With no level at the bottom, what's left to miss is the spread a modem can take: tilted up or down.
            let target_tilt = kit.target_tilt().unwrap_or(0.0);
            let nearest = list
                .iter()
                .min_by(|a, b| {
                    let e = |t: &TapPart<'_>| {
                        (port_levels(band, input, t, None).hi(super::levels::REF)
                            - kit.port_hi_dbmv)
                            .abs()
                    };
                    e(a).total_cmp(&e(b))
                })
                .map(|t| port_levels(band, input, t, None));
            misfit = if (0..3).any(|t| hot.hi(t) > kit.port_hi_dbmv + tol) {
                Misfit::TooHot
            } else if (0..3).any(|t| cold.hi(t) < kit.port_hi_dbmv - tol) {
                Misfit::TooLow
            } else if nearest.is_some_and(|p| p.tilt(super::levels::REF) > target_tilt) {
                Misfit::TooTilted
            } else {
                Misfit::TooFlat
            };
        }
    }
    if short_return && misfit != Misfit::TooHot {
        return Err(Misfit::TooLow);
    }
    Err(misfit)
}

/// The lowest level at the top of the band a structure with `homes` homes needs to fit its taps, chained one after
/// another with each family's lowest value, a terminating tap last where the line `ends`: the most any one port needs,
/// counting the insertion loss of the taps before it.
pub fn tap_floor(kit: &Kit<'_>, band: &Band, homes: usize, ends: bool) -> f64 {
    let sizes = split_homes(kit, homes);
    let floor = kit.port_hi_dbmv - kit.port_tolerance_db;
    let mut before = 0.0;
    let mut need = f64::NEG_INFINITY;
    for (i, n) in sizes.iter().enumerate() {
        let family = family_for(kit, *n);
        let last = i + 1 == sizes.len();
        let tap = if last && ends {
            family.terminating.last().or(family.through.last())
        } else {
            family.through.last()
        };
        let Some(tap) = tap else {
            continue;
        };
        need = need.max(floor + tap_port(tap.part, band.f[HI]) + before);
        before += through(tap.part, band.f[HI]);
    }
    need
}

/// The line equalizer that brings a too-flat signal's tilt back nearest the target's at its first tap, or the largest.
pub fn line_eq_for<'k, 'a>(
    kit: &'k Kit<'a>,
    band: &Band,
    input: &Levels,
) -> Option<&'k LineEq<'a>> {
    let target_tilt = kit.target_tilt().unwrap_or(0.0);
    let now = input.tilt(super::levels::REF);
    let want = target_tilt - now;
    let slack = kit.tilt_tolerance_db.unwrap_or(kit.port_tolerance_db);
    kit.line_eqs
        .iter()
        .find(|e| {
            let added = through(e.part, band.f[LO]) - through(e.part, band.f[HI]);
            added >= want - slack
        })
        .or(kit.line_eqs.last())
}

/// After alignment: each tap's input as the sim reads it, at the check points and all three temperatures, and the value
/// and conditioner re-fitted where the walk's estimate left a port outside the target (§15's residual, which plug-in
/// steps leave, moves an amp's output off its set point). Same family and kind, so the plant around it doesn't change.
/// Returns how many taps it changed.
pub fn retune(kit: &Kit<'_>, plant: &mut schema::plant::Plant) -> usize {
    use schema::fault::Location;
    use schema::plant::{ElementKind, Fitted, PassiveFitted};
    use sim::walk::{Band as SimBand, Grid, Walker};

    let band = Band::new(kit.profile);
    let Ok(graph) = plant_graph::PlantGraph::new(plant, kit.library, kit.profile) else {
        return 0;
    };
    let overlay = sim::Overlay::default();
    let samples: Vec<_> = super::levels::TEMPS
        .iter()
        .map(|t| super::levels::sample(*t))
        .collect();
    let mut captures = Vec::new();
    for s in &samples {
        let Ok(walker) = Walker::new(&graph, s, &overlay) else {
            return 0;
        };
        let Ok(c) = walker.capture(&Grid::new(band.f.to_vec()), SimBand::Down) else {
            return 0;
        };
        captures.push(c);
    }
    let mut changes: Vec<(
        schema::common::ElementId,
        schema::common::ComponentId,
        Option<schema::common::ComponentId>,
    )> = Vec::new();
    // Per active, which way the taps no value can fit want its output to move: −1 down, +1 up.
    let mut nudges: std::collections::BTreeMap<
        schema::common::ElementId,
        std::collections::BTreeSet<i8>,
    > = Default::default();
    let mut tilts: std::collections::BTreeMap<
        schema::common::ElementId,
        std::collections::BTreeSet<i8>,
    > = Default::default();
    for element in plant
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Tap) && e.structure.is_some())
    {
        // The active feeding it, and the port.
        let mut at = element.id.clone();
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
        let Some((active, port)) = feed else {
            continue;
        };
        let a = Location::Port {
            element_id: active.clone(),
            port: port.clone(),
        };
        let b = Location::Port {
            element_id: element.id.clone(),
            port: schema::common::PortId::new("in"),
        };
        let mut input = Levels([[0.0; super::levels::NF]; 3]);
        let mut ok = true;
        for (t, s) in samples.iter().enumerate() {
            let Some(out) = captures[t].outputs.get(&(active.clone(), port.clone())) else {
                ok = false;
                break;
            };
            for (i, f) in band.f.iter().enumerate() {
                match sim::transfer::transfer(&graph, s, &a, &b, *f) {
                    Ok(tr) => input.0[t][i] = out[i] - tr.loss_db,
                    Err(_) => ok = false,
                }
            }
        }
        if !ok {
            continue;
        }
        let Some(part) = kit.library.get(&element.component_ref) else {
            continue;
        };
        let schema::component::ComponentSpec::Tap(spec) = &part.spec else {
            continue;
        };
        let Some(family) = kit.taps.iter().find(|f| f.ports == spec.ports) else {
            continue;
        };
        let conditioner = match &element.fitted {
            Some(Fitted::Passive(p)) => p.conditioner.clone(),
            _ => None,
        };
        let current = super::kit::TapPart { part, spec };
        let current_cond = conditioner
            .as_ref()
            .and_then(|c| kit.conditioners.iter().find(|x| &x.part.id == c));
        // The drops it feeds, shortest and longest, for the modems behind them: a drop to an apartment group's box, which
        // has more than one modem behind it, apart from a drop to a home.
        let modems_behind = |to: &schema::common::ElementId| {
            let base = to.as_str().trim_end_matches("-gb");
            let prefix = format!("{base}-modem-");
            plant
                .elements
                .iter()
                .filter(|e| e.id.as_str().starts_with(&prefix))
                .count()
        };
        let mut lens: Vec<f64> = Vec::new();
        let mut group_lens: Vec<f64> = Vec::new();
        for e in plant
            .edges
            .iter()
            .filter(|e| e.from.element_id == element.id)
        {
            if let schema::plant::EdgeKind::Drop { len_ft, .. } = e.kind {
                if modems_behind(&e.to.element_id) > 1 {
                    group_lens.push(len_ft);
                } else {
                    lens.push(len_ft);
                }
            }
        }
        let ends = |l: &[f64]| -> Vec<f64> {
            if l.is_empty() {
                Vec::new()
            } else {
                vec![
                    l.iter().copied().fold(f64::INFINITY, f64::min),
                    l.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                ]
            }
        };
        let (drops_ft, group_drops_ft) = (ends(&lens), ends(&group_lens));
        let now = port_levels(&band, &input, &current, current_cond);
        if score(kit, &now, 0.0).0
            && super::drops::serves(kit, &band, &now, None, &drops_ft, &group_drops_ft)
        {
            continue;
        }
        let same_kind = TapFamily {
            ports: family.ports,
            through: if spec.terminating {
                Vec::new()
            } else {
                family.through.clone()
            },
            terminating: if spec.terminating {
                family.terminating.clone()
            } else {
                Vec::new()
            },
        };
        let lists_terminating = spec.terminating;
        let fitted = if lists_terminating {
            fit_tap(
                kit,
                &band,
                &input,
                &TapFamily {
                    through: same_kind.terminating.clone(),
                    ..same_kind.clone()
                },
                false,
                &drops_ft,
                &group_drops_ft,
            )
        } else {
            fit_tap(
                kit,
                &band,
                &input,
                &same_kind,
                false,
                &drops_ft,
                &group_drops_ft,
            )
        };
        match fitted {
            Ok(choice) => changes.push((
                element.id.clone(),
                choice.tap.part.id.clone(),
                choice.conditioner.map(|c| c.part.id.clone()),
            )),
            Err(_) => {
                // No value fits: the station moves the way the port as built misses, at the top of the band.
                let port = port_levels(&band, &input, &current, current_cond);
                let tol = kit.port_tolerance_db;
                let want = nudges.entry(active.clone()).or_default();
                let tilt_want = tilts.entry(active.clone()).or_default();
                for t in 0..3 {
                    if port.hi(t) > kit.port_hi_dbmv + tol {
                        want.insert(-1);
                    }
                    if port.hi(t) < kit.port_hi_dbmv - tol {
                        want.insert(1);
                    }
                    // The bottom of the band missing low with the top inside: tilted too far; high: not far enough.
                    if let Some(lo) = kit.port_lo_dbmv {
                        if port.lo(t) < lo - tol {
                            tilt_want.insert(-1);
                        }
                        if port.lo(t) > lo + tol {
                            tilt_want.insert(1);
                        }
                    }
                }
            }
        }
    }
    drop(graph);
    // A station whose taps at the end of their values all want the same way moves half a dB that way, inside the rules'
    // station levels (§15: alignment leaves it a residual the walk didn't see).
    let st = kit.station;
    let mut moved = 0;
    for (id, want) in nudges {
        if want.len() != 1 {
            continue;
        }
        let step = 0.5 * f64::from(*want.iter().next().unwrap_or(&0));
        if let Some(e) = plant.elements.iter_mut().find(|e| e.id == id)
            && let Some(Fitted::Active(f)) = &mut e.fitted
            && let Some(sp) = &mut f.set_point
        {
            let next = sp.out_dbmv_at_hi + step;
            if next <= st.out_dbmv_at_hi.value + 1e-9 && next >= st.min_out_dbmv_at_hi.value - 1e-9
            {
                sp.out_dbmv_at_hi = next;
                moved += 1;
            }
        }
    }
    for (id, want) in tilts {
        if want.len() != 1 {
            continue;
        }
        let step = 0.5 * f64::from(*want.iter().next().unwrap_or(&0));
        if let Some(e) = plant.elements.iter_mut().find(|e| e.id == id)
            && let Some(Fitted::Active(f)) = &mut e.fitted
            && let Some(sp) = &mut f.set_point
        {
            // The bottom missing low wants less tilt (step −0.5), high more; the rules' range bounds it.
            let next = sp.tilt_db + step;
            if next <= st.tilt_db.value + 1e-9 && next >= st.min_tilt_db.value - 1e-9 {
                sp.tilt_db = next;
                moved += 1;
            }
        }
    }
    let n = changes.len() + moved;
    for (id, part, conditioner) in changes {
        if let Some(e) = plant.elements.iter_mut().find(|e| e.id == id) {
            e.component_ref = part;
            match &mut e.fitted {
                Some(Fitted::Passive(p)) => p.conditioner = conditioner,
                fitted => {
                    if conditioner.is_some() {
                        *fitted = Some(Fitted::Passive(PassiveFitted {
                            conditioner,
                            ports: Vec::new(),
                        }));
                    }
                }
            }
        }
    }
    n
}

/// Moves each active's output, inside the rules' station levels and in half-dB steps, to where the taps it feeds sit
/// inside the tap-port tolerance at every temperature if any level does, or else to the middle of what they'd need.
/// Returns how many it moved.
pub fn center_stations(
    kit: &Kit<'_>,
    plant: &mut schema::plant::Plant,
) -> Result<usize, super::DesignError> {
    use schema::common::ElementId;
    use schema::plant::{ElementKind, Fitted};
    use std::collections::BTreeMap as Map;
    let graph = plant_graph::PlantGraph::new(plant, kit.library, kit.profile)
        .map_err(|e| super::DesignError::Rules(format!("{e:?}")))?;
    let mut fed_by: Map<ElementId, ElementId> = Map::new();
    for e in plant
        .elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Tap) && e.structure.is_some())
    {
        let mut at = e.id.clone();
        while let Ok(Some((parent, _))) = graph.parent(&at) {
            if matches!(
                parent.kind,
                ElementKind::Node { .. } | ElementKind::Rpd { .. } | ElementKind::Amp { .. }
            ) {
                fed_by.insert(e.id.clone(), parent.id.clone());
                break;
            }
            at = parent.id.clone();
        }
    }
    drop(graph);
    let tol = kit.port_tolerance_db;
    // The shift each active's taps allow: every port inside ±tol at every temperature.
    let mut bounds: Map<ElementId, (f64, f64)> = Map::new();
    for temp in super::levels::TEMPS {
        for (tap, [_, hi]) in super::bars::tap_port_levels(kit, plant, temp) {
            let Some(active) = fed_by.get(&tap) else {
                continue;
            };
            let e = hi - kit.port_hi_dbmv;
            let b = bounds
                .entry(active.clone())
                .or_insert((f64::NEG_INFINITY, f64::INFINITY));
            b.0 = b.0.max(-tol - e);
            b.1 = b.1.min(tol - e);
        }
    }
    let st = kit.station;
    let mut moved = 0;
    for element in &mut plant.elements {
        let Some((lo, hi)) = bounds.get(&element.id).copied() else {
            continue;
        };
        let Some(Fitted::Active(f)) = &mut element.fitted else {
            continue;
        };
        let Some(sp) = &mut f.set_point else {
            continue;
        };
        let shift = if lo <= hi {
            0.0_f64.clamp(lo, hi)
        } else {
            (lo + hi) / 2.0
        };
        let next = ((sp.out_dbmv_at_hi + shift) * 2.0).round() / 2.0;
        let next = next.clamp(st.min_out_dbmv_at_hi.value, st.out_dbmv_at_hi.value);
        if (next - sp.out_dbmv_at_hi).abs() > 1e-9 {
            sp.out_dbmv_at_hi = next;
            moved += 1;
        }
    }
    Ok(moved)
}

/// The sweep after the pilot alignment (`docs/tasks/25-design-any-map.md`): an amp whose output misses N/10 + 3 dB peak to
/// valley across the forward band [SCTE 183 §6.2], shallowest first, tries each input equalizer of its family with the
/// pad that suits it, everything below it re-aligned, and keeps the one that leaves the least excess over the limit on
/// the node's amps, ties to the flattest. A sweep is how a technician sees what two pilots can't: the bow of the cable's
/// √f against the equalizer's shape between them (§3.1, §15). Returns the plant and how many amps it changed.
pub fn sweep(kit: &Kit<'_>, plant: &schema::plant::Plant) -> Result<Swept, super::DesignError> {
    use schema::common::{ComponentId, ElementId};
    use schema::component::{ComponentSpec, PlugInKind};
    use std::collections::BTreeMap;

    let excess = |responses: &[super::bars::AmpResponse]| -> f64 {
        responses
            .iter()
            .filter(|r| r.outside_family.is_none())
            .map(|r| (r.peak_to_valley_db - r.limit_db).max(0.0))
            .sum()
    };
    let mut current = plant.clone();
    let mut held: BTreeMap<ElementId, ComponentId> = BTreeMap::new();
    let mut changed = 0;
    let (_, mut responses) = super::bars::plant_alignment(kit, &current);
    for _ in 0..8 {
        let mut failing: Vec<&super::bars::AmpResponse> = responses
            .iter()
            .filter(|r| r.outside_family.is_none() && r.peak_to_valley_db > r.limit_db)
            .filter(|r| !held.contains_key(&r.amp))
            .collect();
        failing.sort_by(|a, b| a.n.cmp(&b.n).then(a.amp.cmp(&b.amp)));
        let Some(worst) = failing.first() else {
            break;
        };
        let amp = worst.amp.clone();
        let Some(element) = current.elements.iter().find(|e| e.id == amp) else {
            break;
        };
        let family = kit
            .library
            .get(&element.component_ref)
            .and_then(|c| c.spec.active())
            .and_then(|a| a.plugin_family.clone());
        let Some(family) = family else {
            held.insert(amp.clone(), ComponentId::new(""));
            continue;
        };
        // The equalizers of the family of the pilots' choice's kind, either shape, within 4 dB of its value: the flattest
        // sits beside what the pilots chose, since the two shapes agree at the pilots.
        let chosen =
            element_eq(&current, &amp).and_then(|id| match kit.library.get(&id).map(|c| &c.spec) {
                Some(ComponentSpec::PlugIn(p)) => Some((p.kind, p.value_db)),
                _ => None,
            });
        let eqs: Vec<ComponentId> = kit
            .library
            .components
            .iter()
            .filter(|c| {
                matches!(&c.spec, ComponentSpec::PlugIn(p)
                    if p.family == family
                        && matches!(p.kind, PlugInKind::Equalizer | PlugInKind::CableSimulator)
                        && chosen.is_none_or(|(kind, value)| p.kind == kind && (p.value_db - value).abs() <= 4.0 + 1e-9))
            })
            .map(|c| c.id.clone())
            .collect();
        // Nearest the pilots' value first, the measured shape before the stated one at the same value; the first that brings
        // the amp inside its limit with the node's excess no worse ends the search.
        let mut eqs = eqs;
        let rank = |id: &ComponentId| -> (i64, bool) {
            match kit.library.get(id).map(|c| &c.spec) {
                Some(ComponentSpec::PlugIn(p)) => (
                    chosen.map_or(0, |(_, v)| ((p.value_db - v).abs() * 10.0).round() as i64),
                    p.loss_table.is_empty(),
                ),
                _ => (i64::MAX, true),
            }
        };
        eqs.sort_by_key(rank);
        let limit = worst.limit_db;
        let before = (excess(&responses), worst.peak_to_valley_db);
        let mut best: Option<Trial> = None;
        for eq in eqs {
            let mut trial_held = held.clone();
            trial_held.insert(amp.clone(), eq.clone());
            let fitted =
                sim::align::align_holding(&current, kit.library, kit.profile, &trial_held)?;
            let trial = sim::align::aligned(&current, &fitted);
            let (_, trial_responses) = super::bars::plant_alignment(kit, &trial);
            let pv = trial_responses
                .iter()
                .filter(|r| r.amp == amp)
                .map(|r| r.peak_to_valley_db)
                .fold(0.0, f64::max);
            let score = (excess(&trial_responses), pv);
            let enough = pv <= limit && score.0 <= before.0 + 1e-9;
            if best.as_ref().is_none_or(|(b, ..)| {
                score.0 < b.0 - 1e-9 || (score.0 < b.0 + 1e-9 && score.1 < b.1 - 1e-9)
            }) {
                best = Some((score, trial, trial_responses, eq));
            }
            if enough {
                break;
            }
        }
        match best {
            Some((score, trial, trial_responses, eq))
                if score.0 < before.0 - 1e-9
                    || (score.0 < before.0 + 1e-9 && score.1 < before.1 - 1e-9) =>
            {
                held.insert(amp, eq);
                current = trial;
                responses = trial_responses;
                changed += 1;
            }
            _ => {
                // Nothing flatter: keep what the pilots chose, and don't try it again.
                let kept = element_eq(&current, &amp).unwrap_or_else(|| ComponentId::new(""));
                held.insert(amp, kept);
            }
        }
    }
    Ok((current, changed, held))
}

/// What [`sweep`] did: the plant, how many amps it changed, and the equalizer it holds on each amp it looked at, for any
/// alignment after it to keep.
pub type Swept = (
    schema::plant::Plant,
    usize,
    std::collections::BTreeMap<schema::common::ElementId, schema::common::ComponentId>,
);

/// One equalizer tried in [`sweep`]: its score (the excess over the limits, then the amp's own peak to valley), the plant,
/// its responses and the equalizer.
type Trial = (
    (f64, f64),
    schema::plant::Plant,
    Vec<super::bars::AmpResponse>,
    schema::common::ComponentId,
);

/// The input equalizer fitted on an active.
fn element_eq(
    plant: &schema::plant::Plant,
    id: &schema::common::ElementId,
) -> Option<schema::common::ComponentId> {
    plant
        .elements
        .iter()
        .find(|e| &e.id == id)
        .and_then(|e| match &e.fitted {
            Some(schema::plant::Fitted::Active(f)) => f.input_eq.clone(),
            _ => None,
        })
}
