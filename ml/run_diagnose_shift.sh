#!/usr/bin/env bash
# What ml/diagnose_shift.py reads and runs. Keeps files already written. Run from the repo root. All synthetic.
set -euo pipefail
D=./target/release/datagen
P=.venv/bin/python
M=ml/artifacts/maps_manifest.json
OSM=ml/data/osm
OUT=ml/data/diag
once() { local file=$1; shift; if [ ! -s "$file" ]; then echo "== $file"; "$@"; fi; }
# Moved worlds on the calibrate maps, seed 3. Estimates: 0 tap return loss, 1 active return loss, 6 CIN, 8 modem implementation MER.
for world in "mer_low 8 low" "mer_high 8 high" "tap_low 0 low" "tap_high 0 high" "active_high 1 high" "cin_low 6 low" "varied varied" "mer_minus_10 8 -10" "mer_minus_20 8 -20" "taps_7db 0 -11" "cable_plus_30 9 0.3" "severe severe"; do
  set -- $world; name=$1; shift
  once $OUT/cal_$name.jsonl $D maps-rows $OUT/cal_$name.jsonl "$M" $OSM calibrate 20 3 "$@"
done
# The first of these were a shared error in what we believe. The last five are plant that's really bad, which has to be caught, not forgiven: every
# modem's ceiling 10 or 20 dB down, every tap at 7 dB water-damaged return loss, all cable 30 % lossier, and faults past anything in training.
# Training rows with every estimate drawn across its range. "blind" never moves MER or tap return loss, to ask about an error nobody anticipated.
once $OUT/train_varied.jsonl $D maps-rows $OUT/train_varied.jsonl "$M" $OSM train 40 5 varied
once $OUT/train_varied_blind.jsonl $D maps-rows $OUT/train_varied_blind.jsonl "$M" $OSM train 40 5 varied 8,0
once $OUT/saved.json $P ml/diagnose_shift.py saved $OUT
once $OUT/gbdt.json $P ml/diagnose_shift.py gbdt $OUT
pids=()
for v in relative relative_medians varied varied_blind varied_relative; do
  if [ ! -s $OUT/$v.json ]; then $P ml/diagnose_shift.py $v $OUT > $OUT/$v.log 2>&1 & pids+=($!); fi
done
for p in "${pids[@]}"; do wait "$p"; done
