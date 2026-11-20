//! Fetches the areas in `ml/maps.json` into `ml/data/osm/` for training data (`docs/tasks/15-held-out-maps-selection.md`). A file already
//! there is kept, since OSM changes and a run should hold still. © OpenStreetMap contributors, under the ODbL. Any plant on these is invented.
//!
//! `cargo run --release -p mcp-server --example fetch_maps`

use std::fs;
use std::thread::sleep;
use std::time::Duration;

use mcp_server::overpass::{Source, fetch};
use serde::Deserialize;
use twin::tools::AreaArgs;

#[derive(Deserialize)]
struct Area {
    name: String,
    south_deg: f64,
    west_deg: f64,
    north_deg: f64,
    east_deg: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let areas: Vec<Area> = serde_json::from_str(&fs::read_to_string("ml/maps.json")?)?;
    fs::create_dir_all("ml/data/osm")?;
    let source = Source::new(None);
    let mut failed = 0;
    for area in &areas {
        let path = format!("ml/data/osm/{}.json", area.name);
        if fs::metadata(&path).is_ok() {
            println!("{}: already here", area.name);
            continue;
        }
        let args = AreaArgs {
            south_deg: area.south_deg,
            west_deg: area.west_deg,
            north_deg: area.north_deg,
            east_deg: area.east_deg,
            installation: None,
        };
        match fetch(&source, &args) {
            Ok(text) => {
                println!("{}: {} KB", area.name, text.len() / 1024);
                fs::write(&path, text)?;
            }
            Err(error) => {
                println!("{}: FAILED: {error}", area.name);
                failed += 1;
            }
        }
        // A public server, shared with everyone else.
        sleep(Duration::from_secs(20));
    }
    println!("{} areas, {failed} failed", areas.len());
    Ok(())
}
