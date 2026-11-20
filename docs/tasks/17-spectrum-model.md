# Task 17 — A neural model that reads the whole RxMER curve

**Build-order step:** after 16.
**Depends on:** Tasks 15, 16
**Produces:** field-sized fault ranges, several faults per row, full-resolution spectrum rows with labels computed from the sim, a 1-D CNN trained on them, a Rust forward pass, and a scored result on held-out maps.
**Load:** `CLAUDE.md`; `docs/reference/rxmer-impairments.md`; `docs/rxmer-gallery.md`; `docs/tasks/15-held-out-maps-selection.md`.

## Goal

For one modem's OFDM capture, name each impairment's shape, where it sits in frequency, and how bad it is, and say "don't know" when something is wrong that it can't name. The current models read the channel mean, which moved 1.8 dB under a 40 dB suckout.

Models are neural only, by the user's direction. Task 18 will try editing their weights directly, so the network stays small enough for that: under 200,000 weights.

Everything here is synthetic: real streets, invented plant, simulated faults and telemetry (invariant 11). A score here says how well the model reads this sim. It doesn't say how it would do on real plant. No real labelled data is public, and proprietary data isn't an option.

## Shape and cause are different

The model names what the curve looks like. The GNN and the sim verifier name the cause. A roll-off can be water, a low-rated part or one side of a suckout, so these never merge.

| Shape | From |
|---|---|
| `Ingress` | `DownstreamIngress`, 1 MHz or wider |
| `Spike` | `DownstreamIngress`, narrower than 1 MHz |
| `Dip` | `Suckout`, `ResonantPeak` |
| `RollOff` | `FrequencySelectiveLoss`, low-pass or high-pass |
| `BandLoss` | `FrequencySelectiveLoss`, band-stop |
| `Ripple` | `ConnectorReflection` |
| `Wave` | `WaterIngress` |
| `LowMer` | `ForwardOptics`, `BadDrop`, `AmpDrift` |
| `Edge` | An adjacent SC-QAM's leakage, and `AdjacentLevelOffset` |

This table maps a fault's parameters to a label. It never touches telemetry, so invariant 5 holds.

## Labels computed from the sim

Each row runs the sim with no faults, with each fault alone, and with all of them, on one seed. All of it is a pure derivation of the inputs.

- **Per bin:** a shape is present where that fault alone lowers RxMER by at least 1.5 dB against the clean run. A fault with no visible effect on this modem leaves it normal for this modem.
- **Severity:** the fall from the clean run per bin, all faults together, in dB. Continuous, never low, medium or high.
- **Edge:** the fall from a run of the same plant with no adjacent channel.

## Input

Rust computes it, and Python reads it, so training and inference share one definition. Bins are 0.2 MHz, 960 of them, enough for 192 MHz. Each bin holds two values: the lowest subcarrier, so a spike survives, and the mean. A third channel is the bin's frequency, since LTE and water behave differently by frequency. A narrower channel is padded with its mean, which [FERREIRA2020] found best, and a mask channel says what's padding.

## Sampling

Ranges come from `docs/reference/rxmer-impairments.md` §2 and `docs/rxmer-gallery.md`, spread so each class runs from invisible to severe. **This spreads severity on purpose and says nothing about prevalence.**

- A tenth of rows have no fault. Half of the rest have one, and the others two or three, since about 40 % of Comcast's labelled samples hold several.
- A row's modems: up to 4, always including one the fault can reach when there is one.
- Modem level and estimates vary per row, so a healthy baseline isn't one flat line.
- Some rows put an SC-QAM next to the OFDM channel, with Ncp and Nrp drawn from what's configurable.

## Model

Dilated 1-D convolutions, so a wide roll-off and a 2-subcarrier spike are both in reach, with three heads per bin: which shapes (multi-label), the fall in dB, and normal. **"Don't know":** a bin whose predicted fall is at least 3 dB with no shape above its threshold is `Unknown`. Width and depth are chosen on `calibrate` by the settings sweep, within the weight limit.

## Split

Same maps as task 15: `train`, `calibrate`, `test`. Settings, thresholds and calibration use `calibrate` only. **Test maps are read once, after everything is frozen.** A metro stays in one split.

## Decision rule, fixed before any result

On `test` maps, per capture unless stated. All of these must hold for the model to count as ready for the next step. If any fails, it's reported as failing, and nothing is retuned against the test maps.

| Part | Bar |
|---|---|
| **Catches severe damage** | A capture with a worst fall of 10 dB or more over at least 1 MHz is called not normal at least 99 % of the time; 20 dB or more, at least 99.5 % |
| **Quiet when healthy** | At most 5 % of healthy captures called impaired |
| **Names the shape** | Macro F1 across shapes at least 0.85, on captures where the shape falls at least 3 dB, and no shape under 0.70 |
| **Finds the band** | Median overlap (IoU) at least 0.7 for `Ingress`, `Dip`, `BandLoss` and `RollOff` |
| **Sizes it** | Mean absolute error at most 2 dB on bins that fall at least 3 dB |
| **Says don't know** | With one shape left out of training (`BandLoss`), at least 80 % of its 10 dB or worse captures are called not normal. Run on `calibrate` maps, since it's a separate fit |
| **Runs** | Rust and Python agree to 1e-9 on a parity file, one capture under 50 ms, same bytes twice, under 200,000 weights |

Published numbers are context only: Comcast's F1 of 0.80–0.95 was on real data with expert labels, so it's a different question.

**Read-outs beside the rule:** per shape, per severity band, per map, per profile, multi-fault against single-fault, and the worst examples drawn out. Also the one real capture through the Rust path: mostly healthy, with a spike at 750 MHz. That's one file of unverified origin, so it's a sanity check and not a score.

## Deliverables

1. **`features::sampler`:** `sample_spectrum_faults`, the ranges above with sources in comments. `sample_fault` stays as it is, so step 15's rows don't move.
2. **`features::spectrum`:** bins from any RxMER record or PNM file. **`schema::spectrum`:** a versioned label schema with a round trip and golden sample (invariant 3).
3. **`datagen maps-spectra`:** rows marked `Synthetic`, with fault, observation and label schema versions.
4. **`ml/train_spectrum.py`:** sweep, final fit, export, parity file. **`ml/evaluate_spectrum.py`:** the rule. Both read Rust's files only.
5. **`proposal::spectrum`:** the forward pass in Rust. It refuses a model from another schema.
6. **`docs/model-selection.md`:** a new section from the real run, whichever way it goes.

## Tests

- Binning: hand-worked min and mean on a small curve, a spike survives, a 25 kHz and a 50 kHz channel land on the same grid, padding is marked, and the real capture bins as expected.
- Labels: a fault on another leg leaves a modem normal, a worked case gives the expected band and fall, and two faults keep their own shapes.
- Sampler: every shape appears, severity reaches 20 dB or worse in every class that can, and it's deterministic.
- `maps-spectra` twice gives the same bytes.
- Forward pass parity, determinism and schema refusal as in task 15.

## Out of scope

Feeding the output to the GNN, and wiring anything into the twin, which follow only if this passes. Upstream OFDMA curves. Weight-space editing: task 18. Time across polls.

## Acceptance

`cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace` green. `docs/model-selection.md` written from a real run, with the rule applied as written, even if the result is uncomfortable.

## Deferred

- **Upstream OFDMA,** with Comcast's VHF, analog modulator and RFoG shapes.
- **Spectrum capture and channel estimate as inputs,** which could separate a suckout from a resonant peak or ingress.
- **Prevalence.** No source publishes it, so this model's priors aren't a real network's.

## Decisions made during the task
- **A modem that's dropped off has no curve, and it now drops where the spec says.** The first data run was thrown away before training. The sim took a modem offline once its boot profile passed a 1 % failure rate, while [MULPI §10.6.1.2] keeps a channel in use with its pilots held. Now a modem goes when MAPs and replies stop decoding (`docs/physics.md` §7.4), passes through partial channel and partial service first, and goes up and down near the edge.
- **Where that is depends on the boot profile,** so each row draws one of 16, 64 or 256-QAM. No source gives a share. Measured with `ForwardOptics` on the street fixture, a D3.1 modem from a healthy 41 dB keeps its curve to about 23 dB on 256-QAM, 18 dB on 64-QAM and 12 dB on 16-QAM. So a flat fall of 20 dB is in the data on the two lower ones, and the model has seen it if a real modem stays up through one. Whether real modems do, and which boot profile they have, is unsourced. The user recalls these impairments coming with intermittent downtime and partial bonding, but not whether those modems stayed online.
- **A snapshot, not a history.** Rows take one step, so a modem that's going up and down gives a curve only when it's up, and the model never sees time.
- **Reach, tested on healthy-level modems on a 256-QAM boot profile:** 20 dB or more for `Ingress`, `Spike`, `Dip`, `RollOff` and `BandLoss`. 15 dB for `Wave` and `LowMer`, and `LowMer` passes 20 dB on a 16-QAM boot profile. `Ripple` is under 3 dB at healthy level and needs a low modem.
- **Ripple and water are drawn more often,** 4 and 2 to 1, because they only show on some modems.
- **The test maps are generated with the rest.** Nothing reads them until `evaluate_spectrum.py test`.
- **The first fit missed on `calibrate`, so the network changed before the test maps were read.** A flat fall has no edge to find, so every bin now also gets a summary of the whole channel (`dilated-cnn-2`), at width 64 and 40 epochs. The Rust forward pass refuses the old file. Tuning stopped there, with the fall error bar still missed, rather than chasing the bar.
- **The test maps were read once, and two parts failed.** They're reported as failing and nothing was retuned. Any new fit has to be scored on other maps, such as step 15's three reserve areas.
- **Background low modems count as healthy,** because they're in the clean run. The same `BadDrop` drawn as an impairment counts as `LowMer`, so a flat low curve can carry either answer. That's a weakness in this spec's labels, found after the rule was fixed.
