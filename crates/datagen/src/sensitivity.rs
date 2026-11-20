//! `docs/physics.md` §12: moves one estimate in the world at a time, keeps the diagnoser on the nominal value, and counts how often the
//! answer changes. Which is the list of what to measure first once real data exists.

use std::fs;

use diagnosis::{DiagnoserConfig, diagnose};
use features::DesignReference;
use features::sampler::{CLASSES, sample_fault};
use features::sweep::{observe, random_sample};
use plant_graph::PlantGraph;
use schema::common::{Origin, Timestamp};
use schema::component::{ComponentLibrary, ComponentSpec};
use schema::fault::FaultSet;
use schema::plant::Plant;
use schema::profile::Profile;
use schema::work_order::CauseClass;
use serde::Serialize;
use sim::rng::Rng;

type Error = Box<dyn std::error::Error + Send + Sync>;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, Error> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

type Change = fn(&mut ComponentLibrary, &mut Profile, f64);

fn each_active(lib: &mut ComponentLibrary, f: impl Fn(&mut schema::component::ActiveSpec)) {
    for c in &mut lib.components {
        match &mut c.spec {
            ComponentSpec::Amp(a) => f(a),
            ComponentSpec::Node(n) => f(&mut n.active),
            ComponentSpec::Rpd(r) => f(&mut r.active),
            _ => {}
        }
    }
}

fn each_tap(lib: &mut ComponentLibrary, f: impl Fn(&mut schema::component::TapSpec)) {
    for c in &mut lib.components {
        if let ComponentSpec::Tap(t) = &mut c.spec {
            f(t);
        }
    }
}

fn each_modem(lib: &mut ComponentLibrary, f: impl Fn(&mut schema::component::ModemSpec)) {
    for c in &mut lib.components {
        if let ComponentSpec::Modem(m) = &mut c.spec {
            f(m);
        }
    }
}

fn each_cable(lib: &mut ComponentLibrary, f: impl Fn(&mut schema::component::CableSpec)) {
    for c in &mut lib.components {
        if let ComponentSpec::Cable(s) | ComponentSpec::Drop(s) = &mut c.spec {
            f(s);
        }
    }
}

/// Each estimate, the range we'd believe, and how to apply it.
pub(crate) const ESTIMATES: [(&str, f64, f64, Change); 13] = [
    ("Tap return loss, dB change", -4.0, 6.0, |l, _, d| {
        each_tap(l, |t| t.return_loss_db += d)
    }),
    ("Active return loss, dB change", -4.0, 6.0, |l, _, d| {
        each_active(l, |a| a.return_loss_db += d)
    }),
    ("Modem return loss, dB change", -4.0, 6.0, |l, _, d| {
        each_modem(l, |m| m.return_loss_db += d)
    }),
    ("Tap port isolation, dB change", -6.0, 6.0, |l, _, d| {
        each_tap(l, |t| t.port_isolation_db += d)
    }),
    ("Forward noise figure, dB change", -2.0, 3.0, |l, _, d| {
        each_active(l, |a| a.nf_db += d)
    }),
    ("Return noise figure, dB change", -4.0, 2.0, |l, _, d| {
        each_active(l, |a| a.return_nf_db += d)
    }),
    (
        "CIN at reference, dB change (BLE120 suggests about −10)",
        -10.0,
        4.0,
        |l, _, d| each_active(l, |a| a.cin_ref_db += d),
    ),
    ("AGC range, dB change", -3.0, 3.0, |l, _, d| {
        each_active(l, |a| a.agc_range_db = (a.agc_range_db + d).max(0.0))
    }),
    (
        "Modem implementation MER, dB change",
        -3.0,
        3.0,
        |l, _, d| each_modem(l, |m| m.impl_mer_db += d),
    ),
    ("Hardline and drop loss, share", -0.1, 0.1, |l, _, d| {
        each_cable(l, |c| {
            c.atten_a *= 1.0 + d;
            c.atten_b *= 1.0 + d;
        })
    }),
    (
        "Cable temperature coefficient, per °C change",
        -0.0005,
        0.0005,
        |l, _, d| each_cable(l, |c| c.k_temp += d),
    ),
    ("Loop resistance, share", -0.1, 0.25, |l, _, d| {
        each_cable(l, |c| c.r_loop_ohm_per_kft *= 1.0 + d)
    }),
    (
        "Active power draw, share (BLE120 suggests −60 %)",
        -0.6,
        0.2,
        |l, _, d| each_active(l, |a| a.power_w *= 1.0 + d),
    ),
];

/// Moves one estimate to one end of its range, for a world that differs from what the diagnoser believes. Named by its place in the
/// ranking's list, and `low` or `high`.
pub fn move_estimate(
    library: &ComponentLibrary,
    profile: &Profile,
    index: usize,
    end: &str,
) -> Result<(String, ComponentLibrary), Error> {
    let (name, low, high, change) = ESTIMATES
        .get(index)
        .ok_or_else(|| format!("no estimate {index}"))?;
    let at = match end {
        "low" => *low,
        "high" => *high,
        other => other
            .parse::<f64>()
            .map_err(|_| format!("{other} isn't low, high or a value"))?,
    };
    let (mut world, mut unused) = (library.clone(), profile.clone());
    change(&mut world, &mut unused, at);
    Ok(((*name).to_string(), world))
}

/// Every estimate drawn across its range, one world per seed, except the ones named. Neither the sensitivity report nor a fixed world:
/// this is what "we don't know these numbers" looks like as training data.
pub fn vary_estimates(
    library: &ComponentLibrary,
    profile: &Profile,
    seed: u64,
    excluded: &[usize],
) -> ComponentLibrary {
    let mut rng = sim::rng::Rng::derive(seed, &[0x7761_7279]);
    let (mut world, mut unused) = (library.clone(), profile.clone());
    for (index, (_, low, high, change)) in ESTIMATES.iter().enumerate() {
        let at = rng.range(*low, *high);
        if !excluded.contains(&index) {
            change(&mut world, &mut unused, at);
        }
    }
    world
}

#[derive(Serialize)]
struct Line {
    estimate: String,
    low: f64,
    high: f64,
    /// Share of runs where the top answer differs from nominal, at either end.
    answer_changed: f64,
    /// Top-component accuracy at nominal.
    accuracy_nominal: f64,
    /// At the worse end.
    accuracy_worst: f64,
}

#[derive(Serialize)]
struct Report {
    origin: Origin,
    profile: String,
    draws: usize,
    method: &'static str,
    lines: Vec<Line>,
}

struct Case {
    faults: FaultSet,
    truth: String,
    class: CauseClass,
    sample: schema::environment::EnvironmentSample,
    seed: u64,
}

fn top(answer: &diagnosis::Diagnosis) -> (Option<CauseClass>, Vec<String>) {
    answer
        .hypotheses
        .first()
        .map_or((None, Vec::new()), |h| (h.class, h.components.clone()))
}

/// The diagnoser's top answer: its cause and where.
type TopAnswer = (Option<CauseClass>, Vec<String>);

fn run_cases(
    cases: &[Case],
    plant: &Plant,
    nominal: (&ComponentLibrary, &Profile),
    world: (&ComponentLibrary, &Profile),
) -> Result<Vec<TopAnswer>, Error> {
    let diag_graph = PlantGraph::new(plant, nominal.0, nominal.1).map_err(|e| format!("{e:?}"))?;
    let diag_reference = DesignReference::compute(&diag_graph)?;
    let world_graph = PlantGraph::new(plant, world.0, world.1).map_err(|e| format!("{e:?}"))?;
    let config = DiagnoserConfig::default();
    let mut out = Vec::new();
    for case in cases {
        // The world is as changed. The reference and hypotheses are what we believe.
        let observed = observe(
            &world_graph,
            &diag_reference,
            &case.faults,
            case.sample.clone(),
            case.seed,
        )?;
        let mut at_reading = case.sample.clone();
        at_reading.t = Timestamp(case.sample.t.0 + 300);
        let answer = diagnose(
            &diag_graph,
            &diag_reference,
            &observed,
            &at_reading,
            &config,
        )?;
        out.push(top(&answer));
    }
    Ok(out)
}

pub fn run(args: &[String]) -> Result<String, Error> {
    let [out, draws, seed] = args else {
        return Err("usage: datagen sensitivity <out.json> <draws per class> <seed>".into());
    };
    let draws: usize = draws.parse()?;
    let seed: u64 = seed.parse()?;
    let plant: Plant = read("fixtures/one_street/plant_highsplit_rpd.json")?;
    let library: ComponentLibrary = read("fixtures/library/generic.json")?;
    let profile: Profile = read("fixtures/profiles/d31_highsplit_1218_rpd.json")?;
    let graph = PlantGraph::new(&plant, &library, &profile).map_err(|e| format!("{e:?}"))?;
    let mut cases = Vec::new();
    for class in CLASSES {
        for d in 0..draws {
            let case_seed = Rng::derive(seed, &[class as u64, d as u64]).next_u64();
            let mut rng = Rng::new(case_seed);
            let Some(fault) = sample_fault(&graph, class, &mut rng) else {
                continue;
            };
            let truth = features::sampler::fault_element(&fault.kind).unwrap_or_default();
            cases.push(Case {
                faults: FaultSet {
                    schema_version: schema::fault::SCHEMA_VERSION.to_string(),
                    seed: case_seed,
                    faults: vec![fault],
                },
                truth,
                class,
                sample: random_sample(&mut rng, 0),
                seed: case_seed,
            });
        }
    }
    let score = |answers: &[TopAnswer]| -> f64 {
        let placeable: Vec<_> = cases
            .iter()
            .zip(answers)
            .filter(|(c, _)| c.class != CauseClass::UpstreamIngress)
            .collect();
        placeable
            .iter()
            .filter(|(c, a)| a.1.contains(&c.truth))
            .count() as f64
            / placeable.len().max(1) as f64
    };
    let nominal = run_cases(&cases, &plant, (&library, &profile), (&library, &profile))?;
    let accuracy_nominal = score(&nominal);

    let lines: Vec<Result<Line, Error>> = std::thread::scope(|scope| {
        let handles: Vec<_> = ESTIMATES
            .iter()
            .map(|(name, low, high, change)| {
                let (cases, plant, library, profile, nominal, score) =
                    (&cases, &plant, &library, &profile, &nominal, &score);
                scope.spawn(move || -> Result<Line, Error> {
                    let mut changed = 0usize;
                    let mut runs = 0usize;
                    let mut worst = f64::INFINITY;
                    for end in [*low, *high] {
                        let (mut l, mut p) = (library.clone(), profile.clone());
                        change(&mut l, &mut p, end);
                        let answers = run_cases(cases, plant, (library, profile), (&l, &p))?;
                        changed += answers.iter().zip(nominal).filter(|(a, b)| a != b).count();
                        runs += answers.len();
                        worst = worst.min(score(&answers));
                    }
                    Ok(Line {
                        estimate: (*name).to_string(),
                        low: *low,
                        high: *high,
                        answer_changed: changed as f64 / runs.max(1) as f64,
                        accuracy_nominal,
                        accuracy_worst: worst,
                    })
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Err("a worker panicked".into())))
            .collect()
    });
    let mut lines = lines.into_iter().collect::<Result<Vec<_>, _>>()?;
    lines.sort_by(|a, b| {
        b.answer_changed
            .total_cmp(&a.answer_changed)
            .then(a.estimate.cmp(&b.estimate))
    });
    let report = Report {
        origin: Origin::Synthetic,
        profile: profile.id.to_string(),
        draws: cases.len(),
        method: "One estimate moved in the world at a time, with the diagnoser on nominal values. Same faults and seeds at every point.",
        lines,
    };
    fs::write(out, serde_json::to_string_pretty(&report)?)?;
    Ok(format!(
        "{} estimates ranked over {} synthetic faults, in {out}",
        report.lines.len(),
        report.draws
    ))
}
