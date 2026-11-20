#!/usr/bin/env bash
# Trains the spectrum CNN and chooses its thresholds. Run it after run_spectra.sh. Waits for GPU memory. Doesn't read the test maps.
set -euo pipefail
cd "$(dirname "$0")/.."
for split in train calibrate test; do
    [ -s "ml/data/spectra/$split.jsonl" ] || { echo "no $split rows: run ml/run_spectra.sh first" >&2; exit 1; }
done
P=ml/data/venv-gpu/bin/python
$P ml/train_spectrum.py ml/data/spectra ml/artifacts
$P ml/evaluate_spectrum.py ml/data/spectra ml/artifacts calibrate
