//! Passives and the per-element questions a walk asks (§2, §11).

use plant_graph::PlantGraph;
use schema::component::{
    ActiveSpec, Component, ComponentSpec, Duplex, FilterShape, FilterSpec, FreqRange,
    LineEqualizerSpec, LossPoint, PlugInKind, PlugInSpec, Split,
};
use schema::fault::LossShape;
use schema::plant::{Element, Port, PortRole};
use schema::profile::Direction;

use crate::error::PhysicsError;
use crate::overlay::{ReturnLoss, ShapedLoss};

/// §15: where `f_mhz` sits across a slope band, from 0 at its low edge to 1 at its high, in √f, as cable is. Held at
/// 0 or 1 beyond it. A part with no band reads 0 everywhere.
fn slope_share(band: Option<FreqRange>, f_mhz: f64) -> f64 {
    let Some(band) = band else {
        return 0.0;
    };
    let (lo, hi) = (band.lo_mhz.sqrt(), band.hi_mhz.sqrt());
    if hi <= lo {
        return 0.0;
    }
    ((f_mhz.max(0.0).sqrt() - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// §15: a plug-in's loss at `f_mhz`, dB: its table where it has one, otherwise its value shaped by its kind.
pub fn plugin_loss_db(spec: &PlugInSpec, f_mhz: f64) -> f64 {
    if !spec.loss_table.is_empty() {
        return interpolate(&spec.loss_table, f_mhz);
    }
    let share = slope_share(spec.slope_band, f_mhz);
    let shaped = match spec.kind {
        PlugInKind::Pad | PlugInKind::ReturnPad => spec.value_db,
        PlugInKind::Equalizer | PlugInKind::ReturnEqualizer | PlugInKind::TapConditioner => {
            if spec.slope_band.is_some() {
                spec.value_db * (1.0 - share)
            } else {
                spec.value_db
            }
        }
        PlugInKind::CableSimulator => spec.value_db * share,
    };
    spec.insertion_loss_db + shaped
}

/// §8, §15: how a plug-in's loss at `f_mhz` moves per dB of its value: 1 for a pad, its slope's share for an equalizer or
/// cable simulator, and 1 for a part read from a table (approx: a table has no value to move).
pub fn plugin_value_shape(spec: &PlugInSpec, f_mhz: f64) -> f64 {
    if !spec.loss_table.is_empty() {
        return 1.0;
    }
    let share = slope_share(spec.slope_band, f_mhz);
    match spec.kind {
        PlugInKind::Pad | PlugInKind::ReturnPad => 1.0,
        PlugInKind::Equalizer | PlugInKind::ReturnEqualizer | PlugInKind::TapConditioner => {
            if spec.slope_band.is_some() {
                1.0 - share
            } else {
                1.0
            }
        }
        PlugInKind::CableSimulator => share,
    }
}

/// §15: an in-line equalizer's loss at `f_mhz`, dB.
pub fn line_eq_loss_db(spec: &LineEqualizerSpec, f_mhz: f64) -> f64 {
    if !spec.loss_table.is_empty() {
        return interpolate(&spec.loss_table, f_mhz);
    }
    spec.insertion_loss_db + spec.value_db * (1.0 - slope_share(Some(spec.slope_band), f_mhz))
}

/// §15: an active's forward gain at `f_mhz`, dB: its table, or `max_gain_db` flat.
pub fn active_gain_db(active: &ActiveSpec, f_mhz: f64) -> f64 {
    if active.gain_table.is_empty() {
        return active.max_gain_db;
    }
    let points: Vec<LossPoint> = active
        .gain_table
        .iter()
        .map(|p| LossPoint {
            f_mhz: p.f_mhz,
            loss_db: p.gain_db,
        })
        .collect();
    interpolate(&points, f_mhz)
}

/// §13: a filter's edge where it states no transition, MHz (approx): §8's sharp edge.
const FILTER_EDGE_MHZ: f64 = 2.0;

/// §13: a filter's stop-band loss as one shape, its stop bands in order. `pass_loss_db` isn't in it.
pub fn filter_shape(spec: &FilterSpec) -> ShapedLoss {
    let e = spec.transition_mhz.unwrap_or(FILTER_EDGE_MHZ);
    let mut bands: Vec<_> = spec.stop_bands.clone();
    bands.sort_by(|a, b| a.lo_mhz.total_cmp(&b.lo_mhz));
    let mut points = Vec::new();
    for band in &bands {
        // A low-pass stops from its stop band up, and a high-pass from its stop band down, as §8's shapes do. A band
        // stop or band pass stops each band it names.
        let shape = match spec.shape {
            FilterShape::LowPass => LossShape::LowPass,
            FilterShape::HighPass => LossShape::HighPass,
            FilterShape::BandStop | FilterShape::BandPass => LossShape::BandStop,
        };
        let (lo, hi) = match spec.shape {
            FilterShape::LowPass => (band.lo_mhz, band.lo_mhz),
            FilterShape::HighPass => (band.hi_mhz, band.hi_mhz),
            _ => (band.lo_mhz, band.hi_mhz),
        };
        // §8's low-pass shape ramps in below its `f_hi`, and its high-pass above its `f_lo`.
        let piece = ShapedLoss::skirted(shape, lo, hi, spec.stop_depth_db, e);
        if let ShapedLoss::Piecewise { points: p } = piece {
            points.extend(p);
        }
    }
    ShapedLoss::Piecewise { points }
}

/// §13: a filter's loss at `f_mhz`, dB.
pub fn filter_loss_db(spec: &FilterSpec, shape: &ShapedLoss, f_mhz: f64) -> f64 {
    spec.pass_loss_db + shape.loss_db(f_mhz)
}

/// §13: a filter's return loss: its own in the pass band, falling to its stop-band value with the stop band's depth.
pub fn filter_return_loss(spec: &FilterSpec, shape: &ShapedLoss) -> ReturnLoss {
    ReturnLoss::StopBand {
        pass_rl_db: spec.return_loss_db,
        stop_rl_db: spec.stop_return_loss_db,
        shape: shape.clone(),
        depth_db: spec.stop_depth_db,
    }
}

/// §2.1: interpolate linearly in dB and hold flat past the table's ends, up to the rated band.
/// Outside the rated band a lookup is an error (§11).
pub fn table_loss_db(
    points: &[LossPoint],
    component: &Component,
    f_mhz: f64,
) -> Result<f64, PhysicsError> {
    check_rated(component, f_mhz)?;
    Ok(interpolate(points, f_mhz))
}

/// Errors if `f_mhz` is outside the part's rated band.
pub fn check_rated(component: &Component, f_mhz: f64) -> Result<(), PhysicsError> {
    match component.rated_band() {
        Some((lo_mhz, hi_mhz)) if f_mhz >= lo_mhz && f_mhz <= hi_mhz => Ok(()),
        _ => Err(PhysicsError::OutsideRatedBand {
            component: component.id.clone(),
            f_mhz,
        }),
    }
}

/// The interpolation alone. Tables with no points read 0 dB.
pub fn interpolate(points: &[LossPoint], f_mhz: f64) -> f64 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return 0.0;
    };
    if f_mhz <= first.f_mhz {
        return first.loss_db;
    }
    if f_mhz >= last.f_mhz {
        return last.loss_db;
    }
    for pair in points.windows(2) {
        if f_mhz <= pair[1].f_mhz {
            let share = (f_mhz - pair[0].f_mhz) / (pair[1].f_mhz - pair[0].f_mhz);
            return pair[0].loss_db + share * (pair[1].loss_db - pair[0].loss_db);
        }
    }
    last.loss_db
}

/// §2.2: a directional coupler's through loss.
pub fn coupler_through_db(coupled_value_db: f64, excess_loss_db: f64) -> f64 {
    -10.0 * (1.0 - 10f64.powf(-coupled_value_db / 10.0)).log10() + excess_loss_db
}

/// What crossing an element from one port to another costs, dB. The two ports' roles say which
/// loss applies, in either direction (§2.1, §5.1). An active has no passive loss to give.
pub fn device_loss_db(
    component: &Component,
    element: &Element,
    in_port: &Port,
    out_port: &Port,
    f_mhz: f64,
) -> Result<f64, PhysicsError> {
    use PortRole::{Coupled, Input, Leg, TapPort, Through};
    let undefined = || PhysicsError::NoLossDefined {
        element: element.id.clone(),
        in_port: in_port.id.clone(),
        out_port: out_port.id.clone(),
    };
    match &component.spec {
        ComponentSpec::Tap(tap) => match (in_port.role, out_port.role) {
            (Input, Through) | (Through, Input) => {
                table_loss_db(&tap.insertion_loss, component, f_mhz)
            }
            (Input, TapPort) | (TapPort, Input) => table_loss_db(&tap.tap_loss, component, f_mhz),
            (TapPort, TapPort) | (TapPort, Through) | (Through, TapPort) => {
                check_rated(component, f_mhz)?;
                Ok(tap.port_isolation_db)
            }
            _ => Err(undefined()),
        },
        ComponentSpec::Splitter(splitter) => match (in_port.role, out_port.role) {
            (Input, Leg) | (Leg, Input) => table_loss_db(&splitter.leg_loss, component, f_mhz),
            _ => Err(undefined()),
        },
        ComponentSpec::DirectionalCoupler(coupler) => {
            check_rated(component, f_mhz)?;
            match (in_port.role, out_port.role) {
                (Input, Through) | (Through, Input) => Ok(coupler_through_db(
                    coupler.coupled_value_db,
                    coupler.excess_loss_db,
                )),
                (Input, Coupled) | (Coupled, Input) => {
                    Ok(coupler.coupled_value_db + coupler.excess_loss_db)
                }
                _ => Err(undefined()),
            }
        }
        // §6.2, §14: what an inserter states; one that states nothing is lossless, as in version 1.
        ComponentSpec::PowerInserter(inserter) => {
            table_loss_db(&inserter.insertion_loss, component, f_mhz)
        }
        // §13: a ground block, input to output.
        ComponentSpec::GroundBlock(block) => match (in_port.role, out_port.role) {
            (Input, Through) | (Through, Input) => {
                table_loss_db(&block.insertion_loss, component, f_mhz)
            }
            _ => Err(undefined()),
        },
        // §15: an in-line equalizer, input to output.
        ComponentSpec::LineEqualizer(eq) => match (in_port.role, out_port.role) {
            (Input, Through) | (Through, Input) => {
                check_rated(component, f_mhz)?;
                Ok(line_eq_loss_db(eq, f_mhz))
            }
            _ => Err(undefined()),
        },
        ComponentSpec::Filter(filter) => match (in_port.role, out_port.role) {
            (Input, Through) | (Through, Input) => {
                check_rated(component, f_mhz)?;
                Ok(filter_loss_db(filter, &filter_shape(filter), f_mhz))
            }
            _ => Err(undefined()),
        },
        _ => Err(undefined()),
    }
}

/// §14: a return loss table as the walk reads it. `None` when the table is empty.
fn rl_table(points: &[schema::component::ReturnLossPoint]) -> Option<ReturnLoss> {
    (!points.is_empty()).then(|| ReturnLoss::Table {
        points: points.iter().map(|p| (p.f_mhz, p.return_loss_db)).collect(),
    })
}

/// §5.1, §14: a part's return loss at a port of this role, from its table where it has one, else its single number.
/// A tap reads the table for that class of port.
pub fn part_return_loss(component: &Component, role: PortRole) -> Option<ReturnLoss> {
    use schema::component::TapPortClass;
    let table = match &component.spec {
        ComponentSpec::Tap(tap) => {
            let class = match role {
                PortRole::Input => TapPortClass::Input,
                PortRole::Through => TapPortClass::Through,
                _ => TapPortClass::TapPort,
            };
            tap.return_loss_by_port
                .iter()
                .find(|p| p.port == class)
                .and_then(|p| rl_table(&p.points))
        }
        ComponentSpec::Splitter(spec) => rl_table(&spec.return_loss_table),
        ComponentSpec::DirectionalCoupler(spec) => rl_table(&spec.return_loss_table),
        ComponentSpec::Terminator(spec) => rl_table(&spec.return_loss_table),
        ComponentSpec::Modem(spec) => rl_table(&spec.return_loss_table),
        ComponentSpec::Cpe(spec) => rl_table(&spec.return_loss_table),
        ComponentSpec::Connector(spec) => rl_table(&spec.return_loss_table),
        spec => spec.active().and_then(|a| rl_table(&a.return_loss_table)),
    };
    table.or_else(|| return_loss_db(component).map(|rl_db| ReturnLoss::Flat { rl_db }))
}

/// §14: a passive's loss from its input to a port of this role, as points for `ReturnLoss::Behind`.
pub fn loss_points_to(component: &Component, role: PortRole) -> Vec<(f64, f64)> {
    let points = |table: &[LossPoint]| table.iter().map(|p| (p.f_mhz, p.loss_db)).collect();
    match &component.spec {
        ComponentSpec::Tap(tap) => match role {
            PortRole::Through => points(&tap.insertion_loss),
            _ => points(&tap.tap_loss),
        },
        ComponentSpec::Splitter(spec) => points(&spec.leg_loss),
        ComponentSpec::DirectionalCoupler(spec) => {
            let loss = match role {
                PortRole::Through => coupler_through_db(spec.coupled_value_db, spec.excess_loss_db),
                _ => spec.coupled_value_db + spec.excess_loss_db,
            };
            vec![(0.0, loss)]
        }
        _ => Vec::new(),
    }
}

/// An element's return loss, where it has one (§5.1).
pub fn return_loss_db(component: &Component) -> Option<f64> {
    match &component.spec {
        ComponentSpec::Node(node) => Some(node.active.return_loss_db),
        ComponentSpec::Rpd(rpd) => Some(rpd.active.return_loss_db),
        ComponentSpec::Amp(amp) => Some(amp.return_loss_db),
        ComponentSpec::Tap(tap) => Some(tap.return_loss_db),
        ComponentSpec::Splitter(splitter) => Some(splitter.return_loss_db),
        ComponentSpec::DirectionalCoupler(coupler) => Some(coupler.return_loss_db),
        ComponentSpec::Terminator(term) => Some(term.return_loss_db),
        ComponentSpec::Modem(modem) => Some(modem.return_loss_db),
        // §14: an inserter that states one.
        ComponentSpec::PowerInserter(inserter) => inserter.return_loss_db,
        ComponentSpec::Connector(connector) => Some(connector.return_loss_db),
        // §13: home parts. A filter's is its pass band's; the walk adds its stop band.
        ComponentSpec::GroundBlock(block) => Some(block.return_loss_db),
        ComponentSpec::LineEqualizer(eq) => Some(eq.return_loss_db),
        ComponentSpec::Cpe(cpe) => Some(cpe.return_loss_db),
        ComponentSpec::DropAmp(amp) => Some(amp.return_loss_db),
        ComponentSpec::Filter(filter) => Some(filter.return_loss_db),
        _ => None,
    }
}

/// The split an active runs on this profile, matched by its band edges.
pub fn active_split(duplex: &Duplex, us_hi_mhz: f64, ds_lo_mhz: f64) -> Option<&Split> {
    let Duplex::Diplexed { splits, .. } = duplex else {
        return None;
    };
    splits
        .iter()
        .find(|s| (s.us_hi_mhz - us_hi_mhz).abs() < 1e-9 && (s.ds_lo_mhz - ds_lo_mhz).abs() < 1e-9)
}

/// §11: whether an element passes `f_mhz` in `direction`. Walks ask this instead of assuming one
/// upstream band below one downstream band. A passive carries whatever its rating covers, either
/// way. An active carries what its split's diplexer gives each direction.
pub fn carries(
    graph: &PlantGraph<'_>,
    element: &Element,
    f_mhz: f64,
    direction: Direction,
) -> bool {
    let Some(component) = graph.component(&element.component_ref) else {
        return false;
    };
    if let Some((lo_mhz, hi_mhz)) = component.rated_band()
        && !(f_mhz >= lo_mhz && f_mhz <= hi_mhz)
    {
        return false;
    }
    // §13: a drop amp carries what its diplexer gives each direction, as an active does.
    let duplex = match &component.spec {
        ComponentSpec::DropAmp(amp) => &amp.duplex,
        spec => match spec.active() {
            Some(active) => &active.duplex,
            None => return true,
        },
    };
    let Some((us_hi_mhz, ds_lo_mhz)) = graph.profile().split_edges() else {
        return false;
    };
    match active_split(duplex, us_hi_mhz, ds_lo_mhz) {
        Some(split) => match direction {
            Direction::Us => f_mhz <= split.us_hi_mhz,
            Direction::Ds => f_mhz >= split.ds_lo_mhz,
            Direction::Both => f_mhz <= split.us_hi_mhz || f_mhz >= split.ds_lo_mhz,
        },
        None => false,
    }
}
