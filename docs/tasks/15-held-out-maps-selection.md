# Task 15 — Model selection on held-out maps

**Build-order step:** after 14; reopens the choice step 12 made, on a test closer to deployment.
**Depends on:** Tasks 12, 14
**Produces:** a GNN forward pass in Rust, training data on real street maps, a held-out map test of both models through the hybrid, and an updated `docs/model-selection.md`.
**Load:** `CLAUDE.md`; `docs/model-selection.md`; `docs/tasks/12-training-and-model-selection.md`.

## Goal

Choose the proposal model on the question a deployment asks: on profiles it trained on, how does it do on streets it has never seen?

Step 12 rejected the GNN partly because it failed on a held-out profile. The sim can train on a profile before that equipment goes live, so that hold-out isn't what a rollout faces. Unseen layouts are, and step 14 lets us build plant on real street maps, so those are now the test.

Everything here is synthetic: the streets are real, the plant and telemetry are invented (invariant 11). This says nothing about real plant. The first real plant is still the final held-out set (`docs/DESIGN.md`).

## The split, fixed before any result

Areas are listed in `ml/maps.json`, each with a split, a reserve flag and an installation. A metro is in one split only, so neighbouring streets can't leak.

| Split | Used for | Contents |
|---|---|---|
| `train` | Fitting both models | Generated layouts of all three families, both profiles, and the `train` maps, both profiles |
| `calibrate` | Temperature, model settings, learning curve and the calibrator | Its maps, both profiles |
| `test` | Scored once, after every setting and the calibrator are frozen | Its maps, both profiles |

An area is used only if it has at least 150 homes and at least 3 homes per street mile, and designs at least one clean node on both profiles. A clean node has no design issues and at least 10 homes, since a smaller one is a stray piece of street and hardly tests where a fault is. An area that fails is dropped. If fewer than 6 `test` areas remain, reserves come in, in listed order, under the same rule. This is decided without any model looking at it.

Fresh generated layouts (seed 4) are scored as a reference, not as the choice.

## More data

Generated layouts go from 10 to 60 designs per family and profile, at 40 rows per node, plus the `train` maps. A learning curve on the `calibrate` maps, at an eighth, a quarter, a half and all of the training layouts, says whether more data is still helping each model. Layouts are subsampled whole, never rows.

Each model gets a small settings sweep on the `calibrate` maps, the same effort for both: GNN width 64 or 128 and 8 or 16 epochs; GBDT 200 or 400 trees and 31 or 63 leaves.

## Decision rule, fixed before any result

All on `test` maps, on identical faults, through the hybrid. Intervals are a 95 % bootstrap over nodes, on GNN minus GBDT.

The GNN replaces the GBDT only if all of these hold. Otherwise the GBDT stays, since it's shipped and simpler.

1. **Localization:** the top 3 interval is above 0, or the top 1 interval is above 0 and the top 3 interval isn't entirely below 0.
2. **False alarms:** its healthy-node false alarm rate is no more than 5 points above the GBDT's.
3. **Calibration:** its ECE, with a calibrator fitted on the `calibrate` runs, is no more than 2 points above the GBDT's.
4. **When the sim is wrong:** at the world of six where its own top 3 is lowest (tap return loss, active return loss and hardline and drop loss, each at both ends of the sensitivity range), its top 3 is no more than 3 points below the GBDT's at that same world.
5. **Latency:** one forward pass in Rust under 50 ms on a node of about 100 candidates.

The bar in `docs/DESIGN.md` isn't changed by this. Its family hold-out verdict stays as reported, and the held-out map result is reported beside it as a different question.

## Deliverables

1. **`proposal::graph`:** `graph_nodes`, moved from `datagen`, so inference and training read one definition. The rows it writes don't change.
2. **`proposal::gnn`:** `GraphModel`, the same network as `ml/train_gnn.py` as a data file evaluated in-process. It records its schemas and refuses others (invariant 3). **`proposal::Proposer`** is either kind of model.
3. **`ml/maps.json`** and `crates/mcp-server/examples/fetch_maps.rs`: fetches the areas into `ml/data/osm/`, which isn't committed. Extracts are © OpenStreetMap contributors under the ODbL. Hashes and fetch date are recorded in `ml/artifacts/maps_manifest.json`.
4. **`datagen`:** `maps-survey`, `maps-rows` and `maps-hybrid`. The last takes an optional estimate and end for a world that differs from what the diagnoser believes, and a `proposal` mode that runs the model alone in Rust, so proposal scores on the test maps are read from Rust on identical faults.
5. **`ml/select_maps.py`:** learning curve, sweep, final fits, GNN export and parity file, family and profile hold-outs on generated layouts as stress tests. It never reads the test maps. Settings are chosen by lowest log loss on the `calibrate` maps.
6. **`ml/evaluate_maps.py`:** the decision rule above, over the hybrid runs. Read-outs beside it: per map, per profile, per class, the model alone, every estimate with the model alone, fresh generated layouts, and today's shipped GBDT on the test maps, which says what more data and real maps bought.
7. **`ml/run_maps.sh`:** all of it from nothing.
8. **`docs/model-selection.md`:** updated from the real run, whichever way it goes.

## Tests

- Rust's GNN scores equal a double-precision Python run of the same weights on the parity file, to 1e-9.
- A model with another observation or fault schema is refused.
- The forward pass gives the same bytes twice, and doesn't depend on the order of a plant's lists.
- Rows from `datagen rows` are byte-identical to before the move.
- `Proposer` reads either file. A `maps-rows` run on a real fixture gives rows marked `Synthetic` with the map's name, and twice gives the same bytes.

## Out of scope

Wiring the GNN into the twin, which follows only if it wins. A temporal layer. Time splits, since the rows have no real time, and those wait for field data. Compound faults.

## Acceptance

`cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace` green. `docs/model-selection.md` written from a real run, with the decision rule applied as written, even if the result is uncomfortable.

## Deferred

- **Time and market splits on field data.** When real work orders exist, train on what was live before a date and test on what went live after it, including markets the model never saw. Rows from one node on both sides of a time split would leak a chronic fault.
- **Family hold-outs through the hybrid for the GNN.** Only proposal scores are rerun as a stress test.

## Decisions made during the task
- **Buffalo was dropped.** Its fetch timed out at Overpass, and it wasn't retried. 25 areas are used: 13 `train`, 4 `calibrate`, 8 `test`. No reserve came in.
- **Sampling shares were lowered before any score existed.** Real maps design into about 60 homes per node, not the 120 assumed, so a map has far more nodes than planned.
- **No more data was added after reading the curves.** Both are still rising on the `calibrate` maps at all the data. The GNN's top 1 went 75.3, 77.7, 79.7, 81.3 % and the GBDT's 68.9, 70.0, 70.7, 72.3 %. More data would help a final model, but the GNN leads by more than the GBDT has gained from all its doublings together, so it wouldn't change this choice. Decided before anything read the test maps.
- **Settings, by lowest log loss on `calibrate`:** GNN width 64, 16 epochs. GBDT 400 trees, 31 leaves.
- **The GNN's stress folds are a stage of their own.** Forking workers after the parent had run torch hung on OpenMP's locks for an hour with nothing lost, so they run in a fresh process and write `results_maps_gnn_folds.json`.
- **Three worlds were added after the result:** modem implementation MER at both ends and CIN at reference low, through the hybrid, because they hurt the GNN alone most. They're reported as a read-out, kept out of the rule, and didn't change the verdict. They showed a gap in part 4: it compares the models only at the GNN's own lowest world, so a world where the GNN is much worse than the GBDT but not at its own lowest passes unseen.
- **Result:** the GNN passes all five parts. Wiring it in is a separate decision, with that finding in front of it.
