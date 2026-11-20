//! Invented plant on real street maps, for training and for the held-out test (`docs/tasks/15-held-out-maps-selection.md`). The streets
//! are real, and the plant, faults and telemetry are simulated, so every row is `Synthetic` (invariant 11).
//!
//! `datagen maps-survey <maps.json> <osm dir> <manifest.json>`: which areas meet the rule, without any model looking.
//! `datagen maps-rows <out.jsonl> <manifest.json> <osm dir> <split> <rows per node> <seed>`.
//! `datagen maps-hybrid <out.jsonl> <model.json> <manifest.json> <osm dir> <split> <rows per node> <share> <seed> <floor|nofloor|proposal> [estimate low|high]`.

use std::fs;
use std::sync::Arc;

use design::geo::dist_ft;
use design::osm::read_overpass;
use design::{DesignParts, NodeDesign, design_region};
use schema::component::ComponentLibrary;
use schema::plant::Installation;
use schema::profile::Profile;
use schema::region::{DesignControls, StreetMap};
use serde::{Deserialize, Serialize};
use sim::rng::hash_str;

use crate::rows::{
    Error, Job, Mode, both_profiles, node_jobs, one_hybrid, one_row_in, read, write_lines,
};
use crate::sensitivity::{move_estimate, vary_estimates};

/// The rule for an area, fixed before any model saw one.
pub(crate) const MIN_HOMES: usize = 150;
pub(crate) const MIN_HOMES_PER_STREET_MILE: f64 = 3.0;
/// At least this many `test` areas, or reserves come in.
pub(crate) const MIN_TEST_AREAS: usize = 6;
/// A node this small is a stray piece of street (`docs/tasks/14-map-design.md`), and hardly tests where a fault is.
pub(crate) const MIN_NODE_HOMES: u32 = 10;
const HOMES_PER_NODE: u32 = 120;

#[derive(Deserialize, Serialize, Clone)]
pub(crate) struct Area {
    pub(crate) name: String,
    south_deg: f64,
    west_deg: f64,
    north_deg: f64,
    east_deg: f64,
    pub(crate) split: String,
    pub(crate) installation: Installation,
    pub(crate) reserve: bool,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct ManifestEntry {
    #[serde(flatten)]
    pub(crate) area: Area,
    /// Whether the extract was there to read.
    fetched: bool,
    /// So a later run can tell it's on the same extract. OSM changes.
    pub(crate) extract_hash: String,
    pub(crate) homes: usize,
    pub(crate) street_miles: f64,
    /// Clean nodes of a usable size, per profile.
    clean_nodes: Vec<usize>,
    nodes: Vec<usize>,
    meets_rule: bool,
    pub(crate) used: bool,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Manifest {
    note: String,
    pub(crate) areas: Vec<ManifestEntry>,
}

fn clean(nodes: &[NodeDesign]) -> impl Iterator<Item = &NodeDesign> {
    nodes.iter().filter(|n| {
        n.physics_issues.is_empty()
            && n.topology_issues.is_empty()
            && n.homes_passed >= MIN_NODE_HOMES
    })
}

pub(crate) fn load_map(dir: &str, area: &Area) -> Result<Option<(StreetMap, u64)>, Error> {
    let Ok(text) = fs::read_to_string(format!("{dir}/{}.json", area.name)) else {
        return Ok(None);
    };
    let (map, _) = read_overpass(&area.name, &text, area.installation)?;
    Ok(Some((map, hash_str(&text))))
}

fn controls() -> DesignControls {
    DesignControls {
        homes_per_node: HOMES_PER_NODE,
        seed: 0,
        ..DesignControls::default()
    }
}

/// Reads every area, applies the rule, and brings reserves in if the test split runs short.
pub fn survey(args: &[String]) -> Result<String, Error> {
    let [maps, dir, out] = args else {
        return Err("usage: datagen maps-survey <maps.json> <osm dir> <manifest.json>".into());
    };
    let areas: Vec<Area> = read(maps)?;
    let library: ComponentLibrary = read("fixtures/library/generic.json")?;
    let profiles = both_profiles()?;
    let mut entries = Vec::new();
    for area in areas {
        let mut entry = ManifestEntry {
            area: area.clone(),
            fetched: false,
            extract_hash: String::new(),
            homes: 0,
            street_miles: 0.0,
            clean_nodes: Vec::new(),
            nodes: Vec::new(),
            meets_rule: false,
            used: false,
        };
        if let Some((map, hash)) = load_map(dir, &area)? {
            entry.fetched = true;
            entry.extract_hash = format!("{hash:016x}");
            entry.homes = map.homes.len();
            entry.street_miles = map
                .streets
                .iter()
                .flat_map(|s| s.points.windows(2).map(|p| dist_ft(p[0], p[1])))
                .sum::<f64>()
                / 5280.0;
            for profile in &profiles {
                match design_region(
                    &map,
                    profile,
                    &library,
                    &controls(),
                    &DesignParts::default(),
                ) {
                    Ok(design) => {
                        entry.nodes.push(design.nodes.len());
                        entry.clean_nodes.push(clean(&design.nodes).count());
                    }
                    Err(_) => {
                        entry.nodes.push(0);
                        entry.clean_nodes.push(0);
                    }
                }
            }
            entry.meets_rule = entry.homes >= MIN_HOMES
                && entry.homes as f64 >= entry.street_miles * MIN_HOMES_PER_STREET_MILE
                && entry.clean_nodes.iter().all(|n| *n > 0);
        }
        entries.push(entry);
    }
    let test_kept = entries
        .iter()
        .filter(|e| e.area.split == "test" && !e.area.reserve && e.meets_rule)
        .count();
    let mut reserves_needed = MIN_TEST_AREAS.saturating_sub(test_kept);
    for entry in &mut entries {
        entry.used = entry.meets_rule && !entry.area.reserve;
        if entry.area.reserve && entry.meets_rule && reserves_needed > 0 {
            entry.used = true;
            reserves_needed -= 1;
        }
    }
    let used = entries.iter().filter(|e| e.used).count();
    let manifest = Manifest {
        note: "© OpenStreetMap contributors, under the ODbL. The streets are real and any plant on them is invented.".to_string(),
        areas: entries,
    };
    fs::write(out, serde_json::to_string_pretty(&manifest)?)?;
    Ok(format!("{used} areas used, in {out}"))
}

pub(crate) type MapJobs = (Vec<Job>, ComponentLibrary, [Arc<Profile>; 2]);

pub(crate) fn map_jobs(
    manifest: &str,
    dir: &str,
    split: &str,
    rows_per_node: usize,
    share: f64,
    seed: u64,
) -> Result<MapJobs, Error> {
    let manifest: Manifest = read(manifest)?;
    let library: ComponentLibrary = read("fixtures/library/generic.json")?;
    let profiles = both_profiles()?;
    let mut jobs = Vec::new();
    for entry in manifest
        .areas
        .iter()
        .filter(|e| e.used && e.area.split == split)
    {
        let (map, hash) = load_map(dir, &entry.area)?
            .ok_or_else(|| format!("no extract for {}", entry.area.name))?;
        if format!("{hash:016x}") != entry.extract_hash {
            return Err(format!("{} isn't the extract surveyed", entry.area.name).into());
        }
        for profile in &profiles {
            let design = design_region(
                &map,
                profile,
                &library,
                &controls(),
                &DesignParts::default(),
            )?;
            for node in clean(&design.nodes) {
                node_jobs(
                    &mut jobs,
                    ("Osm", Some(&entry.area.name), &map.id, 0),
                    profile,
                    node,
                    rows_per_node,
                    share,
                    seed,
                );
            }
        }
    }
    Ok((jobs, library, profiles))
}

fn world_of(
    library: &ComponentLibrary,
    profile: &Profile,
    moved: &[String],
) -> Result<ComponentLibrary, Error> {
    match moved {
        [] => Ok(library.clone()),
        [index, end] => Ok(move_estimate(library, profile, index.parse()?, end)?.1),
        _ => Err("an estimate takes an index and low or high".into()),
    }
}

/// Labeled rows on the maps of one split.
pub fn rows(args: &[String]) -> Result<String, Error> {
    let [out, manifest, dir, split, rows_per_node, seed, moved @ ..] = args else {
        return Err("usage: datagen maps-rows <out.jsonl> <manifest.json> <osm dir> <split> <rows per node> <seed> [<estimate> low|high|value | varied [excluded,estimates] | severe]".into());
    };
    let (jobs, library, profiles) = map_jobs(
        manifest,
        dir,
        split,
        rows_per_node.parse()?,
        0.0,
        seed.parse()?,
    )?;
    let total = if let Some(("varied", rest)) = moved.split_first().map(|(a, r)| (a.as_str(), r)) {
        let excluded = rest.first().map_or(Ok(Vec::new()), |list| {
            list.split(',')
                .map(str::parse)
                .collect::<Result<Vec<usize>, _>>()
        })?;
        write_lines(&jobs, out, |job| {
            let world = vary_estimates(&library, &profiles[0], job.row_seed, &excluded);
            one_row_in(job, &library, &world, false)
        })?
    } else if moved.first().is_some_and(|m| m == "severe") {
        write_lines(&jobs, out, |job| one_row_in(job, &library, &library, true))?
    } else {
        let world = world_of(&library, &profiles[0], moved)?;
        write_lines(&jobs, out, |job| one_row_in(job, &library, &world, false))?
    };
    Ok(format!("{total} synthetic rows in {out}"))
}

/// The whole diagnoser on the maps of one split.
pub fn hybrid(args: &[String]) -> Result<String, Error> {
    let [
        out,
        model,
        manifest,
        dir,
        split,
        rows_per_node,
        share,
        seed,
        with_floor,
        moved @ ..,
    ] = args
    else {
        return Err("usage: datagen maps-hybrid <out.jsonl> <model.json> <manifest.json> <osm dir> <split> <rows per node> <share> <seed> <floor|nofloor|proposal> [estimate low|high]".into());
    };
    let mode = Mode::parse(with_floor)?;
    let model = proposal::Proposer::from_json(&fs::read_to_string(model)?)?;
    let (jobs, library, profiles) = map_jobs(
        manifest,
        dir,
        split,
        rows_per_node.parse()?,
        share.parse()?,
        seed.parse()?,
    )?;
    let world = world_of(&library, &profiles[0], moved)?;
    let jobs: Vec<&Job> = jobs.iter().filter(|j| j.run_floor).collect();
    let total = write_lines(&jobs, out, |job| {
        one_hybrid(job, &library, &world, &model, mode)
    })?;
    Ok(format!("{total} synthetic runs in {out}"))
}
