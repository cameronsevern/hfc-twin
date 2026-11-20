# Task 12 — Training data at scale, model selection and sensitivity

**Build-order step:** 12
**Depends on:** Tasks 04, 07, 08
**Produces:** `crates/proposal` (features per component, an in-process model), `crates/datagen` (labeled rows, the no-ML floor on the same faults, the sensitivity report), `ml/` (training only), and `docs/model-selection.md`.
**Load:** `CLAUDE.md`, invariants 3 and 6; `docs/DESIGN.md` "Model selection"; `docs/physics.md` §12.

## Goal

A proposal model chosen by evidence on layouts it has never seen, beating the no-ML floor, and honest about calibration. The same data says which of our estimates change an answer, which is what to measure first once real data exists.

## Deliverables

1. **`proposal::component_rows`.** One row per candidate place a fault could be: span, amp, drop, supply, root, plus "healthy". Features relative to design and split by what's below the candidate and what isn't.
2. **`datagen`.** Generates designs by family and profile, draws faults, simulates with domain randomization and measurement noise, writes rows and graphs as JSON lines. Runs the no-ML diagnoser on a share of them.
3. **`ml/`, Python, training only (invariant 6).** It reads Rust's files and reimplements no physics. Candidates: GBDT on hand features and a GNN. Held out by family and by profile, never by random sample. It reports top-1 and top-3 accuracy against the floor, class accuracy, calibration error and inference latency.
4. **Export.** The winner as a data file, recording every schema version it trained on (invariant 3). It runs in-process from Rust, with a parity test against Python's own output.
5. **Hybrid.** The diagnoser takes a model's proposals as a prior, tried beside the topology's candidates, and the sim still verifies them.
6. **Sensitivity report.** Each `approx` or `Generic` value is moved in the world while the diagnoser keeps the nominal, on the same faults. Ranked by how often the answer changes.

## Tests

- Rows are deterministic, and reordering plant lists changes nothing.
- Rust scores equal Python's on the parity file.
- The artifact carries its schema versions, and a mismatch is refused.
- The hybrid never returns something it didn't simulate.

## Bar (`docs/DESIGN.md`)

On held-out layout families, the faulty component in the top 3 at least 80 % of the time, beating the no-ML floor, with calibration error under 5 %. A miss gets reported, not tuned around.

## Out of scope

Fine-tuning on real work orders (v2). The temporal layer and encoder-embedding GBDT named in `docs/DESIGN.md` come only if these two can't meet the bar.

## Acceptance

`cargo test --workspace` green, clippy clean, and `docs/model-selection.md` written from a real run.

## Decisions made during the task

The bar was missed on calibration. `docs/model-selection.md` has the numbers.

- **Proposals are tried beside the topology's candidates, not instead.** Trying only the model's top 5 was cheaper, and missed the bar at 74.4 % top 3 on Rural, because anything it left out could never be the answer.
- **Confidence is fitted, not the raw posterior.** A logistic fit of "the top answer is right" over five features of the answer, on runs from other seeds. Held-out numbers use a calibrator that never saw the family it's scored on.
- **Transient faults are read while they last** (`features::sampler::reading_t`). Uniform reading times labelled rows with faults that had already ended.
- **GBDT over GNN, despite the GNN's better family hold-outs.** It holds up on a held-out profile and runs from Rust today. The GNN has no Rust forward pass, so it was never run in the hybrid.
- **A step 8 bug surfaced here and was fixed there.** Generated streets crossed with no shared point, losing homes. All data was regenerated. See `docs/tasks/08-design-and-records.md`.
- **Sensitivity runs on the no-ML diagnoser and the one-street fixture.** Cheap enough to run every estimate at both ends, but it likely understates estimates that grow with plant length.

## Deferred

- GBDT on encoder embeddings, GNN with a temporal layer, a Rust forward pass for the GNN.
- Parameter error, compound-fault recall, hold-outs by density, cascade depth and installation, and the hybrid on profile hold-outs.
