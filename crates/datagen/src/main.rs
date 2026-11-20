//! `datagen rows <out.jsonl> <designs per family> <rows per node> <floor share> <seed>`: labeled rows and the floor's answers.
//! `datagen sensitivity <out.json> <draws per value> <seed>`: what each estimate does to the answer.
//! `datagen maps-survey|maps-rows|maps-hybrid`: the same on invented plant on real street maps (`maps.rs`). All synthetic (invariant 11).
//! `datagen ab-survey|ab-rows|ab-maps-rows|ab-hybrid|ab-fiber`: the same on version 2 plant (`as_built.rs`, task 24).

mod ab_sensitivity;
mod as_built;
mod events;
mod maps;
mod rows;
mod sensitivity;
mod spectra;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args.first().map(String::as_str) {
        Some("rows") => rows::run(&args[1..]),
        Some("hybrid") => rows::hybrid(&args[1..]),
        Some("sensitivity") => sensitivity::run(&args[1..]),
        Some("maps-survey") => maps::survey(&args[1..]),
        Some("maps-rows") => maps::rows(&args[1..]),
        Some("maps-hybrid") => maps::hybrid(&args[1..]),
        Some("maps-spectra") => spectra::rows(&args[1..]),
        Some("ab-survey") => as_built::survey(&args[1..]),
        Some("ab-rows") => as_built::rows(&args[1..]),
        Some("ab-maps-rows") => as_built::maps_rows(&args[1..]),
        Some("ab-hybrid") => as_built::hybrid(&args[1..]),
        Some("ab-fiber") => as_built::fiber(&args[1..]),
        Some("ab-spectra") => as_built::spectra(&args[1..]),
        Some("ab-sensitivity") => ab_sensitivity::run(&args[1..]),
        Some("maps-events") => events::rows(&args[1..]),
        Some("events-rule") => events::rule(&args[1..]),
        Some("events-gallery") => events::gallery(&args[1..]),
        _ => Err(
            "usage: datagen rows|hybrid|sensitivity|maps-survey|maps-rows|maps-hybrid ...".into(),
        ),
    };
    match outcome {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
