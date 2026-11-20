//! `docs/physics.md` §12's report, rerun on version 2 plant (task 24): step 12's estimates, and every value task 22 marked
//! approx that a part, an input or the plant holds, each moved in the world one at a time with the diagnoser on nominal
//! values. The ones that live in the sim's own code (indoor temperature, an open port's return loss, a filter's default
//! edge, solar gain) or are a form, not a value (a root holding its set point, the √f plug-in shape, EDFA and node output
//! held constant, no self-heating) can't be moved without changing the sim, and are listed as not moved.
//!
//! `datagen ab-sensitivity <out.json> <draws per class> <seed>`. On `fixtures/one_street_as_built/`, both nodes, with the
//! region's fiber, so the optics' values have something to move. All synthetic (invariant 11).

use std::fs;

use diagnosis::{DiagnoserConfig, diagnose};
use features::DesignReference;
use features::sampler::{AS_BUILT_CLASSES, CLASSES, fault_element, sample_fault_as_built};
use features::sweep::{observe_in, random_sample};
use plant_graph::PlantGraph;
use schema::common::{Origin, Timestamp};
use schema::component::{ComponentLibrary, ComponentSpec};
use schema::environment::AmbientSource;
use schema::fault::{FaultSet, Schedule};
use schema::fiber::FiberPlant;
use schema::plant::{Fitted, Plant};
use schema::profile::{ChannelPlan, Profile};
use schema::work_order::CauseClass;
use serde::Serialize;
use sim::optics::region_links;
use sim::rng::Rng;

use crate::rows::{Error, read};
use crate::sensitivity::ESTIMATES;

/// What the world holds: the parts, both nodes' plant, and the radio around them.
#[derive(Clone)]
struct World {
    library: ComponentLibrary,
    profile_unused: Profile,
    plants: Vec<Plant>,
    ambient: Vec<AmbientSource>,
}

type Move = fn(&mut World, f64);

fn each(lib: &mut ComponentLibrary, f: impl Fn(&mut ComponentSpec)) {
    for c in &mut lib.components {
        f(&mut c.spec);
    }
}

/// Task 22's approx values a part, an input or the plant holds, with the range we'd believe. Ranges are judgement.
const APPROX: [(&str, f64, f64, Move); 7] = [
    (
        "Modem noise figure, dB change (task 22's four generic modems)",
        -2.0,
        3.0,
        |w, d| {
            each(&mut w.library, |s| {
                if let ComponentSpec::Modem(m) = s {
                    m.nf_db += d
                }
            })
        },
    ),
    (
        "Modem transmit ceiling, dB change (task 22's four generic modems)",
        -3.0,
        3.0,
        |w, d| {
            each(&mut w.library, |s| {
                if let ComponentSpec::Modem(m) = s {
                    m.tx_max_total_dbmv += d
                }
            })
        },
    ),
    (
        "Return receiver's lowest input, dB change (rrx-generic-hfc)",
        -3.0,
        3.0,
        |w, d| {
            each(&mut w.library, |s| {
                if let ComponentSpec::OpticalRx(r) = s {
                    r.in_min_dbm += d
                }
            })
        },
    ),
    (
        "Transmitter modulation index, share (rtx-generic-1310-npr's fit)",
        -0.2,
        0.2,
        |w, d| {
            each(&mut w.library, |s| {
                if let ComponentSpec::OpticalTx(t) = s {
                    t.omi_pct *= 1.0 + d
                }
            })
        },
    ),
    (
        "Transmitter RIN, dB change (rtx-generic-1310-npr's fit)",
        -3.0,
        3.0,
        |w, d| {
            each(&mut w.library, |s| {
                if let ComponentSpec::OpticalTx(t) = s {
                    t.rin_db_per_hz += d
                }
            })
        },
    ),
    ("Ambient level, dB change", -10.0, 10.0, |w, d| {
        for a in &mut w.ambient {
            a.level_dbmv_per_mhz += d;
        }
    }),
    (
        "A home's coupling, dB change (the fixture's 60 dB is generic)",
        -20.0,
        20.0,
        |w, d| {
            for plant in &mut w.plants {
                for e in &mut plant.elements {
                    if let Some(Fitted::GroundBlock(g)) = &mut e.fitted {
                        g.coupling_db = (g.coupling_db + d).max(0.0);
                    }
                }
            }
        },
    ),
];

/// Task 22's approx values the sim holds in its own code, or that are a form rather than a value: not moved here.
const NOT_MOVED: [&str; 14] = [
    "INDOOR_TEMP_C 20 °C (sim::cable)",
    "Open port return loss 0 dB (sim::walk)",
    "Filter edge 2 MHz without transition_mhz (sim::passive)",
    "A fitted root holds its set point (sim::walk)",
    "√f plug-in shape (sim::passive)",
    "Fiber loss held flat past a cable's stated wavelengths (sim::optics)",
    "EDFA at constant output (sim::optics)",
    "A pluggable launches mid-range (sim::optics)",
    "Transmitters driven at their reference per 6 MHz; clipping power all in band (sim::optics)",
    "Node RF output held whatever the received power; the composite read at design tilt (sim::walk)",
    "No pickup without a fitted ground block (sim::walk)",
    "SOLAR_GAIN_C_PER_W_M2 0.02; no conductor self-heating; connector couplings linear (sim)",
    "A modem's highest transmit read at the tuner band's bottom; chunked echo synthesis (sim)",
    "Fiber faults read once per scenario (twin)",
];

#[derive(Serialize)]
struct Line {
    estimate: String,
    from: &'static str,
    low: f64,
    high: f64,
    answer_changed: f64,
    accuracy_nominal: f64,
    accuracy_worst: f64,
}

#[derive(Serialize)]
struct Report {
    origin: Origin,
    plant: &'static str,
    /// Faults in the report.
    draws: usize,
    /// Whole draws of every class it rests on, and how many were asked for.
    whole_draws: usize,
    draws_asked: usize,
    method: &'static str,
    lines: Vec<Line>,
    not_moved: Vec<&'static str>,
}

struct Case {
    node: usize,
    faults: FaultSet,
    truth: String,
    class: CauseClass,
    sample: schema::environment::EnvironmentSample,
    seed: u64,
}

type TopAnswer = (Option<CauseClass>, Vec<String>);

fn refs(v: &[(Plant, ChannelPlan)]) -> Vec<(&Plant, &ChannelPlan)> {
    v.iter().map(|(p, c)| (p, c)).collect()
}

/// Each world's links, as the sim reads them with its parts, and the links the diagnoser believes, from nominal parts.
type Links = std::collections::BTreeMap<schema::common::ServiceGroupId, sim::optics::NodeLink>;

fn links_of(fiber: &FiberPlant, profiles: &[Profile], world: &World) -> Result<Links, Error> {
    let pairs: Vec<(Plant, ChannelPlan)> = world
        .plants
        .iter()
        .zip(profiles)
        .map(|(p, pr)| (p.clone(), pr.channel_plan.clone()))
        .collect();
    Ok(region_links(
        fiber,
        &refs(&pairs),
        &world.library,
        &[],
        Timestamp(0),
    )?)
}

/// The diagnoser's top answer on one case in one world, believing nominal values.
fn answer(
    case: &Case,
    profiles: &[Profile],
    nominal: (&World, &Links),
    world: (&World, &Links),
) -> Result<TopAnswer, Error> {
    let ((believed_world, believed_links), (world, seen_links)) = (nominal, world);
    let (plant, believed) = (&world.plants[case.node], &believed_world.plants[case.node]);
    let sg = &believed.service_group;
    let diag = PlantGraph::new(believed, &believed_world.library, &profiles[case.node])
        .map_err(|e| format!("{e:?}"))?;
    let reference = DesignReference::compute_with_link(&diag, believed_links.get(sg))?;
    let real = PlantGraph::new(plant, &world.library, &profiles[case.node])
        .map_err(|e| format!("{e:?}"))?;
    let observed = observe_in(
        &real,
        &reference,
        &case.faults,
        case.sample.clone(),
        &world.ambient,
        case.seed,
        seen_links.get(sg),
    )?;
    let mut at_reading = case.sample.clone();
    at_reading.t = Timestamp(case.sample.t.0 + 300);
    // What the diagnoser believes around the node: its link as designed and the nominal radio.
    let config = DiagnoserConfig {
        link: believed_links.get(sg).cloned(),
        ambient: believed_world.ambient.clone(),
        ..DiagnoserConfig::default()
    };
    let top = diagnose(&diag, &reference, &observed, &at_reading, &config)?;
    Ok(top
        .hypotheses
        .first()
        .map_or((None, Vec::new()), |h| (h.class, h.components.clone())))
}

/// One draw's answers: every world, every case, in a fixed order. Kept per draw so a stopped run keeps whole draws.
#[derive(Serialize, serde::Deserialize)]
struct Draw {
    draw: usize,
    /// Per case: its class and the component it's on.
    cases: Vec<(CauseClass, String)>,
    /// `answers[world][case]`: world 0 is nominal, then each estimate's low end and high end in the list's order.
    answers: Vec<Vec<TopAnswer>>,
}

/// The cases of one draw: draw `d` of every class on both nodes, each seeded from (node, class, draw), so draws are the
/// same whether or not any other draw is run.
fn cases_of(
    d: usize,
    seed: u64,
    plants: &[Plant],
    profiles: &[Profile],
    library: &ComponentLibrary,
) -> Result<Vec<Case>, Error> {
    let mut cases = Vec::new();
    for (node, (plant, profile)) in plants.iter().zip(profiles).enumerate() {
        let graph = PlantGraph::new(plant, library, profile).map_err(|e| format!("{e:?}"))?;
        for class in CLASSES.iter().copied().chain(AS_BUILT_CLASSES) {
            let case_seed = Rng::derive(seed, &[node as u64, class as u64, d as u64]).next_u64();
            let mut rng = Rng::new(case_seed);
            let Some(fault) = sample_fault_as_built(&graph, class, &mut rng) else {
                continue;
            };
            cases.push(Case {
                node,
                truth: fault_element(&fault.kind).unwrap_or_default(),
                class,
                faults: FaultSet {
                    schema_version: schema::fault::SCHEMA_VERSION.to_string(),
                    seed: case_seed,
                    faults: vec![fault],
                },
                sample: random_sample(&mut rng, 0),
                seed: case_seed,
            });
        }
    }
    Ok(cases)
}

/// `datagen ab-sensitivity <out.json> <draws per class> <seed>`. Runs draw by draw, draw 1 of every class first, keeping
/// each whole draw under `<out>.draws/`, and a restart skips the draws already there. With `HFC_STOP_FILE` naming a file
/// that exists, it stops after the draw in hand. The report rests on the whole draws from the first, and says how many.
pub fn run(args: &[String]) -> Result<String, Error> {
    let [out, draws, seed] = args else {
        return Err("usage: datagen ab-sensitivity <out.json> <draws per class> <seed>".into());
    };
    let (draws, seed): (usize, u64) = (draws.parse()?, seed.parse()?);
    let library: ComponentLibrary = read("fixtures/library/generic.json")?;
    let fiber: FiberPlant = read("fixtures/one_street_as_built/fiber.json")?;
    let plants: Vec<Plant> = vec![
        read("fixtures/one_street_as_built/plant_lowsplit_iccap.json")?,
        read("fixtures/one_street_as_built/plant_highsplit_rpd.json")?,
    ];
    let profiles: Vec<Profile> = vec![
        read("fixtures/profiles/d31_lowsplit_1002_iccap.json")?,
        read("fixtures/profiles/d31_highsplit_1218_rpd.json")?,
    ];
    // Nominal radio around the plant: a short-wave band in the return and LTE, at judgement levels.
    let band = |id: &str, lo_mhz: f64, hi_mhz: f64| AmbientSource {
        id: id.into(),
        lo_mhz,
        hi_mhz,
        level_dbmv_per_mhz: -25.0,
        schedule: Schedule {
            hourly_multipliers: [1.0; 24],
        },
    };
    let nominal = World {
        library: library.clone(),
        profile_unused: profiles[1].clone(),
        plants: plants.clone(),
        ambient: vec![
            band("ambient-hf", 5.0, 30.0),
            band("ambient-lte", 698.0, 806.0),
        ],
    };
    // Step 12's estimates through the same world, then task 22's.
    type AnyMove = Box<dyn Fn(&mut World, f64) + Send + Sync>;
    let mut all: Vec<(String, &'static str, f64, f64, AnyMove)> = Vec::new();
    for (name, low, high, change) in ESTIMATES.iter() {
        let change = *change;
        all.push((
            (*name).to_string(),
            "step 12",
            *low,
            *high,
            Box::new(move |w: &mut World, d| change(&mut w.library, &mut w.profile_unused, d)),
        ));
    }
    for (name, low, high, change) in APPROX.iter() {
        let change = *change;
        all.push((
            (*name).to_string(),
            "task 22 approx",
            *low,
            *high,
            Box::new(change),
        ));
    }
    // World 0 is nominal; then each estimate at its low end and its high end.
    let mut worlds = vec![nominal.clone()];
    for (_, _, low, high, change) in &all {
        for end in [*low, *high] {
            let mut world = nominal.clone();
            change(&mut world, end);
            worlds.push(world);
        }
    }
    let world_links: Vec<Links> = worlds
        .iter()
        .map(|w| links_of(&fiber, &profiles, w))
        .collect::<Result<_, _>>()?;

    let dir = format!("{out}.draws");
    fs::create_dir_all(&dir)?;
    let stop_file = std::env::var("HFC_STOP_FILE").ok();
    let stopped = || {
        stop_file
            .as_deref()
            .is_some_and(|f| std::path::Path::new(f).exists())
    };
    for d in 0..draws {
        let path = format!("{dir}/{d:03}.json");
        if std::path::Path::new(&path).exists() {
            continue;
        }
        if stopped() {
            break;
        }
        let cases = cases_of(d, seed, &plants, &profiles, &library)?;
        let jobs: Vec<(usize, usize)> = (0..worlds.len())
            .flat_map(|w| (0..cases.len()).map(move |c| (w, c)))
            .collect();
        let found = crate::as_built::in_parallel(&jobs, |(w, c)| {
            answer(
                &cases[*c],
                &profiles,
                (&worlds[0], &world_links[0]),
                (&worlds[*w], &world_links[*w]),
            )
        })?;
        let answers: Vec<Vec<TopAnswer>> = found
            .chunks(cases.len().max(1))
            .map(<[_]>::to_vec)
            .collect();
        let draw = Draw {
            draw: d,
            cases: cases.iter().map(|c| (c.class, c.truth.clone())).collect(),
            answers,
        };
        let partial = format!("{path}.partial");
        fs::write(&partial, serde_json::to_string(&draw)?)?;
        fs::rename(&partial, &path)?;
        eprintln!("{out}: draw {} of {draws} whole", d + 1);
    }

    // The whole draws from the first, so the ranking is what a run of that many draws gives.
    let mut whole: Vec<Draw> = Vec::new();
    for d in 0..draws {
        match fs::read_to_string(format!("{dir}/{d:03}.json")) {
            Ok(text) => whole.push(serde_json::from_str(&text)?),
            Err(_) => break,
        }
    }
    let cases: Vec<&(CauseClass, String)> = whole.iter().flat_map(|d| &d.cases).collect();
    let in_world = |w: usize| -> Vec<&TopAnswer> {
        whole
            .iter()
            .flat_map(|d| d.answers.get(w).into_iter().flatten())
            .collect()
    };
    let score = |answers: &[&TopAnswer]| -> f64 {
        let placeable: Vec<_> = cases
            .iter()
            .zip(answers)
            .filter(|(c, _)| c.0 != CauseClass::UpstreamIngress)
            .collect();
        placeable.iter().filter(|(c, a)| a.1.contains(&c.1)).count() as f64
            / placeable.len().max(1) as f64
    };
    let nominal_answers = in_world(0);
    let accuracy_nominal = score(&nominal_answers);
    let mut lines = Vec::new();
    for (k, (name, from, low, high, _)) in all.iter().enumerate() {
        let (mut changed, mut runs, mut worst) = (0usize, 0usize, f64::INFINITY);
        for w in [1 + 2 * k, 2 + 2 * k] {
            let answers = in_world(w);
            changed += answers
                .iter()
                .zip(&nominal_answers)
                .filter(|(a, b)| a != b)
                .count();
            runs += answers.len();
            worst = worst.min(score(&answers));
        }
        lines.push(Line {
            estimate: name.clone(),
            from,
            low: *low,
            high: *high,
            answer_changed: changed as f64 / runs.max(1) as f64,
            accuracy_nominal,
            accuracy_worst: worst,
        });
    }
    lines.sort_by(|a, b| {
        b.answer_changed
            .total_cmp(&a.answer_changed)
            .then(a.estimate.cmp(&b.estimate))
    });
    let report = Report {
        origin: Origin::Synthetic,
        plant: "fixtures/one_street_as_built, both nodes, with the region's fiber",
        draws: cases.len(),
        whole_draws: whole.len(),
        draws_asked: draws,
        method: "One estimate moved in the world at a time, with the diagnoser on nominal values. Same faults and seeds at every point. Draw by draw, so the report rests on whole draws.",
        lines,
        not_moved: NOT_MOVED.to_vec(),
    };
    fs::write(out, serde_json::to_string_pretty(&report)?)?;
    Ok(format!(
        "{} estimates ranked over {} synthetic faults, {} of {draws} whole draws, in {out}",
        report.lines.len(),
        report.draws,
        report.whole_draws
    ))
}
