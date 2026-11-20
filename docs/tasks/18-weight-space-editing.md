# Task 18 — Editing a trained model's weights

**Build-order step:** after 17, as tasks 16 and 17 left it ("a harness for an LLM to edit trained weights, with results on
`calibrate` maps only, test maps sealed, and each edited model a new versioned artifact with its edit log").
**Depends on:** Task 17
**Produces:** a harness that reads, edits and scores task 17's spectrum model by hand, an edit log, and whether editing beat training.
**Load:** `CLAUDE.md`; `docs/tasks/17-spectrum-model.md` and its decisions; `docs/model-selection.md` (task 17's section).

## Goal

Task 17's spectrum model, a dilated 1-D CNN under 200,000 weights, missed its rule on two parts: `LowMer` F1 0.697 (bar 0.70 for every
shape) and a fall error of 2.21 dB (bar 2 dB); on `calibrate` it had been 0.707 and 2.14 dB. Training stopped there on purpose rather
than chase the bar. This task asks a narrower question: can an editor that reads the weights and the model's mistakes, and changes
the weights directly, fix what training left, without retraining and without the test maps?

**Since this was planned, task 24 retrained the same network on as-built plant, and that model passes all ten parts of task 17's
rule** (`ml/artifacts/as_built/spectrum_model.json`: lowest shape `Ripple` at 0.760, fall error 1.07 dB). So there are two targets,
and which one is the user's call:

- **Task 17's model**, on version 1 plant, which still fails two parts: the question as first asked. Its `test` maps were read once
  for task 17, so the result is `calibrate` only.
- **Task 24's model**, which passes: whether editing can move its weakest part (`Ripple` F1 0.760, the fall error) without costing
  another. A different, smaller question.

All of it is synthetic (invariant 11). Python is allowed here: this is ML evaluation (invariant 6), and it reads only the Rust-written
rows and the Rust-checked artifact.

## Defaults, to be confirmed by the user

These are choices only the user can make. The harness is built either way; the campaign waits for the first answer.

| Question | Default |
|---|---|
| Which model | None: this is asked before any edit, since the two targets answer different questions |
| Who edits | This Claude Code session, through the harness's commands: no API calls, no spend beyond the session |
| How many edits | At most 20 applied edits, each scored on `calibrate`, and a halt when three in a row don't improve |
| Test maps | Sealed. Nothing in this task reads them, so its results are `calibrate` results and say so |

## The harness

`ml/weight_edit.py`, four commands, each reading `ml/data/spectra/calibrate.jsonl` (never `test`) and an artifact:

- **`inspect ARTIFACT`**: every tensor's shape and statistics, and per layer the channels that fire most on the model's worst
  `calibrate` captures for each failing part (false `LowMer` calls, missed ones, the bins with the largest fall error).
- **`errors ARTIFACT`**: the rule's parts on `calibrate`, per shape and severity, and the worst captures by id.
- **`apply ARTIFACT EDIT.json`**: an edit is a list of operations on named tensors, each `set`, `scale` or `add` on a slice, or a
  rank-one `outer` update, with a rationale in words. It writes a new artifact beside the old one with `parent`, the edit and its
  rationale appended to `edit_log`, and the same schema fields, so `proposal::spectrum` reads it unchanged.
- **`score ARTIFACT`**: the rule's parts on `calibrate` against the parent's, and whether the edit is kept.

An edited artifact is a new version, never written over its parent (invariant 3). Rust and Python agree on it to 1e-9, as task 17's
parity file requires.

## Decision rule, fixed before any result

On `calibrate` maps, the only maps read. An edited model **beats training** if, against task 17's model:

| Part | Bar |
|---|---|
| `LowMer` F1 | At least 0.70 |
| Fall error | At most 2.0 dB on bins that fall at least 3 dB |
| Everything else in task 17's rule | No part worse by more than its noise: 0.5 points of a share, 0.01 of an F1, 0.05 dB |
| Runs | Same bytes twice, parity to 1e-9, under 200,000 weights |

A pass says editing fixed the two parts on `calibrate`. It says nothing about `test`, which stays sealed, and `calibrate` chose task
17's thresholds, so the two aren't independent.

## Deliverables

1. `ml/weight_edit.py` with the four commands, and tests on a toy artifact: an edit round-trips, the log grows, `test` is refused.
2. The edit campaign's artifacts under `ml/artifacts/edits/`, each with its log.
3. A parity file for the last edited artifact, and `proposal::spectrum` reading it.
4. `docs/model-selection.md`: a section from the real campaign, whichever way it goes.

## Out of scope

Retraining. Wiring the model into the twin. The GBDT and GNN, whose weights aren't a network an editor can read the same way.

## Decisions made during the task

### The model, chosen 2026-11-19

The user left the call to this session. **Task 17's model** (`ml/artifacts/spectrum_model.json`, version 1 plant), for three
reasons: it's the question the task was planned to ask, since it's the model with parts that fail; its data still stands, since the
version 1 designs held their pin through task 25; and task 24's model was trained on version 2 designs that task 25 has since
changed, so its `calibrate` rows no longer describe what the designer builds. Task 24's model stays as it is.

### The rule, amended before any edit was scored

Written after reading task 17's recorded results and before the harness scored anything. What it changes, and why:

- **On `calibrate` the parent already meets `LowMer`** (F1 0.707 against 0.70; it missed only on `test`). So on `calibrate` the one
  part to fix is the fall error, 2.138 dB against 2.0. `LowMer` at least 0.70 stays a bar an edit must not break, and an edit
  isn't credited with fixing it.
- **The fall error has a blind side.** The rule reads it only on bins that fall at least 3 dB, so a rescale could buy it by moving
  error onto bins under 3 dB. Added: on present bins that fall less than 3 dB, the error is no worse than the parent's by more than
  0.05 dB.
- **Thresholds are chosen again for each edit,** with task 17's `choose()` on `calibrate`, as a trained model's are; that's the
  score the rule reads. The score with the parent's thresholds held is reported beside it.
- **`calibrate` alone can't say editing beat training.** Twenty edits picked on `calibrate`, whose thresholds were chosen there too,
  are a fit to `calibrate`. Task 17's decisions already name the remedy: score a new fit on task 15's three reserve areas, which no
  model has read (Greensboro, Bend and Omaha; `ml/artifacts/edits/reserve_manifest.json`, a copy of the manifest with them as split
  `reserve`). Captures there come from today's `datagen maps-spectra` at seed 5, 20 rows a node, as task 17's splits were made.
  **One paired read, once:** the parent and the final edit on the same captures, each with its own `calibrate` thresholds. The harness
  refuses a second read.
- **Editing beats training** if the final edit meets the bars on `calibrate` (the fall error at most 2.0 dB, `LowMer` at least 0.70,
  the guard above, nothing else worse than its noise) **and** on the reserve read its fall error is lower than the parent's by more
  than 0.05 dB with no other part worse than the parent's by more than its noise. Otherwise it doesn't, and that's the result.
- **Whether the reserve numbers compare with task 17's `test` numbers** depends on whether today's generator still writes task 17's
  rows: `calibrate` is regenerated at seed 3 and compared with the stored file, schema version strings aside. If it differs, the
  reserve read is paired only.
- **`says_dont_know` is inherited.** It comes from the separate `no_bandloss` fit, which no edit touches.
- **Each kept edit says what it changed:** the head alone (a readout recalibration) or weights inside the network. A result carried by
  the head alone is reported as that.

### What happened (2026-11-19)

**Editing didn't beat training, by the rule above.** Two edits were applied and one kept. Edit 2 meets the bars on `calibrate`
(fall error 1.969 dB, `LowMer` 0.706, nothing worse than its noise). On the reserve read, its fall error is 0.170 dB lower than the
parent's (2.007 against 2.177 dB), the same gain as on `calibrate`, but healthy false alarms rise 0.59 points (1.52 % against 0.92 %
of 5,082), past the 0.5 allowed. `docs/model-selection.md` has the tables.

- **The harness reproduces task 17 from the JSON alone** (fall error 2.1385 dB, `LowMer` 0.7069, the same thresholds) and writes
  only under `ml/artifacts/edits/`. Its tests (`ml/test_weight_edit.py`, standard library `unittest`) cover a round trip, the log, the
  test maps refused, the reserve read refused a second time, an edit with no reason refused, and a parity file.
- **Today's generator writes task 17's `calibrate` rows unchanged:** all 18,367, schema version strings aside. So the reserve numbers
  compare with task 17's `test` numbers.
- **`LowMer` got no edits.** On `calibrate` it already met the bar, and 83 % of its 291 misses are flat curves, which task 17's
  labels let read as either `LowMer` or healthy: a label ceiling, not something weights can fix.
- **The fall readout alone can't do it.** It's linear, and the error is an under-call of 3 to 10 dB falls next to bins under 3 dB that
  it reads well. A scale and offset reach 2.125 dB, and a refit of its 65 numbers about 2.04 dB at the guard.
- **Both edits are fitted, inside the network, and say so.** Twelve of the mix layer's units never fire; eight became one correction
  unit whose 129 numbers were fitted on `calibrate`'s errors, with nonnegative weights over the eight chosen so the shape logits move
  as little as they can. Edit 1 raised healthy false alarms past noise and wasn't kept; edit 2 refitted it with a penalty on healthy
  bins read over 2.5 dB. Edits by hand on top of it (scale and offset of the fall output) measured no better on `calibrate`, so none
  was applied and the campaign stopped at 2 of 20.
- **Edits are numbered across the campaign,** kept or not, and never written over; a kept edit's parent is the last kept artifact.
- **The reserve areas are spent for this question.** Another attempt needs held-out maps nobody has read.
