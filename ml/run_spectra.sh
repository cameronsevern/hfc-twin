#!/usr/bin/env bash
# Spectrum rows for task 17, from nothing. Seed 1 trains, seed 3 calibrates, seed 4 tests, as in task 15. All synthetic.
set -euo pipefail
cd "$(dirname "$0")/.."
D=./target/release/datagen
M=ml/artifacts/maps_manifest.json
OSM=ml/data/osm
OUT=ml/data/spectra
mkdir -p "$OUT"
cargo build --release -p datagen
once() { [ -s "$1" ] || { shift; "$@"; }; }
once $OUT/train.jsonl nice $D maps-spectra $OUT/train.jsonl "$M" $OSM train 20 1
once $OUT/calibrate.jsonl nice $D maps-spectra $OUT/calibrate.jsonl "$M" $OSM calibrate 20 3
once $OUT/test.jsonl nice $D maps-spectra $OUT/test.jsonl "$M" $OSM test 20 4
