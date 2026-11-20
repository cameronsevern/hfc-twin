//! Rows and runs on a real street map. The streets are real, and the plant on them is invented.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn datagen(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_datagen"))
        .current_dir(repo())
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn lines(path: &PathBuf) -> Vec<serde_json::Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn rows_and_runs_on_a_real_map() {
    let dir = repo().join("target/maps-test");
    fs::create_dir_all(&dir).unwrap();
    let at = |name: &str| dir.join(name);
    let text = |path: &PathBuf| path.to_str().unwrap().to_string();
    let area = serde_json::json!([
        {"name": "real_broomfield_large_co", "south_deg": 39.895, "west_deg": -105.10, "north_deg": 39.91, "east_deg": -105.08,
         "split": "test", "installation": "Aerial", "reserve": false},
        {"name": "not_fetched", "south_deg": 1.0, "west_deg": 1.0, "north_deg": 1.1, "east_deg": 1.1,
         "split": "test", "installation": "Aerial", "reserve": false},
        {"name": "real_broomfield_co", "south_deg": 39.9, "west_deg": -105.095, "north_deg": 39.905, "east_deg": -105.088,
         "split": "train", "installation": "Aerial", "reserve": false}
    ]);
    fs::write(at("maps.json"), area.to_string()).unwrap();
    let manifest = text(&at("manifest.json"));
    datagen(&[
        "maps-survey",
        &text(&at("maps.json")),
        "fixtures/osm",
        &manifest,
    ]);

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    let used = |name: &str| {
        written["areas"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == name)
            .unwrap()["used"]
            .as_bool()
            .unwrap()
    };
    assert!(used("real_broomfield_large_co"));
    assert!(!used("not_fetched"), "there's nothing to read");
    assert!(!used("real_broomfield_co"), "69 homes is under the rule");

    let (first, second) = (at("a.jsonl"), at("b.jsonl"));
    for out in [&first, &second] {
        datagen(&[
            "maps-rows",
            &text(out),
            &manifest,
            "fixtures/osm",
            "test",
            "2",
            "7",
        ]);
    }
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    let rows = lines(&first);
    assert!(rows.len() >= 4);
    for row in &rows {
        assert_eq!(row["origin"], "Synthetic");
        assert_eq!(row["map"], "real_broomfield_large_co");
        assert_eq!(row["family"], "Osm");
    }

    // Spectrum rows: marked synthetic, on a real map, with a capture per modem, and the same bytes twice.
    let (spectra_a, spectra_b) = (at("spectra_a.jsonl"), at("spectra_b.jsonl"));
    for out in [&spectra_a, &spectra_b] {
        datagen(&[
            "maps-spectra",
            &text(out),
            &manifest,
            "fixtures/osm",
            "test",
            "1",
            "7",
        ]);
    }
    assert_eq!(fs::read(&spectra_a).unwrap(), fs::read(&spectra_b).unwrap());
    let captures = lines(&spectra_a);
    assert!(!captures.is_empty());
    for capture in &captures {
        assert_eq!(capture["origin"], "Synthetic");
        assert_eq!(capture["map"], "real_broomfield_large_co");
        assert_eq!(capture["mean_hex"].as_str().unwrap().len(), 960 * 2);
        assert_eq!(capture["shapes_hex"].as_str().unwrap().len(), 960 * 4);
    }

    // The model alone, on either kind of file, in a world the diagnoser is wrong about, on the same faults.
    let proposals = |model: &str, out: &str, moved: &[&str]| {
        let out = text(&at(out));
        let mut args = vec![
            "maps-hybrid",
            &out,
            model,
            &manifest,
            "fixtures/osm",
            "test",
            "2",
            "1",
            "7",
            "proposal",
        ];
        args.extend_from_slice(moved);
        datagen(&args);
        lines(&PathBuf::from(&out))
    };
    let trees = proposals("ml/artifacts/proposal_gbdt.json", "t.jsonl", &[]);
    let graph = proposals("ml/artifacts/gnn_maps.json", "g.jsonl", &[]);
    let moved = proposals("ml/artifacts/gnn_maps.json", "m.jsonl", &["0", "low"]);
    assert_eq!(trees.len(), graph.len());
    assert!(!trees.is_empty());
    for ((t, g), m) in trees.iter().zip(&graph).zip(&moved) {
        assert_eq!(t["seed"], g["seed"]);
        assert_eq!(g["seed"], m["seed"]);
        assert_eq!(g["fault"], m["fault"], "the same fault in either world");
        assert!(!g["proposal_top"].as_array().unwrap().is_empty());
        assert_eq!(g["origin"], "Synthetic");
    }
    assert!(
        graph
            .iter()
            .zip(&moved)
            .any(|(a, b)| a["proposal_probability"] != b["proposal_probability"]),
        "moving an estimate changes what the world reports"
    );
}
