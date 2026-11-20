#!/usr/bin/env bash
# Everything `docs/model-selection.md` reports, from nothing. Run from the repo root. A bit over two hours on 16 threads; all synthetic.
set -euo pipefail
cargo build --release -p datagen
D=./target/release/datagen
P=.venv/bin/python
mkdir -p ml/data/fit ml/data/test
$D rows ml/data/rows.jsonl 10 40 0 1
$P ml/train.py ml/data/rows.jsonl ml/artifacts
$P ml/train_gnn.py ml/data/rows.jsonl ml/artifacts 8 | tail -12
# Seed 1 fits the confidence; seed 2's layouts, which nothing has seen, score it.
for f in UrbanGrid Suburban Rural; do
  $D hybrid ml/data/fit/hybrid_$f.jsonl ml/artifacts/gbdt_holdout_$f.json $f 10 40 0.15 1 nofloor
  $D hybrid ml/data/test/hybrid_$f.jsonl ml/artifacts/gbdt_holdout_$f.json $f 10 40 0.15 2 floor
done
$P ml/evaluate_hybrid.py ml/artifacts/results_hybrid.json ml/data/fit ml/data/test ml/artifacts/proposal_gbdt.json
$D sensitivity ml/artifacts/sensitivity.json 40 1
