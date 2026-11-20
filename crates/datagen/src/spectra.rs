//! One modem's capture per row, with labels the sim computes by running each impairment alone against a
//! clean run on one seed (`docs/tasks/17-spectrum-model.md`). Everything is `Synthetic` (invariant 11).
//!
//! `datagen maps-spectra <out.jsonl> <manifest.json> <osm dir> <split> <rows per node> <seed>`.

use std::collections::BTreeMap;

use features::sampler::{FaultGroup, sample_group};
use features::spectrum::{BINS, bin_curve, bin_worst, to_hex, to_quarter_db};
use features::sweep::random_sample;
use plant_graph::PlantGraph;
use schema::common::{ChannelId, ElementId, Origin};
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{Fault, FaultKind, FaultSet, Onset};
use schema::observation::DsRxmerRecord;
use schema::profile::{Profile, Qam};
use schema::spectrum::{Shape, ShapeLabel, SpectrumRow};
use schema::work_order::CauseClass;
use sim::rng::{Rng, hash_str};
use sim::telemetry::{Detail, Scenario, SimOptions, simulate};

use crate::maps::map_jobs;
use crate::rows::{Error, Job, write_lines};
use crate::sensitivity::vary_estimates;

/// A fall this size or more is visible, dB. Below it the scatter hides it.
const VISIBLE_DB: f64 = 1.5;
/// Captures per row: two of the worst hit and two others.
const WORST_MODEMS: usize = 2;
const OTHER_MODEMS: usize = 2;
/// A share of modems sit low in their window, which is healthy and is what makes a suckout deep.
const LOW_MODEM_SHARE: f64 = 0.5;
const LOW_MODEM_DB: f64 = 14.0;
/// The share of rows with an SC-QAM just below the OFDM channel, MHz off its edge.
const NEIGHBOUR_SHARE: f64 = 0.3;
/// Cyclic prefix and roll-off, µs, that PHY allows at Ncp and Nrp's usual settings, with roll-off under prefix.
const CP_ROLLOFF_US: [(f64, f64); 5] = [
    (0.9375, 0.625),
    (1.25, 0.625),
    (2.5, 1.25),
    (3.75, 1.25),
    (5.0, 1.25),
];

/// One run's curve per modem and channel.
type Curves = BTreeMap<(ElementId, ChannelId), DsRxmerRecord>;

fn run(
    graph: &PlantGraph<'_>,
    sample: &EnvironmentSample,
    kinds: &[FaultKind],
    seed: u64,
    scatter_db: f64,
) -> Result<Curves, Error> {
    let environment = Environment {
        schema_version: schema::environment::SCHEMA_VERSION.to_string(),
        samples: vec![sample.clone()],
        ambient_sources: Vec::new(),
    };
    let faults = FaultSet {
        schema_version: schema::fault::SCHEMA_VERSION.to_string(),
        seed,
        faults: kinds
            .iter()
            .cloned()
            .map(|kind| Fault {
                onset: Onset::Instant,
                kind,
            })
            .collect(),
    };
    let steps = simulate(&Scenario {
        graph,
        environment: &environment,
        faults: &faults,
        seed,
        options: SimOptions {
            detail: Detail::Full,
            randomize: Some(seed),
            measurement_noise_db: 0.0,
            rxmer_scatter_db: scatter_db,
        },
        link: None,
    })?;
    let mut curves = Curves::new();
    if let Some(step) = steps.first() {
        for modem in &step.modems {
            for record in modem.rxmer.iter().flatten() {
                curves.insert(
                    (modem.element_id.clone(), record.channel_id.clone()),
                    record.clone(),
                );
            }
        }
    }
    Ok(curves)
}

fn values(record: &DsRxmerRecord) -> Vec<f64> {
    record
        .rxmer_db
        .iter()
        .flatten()
        .map(|v| v.unwrap_or(0.0))
        .collect()
}

/// Per subcarrier, how far `after` sits below `before`, dB.
fn fall(before: &DsRxmerRecord, after: &DsRxmerRecord) -> Vec<f64> {
    values(before)
        .iter()
        .zip(values(after))
        .map(|(b, a)| b - a)
        .collect()
}

fn first_mhz(record: &DsRxmerRecord) -> f64 {
    record.axis.zero_freq_mhz
        + f64::from(record.axis.first_active_index) * record.axis.spacing_khz / 1000.0
}

/// §7.4: the same profile with its boot profile at 256-QAM, as the fixtures have it, or at 64-QAM or
/// 16-QAM, the lower ones [VIAVI2016] names for the worst plant. This spreads how far a modem can fall
/// before dropping off. No source gives a share, so it says nothing about prevalence.
fn with_boot_profile(profile: &Profile, rng: &mut Rng) -> Profile {
    let modulation = [Qam::Qam16, Qam::Qam64, Qam::Qam256][rng.index(3)];
    let mut changed = profile.clone();
    for channel in &mut changed.channel_plan.ofdm {
        for data_profile in &mut channel.data_profiles {
            if data_profile.data_profile_id == 0 {
                data_profile.modulation = modulation;
            }
        }
    }
    changed
}

/// The same profile with an SC-QAM just below the first OFDM channel, and prefix and roll-off as drawn.
fn with_neighbour(profile: &Profile, rng: &mut Rng) -> Option<Profile> {
    let mut changed = profile.clone();
    let plan = &mut changed.channel_plan;
    let mut sc = plan.ds_sc_qam.last()?.clone();
    let first = plan.ofdm.first_mut()?;
    let (cp_us, rolloff_us) = CP_ROLLOFF_US[rng.index(CP_ROLLOFF_US.len())];
    first.cp_us = cp_us;
    first.rolloff_us = rolloff_us;
    sc.channel_id = ChannelId::new("ds-sc-neighbour");
    sc.center_mhz = first.first_active_mhz - rng.range(0.5, 3.0) - sc.width_mhz / 2.0;
    plan.ds_sc_qam.push(sc);
    Some(changed)
}

/// How many impairments a row has. A tenth none, then one, two or three: about 40 % of Comcast's labelled samples hold several.
fn impairment_count(rng: &mut Rng) -> usize {
    let u = rng.uniform();
    if u < 0.1 {
        0
    } else if u < 0.55 {
        1
    } else if u < 0.85 {
        2
    } else {
        3
    }
}

/// Per bin, which shapes show, and each shape's summary, from each impairment's own fall.
fn shape_labels(per_shape: &[(Shape, Vec<f64>)]) -> (Vec<u16>, Vec<ShapeLabel>) {
    let mut bits = vec![0u16; BINS];
    let mut shapes = Vec::new();
    for (shape, worst) in per_shape {
        let bins = worst.iter().filter(|w| **w >= VISIBLE_DB).count();
        if bins > 0 {
            for (bit, w) in bits.iter_mut().zip(worst) {
                if *w >= VISIBLE_DB {
                    *bit |= shape.bit();
                }
            }
            shapes.push(ShapeLabel {
                shape: *shape,
                worst_fall_db: worst.iter().copied().fold(0.0, f64::max),
                bins: bins as u32,
            });
        }
    }
    (bits, shapes)
}

fn draw_weight(shape: Shape) -> usize {
    match shape {
        Shape::Ripple => 4,
        Shape::Wave => 2,
        _ => 1,
    }
}

fn one_row(job: &Job, library: &ComponentLibrary) -> Result<String, Error> {
    capture_rows(
        &job.plant,
        &job.profile,
        library,
        job.row_seed,
        job.map.clone(),
        true,
    )
}

/// One run's capture rows. `low_stand_in` is task 17's stand-in for low modems, a flat bad drop on half of them; version 2
/// plant has premises to spread modems' levels, so task 24's rows leave it out.
pub(crate) fn capture_rows(
    plant: &schema::plant::Plant,
    job_profile: &Profile,
    library: &ComponentLibrary,
    row_seed: u64,
    map: Option<String>,
    low_stand_in: bool,
) -> Result<String, Error> {
    let mut rng = Rng::new(row_seed);
    let world = vary_estimates(library, job_profile, row_seed, &[]);
    let booted = with_boot_profile(job_profile, &mut rng);
    let plain = PlantGraph::new(plant, &world, &booted).map_err(|e| format!("{e:?}"))?;
    let neighbour = (rng.uniform() < NEIGHBOUR_SHARE)
        .then(|| with_neighbour(&booted, &mut rng))
        .flatten();
    let profile = neighbour.as_ref().unwrap_or(&booted);
    let graph = PlantGraph::new(plant, &world, profile).map_err(|e| format!("{e:?}"))?;
    let sample = random_sample(&mut rng, 0);

    // Low modems are part of the healthy plant, so they're in every run.
    let mut background: Vec<FaultKind> = Vec::new();
    for modem in graph.modems().into_iter().filter(|_| low_stand_in) {
        if rng.uniform() < LOW_MODEM_SHARE {
            background.push(FaultKind::BadDrop {
                modem_id: modem.id.clone(),
                extra_loss_db: rng.range(0.0, LOW_MODEM_DB),
                return_loss_db: None,
            });
        }
    }

    let mut available: Vec<Shape> = Shape::ALL
        .iter()
        .copied()
        .filter(|s| *s != Shape::Edge || neighbour.is_some())
        .collect();
    let mut groups: Vec<FaultGroup> = Vec::new();
    for _ in 0..impairment_count(&mut rng) {
        // Ripple and waves only show on some modems, so they're drawn more often to be seen as often.
        let total: usize = available.iter().map(|s| draw_weight(*s)).sum();
        let mut pick = rng.index(total);
        let at = available
            .iter()
            .position(|s| {
                let weight = draw_weight(*s);
                let here = pick < weight;
                pick = pick.saturating_sub(weight);
                here
            })
            .unwrap_or(0);
        let shape = available.remove(at);
        groups.extend(sample_group(&graph, shape, &mut rng));
    }
    let causes: Vec<CauseClass> = {
        let mut causes: Vec<CauseClass> = groups
            .iter()
            .flat_map(|g| g.faults.iter().map(CauseClass::from))
            .collect();
        causes.dedup();
        causes
    };

    let with = |extra: &[FaultKind]| -> Vec<FaultKind> {
        background.iter().chain(extra).cloned().collect()
    };
    let everything: Vec<FaultKind> = groups.iter().flat_map(|g| g.faults.clone()).collect();
    let seed = row_seed;
    let reference = run(&plain, &sample, &with(&[]), seed, 0.0)?;
    let clean = if neighbour.is_some() {
        run(&graph, &sample, &with(&[]), seed, 0.0)?
    } else {
        reference.clone()
    };
    let all = run(&graph, &sample, &with(&everything), seed, 0.0)?;
    let read = run(
        &graph,
        &sample,
        &with(&everything),
        seed,
        rng.range(0.6, 1.0),
    )?;
    let alone: Vec<Curves> = if groups.len() == 1 {
        vec![all.clone()]
    } else {
        groups
            .iter()
            .map(|g| run(&graph, &sample, &with(&g.faults), seed, 0.0))
            .collect::<Result<_, _>>()?
    };

    // The captures: the worst hit, so faults are seen, and some others, so their absence is too.
    let mut hit: Vec<(f64, &(ElementId, ChannelId))> = all
        .iter()
        .filter_map(|(key, after)| {
            let before = reference.get(key)?;
            let worst = fall(before, after).into_iter().fold(0.0, f64::max);
            Some((worst, key))
        })
        .collect();
    hit.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(b.1)));
    let mut chosen: Vec<&(ElementId, ChannelId)> =
        hit.iter().take(WORST_MODEMS).map(|h| h.1).collect();
    let rest: Vec<&(ElementId, ChannelId)> = hit.iter().skip(WORST_MODEMS).map(|h| h.1).collect();
    for k in 0..OTHER_MODEMS.min(rest.len()) {
        let candidate = rest[(rng.next_u64() as usize + k) % rest.len()];
        if !chosen.contains(&candidate) {
            chosen.push(candidate);
        }
    }

    let mut lines = Vec::new();
    for key in chosen {
        let (Some(curve), Some(before), Some(after), Some(base)) = (
            read.get(key),
            reference.get(key),
            all.get(key),
            clean.get(key),
        ) else {
            continue;
        };
        let spacing_khz = curve.axis.spacing_khz;
        let start_mhz = first_mhz(curve);
        let readings: Vec<Option<f64>> = curve.rxmer_db.clone().unwrap_or_default();
        let binned = bin_curve(start_mhz, spacing_khz, &readings);
        let (low, mean) = binned.to_bytes();
        let falls = bin_worst(spacing_khz, &fall(before, after));

        let mut per_shape: Vec<(Shape, Vec<f64>)> = Vec::new();
        if neighbour.is_some() {
            per_shape.push((Shape::Edge, bin_worst(spacing_khz, &fall(before, base))));
        }
        for (group, run_alone) in groups.iter().zip(&alone) {
            if let Some(alone_curve) = run_alone.get(key) {
                per_shape.push((
                    group.shape,
                    bin_worst(spacing_khz, &fall(base, alone_curve)),
                ));
            }
        }
        let (shape_bits, shapes) = shape_labels(&per_shape);
        let bytes_of = |v: &[f64]| -> Vec<u8> { v.iter().map(|x| to_quarter_db(*x)).collect() };
        let row = SpectrumRow {
            schema_version: schema::spectrum::SCHEMA_VERSION.to_string(),
            origin: Origin::Synthetic,
            observation_schema: schema::observation::SCHEMA_VERSION.to_string(),
            fault_schema: schema::fault::SCHEMA_VERSION.to_string(),
            map: map.clone(),
            profile: job_profile.id.clone(),
            service_group: plant.service_group.to_string(),
            modem: key.0.clone(),
            channel: key.1.clone(),
            first_mhz: binned.first_mhz,
            seed: row_seed ^ hash_str(key.0.as_str()),
            causes: causes.clone(),
            worst_fall_db: falls.iter().copied().fold(0.0, f64::max),
            shapes,
            min_hex: to_hex(&low),
            mean_hex: to_hex(&mean),
            fall_hex: to_hex(&bytes_of(&falls)),
            shapes_hex: to_hex(
                &shape_bits
                    .iter()
                    .flat_map(|b| b.to_le_bytes())
                    .collect::<Vec<u8>>(),
            ),
        };
        lines.push(serde_json::to_string(&row)?);
    }
    Ok(lines.join("\n"))
}

/// Spectrum rows on the maps of one split.
pub fn rows(args: &[String]) -> Result<String, Error> {
    let [out, manifest, dir, split, rows_per_node, seed] = args else {
        return Err(
            "usage: datagen maps-spectra <out.jsonl> <manifest.json> <osm dir> <split> <rows per node> <seed>"
                .into(),
        );
    };
    let (jobs, library, _) = map_jobs(
        manifest,
        dir,
        split,
        rows_per_node.parse()?,
        0.0,
        seed.parse()?,
    )?;
    let total = write_lines(&jobs, out, |job| one_row(job, &library))?;
    Ok(format!("{total} synthetic runs of captures in {out}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema::fault::{Location, LossShape};
    use schema::plant::Plant;

    fn read_fixture<T: serde::de::DeserializeOwned>(path: &str) -> T {
        let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(full).unwrap()).unwrap()
    }

    fn spot(plant_edge: &str, pos_ft: f64) -> Location {
        Location::Span {
            span_id: schema::common::EdgeId::new(plant_edge),
            pos_ft,
        }
    }

    /// Labels for one modem on the street: each impairment against a clean run, as a row does it.
    fn labelled(groups: &[(Shape, Vec<FaultKind>)], modem: &str) -> (Vec<u16>, Vec<ShapeLabel>) {
        let plant: Plant = read_fixture("one_street/plant_lowsplit_iccap.json");
        let library: ComponentLibrary = read_fixture("library/generic.json");
        let profile: Profile = read_fixture("profiles/d31_lowsplit_1002_iccap.json");
        let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
        let sample = EnvironmentSample {
            t: schema::common::Timestamp(0),
            temp_c: 20.0,
            ground_temp_c: 20.0,
            rain_mm_per_hr: 0.0,
            solar_w_per_m2: None,
            wind_m_per_s: None,
        };
        let key = (ElementId::new(modem), ChannelId::new("ofdm-1"));
        let clean = run(&graph, &sample, &[], 5, 0.0).unwrap();
        let per_shape: Vec<(Shape, Vec<f64>)> = groups
            .iter()
            .map(|(shape, faults)| {
                let alone = run(&graph, &sample, faults, 5, 0.0).unwrap();
                let falls = fall(&clean[&key], &alone[&key]);
                (*shape, bin_worst(clean[&key].axis.spacing_khz, &falls))
            })
            .collect();
        shape_labels(&per_shape)
    }

    fn suckout(edge: &str, f0_mhz: f64) -> Vec<FaultKind> {
        vec![FaultKind::Suckout {
            location: spot(edge, 5.0),
            f0_mhz,
            depth_db: 40.0,
            width_mhz: 8.0,
        }]
    }

    #[test]
    fn a_fault_on_another_drop_leaves_a_modem_normal() {
        let groups = [(Shape::Dip, suckout("drop-8", 900.0))];
        let (bits, shapes) = labelled(&groups, "modem-2");
        assert!(shapes.is_empty());
        assert!(bits.iter().all(|b| *b == 0));
    }

    #[test]
    fn a_worked_dip_lands_in_its_band() {
        let groups = [(Shape::Dip, suckout("drop-8", 900.0))];
        let (bits, shapes) = labelled(&groups, "modem-8");
        assert_eq!(shapes.len(), 1);
        assert!(
            shapes[0].worst_fall_db > 15.0,
            "{}",
            shapes[0].worst_fall_db
        );
        // 811 MHz is bin 0, so 900 MHz is bin 445.
        assert_ne!(bits[445] & Shape::Dip.bit(), 0);
        assert_eq!(bits[100] & Shape::Dip.bit(), 0);
    }

    #[test]
    fn two_faults_keep_their_own_shapes() {
        let filter = vec![FaultKind::FrequencySelectiveLoss {
            location: spot("drop-8", 5.0),
            shape: LossShape::BandStop,
            f_lo_mhz: 940.0,
            f_hi_mhz: 960.0,
            depth_db: 40.0,
            stop_return_loss_db: 20.0,
            skirt_mhz: None,
        }];
        let groups = [
            (Shape::Dip, suckout("drop-8", 850.0)),
            (Shape::BandLoss, filter),
        ];
        let (bits, shapes) = labelled(&groups, "modem-8");
        assert_eq!(shapes.len(), 2);
        let bin = |f: f64| ((f - 811.0) / 0.2) as usize;
        assert_eq!(bits[bin(850.0)], Shape::Dip.bit());
        assert_eq!(bits[bin(950.0)], Shape::BandLoss.bit());
    }

    /// §7.4: a flat fall takes every channel with it, so past about 16 dB the modem drops off and reports nothing.
    /// A narrow fall doesn't, which is how a curve can show 20 dB and more.
    #[test]
    fn a_modem_too_far_down_stops_reporting() {
        let drop = |extra_loss_db| {
            vec![FaultKind::BadDrop {
                modem_id: ElementId::new("modem-8"),
                extra_loss_db,
                return_loss_db: None,
            }]
        };
        let (_, shapes) = labelled(&[(Shape::LowMer, drop(30.0))], "modem-8");
        assert!(
            shapes[0].worst_fall_db >= 15.0,
            "{}",
            shapes[0].worst_fall_db
        );
        let plant: Plant = read_fixture("one_street/plant_lowsplit_iccap.json");
        let library: ComponentLibrary = read_fixture("library/generic.json");
        let profile: Profile = read_fixture("profiles/d31_lowsplit_1002_iccap.json");
        let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
        let sample = EnvironmentSample {
            t: schema::common::Timestamp(0),
            temp_c: 20.0,
            ground_temp_c: 20.0,
            rain_mm_per_hr: 0.0,
            solar_w_per_m2: None,
            wind_m_per_s: None,
        };
        let gone = run(&graph, &sample, &drop(36.0), 5, 0.0).unwrap();
        assert!(!gone.contains_key(&(ElementId::new("modem-8"), ChannelId::new("ofdm-1"))));
    }

    #[test]
    fn a_robust_boot_profile_keeps_a_curve_through_a_20_db_flat_fall() {
        let plant: Plant = read_fixture("one_street/plant_lowsplit_iccap.json");
        let library: ComponentLibrary = read_fixture("library/generic.json");
        let profile: Profile = read_fixture("profiles/d31_lowsplit_1002_iccap.json");
        let sample = EnvironmentSample {
            t: schema::common::Timestamp(0),
            temp_c: 20.0,
            ground_temp_c: 20.0,
            rain_mm_per_hr: 0.0,
            solar_w_per_m2: None,
            wind_m_per_s: None,
        };
        let mut seen = [0usize; 3];
        for seed in 0..60 {
            let booted = with_boot_profile(&profile, &mut Rng::new(seed));
            let boot = booted.channel_plan.ofdm[0].data_profiles[0].modulation;
            let others_kept = booted.channel_plan.ofdm[0].data_profiles[1..]
                == profile.channel_plan.ofdm[0].data_profiles[1..];
            assert!(others_kept);
            let at = [Qam::Qam16, Qam::Qam64, Qam::Qam256]
                .iter()
                .position(|q| *q == boot)
                .unwrap();
            seen[at] += 1;
        }
        assert!(seen.iter().all(|n| *n > 0), "{seen:?}");

        let on = |boot: Qam, loss: f64| {
            let mut changed = profile.clone();
            changed.channel_plan.ofdm[0].data_profiles[0].modulation = boot;
            let graph = PlantGraph::new(&plant, &library, &changed).unwrap();
            let clean = run(&graph, &sample, &[], 5, 0.0).unwrap();
            let optics = FaultKind::ForwardOptics { cnr_loss_db: loss };
            let after = run(&graph, &sample, &[optics], 5, 0.0).unwrap();
            after
                .iter()
                .flat_map(|(key, record)| fall(&clean[key], record))
                .fold(0.0f64, f64::max)
        };
        // Past the 256-QAM floor nothing reports. On 16-QAM the curve is still there, 20 dB and more down.
        assert_eq!(on(Qam::Qam256, 30.0), 0.0);
        assert!(on(Qam::Qam16, 30.0) >= 20.0);
    }

    #[test]
    fn every_shape_draws_and_the_serious_ones_reach_20_db() {
        let plant: Plant = read_fixture("one_street/plant_lowsplit_iccap.json");
        let library: ComponentLibrary = read_fixture("library/generic.json");
        let profile: Profile = read_fixture("profiles/d31_lowsplit_1002_iccap.json");
        let graph = PlantGraph::new(&plant, &library, &profile).unwrap();
        let sample = EnvironmentSample {
            t: schema::common::Timestamp(0),
            temp_c: 20.0,
            ground_temp_c: 20.0,
            rain_mm_per_hr: 0.0,
            solar_w_per_m2: None,
            wind_m_per_s: None,
        };
        let clean = run(&graph, &sample, &[], 5, 0.0).unwrap();
        for shape in Shape::ALL {
            let mut worst = 0.0f64;
            for seed in 0..40 {
                let mut rng = Rng::new(seed);
                let group = sample_group(&graph, shape, &mut rng).unwrap();
                // A seed gives the same draw.
                assert_eq!(
                    group,
                    sample_group(&graph, shape, &mut Rng::new(seed)).unwrap()
                );
                let after = run(&graph, &sample, &group.faults, 5, 0.0).unwrap();
                for (key, record) in &after {
                    let falls = fall(&clean[key], record);
                    worst = falls.into_iter().fold(worst, f64::max);
                }
            }
            eprintln!("{shape:?} worst {worst:.1}");
            // On modems healthy in level, which is the hardest place to show it: ripple takes a low modem,
            // water nearly gets there, and an edge needs a neighbour this plant doesn't have.
            let reach = match shape {
                Shape::Ripple | Shape::Edge => 0.0,
                Shape::Wave | Shape::LowMer => 15.0,
                _ => 20.0,
            };
            assert!(worst >= reach, "{shape:?} only reaches {worst}");
        }
    }
}
