//! Candidates from topology. A fault lives on a component and affects what's below it, so spans rank
//! by how well their downstream set matches who is affected, and each class proposes its parameters
//! from the features. Every proposal is only that: the sim decides.

use std::collections::BTreeSet;

use features::extract::{bucket_center, ofdm_span_mhz};
use features::{ModemFeatures, StepFeatures};
use plant_graph::PlantGraph;
use schema::common::ElementId;
use schema::fault::{AmpState, Fault, FaultKind, Location, LossShape, Onset};
use schema::plant::{Edge, EdgeKind};
use sim::ac::solve_ac;
use sim::overlay::Overlay;
use sim::walk::{ds_band, us_band};

use crate::who::Who;

/// How many spans get a full set of candidates.
const TOP_SPANS: usize = 8;

/// One upstream pre-eq tap as feet of cavity: T = 0.1953 µs (§5.3), there and back at foam hardline's velocity factor 0.87 (§1.3),
/// 0.1953 × 983.57 × 0.87 / 2 ≈ 83.6 ft. A measured echo distance is only good to about this, since the echo lands on whole taps.
const PREEQ_TAP_FT: f64 = 0.1953 * 983.57 * 0.87 / 2.0;

/// A proposed fault with its prior weight.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The fault.
    pub fault: Fault,
    /// Natural log of its prior: how well the topology fits.
    pub log_prior: f64,
}

fn instant(kind: FaultKind) -> Fault {
    Fault {
        onset: Onset::Instant,
        kind,
    }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[values.len() / 2])
}

/// How well a downstream set matches who is affected, 0 to 1.
fn jaccard(below: &BTreeSet<&ElementId>, affected: &BTreeSet<&ElementId>) -> f64 {
    let both = below.intersection(affected).count() as f64;
    let either = below.union(affected).count() as f64;
    if either == 0.0 { 0.0 } else { both / either }
}

fn modems_in<'f>(features: &'f StepFeatures, set: &BTreeSet<&ElementId>) -> Vec<&'f ModemFeatures> {
    features
        .modems
        .iter()
        .filter(|m| set.contains(&m.element_id))
        .collect()
}

/// Lowest level drift over the affected modems and where, MHz.
fn deepest_level(graph: &PlantGraph<'_>, modems: &[&ModemFeatures]) -> Option<(f64, f64)> {
    let (lo, hi) = ds_band(graph.profile());
    let mut best: Option<(f64, f64)> = None;
    for modem in modems {
        for (i, drift) in modem.level_drift_buckets.iter().enumerate() {
            if let Some(d) = drift
                && best.is_none_or(|b| *d < b.0)
            {
                best = Some((*d, bucket_center(i, lo, hi)));
            }
        }
    }
    best
}

/// Deepest RxMER margin bucket over the affected modems, as (margin, frequency, how many MHz are low).
fn deepest_margin(graph: &PlantGraph<'_>, modems: &[&ModemFeatures]) -> Option<(f64, f64, f64)> {
    let (lo, hi) = ofdm_span_mhz(graph.profile());
    let n = modems
        .iter()
        .filter_map(|m| m.rxmer_margin_buckets.as_ref())
        .map(Vec::len)
        .max()?;
    let mut mean = vec![0.0; n];
    let mut count = 0.0;
    for buckets in modems
        .iter()
        .filter_map(|m| m.rxmer_margin_buckets.as_ref())
    {
        for (i, x) in buckets.iter().enumerate() {
            mean[i] += x;
        }
        count += 1.0;
    }
    if count == 0.0 {
        return None;
    }
    mean.iter_mut().for_each(|x| *x /= count);
    let (at, low) = mean
        .iter()
        .copied()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    let floor = median(mean.clone())?;
    let dip = floor - low;
    let wide = mean.iter().filter(|x| floor - **x > dip / 2.0).count() as f64;
    Some((low, bucket_center(at, lo, hi), wide * (hi - lo) / n as f64))
}

/// Everything worth checking. `who` picks which modems matter; `residual` limits a second pass to
/// the modems the first fault didn't explain.
pub fn propose(
    graph: &PlantGraph<'_>,
    features: &StepFeatures,
    who: &Who,
) -> Result<Vec<Candidate>, sim::PhysicsError> {
    propose_also(graph, features, who, &BTreeSet::new())
}

/// The same, and something to try at each component named in `also`, whatever the telemetry's thresholds say. A faint fault can sit
/// under every threshold, so it's a proposal model's to raise, and the sim's to confirm or not.
pub fn propose_also(
    graph: &PlantGraph<'_>,
    features: &StepFeatures,
    who: &Who,
    also: &BTreeSet<String>,
) -> Result<Vec<Candidate>, sim::PhysicsError> {
    let mut out = Vec::new();
    let named = |id: &str| also.contains(id);
    let affected: BTreeSet<&ElementId> = who.affected.iter().collect();
    let affected_modems = if affected.is_empty() && !also.is_empty() {
        features.modems.iter().collect()
    } else {
        modems_in(features, &affected)
    };
    let drift_median = median(
        affected_modems
            .iter()
            .filter_map(|m| m.level_drift_db)
            .collect(),
    );

    // Spans, ranked by downstream set.
    let mut spans: Vec<(&Edge, f64)> = Vec::new();
    for edge in &graph.plant().edges {
        let EdgeKind::Span { .. } = edge.kind else {
            continue;
        };
        let below = graph.modems_below(&Location::Span {
            span_id: edge.id.clone(),
            pos_ft: 0.0,
        })?;
        let below: BTreeSet<&ElementId> = below.iter().map(|m| &m.id).collect();
        spans.push((edge, jaccard(&below, &affected)));
    }
    spans.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.id.cmp(&b.0.id)));

    // A tap that rose above design points at a cavity length, whichever span it's on.
    let echo_ft = affected_modems
        .iter()
        .filter(|m| m.preeq_echo_rise_db.is_some_and(|x| x > 3.0))
        .max_by(|a, b| {
            a.preeq_echo_rise_db
                .unwrap_or(0.0)
                .total_cmp(&b.preeq_echo_rise_db.unwrap_or(0.0))
        })
        .and_then(|m| m.preeq_echo_ft);
    let (ds_lo, ds_hi) = ds_band(graph.profile());
    let deepest = deepest_level(graph, &affected_modems);
    let margin = deepest_margin(graph, &affected_modems);

    let tried = spans
        .iter()
        .enumerate()
        .filter(|(i, (edge, _))| *i < TOP_SPANS || named(edge.id.as_str()))
        .map(|(_, span)| span);
    for (edge, match_score) in tried {
        let EdgeKind::Span { len_ft, .. } = edge.kind else {
            continue;
        };
        let log_prior = 3.0 * match_score;
        let at = |pos_ft: f64| Location::Span {
            span_id: edge.id.clone(),
            pos_ft,
        };

        // Connector: at each fitting where the span records them (version 2: pins at its ends, splices at reel ends),
        // else at three places along it; and from each end of the cavity the echo implies.
        let fittings: Vec<f64> = graph
            .fittings(&edge.id)
            .map(|f| f.iter().map(|x| x.pos_ft).collect())
            .unwrap_or_default();
        let no_fittings = fittings.is_empty();
        let mut positions = if no_fittings {
            vec![len_ft * 0.2, len_ft * 0.5, len_ft * 0.8]
        } else {
            fittings
        };
        if let Some(d_ft) = echo_ft
            && d_ft < len_ft
        {
            positions.push(d_ft);
            positions.push(len_ft - d_ft);
        }
        for pos_ft in positions {
            out.push(Candidate {
                fault: instant(FaultKind::ConnectorReflection {
                    location: at(pos_ft),
                    return_loss_db: 9.0,
                    couplings: None,
                }),
                log_prior,
            });
        }
        // An echo as long as the whole span is a cavity from one end to the other (§5.1): a bad connector at the device housing on
        // either end, against the mismatch the other end always has. Read to the nearest tap, the distance can land either side of
        // the span's length, so within a tap and a half counts.
        // A span that records its fittings has its end connectors among them already.
        if let Some(d_ft) = echo_ft
            && no_fittings
            && (d_ft - len_ft).abs() <= 1.5 * PREEQ_TAP_FT
        {
            for end in [&edge.to, &edge.from] {
                out.push(Candidate {
                    fault: instant(FaultKind::ConnectorReflection {
                        location: Location::Port {
                            element_id: end.element_id.clone(),
                            port: end.port.clone(),
                        },
                        return_loss_db: 9.0,
                        couplings: None,
                    }),
                    log_prior,
                });
            }
        }

        // Water: loss that grows with √f over part of the span.
        let k_w = drift_median.map_or(2.0, |d| (d.abs() / (len_ft * 0.3 / 100.0)).clamp(0.5, 8.0));
        out.push(Candidate {
            fault: instant(FaultKind::WaterIngress {
                span_id: edge.id.clone(),
                pos_ft: len_ft * 0.35,
                affected_len_ft: len_ft * 0.3,
                k_w_db_per_100ft: k_w,
                rain_coupling_per_mm_per_hr: 0.0,
                impedance_variation_pct: 8.0,
            }),
            log_prior,
        });

        if let Some((depth_db, f0_mhz)) = deepest {
            // Suckout at the deepest level bucket.
            out.push(Candidate {
                fault: instant(FaultKind::Suckout {
                    location: at(len_ft * 0.5),
                    f0_mhz,
                    depth_db: depth_db.abs().max(6.0),
                    width_mhz: 8.0,
                }),
                log_prior,
            });
            // A band gone from there, and a filter's edge above it.
            out.push(Candidate {
                fault: instant(FaultKind::FrequencySelectiveLoss {
                    location: at(len_ft * 0.5),
                    shape: LossShape::BandStop,
                    f_lo_mhz: (f0_mhz - 40.0).max(ds_lo),
                    f_hi_mhz: (f0_mhz + 40.0).min(ds_hi),
                    depth_db: depth_db.abs().max(6.0),
                    stop_return_loss_db: 8.0,
                    skirt_mhz: None,
                }),
                log_prior,
            });
            out.push(Candidate {
                fault: instant(FaultKind::FrequencySelectiveLoss {
                    location: at(len_ft * 0.5),
                    shape: LossShape::LowPass,
                    f_lo_mhz: 0.0,
                    f_hi_mhz: (f0_mhz - 30.0).max(ds_lo),
                    depth_db: depth_db.abs().max(6.0),
                    stop_return_loss_db: 8.0,
                    skirt_mhz: None,
                }),
                log_prior,
            });
        }
        if let Some((_, f_mhz, wide_mhz)) = margin {
            // Ingress where RxMER dips and levels don't.
            out.push(Candidate {
                fault: instant(FaultKind::DownstreamIngress {
                    entry_location: at(len_ft * 0.5),
                    f_mhz,
                    bw_mhz: wide_mhz.clamp(4.0, 30.0),
                    power_dbmv: 22.0,
                    schedule: features::sampler::flat_schedule(),
                }),
                log_prior,
            });
            if deepest.is_none_or(|d| (d.1 - f_mhz).abs() > 15.0) {
                out.push(Candidate {
                    fault: instant(FaultKind::Suckout {
                        location: at(len_ft * 0.5),
                        f0_mhz: f_mhz,
                        depth_db: 18.0,
                        width_mhz: wide_mhz.clamp(3.0, 20.0),
                    }),
                    log_prior,
                });
            }
        }
    }

    // Amps: their section is what they'd move.
    for amp in graph.amps() {
        let location = Location::Port {
            element_id: amp.id.clone(),
            port: amp
                .ports
                .iter()
                .find(|p| p.role == schema::plant::PortRole::Input)
                .map_or_else(|| schema::common::PortId::new("in"), |p| p.id.clone()),
        };
        let below = graph.modems_below(&location)?;
        let below: BTreeSet<&ElementId> = below.iter().map(|m| &m.id).collect();
        let log_prior = 3.0 * jaccard(&below, &affected);
        let reports = features.actives.iter().find(|a| a.element_id == amp.id);
        let reported_gain = reports.and_then(|a| a.out_level_drift_db);
        let silent = reports.is_some_and(|a| a.silent);
        let in_section = modems_in(features, &below);
        let down_share = in_section.iter().filter(|m| !m.operational).count() as f64
            / in_section.len().max(1) as f64;
        if silent || down_share > 0.5 {
            out.push(Candidate {
                fault: instant(FaultKind::AmpDrift {
                    amp_id: amp.id.clone(),
                    state: AmpState::Failed,
                }),
                log_prior,
            });
        }
        let gain = reported_gain
            .or_else(|| median(in_section.iter().filter_map(|m| m.level_drift_db).collect()));
        // Named with no drift to go on: a small one, for refinement to size.
        let gain = gain
            .filter(|g| g.abs() > 0.5)
            .or(named(amp.id.as_str()).then_some(-2.0));
        if let Some(gain_delta_db) = gain {
            let tilt_delta_db = median(
                in_section
                    .iter()
                    .filter_map(|m| m.level_drift_slope_db_per_100mhz)
                    .collect(),
            )
            .map_or(0.0, |slope| {
                let (lo, hi) = ds_band(graph.profile());
                slope * (hi - lo) / 100.0
            });
            for tilt in [0.0, tilt_delta_db] {
                out.push(Candidate {
                    fault: instant(FaultKind::AmpDrift {
                        amp_id: amp.id.clone(),
                        state: AmpState::Drift {
                            gain_delta_db,
                            tilt_delta_db: tilt,
                            compression_onset_dbmv: None,
                        },
                    }),
                    log_prior,
                });
            }
        }
    }

    // Fitted actives: a plug-in set wrong (§15). A pad too big takes the same off every channel, an equalizer too big
    // takes its value at the bottom of its slope band and nothing at the top, in √f as cable does. Both are read off the
    // section's mean level drift by least squares, alone and together, for the sim to size.
    for active in graph.actives() {
        let Some(schema::plant::Fitted::Active(fitted)) = &active.fitted else {
            continue;
        };
        let below: BTreeSet<&ElementId> = if active.id == graph.root().id {
            features.modems.iter().map(|m| &m.element_id).collect()
        } else {
            let location = Location::Port {
                element_id: active.id.clone(),
                port: active
                    .ports
                    .iter()
                    .find(|p| p.role == schema::plant::PortRole::Input)
                    .map_or_else(|| schema::common::PortId::new("in"), |p| p.id.clone()),
            };
            graph
                .modems_below(&location)?
                .iter()
                .map(|m| &m.id)
                .collect()
        };
        let match_score = jaccard(&below, &affected);
        // Where topology puts it: a section that matches who's affected at least half as well as a perfect match. A wrong
        // plug-in sized anywhere else takes up the plant's own spread of parts, not a fault.
        if match_score < 0.5 {
            continue;
        }
        let log_prior = 3.0 * match_score;
        let band = fitted
            .input_eq
            .as_ref()
            .and_then(|id| graph.component(id))
            .and_then(|c| match &c.spec {
                schema::component::ComponentSpec::PlugIn(spec) => spec.slope_band,
                _ => None,
            })
            .map_or((ds_lo, ds_hi), |b| (b.lo_mhz, b.hi_mhz));
        let share = |f: f64| {
            ((band.1.sqrt() - f.max(0.0).sqrt()) / (band.1.sqrt() - band.0.sqrt())).clamp(0.0, 1.0)
        };
        let section = modems_in(features, &below);
        let buckets = section
            .iter()
            .map(|m| m.level_drift_buckets.len())
            .max()
            .unwrap_or(0);
        let (mut n, mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for i in 0..buckets {
            let values: Vec<f64> = section
                .iter()
                .filter_map(|m| m.level_drift_buckets.get(i).copied().flatten())
                .collect();
            if values.is_empty() {
                continue;
            }
            let y = values.iter().sum::<f64>() / values.len() as f64;
            let x = share(bucket_center(i, ds_lo, ds_hi));
            n += 1.0;
            sx += x;
            sy += y;
            sxx += x * x;
            sxy += x * y;
        }
        if n < 2.0 {
            continue;
        }
        // drift = −pad − eq × share: the pad alone, the equalizer alone, and both.
        let pad_alone = -sy / n;
        let eq_alone = if sxx > 0.0 { -sxy / sxx } else { 0.0 };
        let det = n * sxx - sx * sx;
        let both = (det.abs() > 1e-6 * n * n).then(|| {
            let eq = -(n * sxy - sx * sy) / det;
            let pad = -(sy - (-eq) * sx) / n;
            (pad, eq)
        });
        let mut tries = vec![(pad_alone, 0.0), (0.0, eq_alone)];
        tries.extend(both);
        for (pad_delta_db, eq_delta_db) in tries {
            // A wrong plug-in is a whole number of 1 dB steps off; less is within its tolerance.
            let (pad_delta_db, eq_delta_db) = (pad_delta_db.round(), eq_delta_db.round());
            if pad_delta_db.abs() < 1.0 && eq_delta_db.abs() < 1.0 {
                continue;
            }
            out.push(Candidate {
                fault: instant(FaultKind::Misalignment {
                    active_id: active.id.clone(),
                    pad_delta_db: pad_delta_db.clamp(-10.0, 10.0),
                    eq_delta_db: eq_delta_db.clamp(-10.0, 10.0),
                    return_pad_delta_db: 0.0,
                }),
                log_prior,
            });
        }
    }

    // Supplies: a voltage below what the plant should have.
    let healthy = solve_ac(graph, &Overlay::default())?;
    for active in &features.actives {
        if let (Some(v_drift), Some(power)) =
            (active.v_drift, healthy.actives.get(&active.element_id))
            && v_drift < -2.0
        {
            out.push(Candidate {
                fault: instant(FaultKind::PowerSupplySag {
                    psu_id: power.supply.clone(),
                    v_delta: v_drift,
                    schedule: features::sampler::flat_schedule(),
                }),
                log_prior: 3.0,
            });
        }
    }
    for supply in &graph.plant().power_supplies {
        if named(supply.id.as_str()) {
            out.push(Candidate {
                fault: instant(FaultKind::PowerSupplySag {
                    psu_id: supply.id.clone(),
                    v_delta: -10.0,
                    schedule: features::sampler::flat_schedule(),
                }),
                log_prior: 0.0,
            });
        }
    }
    let many_down = who.out_of_service > 0;
    if many_down
        && !out
            .iter()
            .any(|c| matches!(c.fault.kind, FaultKind::PowerSupplySag { .. }))
    {
        for supply in &graph.plant().power_supplies {
            out.push(Candidate {
                fault: instant(FaultKind::PowerSupplySag {
                    psu_id: supply.id.clone(),
                    v_delta: -30.0,
                    schedule: features::sampler::flat_schedule(),
                }),
                log_prior: 0.0,
            });
        }
    }

    // Drops: the modems that differ from their siblings, or stand alone. Each one's home is a place to look as well.
    let mut homes: BTreeSet<schema::common::EdgeId> = match &who.scope {
        Some(crate::who::Scope::InHome { drop } | crate::who::Scope::DropOrHome { drop }) => {
            [drop.clone()].into_iter().collect()
        }
        _ => BTreeSet::new(),
    };
    // A share, as well as a count: on a big node, a share of modems read past a threshold from randomization alone.
    let few = who.affected.len() <= 3.max(features.modems.len() / 4);
    for modem in &features.modems {
        let stands_out = modem.sibling_level_dev_db.is_some_and(|d| d.abs() > 2.0)
            || modem.sibling_us_tx_dev_db.is_some_and(|d| d.abs() > 2.0);
        let alone = few && affected.contains(&modem.element_id);
        if !(stands_out || alone || named(modem.element_id.as_str())) {
            continue;
        }
        let extra = modem
            .level_drift_db
            .map(f64::abs)
            .or(modem.us_tx_drift_db.map(f64::abs))
            .unwrap_or(6.0)
            .max(1.0);
        let reflective = modem.preeq_nmter_db.is_some_and(|x| x > -35.0);
        if let Some(home) = graph.premises_of(&modem.element_id).ok().flatten() {
            homes.insert(home.drop.id.clone());
        }
        out.push(Candidate {
            fault: instant(FaultKind::BadDrop {
                modem_id: modem.element_id.clone(),
                extra_loss_db: extra,
                return_loss_db: None,
            }),
            log_prior: 3.0,
        });
        if reflective {
            out.push(Candidate {
                fault: instant(FaultKind::BadDrop {
                    modem_id: modem.element_id.clone(),
                    extra_loss_db: extra,
                    return_loss_db: Some(8.0),
                }),
                log_prior: 3.0,
            });
        }
    }

    // Inside a home, where the scope rules put it there or at its drop (PNM-HFC §8.8.9.3), and wherever a drop stands
    // out: a bad connector at each fitting on the drop, and at each of the home's parts and on its in-home cable. Plant
    // whose homes are a modem on its drop, with no fittings recorded, has nothing here.
    for drop in &homes {
        let Ok(edge) = graph.edge(drop) else {
            continue;
        };
        for fitting in graph.fittings(drop).unwrap_or_default() {
            out.push(Candidate {
                fault: instant(FaultKind::ConnectorReflection {
                    location: Location::Span {
                        span_id: drop.clone(),
                        pos_ft: fitting.pos_ft,
                    },
                    return_loss_db: 8.0,
                    couplings: None,
                }),
                log_prior: 3.0,
            });
        }
        let Some(home) = graph.premises_of(&edge.to.element_id).ok().flatten() else {
            continue;
        };
        for part in home.elements.iter().filter(|e| {
            !matches!(
                e.kind,
                schema::plant::ElementKind::Modem | schema::plant::ElementKind::Cpe
            )
        }) {
            let Some(port) = part
                .ports
                .iter()
                .find(|p| p.role == schema::plant::PortRole::Input)
            else {
                continue;
            };
            out.push(Candidate {
                fault: instant(FaultKind::ConnectorReflection {
                    location: Location::Port {
                        element_id: part.id.clone(),
                        port: port.id.clone(),
                    },
                    return_loss_db: 8.0,
                    couplings: None,
                }),
                log_prior: 3.0,
            });
        }
        for cable in &home.house_edges {
            if let EdgeKind::House { len_ft, .. } = cable.kind {
                // At each fitting the cable records, else halfway.
                let mut at: Vec<f64> = graph
                    .fittings(&cable.id)
                    .unwrap_or_default()
                    .iter()
                    .map(|f| f.pos_ft)
                    .collect();
                if at.is_empty() {
                    at.push(len_ft * 0.5);
                }
                for pos_ft in at {
                    out.push(Candidate {
                        fault: instant(FaultKind::ConnectorReflection {
                            location: Location::Span {
                                span_id: cable.id.clone(),
                                pos_ft,
                            },
                            return_loss_db: 8.0,
                            couplings: None,
                        }),
                        log_prior: 3.0,
                    });
                }
            }
        }
    }

    // Upstream ingress: modem data doesn't place it (§7.3), so a root port's where it stops.
    if who.upstream_degraded || named(graph.root().id.as_str()) {
        let (us_lo, us_hi) = us_band(graph.profile());
        let f_mhz = features
            .service_group
            .us_noise_peak_mhz
            .unwrap_or((us_lo + us_hi.min(us_lo + 40.0)) / 2.0);
        let root = graph.root();
        for port in root.ports.iter() {
            for (bw_mhz, power_dbmv) in [(10.0, 0.0), (20.0, -10.0)] {
                out.push(Candidate {
                    fault: instant(FaultKind::UpstreamIngress {
                        entry_location: Location::Port {
                            element_id: root.id.clone(),
                            port: port.id.clone(),
                        },
                        f_mhz,
                        bw_mhz,
                        power_dbmv,
                        schedule: features::sampler::flat_schedule(),
                        burst: None,
                    }),
                    log_prior: 3.0,
                });
            }
        }
    }
    // The RPD itself. Its reports narrow this down, and the sim still decides.
    if let Some(rpd) = &features.rpd {
        let rpd_id = rpd.element_id.clone();
        let root_affected = who.rpd_abnormal
            || named(rpd_id.as_str())
            || (!affected.is_empty() && affected.len() == features.modems.len());
        if root_affected {
            for kind in [
                FaultKind::RpdFailure {
                    rpd_id: rpd_id.clone(),
                    mode: schema::fault::RpdFailureMode::Dead,
                },
                FaultKind::RpdFailure {
                    rpd_id: rpd_id.clone(),
                    mode: schema::fault::RpdFailureMode::HardReset { recovery_s: 1e9 },
                },
                FaultKind::RpdFailure {
                    rpd_id: rpd_id.clone(),
                    mode: schema::fault::RpdFailureMode::SoftReset { recovery_s: 1e9 },
                },
                FaultKind::CinLinkCut {
                    rpd_id: rpd_id.clone(),
                    repair_s: None,
                },
                FaultKind::GcpLoss {
                    rpd_id: rpd_id.clone(),
                    duration_s: 1e9,
                },
                // Reset at the sample, so the state it's in is the state it's seen in: time since a loss isn't reported.
                FaultKind::GcpLoss {
                    rpd_id: rpd_id.clone(),
                    duration_s: 1e9,
                },
                FaultKind::TimingLoss {
                    rpd_id: rpd_id.clone(),
                    duration_s: 1e9,
                },
            ] {
                out.push(Candidate {
                    fault: instant(kind),
                    log_prior: 3.0,
                });
            }
        }
        // All modems down together, or all moved alike, with every report normal: the RF output itself.
        let reported_gain = features
            .actives
            .iter()
            .find(|a| a.element_id == rpd_id)
            .and_then(|a| a.out_level_drift_db);
        if let Some(gain) = reported_gain
            .filter(|g| g.abs() > 0.5)
            .or(drift_median.filter(|d| d.abs() > 0.5 && affected.len() == features.modems.len()))
        {
            out.push(Candidate {
                fault: instant(FaultKind::RfmMisalignment {
                    rpd_id,
                    ds_gain_delta_db: gain,
                    tilt_delta_db: 0.0,
                }),
                log_prior: 3.0,
            });
        }
    }
    Ok(out)
}
