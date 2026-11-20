//! `sweep <plant.json> <profile.json> <library.json> <n> <seed> <out.jsonl>`: labeled feature rows.

use std::fs::{self, File};
use std::io::BufWriter;
use std::process::ExitCode;

use features::sweep::run_sweep;
use plant_graph::PlantGraph;
use schema::component::ComponentLibrary;
use schema::plant::Plant;
use schema::profile::Profile;

fn run(args: &[String]) -> Result<usize, Box<dyn std::error::Error>> {
    let [plant, profile, library, n, seed, out] = args else {
        return Err("usage: sweep <plant> <profile> <library> <n> <seed> <out.jsonl>".into());
    };
    let plant: Plant = serde_json::from_str(&fs::read_to_string(plant)?)?;
    let profile: Profile = serde_json::from_str(&fs::read_to_string(profile)?)?;
    let library: ComponentLibrary = serde_json::from_str(&fs::read_to_string(library)?)?;
    let graph = PlantGraph::new(&plant, &library, &profile).map_err(|e| format!("{e:?}"))?;
    let mut writer = BufWriter::new(File::create(out)?);
    Ok(run_sweep(&graph, n.parse()?, seed.parse()?, &mut writer)?)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(rows) => {
            println!("{rows} synthetic rows");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
