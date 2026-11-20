//! Draws a physically plausible fault for a class at a random place in a plant, from a seed. The
//! ranges are ours (approx), wide on purpose: the sweep is what keeps models agnostic to any one fault.

use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId};
use schema::fault::{AmpState, Burst, Fault, FaultKind, Location, LossShape, Onset, Schedule};
use schema::plant::{Edge, EdgeKind};
use schema::profile::{NodeKind, Profile};
use schema::spectrum::Shape;
use schema::work_order::CauseClass;
use sim::rng::Rng;
use sim::walk::us_band;

/// The nine fault classes, in `CauseClass` terms.
pub const CLASSES: [CauseClass; 9] = [
    CauseClass::WaterIngress,
    CauseClass::ConnectorReflection,
    CauseClass::Suckout,
    CauseClass::FrequencySelectiveLoss,
    CauseClass::AmpDrift,
    CauseClass::UpstreamIngress,
    CauseClass::DownstreamIngress,
    CauseClass::PowerSupplySag,
    CauseClass::BadDrop,
];

/// The classes only a DAA node has.
pub const RPD_CLASSES: [CauseClass; 5] = [
    CauseClass::RpdFailure,
    CauseClass::CinLinkCut,
    CauseClass::GcpLoss,
    CauseClass::TimingLoss,
    CauseClass::RfmMisalignment,
];

/// A fault's cause class, as the work order reads it.
pub fn class_of(kind: &FaultKind) -> CauseClass {
    CauseClass::from(kind)
}

/// A fault's cause class read with its plant: a connector, notch or loss inside one home, past its drop, on a part or its
/// in-home cable, is in-home wiring (impairments.md §12). A bad drop stays one; on plant whose homes are a modem on its drop
/// there's nothing past the drop, so every class reads as `class_of` has it.
pub fn class_in(graph: &PlantGraph<'_>, kind: &FaultKind) -> CauseClass {
    let location = match kind {
        FaultKind::ConnectorReflection { location, .. }
        | FaultKind::Suckout { location, .. }
        | FaultKind::FrequencySelectiveLoss { location, .. } => Some(location),
        _ => None,
    };
    let inside = match location {
        Some(schema::fault::Location::Port { element_id, .. }) => {
            graph.element(element_id).is_ok_and(|e| {
                e.kind.is_premises() && graph.premises_of(element_id).ok().flatten().is_some()
            })
        }
        Some(schema::fault::Location::Span { span_id, .. }) => graph
            .edge(span_id)
            .is_ok_and(|e| matches!(e.kind, schema::plant::EdgeKind::House { .. })),
        None => false,
    };
    if inside {
        CauseClass::InHomeWiring
    } else {
        class_of(kind)
    }
}

/// When to take a reading of this fault, seconds. A transient that's over by the reading leaves nothing to see, and a row labelled
/// with it would teach a fault no telemetry shows, so these are read while they last. `observe` reads 300 s after this.
pub fn reading_t(fault: Option<&Fault>, rng: &mut Rng) -> i64 {
    let lasts_s = fault.and_then(|f| match &f.kind {
        FaultKind::GcpLoss { duration_s, .. } | FaultKind::TimingLoss { duration_s, .. } => {
            Some(*duration_s)
        }
        FaultKind::RpdFailure {
            mode:
                schema::fault::RpdFailureMode::HardReset { recovery_s }
                | schema::fault::RpdFailureMode::SoftReset { recovery_s },
            ..
        } => Some(*recovery_s),
        FaultKind::CinLinkCut { repair_s, .. } => *repair_s,
        _ => None,
    });
    match lasts_s {
        Some(s) => rng.index(((s - 301.0).max(1.0)) as usize) as i64,
        None => rng.index(86_400) as i64,
    }
}

fn spans<'a>(graph: &PlantGraph<'a>) -> Vec<&'a Edge> {
    let mut edges: Vec<&Edge> = graph
        .plant()
        .edges
        .iter()
        .filter(|e| matches!(e.kind, EdgeKind::Span { .. }))
        .collect();
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    edges
}

fn span_len_ft(edge: &Edge) -> f64 {
    edge.kind.cable().map_or(0.0, |(_, len_ft)| len_ft)
}

fn point_on_span(graph: &PlantGraph<'_>, rng: &mut Rng) -> Option<Location> {
    let spans = spans(graph);
    if spans.is_empty() {
        return None;
    }
    let edge = spans[rng.index(spans.len())];
    Some(Location::Span {
        span_id: edge.id.clone(),
        pos_ft: rng.range(0.05, 0.95) * span_len_ft(edge),
    })
}

/// A frequency some channel occupies. Below the channel plan a fault shows nothing in any record but a
/// full-band capture, which sweeps leave out.
pub fn loaded_frequency(profile: &Profile, rng: &mut Rng) -> f64 {
    let plan = &profile.channel_plan;
    let ranges: Vec<(f64, f64)> = plan
        .ds_sc_qam
        .iter()
        .map(|c| {
            (
                c.center_mhz - c.width_mhz / 2.0,
                c.center_mhz + c.width_mhz / 2.0,
            )
        })
        .chain(
            plan.ofdm
                .iter()
                .map(|c| (c.first_active_mhz, c.last_active_mhz)),
        )
        .collect();
    let total: f64 = ranges.iter().map(|r| r.1 - r.0).sum();
    let mut pick = rng.uniform() * total;
    for (lo, hi) in &ranges {
        if pick <= hi - lo {
            return lo + pick;
        }
        pick -= hi - lo;
    }
    ranges.last().map_or(0.0, |r| r.1)
}

/// A flat day, for ingress with no time-of-day pattern.
pub fn flat_schedule() -> Schedule {
    Schedule {
        hourly_multipliers: [1.0; 24],
    }
}

/// Draws one fault of `class`, if the plant has somewhere to put it.
pub fn sample_fault(graph: &PlantGraph<'_>, class: CauseClass, rng: &mut Rng) -> Option<Fault> {
    let (us_lo, us_hi) = us_band(graph.profile());
    let kind = match class {
        // Not drawn until the data is regenerated for the spectrum model (`docs/tasks/16-rxmer-impairment-sim.md`).
        CauseClass::Unknown
        | CauseClass::ResonantPeak
        | CauseClass::AdjacentLevelOffset
        | CauseClass::ForwardOptics
        | CauseClass::CommonPathDistortion => return None,
        // Drawn by the version 2 sampler (task 24), not here: a part of a home, and faults up the fiber.
        CauseClass::InHomeWiring
        | CauseClass::Misalignment
        | CauseClass::FiberCut
        | CauseClass::FiberEventLoss
        | CauseClass::OpticalTransmitterDrift => return None,
        CauseClass::WaterIngress => {
            let spans = spans(graph);
            if spans.is_empty() {
                return None;
            }
            let edge = spans[rng.index(spans.len())];
            let len_ft = span_len_ft(edge);
            let affected_len_ft = len_ft * rng.range(0.1, 0.5);
            FaultKind::WaterIngress {
                span_id: EdgeId::new(edge.id.as_str()),
                pos_ft: rng.uniform() * (len_ft - affected_len_ft),
                affected_len_ft,
                k_w_db_per_100ft: rng.range(1.0, 6.0),
                rain_coupling_per_mm_per_hr: rng.range(0.0, 0.3),
                impedance_variation_pct: rng.range(3.0, 12.0),
            }
        }
        CauseClass::ConnectorReflection => FaultKind::ConnectorReflection {
            location: point_on_span(graph, rng)?,
            return_loss_db: rng.range(4.0, 14.0),
            couplings: None,
        },
        CauseClass::Suckout => FaultKind::Suckout {
            location: point_on_span(graph, rng)?,
            f0_mhz: loaded_frequency(graph.profile(), rng),
            depth_db: rng.range(10.0, 30.0),
            width_mhz: rng.range(3.0, 15.0),
        },
        CauseClass::FrequencySelectiveLoss => {
            let f_lo_mhz = loaded_frequency(graph.profile(), rng) - rng.range(0.0, 30.0);
            FaultKind::FrequencySelectiveLoss {
                location: point_on_span(graph, rng)?,
                shape: match rng.index(3) {
                    0 => LossShape::BandStop,
                    1 => LossShape::LowPass,
                    _ => LossShape::BandStop,
                },
                f_lo_mhz,
                f_hi_mhz: f_lo_mhz + rng.range(30.0, 120.0),
                depth_db: rng.range(10.0, 40.0),
                stop_return_loss_db: rng.range(3.0, 10.0),
                skirt_mhz: None,
            }
        }
        CauseClass::AmpDrift => {
            let amps = graph.amps();
            if amps.is_empty() {
                return None;
            }
            let amp = amps[rng.index(amps.len())];
            FaultKind::AmpDrift {
                amp_id: ElementId::new(amp.id.as_str()),
                state: if rng.uniform() < 0.25 {
                    AmpState::Failed
                } else {
                    AmpState::Drift {
                        gain_delta_db: if rng.uniform() < 0.5 {
                            rng.range(-9.0, -2.0)
                        } else {
                            rng.range(2.0, 6.0)
                        },
                        tilt_delta_db: rng.range(-3.0, 3.0),
                        compression_onset_dbmv: None,
                    }
                },
            }
        }
        CauseClass::UpstreamIngress => FaultKind::UpstreamIngress {
            entry_location: random_entry(graph, rng)?,
            f_mhz: rng.range(us_lo + 3.0, (us_lo + 35.0).min(us_hi - 3.0)),
            bw_mhz: rng.range(4.0, 20.0),
            power_dbmv: rng.range(0.0, 30.0),
            schedule: flat_schedule(),
            burst: (rng.uniform() < 0.3).then(|| Burst {
                duration_ms: rng.range(1.0, 10.0),
                rate_per_s: rng.range(1.0, 30.0),
            }),
        },
        CauseClass::DownstreamIngress => FaultKind::DownstreamIngress {
            entry_location: point_on_span(graph, rng)?,
            f_mhz: loaded_frequency(graph.profile(), rng),
            bw_mhz: rng.range(5.0, 20.0),
            power_dbmv: rng.range(10.0, 35.0),
            schedule: flat_schedule(),
        },
        CauseClass::PowerSupplySag => {
            let supplies = &graph.plant().power_supplies;
            if supplies.is_empty() {
                return None;
            }
            FaultKind::PowerSupplySag {
                psu_id: supplies[rng.index(supplies.len())].id.clone(),
                v_delta: rng.range(-35.0, -10.0),
                schedule: flat_schedule(),
            }
        }
        CauseClass::RpdFailure
        | CauseClass::CinLinkCut
        | CauseClass::GcpLoss
        | CauseClass::TimingLoss
        | CauseClass::RfmMisalignment => {
            let root = graph.root();
            if !matches!(root.kind, schema::plant::ElementKind::Rpd { .. }) {
                return None;
            }
            let rpd_id = root.id.clone();
            match class {
                CauseClass::RpdFailure => FaultKind::RpdFailure {
                    rpd_id,
                    mode: if rng.uniform() < 0.5 {
                        schema::fault::RpdFailureMode::Dead
                    } else {
                        schema::fault::RpdFailureMode::HardReset { recovery_s: 3600.0 }
                    },
                },
                CauseClass::CinLinkCut => FaultKind::CinLinkCut {
                    rpd_id,
                    repair_s: None,
                },
                CauseClass::GcpLoss => FaultKind::GcpLoss {
                    rpd_id,
                    duration_s: rng.range(600.0, 3600.0),
                },
                CauseClass::TimingLoss => FaultKind::TimingLoss {
                    rpd_id,
                    duration_s: rng.range(600.0, 3600.0),
                },
                _ => FaultKind::RfmMisalignment {
                    rpd_id,
                    ds_gain_delta_db: rng.range(-5.0, 5.0),
                    tilt_delta_db: rng.range(-3.0, 3.0),
                },
            }
        }
        CauseClass::BadDrop => {
            let modems = graph.modems();
            if modems.is_empty() {
                return None;
            }
            FaultKind::BadDrop {
                modem_id: modems[rng.index(modems.len())].id.clone(),
                extra_loss_db: rng.range(4.0, 20.0),
                return_loss_db: (rng.uniform() < 0.3).then(|| rng.range(4.0, 14.0)),
            }
        }
    };
    Some(Fault {
        onset: Onset::Instant,
        kind,
    })
}

/// Ingress mostly comes in at a home.
fn random_entry(graph: &PlantGraph<'_>, rng: &mut Rng) -> Option<Location> {
    let modems = graph.modems();
    if !modems.is_empty() && rng.uniform() < 0.7 {
        let modem = modems[rng.index(modems.len())];
        let port = modem.ports.first()?;
        return Some(Location::Port {
            element_id: modem.id.clone(),
            port: port.id.clone(),
        });
    }
    point_on_span(graph, rng)
}

/// The plant element a fault's location names: where a work order sends a technician.
pub fn fault_element(kind: &FaultKind) -> Option<String> {
    let of_location = |location: &Location| match location {
        Location::Span { span_id, .. } => span_id.to_string(),
        Location::Port { element_id, .. } => element_id.to_string(),
    };
    Some(match kind {
        FaultKind::WaterIngress { span_id, .. } => span_id.to_string(),
        FaultKind::ConnectorReflection { location, .. }
        | FaultKind::Suckout { location, .. }
        | FaultKind::FrequencySelectiveLoss { location, .. } => of_location(location),
        FaultKind::AmpDrift { amp_id, .. }
        | FaultKind::ResonantPeak { amp_id, .. }
        | FaultKind::Misalignment {
            active_id: amp_id, ..
        } => amp_id.to_string(),
        // At the head end or node, so there's no component of the node's plant to name.
        FaultKind::AdjacentLevelOffset { .. } | FaultKind::ForwardOptics { .. } => return None,
        FaultKind::CommonPathDistortion { location, .. } => of_location(location),
        FaultKind::UpstreamIngress { entry_location, .. }
        | FaultKind::DownstreamIngress { entry_location, .. } => of_location(entry_location),
        FaultKind::PowerSupplySag { psu_id, .. } => psu_id.to_string(),
        FaultKind::BadDrop { modem_id, .. } => modem_id.to_string(),
        FaultKind::RpdFailure { rpd_id, .. }
        | FaultKind::CinLinkCut { rpd_id, .. }
        | FaultKind::GcpLoss { rpd_id, .. }
        | FaultKind::TimingLoss { rpd_id, .. }
        | FaultKind::RfmMisalignment { rpd_id, .. } => rpd_id.to_string(),
        FaultKind::SpanCut { span_id, .. } | FaultKind::PowerShort { span_id, .. } => {
            span_id.to_string()
        }
        FaultKind::AnalogNodeFailure { node_id, .. } => node_id.to_string(),
        FaultKind::WaterInDevice { element_id, .. }
        | FaultKind::DamagedDevice { element_id, .. } => element_id.to_string(),
        // An area, not a component of the plant: the utility's, not a technician's.
        FaultKind::UtilityOutage { .. } => return None,
    })
}

/// One impairment for a spectrum row: its shape and the faults that make it. Run alone, these say
/// where that shape is (`docs/tasks/17-spectrum-model.md`).
#[derive(Debug, Clone, PartialEq)]
pub struct FaultGroup {
    /// What it looks like.
    pub shape: Shape,
    /// The faults.
    pub faults: Vec<FaultKind>,
}

/// A frequency inside an OFDM channel, MHz.
fn ofdm_frequency(profile: &Profile, rng: &mut Rng) -> Option<f64> {
    let channels = &profile.channel_plan.ofdm;
    if channels.is_empty() {
        return None;
    }
    let channel = &channels[rng.index(channels.len())];
    Some(rng.range(channel.first_active_mhz, channel.last_active_mhz))
}

/// A point on a span or a drop.
fn point_on_edge(graph: &PlantGraph<'_>, rng: &mut Rng) -> Option<(Location, f64)> {
    let mut edges: Vec<&Edge> = graph
        .plant()
        .edges
        .iter()
        .filter(|e| !matches!(e.kind, EdgeKind::Direct))
        .collect();
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    if edges.is_empty() {
        return None;
    }
    let edge = edges[rng.index(edges.len())];
    let len_ft = span_len_ft(edge);
    Some((
        Location::Span {
            span_id: edge.id.clone(),
            pos_ft: rng.range(0.05, 0.95) * len_ft,
        },
        len_ft,
    ))
}

/// Draws one impairment. Ranges run from invisible to past the worst published capture, spread on purpose:
/// this says nothing about how common any of it is. Targets are in `docs/rxmer-gallery.md`.
pub fn sample_group(graph: &PlantGraph<'_>, shape: Shape, rng: &mut Rng) -> Option<FaultGroup> {
    let profile = graph.profile();
    let channel = profile.channel_plan.ofdm.first()?;
    let (first_mhz, last_mhz) = (channel.first_active_mhz, channel.last_active_mhz);
    let mut faults = Vec::new();
    match shape {
        Shape::Ingress => {
            // One bad shield often lets in several bands [PNM-HFC §7.1.4.2].
            let (entry, _) = point_on_edge(graph, rng)?;
            for _ in 0..1 + rng.index(3) {
                faults.push(FaultKind::DownstreamIngress {
                    entry_location: entry.clone(),
                    f_mhz: ofdm_frequency(profile, rng)?,
                    bw_mhz: rng.range(3.0, 20.0),
                    power_dbmv: rng.range(-10.0, 35.0),
                    schedule: flat_schedule(),
                });
            }
        }
        Shape::Spike => {
            let (entry, _) = point_on_edge(graph, rng)?;
            let power_dbmv = rng.range(-30.0, 15.0);
            if rng.uniform() < 0.3 {
                // A sweep generator's comb [FERREIRA2020 Fig 1].
                let mut f_mhz = first_mhz + rng.range(0.0, 6.0);
                while f_mhz < last_mhz {
                    faults.push(FaultKind::DownstreamIngress {
                        entry_location: entry.clone(),
                        f_mhz,
                        bw_mhz: 0.09,
                        power_dbmv,
                        schedule: flat_schedule(),
                    });
                    f_mhz += 6.0;
                }
            } else {
                for _ in 0..1 + rng.index(7) {
                    faults.push(FaultKind::DownstreamIngress {
                        entry_location: entry.clone(),
                        f_mhz: ofdm_frequency(profile, rng)?,
                        bw_mhz: rng.range(0.05, 0.5),
                        power_dbmv: rng.range(-30.0, 15.0),
                        schedule: flat_schedule(),
                    });
                }
            }
        }
        Shape::Dip => {
            let amps = graph.amps();
            if !amps.is_empty() && rng.uniform() < 0.3 {
                faults.push(FaultKind::ResonantPeak {
                    amp_id: ElementId::new(amps[rng.index(amps.len())].id.as_str()),
                    f0_mhz: ofdm_frequency(profile, rng)?,
                    gain_db: rng.range(3.0, 25.0),
                    width_mhz: rng.range(3.0, 15.0),
                });
            } else {
                // Up to 3 on one channel, rare [ZHU2020 Fig 4].
                for _ in 0..if rng.uniform() < 0.8 { 1 } else { 3 } {
                    faults.push(FaultKind::Suckout {
                        location: point_on_edge(graph, rng)?.0,
                        f0_mhz: ofdm_frequency(profile, rng)?,
                        depth_db: rng.range(5.0, 50.0),
                        width_mhz: rng.range(2.0, 25.0),
                    });
                }
            }
        }
        Shape::RollOff => {
            let skirt_mhz = rng.range(10.0, 120.0);
            let high_end = rng.uniform() < 0.7;
            faults.push(FaultKind::FrequencySelectiveLoss {
                location: point_on_edge(graph, rng)?.0,
                shape: if high_end {
                    LossShape::LowPass
                } else {
                    LossShape::HighPass
                },
                f_lo_mhz: first_mhz - rng.range(0.0, skirt_mhz / 2.0),
                f_hi_mhz: last_mhz + rng.range(0.0, skirt_mhz / 2.0),
                depth_db: rng.range(10.0, 60.0),
                stop_return_loss_db: rng.range(15.0, 25.0),
                skirt_mhz: Some(skirt_mhz),
            });
        }
        Shape::BandLoss => {
            let f_lo_mhz = ofdm_frequency(profile, rng)?;
            faults.push(FaultKind::FrequencySelectiveLoss {
                location: point_on_edge(graph, rng)?.0,
                shape: LossShape::BandStop,
                f_lo_mhz,
                f_hi_mhz: f_lo_mhz + rng.range(5.0, 60.0),
                depth_db: rng.range(10.0, 50.0),
                stop_return_loss_db: rng.range(3.0, 15.0),
                skirt_mhz: None,
            });
        }
        Shape::Ripple => {
            let (location, len_ft) = point_on_edge(graph, rng)?;
            let Location::Span { span_id, .. } = location else {
                return None;
            };
            // Two bad reflectors, a cavity of a period the field shows.
            let apart_ft = rng.range(20.0, 200.0).min(len_ft * 0.8);
            let pos_ft = rng.uniform() * (len_ft - apart_ft);
            for at_ft in [pos_ft, pos_ft + apart_ft] {
                faults.push(FaultKind::ConnectorReflection {
                    location: Location::Span {
                        span_id: span_id.clone(),
                        pos_ft: at_ft,
                    },
                    return_loss_db: rng.range(2.0, 12.0),
                    couplings: None,
                });
            }
        }
        Shape::Wave => {
            let (location, len_ft) = point_on_edge(graph, rng)?;
            let Location::Span { span_id, .. } = location else {
                return None;
            };
            let affected_len_ft = rng.range(30.0, 150.0).min(len_ft * 0.9);
            faults.push(FaultKind::WaterIngress {
                span_id,
                pos_ft: rng.uniform() * (len_ft - affected_len_ft),
                affected_len_ft,
                k_w_db_per_100ft: rng.range(5.0, 80.0),
                rain_coupling_per_mm_per_hr: 0.0,
                impedance_variation_pct: rng.range(10.0, 90.0),
            });
        }
        Shape::LowMer => {
            let modems = graph.modems();
            let amps = graph.amps();
            let analog = profile.node_kind == NodeKind::Analog;
            let pick = rng.uniform();
            if analog && pick < 0.4 {
                faults.push(FaultKind::ForwardOptics {
                    cnr_loss_db: rng.range(5.0, 30.0),
                });
            } else if !amps.is_empty() && pick < 0.6 {
                faults.push(FaultKind::AmpDrift {
                    amp_id: ElementId::new(amps[rng.index(amps.len())].id.as_str()),
                    state: AmpState::Drift {
                        gain_delta_db: rng.range(-30.0, -5.0),
                        tilt_delta_db: rng.range(-3.0, 3.0),
                        compression_onset_dbmv: None,
                    },
                });
            } else {
                if modems.is_empty() {
                    return None;
                }
                faults.push(FaultKind::BadDrop {
                    modem_id: modems[rng.index(modems.len())].id.clone(),
                    extra_loss_db: rng.range(5.0, 40.0),
                    return_loss_db: None,
                });
            }
        }
        Shape::Edge => {
            // A block off at the head end. It shows only with an SC-QAM next to the channel.
            faults.push(FaultKind::AdjacentLevelOffset {
                f_lo_mhz: first_mhz - 300.0,
                f_hi_mhz: first_mhz - 0.5,
                offset_db: rng.range(3.0, 12.0),
            });
        }
    }
    Some(FaultGroup { shape, faults })
}

/// The classes version 2 plant adds (task 24): a fault in a home, and a plug-in set wrong at a fitted active.
pub const AS_BUILT_CLASSES: [CauseClass; 2] = [CauseClass::InHomeWiring, CauseClass::Misalignment];

/// The fiber's classes (task 24), drawn on a node's own links by [`sample_fiber_fault`].
pub const FIBER_CLASSES: [CauseClass; 3] = [
    CauseClass::FiberCut,
    CauseClass::FiberEventLoss,
    CauseClass::OpticalTransmitterDrift,
];

/// Every place along a cable a fitting is recorded, by cable and feet from its upstream end, in a fixed order.
fn fittings_on<'a>(
    graph: &PlantGraph<'a>,
    keep: impl Fn(&EdgeKind) -> bool,
) -> Vec<(&'a Edge, f64)> {
    let mut edges: Vec<&Edge> = graph
        .plant()
        .edges
        .iter()
        .filter(|e| keep(&e.kind))
        .collect();
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    let mut out = Vec::new();
    for edge in edges {
        for fitting in graph.fittings(&edge.id).unwrap_or_default() {
            out.push((edge, fitting.pos_ft));
        }
    }
    out
}

/// A connector's return loss, from a good one's to a failed one's. Parts range from a few dB to 30+ dB, and a
/// water-damaged tap reads about 7 dB [SCTE 183 §9.1] (impairments.md): uniform over 2 to 25 dB, so the spread runs
/// from nothing a modem could tell from a sound fitting to an open.
fn connector_return_loss_db(rng: &mut Rng) -> f64 {
    rng.range(2.0, 25.0)
}

/// Draws one fault of `class` on version 2 plant (task 24), where the plant has somewhere to put it. A connector goes at a
/// fitting the plant records, on a span or a drop; a fault in a home at one of its parts or on its in-home cable; a plug-in
/// set wrong at a fitted active. Everything else is drawn as [`sample_fault`] draws it. **This spreads severity on
/// purpose and says nothing about prevalence.** On plant that records no fittings, parts or fitted actives, the new
/// classes have nowhere to go and a connector falls back to a point along a span.
pub fn sample_fault_as_built(
    graph: &PlantGraph<'_>,
    class: CauseClass,
    rng: &mut Rng,
) -> Option<Fault> {
    let kind = match class {
        CauseClass::ConnectorReflection => {
            let places = fittings_on(graph, |k| {
                matches!(k, EdgeKind::Span { .. } | EdgeKind::Drop { .. })
            });
            if places.is_empty() {
                return sample_fault(graph, class, rng);
            }
            let (edge, pos_ft) = places[rng.index(places.len())];
            FaultKind::ConnectorReflection {
                location: Location::Span {
                    span_id: edge.id.clone(),
                    pos_ft,
                },
                return_loss_db: connector_return_loss_db(rng),
                couplings: None,
            }
        }
        CauseClass::InHomeWiring => {
            // A part's own input, or a fitting on the in-home cable: a loose or corroded connector, a cracked splitter.
            let mut places: Vec<Location> = Vec::new();
            let mut parts: Vec<&schema::plant::Element> = graph
                .plant()
                .elements
                .iter()
                .filter(|e| {
                    e.kind.is_premises()
                        && !matches!(
                            e.kind,
                            schema::plant::ElementKind::Modem | schema::plant::ElementKind::Cpe
                        )
                        && graph.premises_of(&e.id).ok().flatten().is_some()
                })
                .collect();
            parts.sort_by(|a, b| a.id.cmp(&b.id));
            for part in parts {
                if let Some(port) = part
                    .ports
                    .iter()
                    .find(|p| p.role == schema::plant::PortRole::Input)
                {
                    places.push(Location::Port {
                        element_id: part.id.clone(),
                        port: port.id.clone(),
                    });
                }
            }
            for (edge, pos_ft) in fittings_on(graph, |k| matches!(k, EdgeKind::House { .. })) {
                places.push(Location::Span {
                    span_id: edge.id.clone(),
                    pos_ft,
                });
            }
            if places.is_empty() {
                return None;
            }
            FaultKind::ConnectorReflection {
                location: places[rng.index(places.len())].clone(),
                return_loss_db: connector_return_loss_db(rng),
                couplings: None,
            }
        }
        CauseClass::Misalignment => {
            let mut actives: Vec<&schema::plant::Element> = graph
                .actives()
                .into_iter()
                .filter(|e| matches!(e.fitted, Some(schema::plant::Fitted::Active(_))))
                .collect();
            actives.sort_by(|a, b| a.id.cmp(&b.id));
            if actives.is_empty() {
                return None;
            }
            let active = actives[rng.index(actives.len())];
            // Pads and equalizers come in 1 dB steps (design-practice.md, CS-NC2000), and the wrong one is from one step
            // off to ten: the pad alone, the equalizer alone, or both.
            let size = |rng: &mut Rng| {
                let sign = if rng.uniform() < 0.5 { -1.0 } else { 1.0 };
                sign * rng.range(0.5, 10.0)
            };
            let which = rng.uniform();
            let (pad_delta_db, eq_delta_db) = if which < 0.4 {
                (size(rng), 0.0)
            } else if which < 0.8 {
                (0.0, size(rng))
            } else {
                (size(rng), size(rng))
            };
            FaultKind::Misalignment {
                active_id: active.id.clone(),
                pad_delta_db,
                eq_delta_db,
                return_pad_delta_db: 0.0,
            }
        }
        CauseClass::FiberCut | CauseClass::FiberEventLoss | CauseClass::OpticalTransmitterDrift => {
            return None;
        }
        _ => return sample_fault(graph, class, rng),
    };
    Some(Fault {
        onset: Onset::Instant,
        kind,
    })
}

/// Draws one fiber fault of `class` on `service_group`'s own links (task 24): a cut through a cable they pass, a loss at a
/// cable, closure, panel or passive they pass, or a transmitter at either end moving. Dirty or degraded splices and
/// connectors, stress and bends add loss [SCTE 181 §18.5.1] (fiber-optics.md): 0.2 dB, under what a receiver's reading
/// would show, to 8 dB, past most links' margin. A transmitter's launch moves −6 to +2 dB and its noise 0 to 10 dB/Hz.
pub fn sample_fiber_fault(
    fiber: &schema::fiber::FiberPlant,
    service_group: &schema::common::ServiceGroupId,
    class: CauseClass,
    rng: &mut Rng,
) -> Option<schema::fiber_fault::FiberFault> {
    use schema::fiber::{FiberElementKind, Hop};
    use schema::fiber_fault::{FiberFault, FiberFaultKind, FiberLocation};
    let links: Vec<&schema::fiber::OpticalLink> = fiber
        .links
        .iter()
        .filter(|l| &l.service_group == service_group)
        .collect();
    let mut cables = std::collections::BTreeSet::new();
    let mut elements = std::collections::BTreeSet::new();
    let mut ends = std::collections::BTreeSet::new();
    for link in &links {
        for hop in &link.hops {
            match hop {
                Hop::Fiber { cable, .. } => {
                    cables.insert(cable.clone());
                }
                Hop::Splice { at, .. } | Hop::Connector { at, .. } => {
                    elements.insert(at.clone());
                }
                Hop::Through { element, .. } => {
                    elements.insert(element.clone());
                }
            }
        }
        if let Some(Hop::Connector { at, .. }) = link.hops.first() {
            ends.insert(at.clone());
        }
    }
    let kind_of = |id: &schema::common::FiberElementId| {
        fiber
            .elements
            .iter()
            .find(|e| &e.id == id)
            .map(|e| e.kind.clone())
    };
    let cables: Vec<_> = cables.into_iter().collect();
    let lossy: Vec<_> = elements
        .into_iter()
        .filter(|id| {
            matches!(
                kind_of(id),
                Some(
                    FiberElementKind::SpliceClosure
                        | FiberElementKind::PatchPanel
                        | FiberElementKind::OpticalPassive
                )
            )
        })
        .collect();
    let transmitters: Vec<_> = ends
        .into_iter()
        .filter(|id| {
            matches!(
                kind_of(id),
                Some(
                    FiberElementKind::OpticalTx
                        | FiberElementKind::Transceiver
                        | FiberElementKind::NodeEnd { .. }
                )
            )
        })
        .collect();
    let len_of = |cable: &schema::common::FiberCableId| {
        fiber
            .cables
            .iter()
            .find(|c| &c.id == cable)
            .map_or(0.0, |c| c.len_ft)
    };
    let kind = match class {
        CauseClass::FiberCut => {
            if cables.is_empty() {
                return None;
            }
            let cable = cables[rng.index(cables.len())].clone();
            let pos_ft = rng.uniform() * len_of(&cable);
            FiberFaultKind::FiberCut {
                cable,
                pos_ft,
                repair_s: None,
            }
        }
        CauseClass::FiberEventLoss => {
            let on_cable = lossy.is_empty() || rng.uniform() < 0.6;
            let location = if on_cable {
                if cables.is_empty() {
                    return None;
                }
                let cable = cables[rng.index(cables.len())].clone();
                let pos_ft = rng.uniform() * len_of(&cable);
                FiberLocation::Cable { cable, pos_ft }
            } else {
                FiberLocation::Element {
                    element: lossy[rng.index(lossy.len())].clone(),
                }
            };
            FiberFaultKind::EventLoss {
                location,
                loss_db: rng.range(0.2, 8.0),
            }
        }
        CauseClass::OpticalTransmitterDrift => {
            if transmitters.is_empty() {
                return None;
            }
            FiberFaultKind::TransmitterDrift {
                element: transmitters[rng.index(transmitters.len())].clone(),
                power_delta_db: rng.range(-6.0, 2.0),
                rin_delta_db: rng.range(0.0, 10.0),
            }
        }
        _ => return None,
    };
    Some(FiberFault {
        onset: Onset::Instant,
        kind,
    })
}

/// Task 19's events, as the sampler draws them.
#[derive(
    serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum DamageKind {
    /// A hardline span cut through, its live end open or shorted (§20.1).
    SpanCut,
    /// An analog node that stops, by its power or its RF section (§20.2).
    NodeFailure,
    /// A fiber cable cut through, every strand (task 22's `FiberCut`).
    FiberCut,
    /// The forward strand broken at a splice, the return lit (task 22's `EventLoss`).
    ForwardLoss,
    /// The return strand broken at a splice, the forward lit.
    ReturnLoss,
    /// A utility outage over homes with no plant supply inside: house power only, the confounder (§20.6).
    HousePower,
    /// A utility outage over a plant supply, which carries its load on batteries if it has them (§20.6).
    SupplyOutage,
    /// Water inside a tap, amp, node or premises part (§20.3).
    Water,
    /// A short across a hardline span (§20.4).
    Short,
    /// A tap struck: through loss, mismatch, ingress, corrosion and ports lost together (§20.5).
    Damaged,
}

/// Every event kind, in the order a row's index walks them.
pub const DAMAGE_KINDS: [DamageKind; 10] = [
    DamageKind::SpanCut,
    DamageKind::NodeFailure,
    DamageKind::FiberCut,
    DamageKind::ForwardLoss,
    DamageKind::ReturnLoss,
    DamageKind::HousePower,
    DamageKind::SupplyOutage,
    DamageKind::Water,
    DamageKind::Short,
    DamageKind::Damaged,
];

/// One drawn event: the plant fault or the fiber fault it is, and how severe on a 0 to 1 scale.
#[derive(Debug, Clone, PartialEq)]
pub struct DamageEvent {
    /// Which kind.
    pub kind: DamageKind,
    /// The plant fault, for every kind but the fiber's.
    pub fault: Option<Fault>,
    /// The fiber fault, for the fiber's kinds.
    pub fiber: Option<schema::fiber_fault::FiberFault>,
    /// 0 is minor, 1 total, where the kind has a range; 1 for a cut.
    pub severity: f64,
}

/// Draws one event of a kind at a random place, or `None` where the plant has nowhere for it (an RPD has no analog node to
/// fail, a link with no splice no strand to break, a street no home outside every supply's reach).
///
/// **No source gives how often any of these happens, or how severe**, so the ranges run from minor to total and say
/// nothing about prevalence. Where a range has a source it's cited; the rest is [inferred]. One cause sets every effect
/// together: a severity drawn once moves them all.
pub fn sample_damage_event(
    graph: &PlantGraph<'_>,
    fiber: Option<&schema::fiber::FiberPlant>,
    kind: DamageKind,
    rng: &mut Rng,
) -> Option<DamageEvent> {
    use schema::fault::{CutEnd, NodeFailureMode};
    use schema::fiber::Hop;
    use schema::fiber_fault::{FiberFault, FiberFaultKind, FiberLocation, StrandRef};
    use schema::plant::ElementKind;

    let plant = graph.plant();
    let spans: Vec<&Edge> = plant
        .edges
        .iter()
        .filter(|e| matches!(e.kind, EdgeKind::Span { .. }))
        .collect();
    let len_of = |e: &Edge| match &e.kind {
        EdgeKind::Span { len_ft, .. } | EdgeKind::Drop { len_ft, .. } => *len_ft,
        _ => 0.0,
    };
    let instant = |kind: FaultKind| Fault {
        onset: Onset::Instant,
        kind,
    };
    let mut severity = 1.0;
    let fault = match kind {
        DamageKind::SpanCut => {
            // A backhoe or a falling limb hits cable by the foot, so a span is drawn by its length [inferred]: a jumper of a
            // few feet inside a housing is rarely the one cut.
            let span = by_length(&spans, &len_of, rng)?;
            // Which way the conductors end up isn't sourced: half open, half shorted [inferred].
            let end = if rng.uniform() < 0.5 {
                CutEnd::Open
            } else {
                CutEnd::Shorted
            };
            Some(FaultKind::SpanCut {
                span_id: span.id.clone(),
                pos_ft: rng.uniform() * len_of(span),
                end,
                repair_s: Some(rng.range(1800.0, 14_400.0)),
            })
        }
        DamageKind::NodeFailure => {
            if !matches!(graph.root().kind, ElementKind::Node { .. }) {
                return None;
            }
            let mode = if rng.uniform() < 0.5 {
                NodeFailureMode::Power
            } else {
                NodeFailureMode::RfSection
            };
            Some(FaultKind::AnalogNodeFailure {
                node_id: graph.root().id.clone(),
                mode,
                repair_s: Some(rng.range(1800.0, 14_400.0)),
            })
        }
        DamageKind::HousePower | DamageKind::SupplyOutage => {
            let homes: Vec<schema::common::LonLat> =
                graph.modems().iter().filter_map(|m| m.geo).collect();
            let supplies: Vec<schema::common::LonLat> =
                plant.power_supplies.iter().filter_map(|s| s.geo).collect();
            if homes.is_empty() {
                return None;
            }
            // A neighbourhood's outage, a few homes to several streets [inferred]: the shares on real streets are unknown.
            let mut drawn = None;
            for _ in 0..20 {
                let radius_ft = rng.range(100.0, 800.0);
                let center = if kind == DamageKind::HousePower || supplies.is_empty() {
                    homes[rng.index(homes.len())]
                } else {
                    supplies[rng.index(supplies.len())]
                };
                let reaches_supply = supplies
                    .iter()
                    .any(|s| sim::faults::dist_ft(center, *s) <= radius_ft);
                if reaches_supply == (kind == DamageKind::SupplyOutage) {
                    severity = (radius_ft - 100.0) / 700.0;
                    drawn = Some(FaultKind::UtilityOutage {
                        center,
                        radius_ft,
                        repair_s: Some(rng.range(1800.0, 14_400.0)),
                    });
                    break;
                }
            }
            Some(drawn?)
        }
        DamageKind::Water => {
            let places: Vec<&schema::plant::Element> = plant
                .elements
                .iter()
                .filter(|e| {
                    matches!(
                        e.kind,
                        ElementKind::Tap
                            | ElementKind::Amp { .. }
                            | ElementKind::Node { .. }
                            | ElementKind::GroundBlock
                            | ElementKind::Splitter
                            | ElementKind::DropAmp
                    )
                })
                .collect();
            let at = places.get(rng.index(places.len().max(1)))?;
            severity = rng.uniform();
            // A water-soaked tap loses about 30 dB by 750 MHz, k = 34.6 dB at 1 GHz, and its return loss falls to about
            // 7 dB [PNM-HFC §7.1.6.4] [SCTE 183 §9.1] (impairments.md): total here is 40 dB and 4 dB.
            Some(FaultKind::WaterInDevice {
                element_id: at.id.clone(),
                loss_db_at_1ghz: 2.0 + 38.0 * severity,
                return_loss_db: 20.0 - 16.0 * severity,
                rain_coupling_per_mm_per_hr: rng.range(0.0, 0.05),
                failed_after_s: (rng.uniform() < severity * severity)
                    .then(|| rng.range(60.0, 1200.0)),
            })
        }
        DamageKind::Short => {
            let powered: Vec<&&Edge> = spans
                .iter()
                .filter(|e| {
                    matches!(
                        e.kind,
                        EdgeKind::Span {
                            power_passing: true,
                            ..
                        }
                    )
                })
                .collect();
            let powered: Vec<&Edge> = powered.into_iter().copied().collect();
            let span = by_length(&powered, &len_of, rng)?;
            // From a dead short to a leak through wet insulation: 0.1 to 100 Ω, even on a log scale [inferred].
            severity = rng.uniform();
            Some(FaultKind::PowerShort {
                span_id: span.id.clone(),
                pos_ft: rng.uniform() * len_of(span),
                fault_resistance_ohm: 10f64.powf(2.0 - 3.0 * severity),
                repair_s: Some(rng.range(1800.0, 14_400.0)),
            })
        }
        DamageKind::Damaged => {
            let taps: Vec<&schema::plant::Element> = plant
                .elements
                .iter()
                .filter(|e| e.kind == ElementKind::Tap)
                .collect();
            let tap = taps.get(rng.index(taps.len().max(1)))?;
            severity = rng.uniform();
            let ports_lost = graph
                .child_edges(&tap.id)
                .iter()
                .filter(|e| matches!(e.kind, EdgeKind::Drop { .. }))
                .filter(|_| rng.uniform() < severity)
                .map(|e| e.from.port.clone())
                .collect();
            // How ingress and corrosion move with the blow has no source: drawn with it, and tagged [inferred].
            Some(FaultKind::DamagedDevice {
                element_id: tap.id.clone(),
                through_loss_db: 30.0 * severity,
                return_loss_db: 20.0 - 18.0 * severity,
                ports_lost,
                ingress_power_dbmv: (rng.uniform() < 0.7).then(|| -40.0 + 40.0 * severity),
                cpd_second_order_dbmv_per_mhz: (rng.uniform() < 0.5)
                    .then(|| -60.0 + 30.0 * severity),
            })
        }
        DamageKind::FiberCut | DamageKind::ForwardLoss | DamageKind::ReturnLoss => None,
    };
    if let Some(fault_kind) = fault {
        return Some(DamageEvent {
            kind,
            fault: Some(instant(fault_kind)),
            fiber: None,
            severity,
        });
    }
    let fiber = fiber?;
    let sg = &plant.service_group;
    let fiber_fault = match kind {
        DamageKind::FiberCut => sample_fiber_fault(fiber, sg, CauseClass::FiberCut, rng)?,
        _ => {
            let direction = if kind == DamageKind::ForwardLoss {
                schema::component::OpticalDirection::Forward
            } else {
                schema::component::OpticalDirection::Return
            };
            let link = fiber
                .links
                .iter()
                .find(|l| &l.service_group == sg && l.direction == direction)?;
            let splices: Vec<usize> = (1..link.hops.len().saturating_sub(1))
                .filter(|i| matches!(link.hops[*i], Hop::Splice { .. }))
                .collect();
            let at = *splices.get(rng.index(splices.len().max(1)))?;
            let strand = |hop: &Hop| match hop {
                Hop::Fiber { cable, strand } => Some(StrandRef {
                    cable: cable.clone(),
                    strand: *strand,
                }),
                _ => None,
            };
            let Hop::Splice { at: closure, .. } = &link.hops[at] else {
                return None;
            };
            // A strand broken or pulled at a splice: a fraction of a percent of the light gets through [inferred]. An analog
            // link counts as up while any light arrives, so a loss like task 22's few dB would leave it carrying service.
            FiberFault {
                onset: Onset::Instant,
                kind: FiberFaultKind::EventLoss {
                    location: FiberLocation::Splice {
                        element: closure.clone(),
                        strands: [strand(&link.hops[at - 1])?, strand(&link.hops[at + 1])?],
                    },
                    loss_db: rng.range(40.0, 60.0),
                },
            }
        }
    };
    Some(DamageEvent {
        kind,
        fault: None,
        fiber: Some(fiber_fault),
        severity: 1.0,
    })
}

/// One of `edges`, drawn with chance in proportion to its length.
fn by_length<'e>(
    edges: &[&'e Edge],
    len_of: &dyn Fn(&Edge) -> f64,
    rng: &mut Rng,
) -> Option<&'e Edge> {
    let total: f64 = edges.iter().map(|e| len_of(e).max(0.0)).sum();
    if edges.is_empty() || total <= 0.0 {
        return None;
    }
    let mut at = rng.uniform() * total;
    for edge in edges {
        at -= len_of(edge).max(0.0);
        if at < 0.0 {
            return Some(edge);
        }
    }
    edges.last().copied()
}
