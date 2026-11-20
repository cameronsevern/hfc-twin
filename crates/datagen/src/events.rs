//! Damage events on real maps (`docs/tasks/19-damage-events.md`): each event is drawn at a node of a version 2 design, run
//! through the sim from before its onset to after its repair, and read by `diagnosis::outage`. The streets of a map are real
//! and everything on them is invented, so every row is `Synthetic` (invariant 11).
//!
//! `datagen maps-events <out.jsonl> <manifest.json> <osm dir> <split> <events per node> <seed>`: the rows.
//! `datagen events-rule <rows.jsonl> <out.json>`: task 19's decision rule on them, intervals a bootstrap over nodes.
//! `datagen events-gallery <rows.jsonl> <out.md>`: each kind at its most minor and most total.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use diagnosis::fiber::LinkReading;
use diagnosis::outage::{LostWay, OutageReading, OutageVerdict, hops, hops_to_span, read_outage};
use features::sampler::{DAMAGE_KINDS, DamageKind, sample_damage_event};
use features::spectrum::bin_curve;
use features::sweep::random_sample;
use plant_graph::PlantGraph;
use proposal::spectrum::SpectrumModel;
use schema::common::{Origin, Timestamp};
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{Fault, FaultKind, FaultSet};
use schema::fiber_fault::FiberFault;
use schema::observation::{ModemObservation, RegState};
use schema::profile::NodeKind;
use serde::{Deserialize, Serialize};
use sim::optics::{NodeLink, region_links};
use sim::rng::{Rng, hash_str};
use sim::telemetry::{Detail, Scenario, SimOptions, StepObservation, simulate};

use crate::as_built::{Region, inputs_on, map_regions};
use crate::rows::{Error, read, write_sharded};

/// The steps every row holds, seconds from onset; the read is the last. A modem that loses only its return reinitializes
/// after 17 missed ranging opportunities 15 s apart (§7.4), so the read waits past that and its re-registration.
const STEPS: [i64; 7] = [-60, 0, 60, 120, 300, 450, 600];
const READ_T: i64 = 600;
/// After a repair, the row reads again this long after: past `reregistration_s` with room.
const AFTER_REPAIR_S: i64 = 900;
/// The rule's least: a break counts where it leaves this many modems not operational.
const RULE_MODEMS: usize = 5;
const BOOTSTRAP: usize = 2000;

struct Job {
    region: Arc<Region>,
    node: usize,
    seed: u64,
    index: usize,
}

/// One step's modems, counted.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct StepCount {
    /// Seconds from onset.
    pub t: i64,
    /// Operational.
    pub up: usize,
    /// No downstream.
    pub dark: usize,
    /// Downstream acquired, not registered.
    pub deaf: usize,
}

impl StepCount {
    fn of(t: i64, modems: &[ModemObservation]) -> StepCount {
        let count = |f: &dyn Fn(RegState) -> bool| modems.iter().filter(|m| f(m.reg_state)).count();
        StepCount {
            t,
            up: count(&|s| s == RegState::Operational),
            dark: count(&|s| s == RegState::NotSynchronized),
            deaf: count(&|s| s != RegState::Operational && s != RegState::NotSynchronized),
        }
    }
}

/// The spectrum model on the curves of the modems still up at the read.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Survivors {
    /// Modems up at the read.
    pub modems: usize,
    /// Their captures read, one per OFDM channel.
    pub captures: usize,
    /// Of those, called not normal.
    pub not_normal: usize,
    /// Shapes named, by name.
    pub shapes: BTreeMap<String, usize>,
}

/// One event's row.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EventRow {
    /// Always `Synthetic`.
    pub origin: Origin,
    /// Observation schema the sim wrote.
    pub observation_schema: String,
    /// Fault schema of the event.
    pub fault_schema: String,
    /// Design rules.
    pub rules_id: String,
    /// The map.
    pub map: Option<String>,
    /// The profile.
    pub profile: String,
    /// Analog or RPD.
    pub node_kind: String,
    /// Greenfield, or legacy then upgraded.
    pub design_mode: String,
    /// The node.
    pub node: String,
    /// Its seed.
    pub seed: u64,
    /// The event kind.
    pub kind: DamageKind,
    /// 0 minor to 1 total.
    pub severity: f64,
    /// The plant fault.
    pub fault: Option<Fault>,
    /// The fiber fault.
    pub fiber_fault: Option<FiberFault>,
    /// The span a cut is on.
    pub truth_span: Option<String>,
    /// The element a node failure or fiber loss is at: the node.
    pub truth_element: Option<String>,
    /// Modems on the node.
    pub modems: usize,
    /// Every step, counted.
    pub steps: Vec<StepCount>,
    /// The modems not operational at the read, by plant location.
    pub gone: Vec<String>,
    /// The reading at the read step.
    pub reading: OutageReading,
    /// Hops from the element named to the true point, where both exist.
    pub hops: Option<usize>,
    /// The reading with the node's optics withheld.
    pub reading_no_optics: OutageVerdict,
    /// After the repair, where there is one: counts and the reading.
    pub after: Option<(StepCount, OutageVerdict)>,
    /// The spectrum model on the survivors.
    pub survivors: Survivors,
}

fn spectrum_model() -> &'static Result<SpectrumModel, String> {
    static MODEL: OnceLock<Result<SpectrumModel, String>> = OnceLock::new();
    MODEL.get_or_init(|| {
        let model: SpectrumModel =
            read("ml/artifacts/as_built/spectrum_model.json").map_err(|e| e.to_string())?;
        model.check().map_err(|e| format!("{e:?}"))?;
        Ok(model)
    })
}

fn run(
    graph: &PlantGraph<'_>,
    faults: &FaultSet,
    link: Option<&NodeLink>,
    samples: Vec<EnvironmentSample>,
    seed: u64,
    detail: Detail,
) -> Result<Vec<StepObservation>, Error> {
    let environment = Environment {
        schema_version: schema::environment::SCHEMA_VERSION.to_string(),
        samples,
        ambient_sources: Vec::new(),
    };
    Ok(simulate(&Scenario {
        graph,
        environment: &environment,
        faults,
        seed,
        options: SimOptions {
            detail,
            ..SimOptions::default()
        },
        link,
    })?)
}

fn at(base: &EnvironmentSample, t: i64) -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(t),
        ..base.clone()
    }
}

fn repair_of(fault: Option<&Fault>) -> Option<f64> {
    match &fault?.kind {
        FaultKind::SpanCut { repair_s, .. }
        | FaultKind::AnalogNodeFailure { repair_s, .. }
        | FaultKind::PowerShort { repair_s, .. }
        | FaultKind::UtilityOutage { repair_s, .. } => *repair_s,
        _ => None,
    }
}

fn one_event(job: &Job) -> Result<String, Error> {
    let region = &job.region;
    let plant = &region.plants[job.node];
    let library = &inputs()?.library;
    let graph = PlantGraph::new(plant, library, &region.profile).map_err(|e| format!("{e:?}"))?;
    let mut rng = Rng::new(job.seed);
    let kind = DAMAGE_KINDS[job.index % DAMAGE_KINDS.len()];
    let Some(event) = sample_damage_event(&graph, Some(&region.fiber), kind, &mut rng) else {
        return Ok(String::new());
    };
    let sg = &plant.service_group;
    let healthy = region.healthy.get(sg);
    let faulted = match &event.fiber {
        Some(f) => region_links(
            &region.fiber,
            &region.pairs(),
            library,
            std::slice::from_ref(f),
            Timestamp(0),
        )?
        .remove(sg),
        None => healthy.cloned(),
    };
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.to_string(),
        seed: job.seed,
        faults: event.fault.iter().cloned().collect(),
    };
    let base = random_sample(&mut rng, 0);
    let repair = repair_of(event.fault.as_ref());
    let mut times: Vec<i64> = STEPS.to_vec();
    if let Some(r) = repair {
        times.push(r.ceil() as i64 + AFTER_REPAIR_S);
    }
    let summary = Detail::Summary { stride: 8 };
    // A fiber fault is a link state, not a time series: before onset the link is as designed, so that step is its own run.
    let steps: Vec<StepObservation> = if event.fiber.is_some() {
        let mut before = run(
            &graph,
            &faults,
            healthy,
            vec![at(&base, times[0])],
            job.seed,
            summary,
        )?;
        let after: Vec<EnvironmentSample> = times[1..].iter().map(|t| at(&base, *t)).collect();
        before.extend(run(
            &graph,
            &faults,
            faulted.as_ref(),
            after,
            job.seed,
            summary,
        )?);
        before
    } else {
        let all = times.iter().map(|t| at(&base, *t)).collect();
        run(&graph, &faults, faulted.as_ref(), all, job.seed, summary)?
    };
    let read_at = STEPS.len() - 1;
    let step = &steps[read_at];
    let design = healthy.map(LinkReading::of);
    let reading = read_outage(
        &graph,
        &step.modems,
        step.service_group.optics.as_ref(),
        design.as_ref(),
    );
    let reading_no_optics = read_outage(&graph, &step.modems, None, None).verdict;

    let root = graph.root().id.clone();
    let (truth_span, truth_element) = match (&event.fault, kind) {
        (
            Some(Fault {
                kind: FaultKind::SpanCut { span_id, .. },
                ..
            }),
            _,
        ) => (Some(span_id.clone()), None),
        (
            _,
            DamageKind::NodeFailure
            | DamageKind::FiberCut
            | DamageKind::ForwardLoss
            | DamageKind::ReturnLoss,
        ) => (None, Some(root.clone())),
        _ => (None, None),
    };
    let named = match &reading.verdict {
        OutageVerdict::PlantBreak { element, .. } | OutageVerdict::Premises { element } => {
            Some(element)
        }
        _ => None,
    };
    let distance = match (named, &truth_span, &truth_element) {
        (Some(n), Some(span), _) => hops_to_span(&graph, n, span),
        (Some(n), None, Some(e)) => hops(&graph, n, e),
        _ => None,
    };
    let after = (steps.len() > STEPS.len()).then(|| {
        let last = &steps[steps.len() - 1];
        let t = last.t.0;
        (
            StepCount::of(t, &last.modems),
            read_outage(
                &graph,
                &last.modems,
                last.service_group.optics.as_ref(),
                design.as_ref(),
            )
            .verdict,
        )
    });

    // The survivors' curves, at full detail at the read: a curve is physics at a step, not history, so one step serves.
    let mut survivors = Survivors {
        modems: step
            .modems
            .iter()
            .filter(|m| m.reg_state == RegState::Operational)
            .count(),
        ..Survivors::default()
    };
    if survivors.modems > 0 {
        let model = spectrum_model().as_ref().map_err(|e| e.clone())?;
        let up: std::collections::BTreeSet<_> = step
            .modems
            .iter()
            .filter(|m| m.reg_state == RegState::Operational)
            .map(|m| m.element_id.clone())
            .collect();
        let full = run(
            &graph,
            &faults,
            faulted.as_ref(),
            vec![at(&base, READ_T)],
            job.seed,
            Detail::Full,
        )?;
        for modem in full[0].modems.iter().filter(|m| up.contains(&m.element_id)) {
            for record in modem.rxmer.iter().flatten() {
                let Some(values) = &record.rxmer_db else {
                    continue;
                };
                let first_mhz = record.axis.zero_freq_mhz
                    + f64::from(record.axis.first_active_index) * record.axis.spacing_khz / 1000.0;
                let Some(r) = model.read(&bin_curve(first_mhz, record.axis.spacing_khz, values))
                else {
                    continue;
                };
                survivors.captures += 1;
                if !r.normal() {
                    survivors.not_normal += 1;
                }
                for shape in &r.shapes {
                    *survivors
                        .shapes
                        .entry(format!("{:?}", shape.shape))
                        .or_insert(0) += 1;
                }
            }
        }
    }

    let row = EventRow {
        origin: Origin::Synthetic,
        observation_schema: schema::observation::SCHEMA_VERSION.to_string(),
        fault_schema: schema::fault::SCHEMA_VERSION.to_string(),
        rules_id: region.rules_id.clone(),
        map: region.map.clone(),
        profile: region.profile.id.to_string(),
        node_kind: match region.profile.node_kind {
            NodeKind::Analog => "Analog".into(),
            NodeKind::Rpd => "Rpd".into(),
        },
        design_mode: region.mode.to_string(),
        node: sg.to_string(),
        seed: job.seed,
        kind,
        severity: event.severity,
        fault: event.fault.clone(),
        fiber_fault: event.fiber.clone(),
        truth_span: truth_span.map(|s| s.to_string()),
        truth_element: truth_element.map(|e| e.to_string()),
        modems: step.modems.len(),
        steps: times
            .iter()
            .zip(&steps)
            .map(|(t, s)| StepCount::of(*t, &s.modems))
            .collect(),
        gone: step
            .modems
            .iter()
            .filter(|m| m.reg_state != RegState::Operational)
            .map(|m| m.element_id.to_string())
            .collect(),
        reading,
        hops: distance,
        reading_no_optics,
        after,
        survivors,
    };
    Ok(serde_json::to_string(&row)?)
}

fn inputs() -> Result<&'static crate::as_built::Inputs, Error> {
    static INPUTS: OnceLock<Result<crate::as_built::Inputs, String>> = OnceLock::new();
    INPUTS
        .get_or_init(|| inputs_on(true).map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| Error::from(e.clone()))
}

/// `datagen maps-events`.
pub fn rows(args: &[String]) -> Result<String, Error> {
    let [out, manifest, dir, split, per_node, seed] = args else {
        return Err(
            "usage: datagen maps-events <out.jsonl> <manifest.json> <osm dir> <split> <events per node> <seed>".into(),
        );
    };
    let per_node: usize = per_node.parse()?;
    let seed: u64 = seed.parse()?;
    let inputs = inputs()?;
    let mut jobs = Vec::new();
    for (region, issues) in map_regions(inputs, manifest, dir, split)? {
        for (node, plant) in region.plants.iter().enumerate() {
            if issues[node] {
                continue;
            }
            for index in 0..per_node {
                let job_seed = Rng::derive(
                    seed,
                    &[
                        hash_str(region.map.as_deref().unwrap_or("")),
                        hash_str(region.mode),
                        hash_str(region.profile.id.as_str()),
                        hash_str(plant.service_group.as_str()),
                        index as u64,
                    ],
                )
                .next_u64();
                jobs.push(Job {
                    region: Arc::clone(&region),
                    node,
                    seed: job_seed,
                    index,
                });
            }
        }
    }
    let total = write_sharded(&jobs, out, 64, one_event)?;
    Ok(format!("{total} synthetic event rows in {out}"))
}

/// A share with its bootstrap interval over nodes.
#[derive(Serialize, Debug)]
struct Share {
    hits: usize,
    of: usize,
    share: f64,
    low: f64,
    high: f64,
}

fn share(items: &[(String, bool)], seed: u64) -> Share {
    let hits = items.iter().filter(|(_, h)| *h).count();
    let of = items.len();
    let mut by_node: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for (node, hit) in items {
        let e = by_node.entry(node).or_insert((0, 0));
        e.0 += usize::from(*hit);
        e.1 += 1;
    }
    let nodes: Vec<(usize, usize)> = by_node.into_values().collect();
    let mut rng = Rng::new(seed);
    let mut draws: Vec<f64> = (0..BOOTSTRAP)
        .map(|_| {
            let (mut h, mut n) = (0, 0);
            for _ in 0..nodes.len() {
                let (a, b) = nodes[rng.index(nodes.len())];
                h += a;
                n += b;
            }
            h as f64 / n.max(1) as f64
        })
        .collect();
    draws.sort_by(f64::total_cmp);
    let pick = |q: f64| {
        draws
            .get(((draws.len() as f64 - 1.0) * q).round() as usize)
            .copied()
            .unwrap_or(0.0)
    };
    Share {
        hits,
        of,
        share: hits as f64 / of.max(1) as f64,
        low: pick(0.025),
        high: pick(0.975),
    }
}

/// `datagen events-rule`.
pub fn rule(args: &[String]) -> Result<String, Error> {
    let [rows, out] = args else {
        return Err("usage: datagen events-rule <rows.jsonl> <out.json>".into());
    };
    let text = std::fs::read_to_string(rows)?;
    let events: Vec<EventRow> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    if events.iter().any(|e| e.origin != Origin::Synthetic) {
        return Err("task 19's rule reads synthetic rows only".into());
    }
    let key = |e: &EventRow| {
        format!(
            "{}/{}/{}/{}",
            e.map.as_deref().unwrap_or(""),
            e.profile,
            e.design_mode,
            e.node
        )
    };
    let gone = |e: &EventRow| e.reading.vanished;
    let named = |e: &EventRow| e.reading.is_plant_break() && e.hops.is_some_and(|h| h <= 1);

    let mut breaks: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    let mut confounder = Vec::new();
    let mut direction: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    let mut direction_no_optics: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    let mut reported: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut strand_losses_left_out = 0usize;
    for e in &events {
        let verdict = match &e.reading.verdict {
            OutageVerdict::Normal => "Normal",
            OutageVerdict::PlantBreak { .. } => "PlantBreak",
            OutageVerdict::SupplyRun { .. } => "SupplyRun",
            OutageVerdict::Premises { .. } => "Premises",
            OutageVerdict::NoOnePoint { .. } => "NoOnePoint",
        };
        *reported
            .entry(format!("{:?}", e.kind))
            .or_default()
            .entry(verdict.into())
            .or_insert(0) += 1;
        match e.kind {
            DamageKind::SpanCut | DamageKind::NodeFailure | DamageKind::FiberCut
                if gone(e) >= RULE_MODEMS =>
            {
                let label = match (&e.kind, &e.fault) {
                    (
                        DamageKind::SpanCut,
                        Some(Fault {
                            kind: FaultKind::SpanCut { end, .. },
                            ..
                        }),
                    ) => {
                        format!("SpanCut {end:?}")
                    }
                    (k, _) => format!("{k:?}"),
                };
                breaks.entry(label).or_default().push((key(e), named(e)));
            }
            DamageKind::HousePower if e.reading.dark >= RULE_MODEMS => {
                confounder.push((key(e), e.reading.is_plant_break()));
            }
            // Past the link's margin is where the loss takes the node's service out, as the other parts count a break.
            DamageKind::ForwardLoss | DamageKind::ReturnLoss if gone(e) >= RULE_MODEMS => {
                let right = |v: &OutageVerdict| match (e.kind, v) {
                    (DamageKind::ForwardLoss, OutageVerdict::PlantBreak { lost, .. }) => {
                        matches!(lost, LostWay::ForwardOnly | LostWay::Downstream)
                    }
                    (DamageKind::ReturnLoss, OutageVerdict::PlantBreak { lost, .. }) => {
                        *lost == LostWay::ReturnOnly
                    }
                    _ => false,
                };
                let label = format!("{} {:?}", e.node_kind, e.kind);
                direction
                    .entry(label.clone())
                    .or_default()
                    .push((key(e), right(&e.reading.verdict)));
                if e.node_kind == "Analog" {
                    direction_no_optics
                        .entry(label)
                        .or_default()
                        .push((key(e), right(&e.reading_no_optics)));
                }
            }
            DamageKind::ForwardLoss | DamageKind::ReturnLoss => strand_losses_left_out += 1,
            _ => {}
        }
    }
    let all = |m: &BTreeMap<String, Vec<(String, bool)>>| -> Vec<(String, bool)> {
        m.values().flatten().cloned().collect()
    };
    let names = share(&all(&breaks), 1);
    let over = share(&confounder, 2);
    let tells = share(&all(&direction), 3);
    let out_json = serde_json::json!({
        "origin": "Synthetic",
        "note": "Real streets, invented plant, simulated events and telemetry. A pass says the reading reads this sim.",
        "events": events.len(),
        "rule": {
            "names_the_break": {"share": names, "bar": 0.90, "pass": names.share >= 0.90 && names.of >= 200},
            "doesnt_over_call": {"share": over, "bar": 0.10, "pass": over.share <= 0.10 && over.of >= 200},
            "tells_forward_from_return": {"share": tells, "bar": 0.95, "pass": tells.share >= 0.95 && tells.of >= 200},
        },
        "names_the_break_by_kind": breaks.iter().map(|(k, v)| (k.clone(), share(v, 4))).collect::<BTreeMap<_, _>>(),
        "direction_by_kind": direction.iter().map(|(k, v)| (k.clone(), share(v, 5))).collect::<BTreeMap<_, _>>(),
        "direction_analog_no_optics": direction_no_optics.iter().map(|(k, v)| (k.clone(), share(v, 6))).collect::<BTreeMap<_, _>>(),
        "verdicts_by_kind": reported,
        "strand_losses_left_out": strand_losses_left_out,
    });
    std::fs::write(out, serde_json::to_string_pretty(&out_json)?)?;
    Ok(serde_json::to_string_pretty(&out_json["rule"])?)
}

/// `datagen events-gallery <rows.jsonl> <out.md>`: each event kind at its most minor and most total, from the rows: which
/// modems drop and when, what the reading says, and how the spectrum model reads the survivors' curves.
pub fn gallery(args: &[String]) -> Result<String, Error> {
    let [rows, out] = args else {
        return Err("usage: datagen events-gallery <rows.jsonl> <out.md>".into());
    };
    let text = std::fs::read_to_string(rows)?;
    let events: Vec<EventRow> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let mut md = String::from(
        "# Damage events: a gallery\n\nGenerated by `datagen events-gallery` from task 19's test-map rows. **Synthetic:** real \
         streets, invented plant, simulated events and telemetry. For each event kind, the row with the lowest severity and the \
         one with the highest. Counts are modems operational (up), with no downstream (dark), and with downstream but not \
         registered (deaf), at each step in seconds from the onset; the reading is `diagnosis::outage` at 600 s.\n",
    );
    let verdict = |v: &OutageVerdict| match v {
        OutageVerdict::Normal => "normal".to_string(),
        OutageVerdict::PlantBreak {
            element,
            span,
            lost,
        } => format!(
            "a break at `{element}`{}, {lost:?}",
            span.as_ref()
                .map(|s| format!(" (span `{s}`)"))
                .unwrap_or_default()
        ),
        OutageVerdict::SupplyRun { supply } => format!("everything supply `{supply}` feeds"),
        OutageVerdict::Premises { element } => format!("inside the premises at `{element}`"),
        OutageVerdict::NoOnePoint { groups } => format!("no one point ({groups} separate groups)"),
    };
    for kind in DAMAGE_KINDS {
        let mut of: Vec<&EventRow> = events.iter().filter(|e| e.kind == kind).collect();
        if of.is_empty() {
            continue;
        }
        // By severity, then by how many modems went: a cut is always total, and its reach is what differs.
        of.sort_by(|a, b| {
            a.severity
                .total_cmp(&b.severity)
                .then(a.reading.vanished.cmp(&b.reading.vanished))
                .then(a.seed.cmp(&b.seed))
        });
        md.push_str(&format!("\n## {kind:?}\n\n{} events.\n", of.len()));
        let mut picks = vec![("Most minor", of[0])];
        if of.len() > 1 {
            picks.push(("Most total", of[of.len() - 1]));
        }
        for (label, e) in picks {
            let what = e
                .fault
                .as_ref()
                .map(|f| serde_json::to_string(&f.kind))
                .or_else(|| {
                    e.fiber_fault
                        .as_ref()
                        .map(|f| serde_json::to_string(&f.kind))
                })
                .transpose()?
                .unwrap_or_default();
            md.push_str(&format!(
                "\n**{label}** (severity {:.2}, {} modems not operational at the read): {}, {}, {}, node `{}`, {} modems.\n\n`{}`\n\n| t, s | up | dark | deaf |\n|---|---|---|---|\n",
                e.severity,
                e.reading.vanished,
                e.map.as_deref().unwrap_or("generated"),
                e.profile,
                e.design_mode,
                e.node,
                e.modems,
                what
            ));
            for s in &e.steps {
                md.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    s.t, s.up, s.dark, s.deaf
                ));
            }
            md.push_str(&format!(
                "\nRead at 600 s: {}; {} gone, {} outside the point named. Without the optics: {}.",
                verdict(&e.reading.verdict),
                e.reading.vanished,
                e.reading.contradicting.len(),
                verdict(&e.reading_no_optics)
            ));
            if let Some((count, v)) = &e.after {
                md.push_str(&format!(
                    " After the repair, at {} s: {} up, {} dark, {} deaf, read as {}.",
                    count.t,
                    count.up,
                    count.dark,
                    count.deaf,
                    verdict(v)
                ));
            }
            let s = &e.survivors;
            let shapes: Vec<String> = s.shapes.iter().map(|(k, n)| format!("{k} {n}")).collect();
            md.push_str(&format!(
                " Survivors: {} modems, {} captures, {} called not normal{}.\n",
                s.modems,
                s.captures,
                s.not_normal,
                if shapes.is_empty() {
                    String::new()
                } else {
                    format!(" (shapes named: {})", shapes.join(", "))
                }
            ));
        }
    }
    std::fs::write(out, md)?;
    Ok(format!("wrote {out}"))
}
