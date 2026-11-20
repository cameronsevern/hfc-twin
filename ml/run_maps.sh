#!/usr/bin/env bash
# Everything docs/tasks/15-held-out-maps-selection.md reports, from nothing. Run from the repo root. Hours on 16 threads; all synthetic.
# A file already written is kept, so a stopped run can carry on. Maps come from `cargo run --release -p mcp-server --example fetch_maps`.
set -euo pipefail
cargo build --release -p datagen --examples
cargo build --release -p datagen
D=./target/release/datagen
P=.venv/bin/python
OSM=ml/data/osm
M=ml/artifacts/maps_manifest.json
OUT=ml/data/maps
mkdir -p "$OUT"
once() { local file=$1; shift; if [ ! -s "$file" ]; then echo "== $file"; "$@"; fi; }

# Seed 1 trains, seed 3 calibrates, seed 4 tests. Seed 2 was step 12's test.
once "$M" $D maps-survey ml/maps.json $OSM "$M"
once ml/data/rows_generated.jsonl $D rows ml/data/rows_generated.jsonl 60 40 0 1
once ml/data/rows_maps_train.jsonl $D maps-rows ml/data/rows_maps_train.jsonl "$M" $OSM train 40 1
once ml/data/rows_maps_calibrate.jsonl $D maps-rows ml/data/rows_maps_calibrate.jsonl "$M" $OSM calibrate 40 3

TRAIN="ml/data/rows_generated.jsonl ml/data/rows_maps_train.jsonl ml/data/rows_maps_calibrate.jsonl"
once ml/artifacts/gbdt_maps.json $P ml/select_maps.py gbdt ml/artifacts $TRAIN
once ml/artifacts/results_maps_gnn.json $P ml/select_maps.py gnn ml/artifacts $TRAIN
once ml/artifacts/results_maps_gnn_folds.json $P ml/select_maps.py gnn-folds ml/artifacts $TRAIN
once ml/artifacts/gnn_latency.json ./target/release/examples/proposal_latency
# Stop here to read the learning curve, while more data can still be added. Nothing above reads the test maps.
if [ "${1:-}" = train ]; then exit 0; fi

GBDT=ml/artifacts/gbdt_maps.json
GNN=ml/artifacts/gnn_maps.json
# The confidence, on maps neither training nor the test saw.
once $OUT/fit_gbdt.jsonl $D maps-hybrid $OUT/fit_gbdt.jsonl $GBDT "$M" $OSM calibrate 40 0.08 3 nofloor
once $OUT/fit_gnn.jsonl $D maps-hybrid $OUT/fit_gnn.jsonl $GNN "$M" $OSM calibrate 40 0.08 3 nofloor
# Scored once. Everything above is frozen by now.
once $OUT/test_gbdt.jsonl $D maps-hybrid $OUT/test_gbdt.jsonl $GBDT "$M" $OSM test 40 0.1 4 floor
once $OUT/test_gnn.jsonl $D maps-hybrid $OUT/test_gnn.jsonl $GNN "$M" $OSM test 40 0.1 4 nofloor
once $OUT/test_shipped_proposal.jsonl $D maps-hybrid $OUT/test_shipped_proposal.jsonl ml/artifacts/proposal_gbdt.json "$M" $OSM test 40 0.1 4 proposal
for f in UrbanGrid Suburban Rural; do
  once $OUT/generated_gbdt_$f.jsonl $D hybrid $OUT/generated_gbdt_$f.jsonl $GBDT $f 10 40 0.08 4 floor
  once $OUT/generated_gnn_$f.jsonl $D hybrid $OUT/generated_gnn_$f.jsonl $GNN $f 10 40 0.08 4 nofloor
done
# Worlds the sim is wrong about: the three that moved answers most in step 12's sensitivity report, through the hybrid.
for e in 0 1 9; do
  for end in low high; do
    once $OUT/world_gbdt_${e}_$end.jsonl $D maps-hybrid $OUT/world_gbdt_${e}_$end.jsonl $GBDT "$M" $OSM test 40 0.02 4 nofloor $e $end
    once $OUT/world_gnn_${e}_$end.jsonl $D maps-hybrid $OUT/world_gnn_${e}_$end.jsonl $GNN "$M" $OSM test 40 0.02 4 nofloor $e $end
  done
done
# Added after the result, so not in the rule: the estimates that hurt the GNN alone most and aren't in the worlds above.
for world in "8 low" "8 high" "6 low"; do
  set -- $world
  once $OUT/worldr_gbdt_$1_$2.jsonl $D maps-hybrid $OUT/worldr_gbdt_$1_$2.jsonl $GBDT "$M" $OSM test 40 0.02 4 nofloor $1 $2
  once $OUT/worldr_gnn_$1_$2.jsonl $D maps-hybrid $OUT/worldr_gnn_$1_$2.jsonl $GNN "$M" $OSM test 40 0.02 4 nofloor $1 $2
done
# Every estimate, the model alone.
for e in 0 1 2 3 4 5 6 7 8 9 10 11 12; do
  for end in low high; do
    once $OUT/worldp_gbdt_${e}_$end.jsonl $D maps-hybrid $OUT/worldp_gbdt_${e}_$end.jsonl $GBDT "$M" $OSM test 40 0.03 4 proposal $e $end
    once $OUT/worldp_gnn_${e}_$end.jsonl $D maps-hybrid $OUT/worldp_gnn_${e}_$end.jsonl $GNN "$M" $OSM test 40 0.03 4 proposal $e $end
  done
done
$P ml/evaluate_maps.py $OUT ml/artifacts
