#!/usr/bin/env bash
# Everything docs/tasks/24-as-built-diagnosis-and-selection.md reports, from nothing. Run from the repo root. All synthetic: real streets
# on the maps, invented version 2 plant, simulated faults and telemetry. About 25 hours on 16 threads.
#
# Resumable: a file already written is kept, a long command writes shards under `<file>.shards-*` and skips the ones there, and
# sensitivity keeps whole draws under `<file>.draws`. Every row and run is seeded from its index, so a resumed run writes the same
# bytes. `touch ml/data/as_built/STOP` stops the queue after the shard in hand. Maps come from
# `cargo run --release -p mcp-server --example fetch_maps`; task 15's manifest is ml/artifacts/maps_manifest.json.
#
# Launch it so it outlives the shell that started it:
#   setsid nohup bash -c 'ml/run_as_built.sh > ml/data/as_built/run.log 2>&1; echo $? > ml/data/as_built/run.done' &
#
#   ml/run_as_built.sh rows      the rows only
#   ml/run_as_built.sh train     up to the trained models, before anything reads the test maps
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p datagen --examples
cargo build --release -p datagen
cargo build --release -p proposal --examples
D=./target/release/datagen
P=.venv/bin/python
G=ml/data/venv-gpu/bin/python
OSM=ml/data/osm
A=ml/artifacts/as_built
OUT=ml/data/as_built
M=$A/maps_manifest.json
mkdir -p "$A" "$OUT" "$OUT/spectra"
# A design is derived from its inputs and kept by their hash, so each map is designed once however many commands read it.
export HFC_DESIGN_CACHE=$OUT/designs
export HFC_STOP_FILE=$OUT/STOP
stamp() { date +%s > "$OUT/$1.at"; echo "== $1 at $(date)"; }
stopped() { if [ -e "$HFC_STOP_FILE" ]; then echo "== stopped: $HFC_STOP_FILE is there"; exit 3; fi; }
once() { local file=$1; shift; stopped; if [ ! -s "$file" ]; then echo "== $file at $(date)"; "$@"; fi; }
written() { local file=$1; shift; stopped; if [ ! -s "$file" ]; then echo "== $file"; "$@" > "$file.partial" && mv "$file.partial" "$file"; fi; }
[ -e "$OUT/queue.at" ] || stamp queue

# The counts, written in the spec before anything was generated: 40 rows a node, as task 15's; 29 generated designs per family and
# profile, the fewest that give training at least task 15's 67,600 rows. Seed 1 trains, seed 3 calibrates, seed 4 tests, as task 15's.
DESIGNS=29
once "$M" $D ab-survey ml/artifacts/maps_manifest.json $OSM "$M"
once $OUT/rows_generated.jsonl $D ab-rows $OUT/rows_generated.jsonl $DESIGNS 40 0 1
once $OUT/rows_maps_train.jsonl $D ab-maps-rows $OUT/rows_maps_train.jsonl "$M" $OSM train 40 0 1
once $OUT/rows_maps_calibrate.jsonl $D ab-maps-rows $OUT/rows_maps_calibrate.jsonl "$M" $OSM calibrate 40 0 3
once $OUT/spectra/train.jsonl $D ab-spectra $OUT/spectra/train.jsonl "$M" $OSM train 20 1
once $OUT/spectra/calibrate.jsonl $D ab-spectra $OUT/spectra/calibrate.jsonl "$M" $OSM calibrate 20 3
once $OUT/spectra/test.jsonl $D ab-spectra $OUT/spectra/test.jsonl "$M" $OSM test 20 4
if [ "${1:-}" = rows ]; then exit 0; fi

# Training: task 15's settings sweep, curve and folds, on the calibrate maps; the spectrum CNN under task 17's limit on the GPU, beside
# it. Nothing here reads the test maps. Sensitivity reads no model, so it starts here on the two threads the GNN's sweep leaves, and is
# stopped before the latency is timed on a quiet machine; it finishes last, from the draws it kept.
stamp training
written $A/feature_names.json ./target/release/examples/feature_names
TRAIN="$OUT/rows_generated.jsonl $OUT/rows_maps_train.jsonl $OUT/rows_maps_calibrate.jsonl"
SENS=""
if [ ! -s $A/sensitivity.json ]; then
  HFC_THREADS=2 $D ab-sensitivity $OUT/sensitivity.json 40 1 > $OUT/sensitivity_beside.log 2>&1 &
  SENS=$!
fi
SPECTRUM=""
if [ ! -s $A/spectrum_model.json ]; then
  $G ml/train_spectrum.py $OUT/spectra $A > $OUT/train_spectrum.log 2>&1 &
  SPECTRUM=$!
fi
once $A/gbdt_maps.json $P ml/select_maps.py gbdt $A $TRAIN
once $A/results_maps_gnn.json env HFC_AS_BUILT=1 $P ml/select_maps.py gnn $A $TRAIN
once $A/results_maps_gnn_folds.json env HFC_AS_BUILT=1 $P ml/select_maps.py gnn-folds $A $TRAIN
if [ -n "$SPECTRUM" ]; then wait $SPECTRUM; fi
[ -s $A/spectrum_model.json ] || { echo "== the spectrum model wasn't written: see $OUT/train_spectrum.log"; exit 4; }
# A whole draw is kept; the one in hand is redone when sensitivity runs again.
if [ -n "$SENS" ]; then kill $SENS 2>/dev/null || true; wait $SENS 2>/dev/null || true; fi
once $A/gnn_latency.json ./target/release/examples/ab_latency
if [ "${1:-}" = train ]; then exit 0; fi

# Hybrids: everything a rule reads. Two hours in, a guard compares the rate with the spec's table and stops the queue if the whole plan
# now reads over 30 hours.
[ -e "$OUT/hybrids.at" ] || stamp hybrids
guard() {
  sleep 7200
  local done_cpu=0 n cost f
  # The shards hold everything written so far; a finished file is their concatenation.
  for f in $OUT/fit_*.shards-* $OUT/test_*.shards-* $OUT/world_*.shards-* $OUT/worldp_*.shards-* $OUT/full_*.shards-*; do
    [ -d "$f" ] || continue
    case "$f" in
      *test_shipped*|*worldp_*) cost=0.43 ;;
      *test_gbdt*) cost=150 ;;
      *) cost=78.5 ;;
    esac
    n=$(cat "$f"/*.jsonl 2>/dev/null | wc -l)
    done_cpu=$(awk -v a="$done_cpu" -v n="$n" -v c="$cost" 'BEGIN { print a + n * c }')
  done
  local start=$(cat "$OUT/queue.at") hyb=$(cat "$OUT/hybrids.at") now=$(date +%s)
  # CPU seconds a wall second, so far; the hybrids' remaining work and sensitivity's at that rate.
  awk -v d="$done_cpu" -v s="$start" -v h="$hyb" -v t="$now" 'BEGIN {
    rate = d / (t - h); left = (790079 - d) / rate + 295856 / rate;
    total = (t - s + left) / 3600;
    printf "guard at %s: %.0f CPU s of hybrids in %.0f s, %.1f CPU s a second; the plan now reads %.1f hours\n", t, d, t - h, rate, total;
    exit (total > 30) }' > "$OUT/guard.txt" || { cat "$OUT/guard.txt"; touch "$HFC_STOP_FILE"; echo "guard: over 30 hours, stopping" >> "$OUT/guard.txt"; }
  cat "$OUT/guard.txt"
}
if [ ! -e "$OUT/guard.txt" ]; then guard & GUARD=$!; else GUARD=""; fi
trap '[ -n "${GUARD:-}" ] && kill $GUARD 2>/dev/null || true' EXIT

GBDT=$A/gbdt_maps.json
GNN=$A/gnn_maps.json
# The confidence, on maps neither training nor the test saw.
once $OUT/fit_gbdt.jsonl $D ab-hybrid $OUT/fit_gbdt.jsonl $GBDT "$M" $OSM calibrate 40 0.08 3 nofloor
once $OUT/fit_gnn.jsonl $D ab-hybrid $OUT/fit_gnn.jsonl $GNN "$M" $OSM calibrate 40 0.08 3 nofloor
# Scored once. Everything above is frozen by now.
once $OUT/test_gbdt.jsonl $D ab-hybrid $OUT/test_gbdt.jsonl $GBDT "$M" $OSM test 40 0.1 4 floor
once $OUT/test_gnn.jsonl $D ab-hybrid $OUT/test_gnn.jsonl $GNN "$M" $OSM test 40 0.1 4 nofloor
once $OUT/test_shipped_proposal.jsonl $D ab-hybrid $OUT/test_shipped_proposal.jsonl ml/artifacts/proposal_gbdt.json "$M" $OSM test 40 0.1 4 proposal
# Task 15's worlds: the three estimates that moved answers most in step 12's sensitivity report, at either end.
for e in 0 1 9; do
  for end in low high; do
    once $OUT/world_gbdt_${e}_$end.jsonl $D ab-hybrid $OUT/world_gbdt_${e}_$end.jsonl $GBDT "$M" $OSM test 40 0.02 4 nofloor $e $end
    once $OUT/world_gnn_${e}_$end.jsonl $D ab-hybrid $OUT/world_gnn_${e}_$end.jsonl $GNN "$M" $OSM test 40 0.02 4 nofloor $e $end
  done
done
# Every estimate, the model alone.
for e in 0 1 2 3 4 5 6 7 8 9 10 11 12; do
  for end in low high; do
    once $OUT/worldp_gbdt_${e}_$end.jsonl $D ab-hybrid $OUT/worldp_gbdt_${e}_$end.jsonl $GBDT "$M" $OSM test 40 0.03 4 proposal $e $end
    once $OUT/worldp_gnn_${e}_$end.jsonl $D ab-hybrid $OUT/worldp_gnn_${e}_$end.jsonl $GNN "$M" $OSM test 40 0.03 4 proposal $e $end
  done
done
# One read-out on the fully loaded profiles, which report optics and each port's level. Part of no rule.
once $OUT/full_gbdt.jsonl $D ab-hybrid $OUT/full_gbdt.jsonl $GBDT "$M" $OSM test 40 0.02 4 nofloor full
once $OUT/full_gnn.jsonl $D ab-hybrid $OUT/full_gnn.jsonl $GNN "$M" $OSM test 40 0.02 4 nofloor full

# The fiber's bars, on generated regions of six nodes or more.
once $OUT/fiber.jsonl $D ab-fiber $OUT/fiber.jsonl 20 4
# Step 12's report on version 2 plant with task 22's approx values, draw by draw, from the draws kept beside training.
stamp sensitivity
once $A/sensitivity.json bash -c "$D ab-sensitivity $OUT/sensitivity.json 40 1 && cp $OUT/sensitivity.json $A/sensitivity.json"

stamp evaluation
$P ml/evaluate_as_built.py $OUT $A
$G ml/evaluate_spectrum.py $OUT/spectra $A calibrate
# The decision rule reads one forward pass's latency, measured in Rust on the model as shipped.
once $A/spectrum_latency.json cargo run --release -q -p proposal --example spectrum_read $A
$G ml/evaluate_spectrum.py $OUT/spectra $A test
stamp done
