//! The inference: propose from topology, score by simulating, refine, weigh.

use std::collections::BTreeSet;

use features::reference::SUMMARY_STRIDE;
use features::sampler::class_in;
use features::{DesignReference, StepFeatures, features_for};
use plant_graph::PlantGraph;
use schema::common::{ElementId, Timestamp};
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{AmpState, Fault, FaultKind, FaultSet, Location};
use schema::work_order::CauseClass;
use serde::{Deserialize, Serialize};
use sim::PhysicsError;
use sim::telemetry::{Detail, Scenario, SimOptions, simulate};

use crate::candidates::{Candidate, propose, propose_also};
use crate::fit::{FitResult, fit};
use crate::who::{Who, WhoThresholds, modem_affected, who_is_affected};

/// How hard the diagnoser works and how it reads its numbers.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DiagnoserConfig {
    /// What counts as affected.
    pub who: WhoThresholds,
    /// Hypotheses refined by coordinate descent.
    pub refine_top: usize,
    /// Rounds of coordinate descent.
    pub refine_rounds: usize,
    /// How many ranked hypotheses to return.
    pub keep: usize,
    /// Whether to try a second fault when the best one leaves a residual.
    pub compound: bool,
    /// Spreads the posterior: features are correlated across modems, so χ² overstates how many
    /// independent facts it's looking at. Fitted to the fixture at `examples/accuracy.rs`.
    pub posterior_temperature: f64,
    /// χ² per degree of freedom below which a hypothesis counts as explaining the data.
    pub explained_chi2_per_dof: f64,
    /// Threads to simulate hypotheses on. Answers don't depend on it (invariant 4).
    pub threads: usize,
    /// The node's fiber link as it was designed (§16), so every hypothesis is simulated through the same optics the plant
    /// has; a fault on the fiber is then what differs from it. `None` keeps the root part's constant optics (task 24).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<sim::optics::NodeLink>,
    /// Radio around the plant that healthy homes' wiring picks up (§18), as the environment the node was seen in has it.
    /// A healthy node with it has the noise floor it brings, so it isn't anyone's ingress (task 24).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ambient: Vec<schema::environment::AmbientSource>,
}

impl Default for DiagnoserConfig {
    fn default() -> Self {
        DiagnoserConfig {
            who: WhoThresholds::default(),
            refine_top: 3,
            refine_rounds: 2,
            keep: 8,
            threads: 1,
            compound: true,
            posterior_temperature: 2.0,
            explained_chi2_per_dof: 0.2,
            link: None,
            ambient: Vec::new(),
        }
    }
}

/// One explanation of what was seen.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Hypothesis {
    /// The faults it proposes. Empty is "healthy".
    pub faults: Vec<Fault>,
    /// The first fault's cause, or `None` for healthy.
    pub class: Option<CauseClass>,
    /// The components, in the same order as `faults`.
    pub components: Vec<String>,
    /// Posterior probability among the hypotheses checked.
    pub posterior: f64,
    /// Misfit.
    pub chi2: f64,
    /// Terms in the misfit.
    pub dof: usize,
    /// Modems this hypothesis says are affected.
    pub affected_modems: u32,
    /// Simulated telemetry under this hypothesis, beside what was observed.
    pub predicted: StepFeatures,
}

/// The answer to who, what and how far along it got.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Diagnosis {
    /// Who is affected.
    pub who: Who,
    /// Ranked, best first.
    pub hypotheses: Vec<Hypothesis>,
    /// How many simulations it took.
    pub simulations: usize,
}

/// The component a fault is on, in the plant's own names.
pub fn fault_component(fault: &Fault) -> String {
    features::sampler::fault_element(&fault.kind).unwrap_or_default()
}

struct Scorer<'a> {
    graph: &'a PlantGraph<'a>,
    reference: &'a DesignReference,
    sample: &'a EnvironmentSample,
    observed: &'a StepFeatures,
    runs: std::sync::atomic::AtomicUsize,
    thresholds: &'a WhoThresholds,
    link: Option<&'a sim::optics::NodeLink>,
    ambient: &'a [schema::environment::AmbientSource],
}

struct Scored {
    faults: Vec<Fault>,
    log_prior: f64,
    result: FitResult,
    predicted: StepFeatures,
}

impl Scorer<'_> {
    fn score(&self, faults: Vec<Fault>, log_prior: f64) -> Result<Scored, PhysicsError> {
        let environment = Environment {
            schema_version: schema::environment::SCHEMA_VERSION.to_string(),
            samples: vec![EnvironmentSample {
                t: Timestamp(self.sample.t.0),
                ..self.sample.clone()
            }],
            ambient_sources: self.ambient.to_vec(),
        };
        let set = FaultSet {
            schema_version: schema::fault::SCHEMA_VERSION.to_string(),
            seed: 0,
            faults: faults.clone(),
        };
        let steps = simulate(&Scenario {
            graph: self.graph,
            environment: &environment,
            faults: &set,
            seed: 0,
            options: SimOptions {
                detail: Detail::Summary {
                    stride: SUMMARY_STRIDE,
                },
                randomize: None,
                measurement_noise_db: 0.0,
                rxmer_scatter_db: 0.0,
            },
            link: self.link,
        })?;
        self.runs.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let predicted = features_for(self.graph, &steps[0], self.reference, None);
        let result = fit(self.observed, &predicted);
        Ok(Scored {
            faults,
            log_prior,
            result,
            predicted,
        })
    }

    fn affected_under(&self, predicted: &StepFeatures) -> u32 {
        predicted
            .modems
            .iter()
            .filter(|m| modem_affected(m, self.thresholds))
            .count() as u32
    }

    /// Coordinate descent on one fault's main parameters.
    fn refine(
        &self,
        mut best: Scored,
        rounds: usize,
        index: usize,
    ) -> Result<Scored, PhysicsError> {
        for _ in 0..rounds {
            for variant in 0..PARAM_COUNT {
                for step in [-1.0, 1.0] {
                    let Some(moved) = move_param(self.graph, &best.faults[index], variant, step)
                    else {
                        continue;
                    };
                    let mut faults = best.faults.clone();
                    faults[index] = moved;
                    let trial = self.score(faults, best.log_prior)?;
                    if trial.result.chi2 < best.result.chi2 {
                        best = trial;
                    }
                }
            }
        }
        Ok(best)
    }
}

const PARAM_COUNT: usize = 4;

fn scale(value: f64, step: f64, factor: f64) -> f64 {
    if step > 0.0 {
        value * factor
    } else {
        value / factor
    }
}

fn span_len_ft(graph: &PlantGraph<'_>, id: &schema::common::EdgeId) -> Option<f64> {
    graph.edge(id).ok()?.kind.cable().map(|(_, len_ft)| len_ft)
}

/// The location moved along its span by a share of its length.
fn nudged(graph: &PlantGraph<'_>, location: &Location, step: f64) -> Option<Location> {
    let Location::Span { span_id, pos_ft } = location else {
        return None;
    };
    let len_ft = span_len_ft(graph, span_id)?;
    Some(Location::Span {
        span_id: span_id.clone(),
        pos_ft: (pos_ft + step * len_ft * 0.2).clamp(0.0, len_ft),
    })
}

/// One parameter of one fault moved one step, or `None` if the fault has no such parameter.
fn move_param(graph: &PlantGraph<'_>, fault: &Fault, variant: usize, step: f64) -> Option<Fault> {
    let kind = match (&fault.kind, variant) {
        (
            FaultKind::ConnectorReflection {
                location,
                return_loss_db,
                couplings,
            },
            0,
        ) => FaultKind::ConnectorReflection {
            location: location.clone(),
            return_loss_db: (return_loss_db + step * 2.5).clamp(2.0, 30.0),
            couplings: couplings.clone(),
        },
        (
            FaultKind::ConnectorReflection {
                location,
                return_loss_db,
                couplings,
            },
            1,
        ) => FaultKind::ConnectorReflection {
            location: nudged(graph, location, step)?,
            return_loss_db: *return_loss_db,
            couplings: couplings.clone(),
        },
        (
            FaultKind::Suckout {
                location,
                f0_mhz,
                depth_db,
                width_mhz,
            },
            v @ 0..=2,
        ) => FaultKind::Suckout {
            location: location.clone(),
            f0_mhz: f0_mhz + if v == 0 { step * 4.0 } else { 0.0 },
            depth_db: if v == 1 {
                scale(*depth_db, step, 1.4)
            } else {
                *depth_db
            },
            width_mhz: if v == 2 {
                scale(*width_mhz, step, 1.4)
            } else {
                *width_mhz
            },
        },
        (
            FaultKind::FrequencySelectiveLoss {
                location,
                shape,
                f_lo_mhz,
                f_hi_mhz,
                depth_db,
                stop_return_loss_db,
                skirt_mhz,
            },
            v @ 0..=2,
        ) => FaultKind::FrequencySelectiveLoss {
            location: location.clone(),
            shape: *shape,
            f_lo_mhz: f_lo_mhz
                + if v == 0 && *f_lo_mhz > 0.0 {
                    step * 15.0
                } else {
                    0.0
                },
            f_hi_mhz: f_hi_mhz + if v == 1 { step * 15.0 } else { 0.0 },
            depth_db: if v == 2 {
                scale(*depth_db, step, 1.4)
            } else {
                *depth_db
            },
            stop_return_loss_db: *stop_return_loss_db,
            skirt_mhz: *skirt_mhz,
        },
        (
            FaultKind::WaterIngress {
                span_id,
                pos_ft,
                affected_len_ft,
                k_w_db_per_100ft,
                rain_coupling_per_mm_per_hr,
                impedance_variation_pct,
            },
            v @ 0..=1,
        ) => FaultKind::WaterIngress {
            span_id: span_id.clone(),
            pos_ft: *pos_ft,
            affected_len_ft: *affected_len_ft,
            k_w_db_per_100ft: if v == 0 {
                scale(*k_w_db_per_100ft, step, 1.4)
            } else {
                *k_w_db_per_100ft
            },
            rain_coupling_per_mm_per_hr: *rain_coupling_per_mm_per_hr,
            impedance_variation_pct: if v == 1 {
                scale(*impedance_variation_pct, step, 1.4)
            } else {
                *impedance_variation_pct
            },
        },
        (
            FaultKind::AmpDrift {
                amp_id,
                state:
                    AmpState::Drift {
                        gain_delta_db,
                        tilt_delta_db,
                        compression_onset_dbmv,
                    },
            },
            v @ 0..=1,
        ) => FaultKind::AmpDrift {
            amp_id: amp_id.clone(),
            state: AmpState::Drift {
                gain_delta_db: gain_delta_db + if v == 0 { step * 0.75 } else { 0.0 },
                tilt_delta_db: tilt_delta_db + if v == 1 { step * 0.75 } else { 0.0 },
                compression_onset_dbmv: *compression_onset_dbmv,
            },
        },
        (
            FaultKind::UpstreamIngress {
                entry_location,
                f_mhz,
                bw_mhz,
                power_dbmv,
                schedule,
                burst,
            },
            v @ 0..=2,
        ) => FaultKind::UpstreamIngress {
            entry_location: entry_location.clone(),
            f_mhz: f_mhz + if v == 0 { step * 3.0 } else { 0.0 },
            bw_mhz: if v == 1 {
                scale(*bw_mhz, step, 1.4)
            } else {
                *bw_mhz
            },
            power_dbmv: power_dbmv + if v == 2 { step * 5.0 } else { 0.0 },
            schedule: schedule.clone(),
            burst: burst.clone(),
        },
        (
            FaultKind::DownstreamIngress {
                entry_location,
                f_mhz,
                bw_mhz,
                power_dbmv,
                schedule,
            },
            v @ 0..=2,
        ) => FaultKind::DownstreamIngress {
            entry_location: entry_location.clone(),
            f_mhz: f_mhz + if v == 0 { step * 3.0 } else { 0.0 },
            bw_mhz: if v == 1 {
                scale(*bw_mhz, step, 1.4)
            } else {
                *bw_mhz
            },
            power_dbmv: power_dbmv + if v == 2 { step * 4.0 } else { 0.0 },
            schedule: schedule.clone(),
        },
        (
            FaultKind::PowerSupplySag {
                psu_id,
                v_delta,
                schedule,
            },
            0,
        ) => FaultKind::PowerSupplySag {
            psu_id: psu_id.clone(),
            v_delta: v_delta + step * 2.0,
            schedule: schedule.clone(),
        },
        (
            FaultKind::BadDrop {
                modem_id,
                extra_loss_db,
                return_loss_db,
            },
            0,
        ) => FaultKind::BadDrop {
            modem_id: modem_id.clone(),
            extra_loss_db: (extra_loss_db + step * 1.5).max(0.5),
            return_loss_db: *return_loss_db,
        },
        (
            FaultKind::Misalignment {
                active_id,
                pad_delta_db,
                eq_delta_db,
                return_pad_delta_db,
            },
            v @ 0..=1,
        ) => FaultKind::Misalignment {
            active_id: active_id.clone(),
            pad_delta_db: pad_delta_db + if v == 0 { step } else { 0.0 },
            eq_delta_db: eq_delta_db + if v == 1 { step } else { 0.0 },
            return_pad_delta_db: *return_pad_delta_db,
        },
        (
            FaultKind::RfmMisalignment {
                rpd_id,
                ds_gain_delta_db,
                tilt_delta_db,
            },
            v @ 0..=1,
        ) => FaultKind::RfmMisalignment {
            rpd_id: rpd_id.clone(),
            ds_gain_delta_db: ds_gain_delta_db + if v == 0 { step * 0.75 } else { 0.0 },
            tilt_delta_db: tilt_delta_db + if v == 1 { step * 0.75 } else { 0.0 },
        },
        _ => return None,
    };
    Some(Fault {
        onset: fault.onset.clone(),
        kind,
    })
}

/// §15: a plug-in set wrong, sized by the sim. Downstream amps' AGC reads a wrong pad or equalizer upstream of them and
/// takes back what it can in the section's cable shape, up to its range (§3.4), so what the modems see is neither the
/// plug-in's own shape nor linear in its size. For the actives topology placed best, the observed level drift is solved
/// for pad and equalizer by Gauss–Newton: two steps, each from the sim's response to a unit of each about the last
/// estimate. The estimates, alone and together, are candidates the sim then scores like any other.
fn sized_misalignments(
    scorer: &Scorer<'_>,
    healthy: &StepFeatures,
    candidates: &[Candidate],
    threads: usize,
) -> Result<Vec<Candidate>, PhysicsError> {
    const UNIT_DB: f64 = 2.0;
    const MOST_ACTIVES: usize = 4;
    const MIN_WRONG_DB: f64 = 1.0;
    const MIN_MATCH: f64 = 0.5;
    const LIMIT_DB: f64 = 10.0;
    let mut actives: Vec<(ElementId, f64)> = Vec::new();
    for c in candidates {
        if let FaultKind::Misalignment { active_id, .. } = &c.fault.kind
            && !actives.iter().any(|(a, _)| a == active_id)
        {
            actives.push((active_id.clone(), c.log_prior));
        }
    }
    // Where topology puts it: an active whose section matches who's affected at least as well as half (its candidates'
    // prior is three times that match), the best few of them.
    actives.retain(|a| a.1 >= 3.0 * MIN_MATCH);
    actives.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    actives.truncate(MOST_ACTIVES);
    let fault = |active_id: &ElementId, (pad, eq): (f64, f64)| Fault {
        onset: schema::fault::Onset::Instant,
        kind: FaultKind::Misalignment {
            active_id: active_id.clone(),
            pad_delta_db: pad.clamp(-LIMIT_DB, LIMIT_DB),
            eq_delta_db: eq.clamp(-LIMIT_DB, LIMIT_DB),
            return_pad_delta_db: 0.0,
        },
    };
    // Bucket by bucket, modem by modem.
    let column = |f: &StepFeatures| -> Vec<Option<f64>> {
        f.modems
            .iter()
            .flat_map(|m| m.level_drift_buckets.iter().copied())
            .collect()
    };
    let seen = column(scorer.observed);
    // One Gauss–Newton step from `at`, whose prediction is `here`: the least-squares move along pad, along the
    // equalizer, and along both.
    let step = |here: &[Option<f64>], pad: &[Option<f64>], eq: &[Option<f64>]| {
        let (mut pp, mut ee, mut pe, mut pr, mut er) = (0.0, 0.0, 0.0, 0.0, 0.0);
        if here.len() != seen.len() || pad.len() != seen.len() || eq.len() != seen.len() {
            return None;
        }
        for j in 0..seen.len() {
            let (Some(o), Some(h), Some(p), Some(e)) = (seen[j], here[j], pad[j], eq[j]) else {
                continue;
            };
            let (dp, de, r) = ((p - h) / UNIT_DB, (e - h) / UNIT_DB, o - h);
            pp += dp * dp;
            ee += de * de;
            pe += dp * de;
            pr += dp * r;
            er += de * r;
        }
        let alone_pad = (pp > 1e-9).then(|| (pr / pp, 0.0));
        let alone_eq = (ee > 1e-9).then(|| (0.0, er / ee));
        let det = pp * ee - pe * pe;
        let both = (det > 1e-6 * pp.max(ee).powi(2))
            .then(|| ((pr * ee - er * pe) / det, (er * pp - pr * pe) / det));
        Some([alone_pad, alone_eq, both])
    };
    let predict = |jobs: Vec<Fault>| {
        in_parallel(threads, jobs, |f| {
            scorer.score(vec![f], 0.0).map(|s| column(&s.predicted))
        })
    };
    // First step, from no fault.
    let base = column(healthy);
    let units = predict(
        actives
            .iter()
            .flat_map(|(a, _)| [fault(a, (UNIT_DB, 0.0)), fault(a, (0.0, UNIT_DB))])
            .collect(),
    )?;
    let mut first: Vec<(usize, (f64, f64))> = Vec::new();
    for i in 0..actives.len() {
        for estimate in step(&base, &units[2 * i], &units[2 * i + 1])
            .into_iter()
            .flatten()
            .flatten()
        {
            first.push((
                i,
                (
                    estimate.0.clamp(-LIMIT_DB, LIMIT_DB),
                    estimate.1.clamp(-LIMIT_DB, LIMIT_DB),
                ),
            ));
        }
    }
    // Second step, from each estimate: its prediction, and a unit more of each.
    let around = predict(
        first
            .iter()
            .flat_map(|(i, (p, e))| {
                let a = &actives[*i].0;
                [
                    fault(a, (*p, *e)),
                    fault(a, (p + UNIT_DB, *e)),
                    fault(a, (*p, e + UNIT_DB)),
                ]
            })
            .collect(),
    )?;
    let mut out = Vec::new();
    for (k, (i, (p, e))) in first.iter().enumerate() {
        let (active_id, log_prior) = &actives[*i];
        // A wrong plug-in is a whole number of 1 dB steps off (design-practice.md): pads and equalizers come in steps, and
        // what's between them is a plug-in's tolerance, the healthy plant's own draw. So the estimates go to whole steps.
        let mut push = |pad: f64, eq: f64| {
            let (pad, eq) = (pad.round(), eq.round());
            if (pad.abs() >= MIN_WRONG_DB || eq.abs() >= MIN_WRONG_DB)
                && !out
                    .iter()
                    .any(|c: &Candidate| c.fault == fault(active_id, (pad, eq)))
            {
                out.push(Candidate {
                    fault: fault(active_id, (pad, eq)),
                    log_prior: *log_prior,
                });
            }
        };
        push(*p, *e);
        let moves = step(&around[3 * k], &around[3 * k + 1], &around[3 * k + 2]);
        // The move that keeps this estimate's own shape: pad alone stays pad alone.
        let which = match (p.abs() > 0.0, e.abs() > 0.0) {
            (true, false) => 0,
            (false, true) => 1,
            _ => 2,
        };
        if let Some((dp, de)) = moves.and_then(|m| m[which]) {
            push(p + dp, e + de);
        }
    }
    Ok(out)
}

/// Which hypothesis best explains `observed`, at the environment it was seen in.
pub fn diagnose(
    graph: &PlantGraph<'_>,
    reference: &DesignReference,
    observed: &StepFeatures,
    sample: &EnvironmentSample,
    config: &DiagnoserConfig,
) -> Result<Diagnosis, PhysicsError> {
    infer(graph, reference, observed, sample, config, None)
}

/// As `diagnose`, with a model's proposals as `(component, probability)`. Those are tried beside what topology proposes, and the model's probabilities are the prior, weighted by
/// what the model gave them, and the sim verifies every one: a proposal never reaches an answer unsimulated (`docs/DESIGN.md`).
pub fn diagnose_with_prior(
    graph: &PlantGraph<'_>,
    reference: &DesignReference,
    observed: &StepFeatures,
    sample: &EnvironmentSample,
    config: &DiagnoserConfig,
    prior: &[(String, f64)],
) -> Result<Diagnosis, PhysicsError> {
    infer(graph, reference, observed, sample, config, Some(prior))
}

/// `f` over `items` on up to `threads` threads, with the results in the order of the items, so an answer can't depend on who finished first.
fn in_parallel<T: Send, R: Send>(
    threads: usize,
    items: Vec<T>,
    f: impl Fn(T) -> Result<R, PhysicsError> + Sync,
) -> Result<Vec<R>, PhysicsError> {
    if threads <= 1 || items.len() <= 1 {
        return items.into_iter().map(f).collect();
    }
    let queue = std::sync::Mutex::new(items.into_iter().enumerate());
    let mut out: Vec<(usize, R)> = Vec::new();
    std::thread::scope(|scope| -> Result<(), PhysicsError> {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| -> Result<Vec<(usize, R)>, PhysicsError> {
                    let mut local = Vec::new();
                    loop {
                        let next = queue.lock().map_or(None, |mut q| q.next());
                        let Some((at, item)) = next else {
                            return Ok(local);
                        };
                        local.push((at, f(item)?));
                    }
                })
            })
            .collect();
        for handle in handles {
            match handle.join() {
                Ok(part) => out.extend(part?),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        Ok(())
    })?;
    out.sort_by_key(|r| r.0);
    Ok(out.into_iter().map(|r| r.1).collect())
}

/// What a candidate the model didn't shortlist is taken to have.
const UNPROPOSED_PROBABILITY: f64 = 0.002;

/// The top answer, as a calibrator reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct TopAnswer {
    /// Its component, empty for healthy.
    pub component: String,
    /// Its posterior.
    pub posterior: f64,
    /// Its misfit.
    pub chi2_per_dof: f64,
    /// The share of modems it says are affected.
    pub affected_share: f64,
}

/// The answer's top hypothesis, if it has one.
pub fn top_answer_summary(answer: &Diagnosis, modems: usize) -> Option<TopAnswer> {
    let top = answer.hypotheses.first()?;
    Some(TopAnswer {
        component: top.components.first().cloned().unwrap_or_default(),
        posterior: top.posterior,
        chi2_per_dof: top.chi2 / top.dof.max(1) as f64,
        affected_share: top.affected_modems as f64 / modems.max(1) as f64,
    })
}

fn infer(
    graph: &PlantGraph<'_>,
    reference: &DesignReference,
    observed: &StepFeatures,
    sample: &EnvironmentSample,
    config: &DiagnoserConfig,
    prior: Option<&[(String, f64)]>,
) -> Result<Diagnosis, PhysicsError> {
    let mut who = who_is_affected(observed, &config.who);
    who.scope = crate::who::scope_of(graph, observed, &who.affected);
    let scorer = Scorer {
        graph,
        reference,
        sample,
        observed,
        runs: std::sync::atomic::AtomicUsize::new(0),
        thresholds: &config.who,
        link: config.link.as_ref(),
        ambient: &config.ambient,
    };
    // With a model, one scale for every prior: its probability, or a small one for what it didn't shortlist.
    let model_prior = |component: &str| {
        prior.map(|p| {
            p.iter()
                .find(|(name, _)| name == component)
                .map_or(UNPROPOSED_PROBABILITY, |(_, probability)| *probability)
                .max(1e-6)
                .ln()
        })
    };
    let healthy = scorer.score(Vec::new(), model_prior("").unwrap_or(2.0))?;

    let healthy_chi2_per_dof = healthy.result.chi2 / healthy.result.dof as f64;
    let mut scored = vec![healthy];
    let quiet = who.affected.is_empty()
        && !who.rpd_abnormal
        && !who.upstream_degraded
        && who.actives.is_empty()
        && healthy_chi2_per_dof < config.explained_chi2_per_dof;
    if !quiet || prior.is_some() {
        let also: BTreeSet<String> = prior
            .into_iter()
            .flatten()
            .filter(|(name, _)| !name.is_empty())
            .map(|(name, _)| name.clone())
            .collect();
        let mut candidates: Vec<Candidate> = propose_also(graph, observed, &who, &also)?;
        candidates.extend(sized_misalignments(
            &scorer,
            &scored[0].predicted,
            &candidates,
            config.threads,
        )?);
        // The same fault proposed twice would only be simulated twice.
        let mut seen = BTreeSet::new();
        candidates.retain(|c| seen.insert(format!("{:?}", c.fault.kind)));
        // Upstream ingress sits on a root port wherever the noise really entered (§7.3), so it's proposed by its root, like any
        // other fault there.
        for candidate in &mut candidates {
            if let Some(log_prior) = model_prior(&fault_component(&candidate.fault)) {
                candidate.log_prior = log_prior;
            }
        }
        scored.extend(in_parallel(config.threads, candidates, |candidate| {
            scorer.score(vec![candidate.fault], candidate.log_prior)
        })?);
        let by_fit = |a: &Scored, b: &Scored| a.result.chi2.total_cmp(&b.result.chi2);
        scored.sort_by(by_fit);

        let mut to_refine = Vec::new();
        let mut rest = Vec::new();
        for item in scored {
            if to_refine.len() < config.refine_top && !item.faults.is_empty() {
                to_refine.push(item);
            } else {
                rest.push(item);
            }
        }
        let mut refined = in_parallel(config.threads, to_refine, |item| {
            scorer.refine(item, config.refine_rounds, 0)
        })?;
        refined.extend(rest);
        refined.sort_by(by_fit);

        // A second fault, if what's left over is big enough to need one.
        if config.compound
            && let Some(best) = refined.first()
            && !best.faults.is_empty()
            && best.result.chi2 / best.result.dof as f64 > config.explained_chi2_per_dof
        {
            let residual: Vec<ElementId> = best
                .result
                .per_modem
                .iter()
                .filter(|(_, chi2)| **chi2 > 100.0)
                .map(|(id, _)| id.clone())
                .collect();
            let base = best.faults.clone();
            let base_prior = best.log_prior;
            let base_chi2 = best.result.chi2;
            if !residual.is_empty() {
                let scope = crate::who::scope_of(graph, observed, &residual);
                let who_left = Who {
                    affected: residual,
                    out_of_service: 0,
                    upstream_degraded: false,
                    actives: Vec::new(),
                    rpd_abnormal: false,
                    scope,
                };
                let mut seen = BTreeSet::new();
                let mut extras = Vec::new();
                for candidate in propose(graph, observed, &who_left)? {
                    if !seen.insert(format!("{:?}", candidate.fault.kind)) {
                        continue;
                    }
                    let mut faults = base.clone();
                    faults.push(candidate.fault);
                    let trial = scorer.score(faults, base_prior - 2.0)?;
                    if trial.result.chi2 < base_chi2 * 0.7 {
                        extras.push(trial);
                    }
                }
                refined.extend(extras);
                refined.sort_by(by_fit);
            }
        }
        scored = refined;
    }

    // §DESIGN: a posterior, spread by how badly even the best hypothesis fits.
    let best_chi2_per_dof = scored
        .iter()
        .map(|s| s.result.chi2 / s.result.dof as f64)
        .fold(f64::INFINITY, f64::min);
    let temperature = config.posterior_temperature * best_chi2_per_dof.max(1.0);
    let log_post: Vec<f64> = scored
        .iter()
        .map(|s| s.log_prior - s.result.chi2 / (2.0 * temperature))
        .collect();
    let peak = log_post.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let total: f64 = log_post.iter().map(|l| (l - peak).exp()).sum();

    let mut ranked: Vec<(f64, Scored)> = log_post
        .iter()
        .map(|l| (l - peak).exp() / total)
        .zip(scored)
        .collect();
    ranked.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(a.1.result.chi2.total_cmp(&b.1.result.chi2))
    });

    // One answer per cause and place: parameter variants of it share its posterior.
    let mut merged: Vec<(f64, Scored)> = Vec::new();
    for (posterior, s) in ranked {
        let same = |other: &Scored| {
            other.faults.len() == s.faults.len()
                && other.faults.iter().zip(&s.faults).all(|(a, b)| {
                    class_in(graph, &a.kind) == class_in(graph, &b.kind)
                        && fault_component(a) == fault_component(b)
                })
        };
        match merged.iter_mut().find(|(_, other)| same(other)) {
            Some((total, _)) => *total += posterior,
            None => merged.push((posterior, s)),
        }
    }

    let hypotheses = merged
        .into_iter()
        .take(config.keep)
        .map(|(posterior, s)| Hypothesis {
            class: s.faults.first().map(|f| class_in(graph, &f.kind)),
            components: s.faults.iter().map(fault_component).collect(),
            posterior,
            chi2: s.result.chi2,
            dof: s.result.dof,
            affected_modems: scorer.affected_under(&s.predicted),
            faults: s.faults,
            predicted: s.predicted,
        })
        .collect();
    Ok(Diagnosis {
        who,
        hypotheses,
        simulations: scorer.runs.load(std::sync::atomic::Ordering::Relaxed),
    })
}
