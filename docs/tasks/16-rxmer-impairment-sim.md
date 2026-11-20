# Task 16 — RxMER impairments at field shape and severity

**Build-order step:** after 15. Comes before any spectrum model, since a model can only be as real as the curves it trains on.
**Depends on:** Tasks 03, 04, 12, 15
**Produces:** corrected noise physics, every CableLabs impairment reachable from physical parameters, tests against published captures, and a gallery.
**Load:** `CLAUDE.md`; `docs/reference/rxmer-impairments.md`; `docs/reference/impairments.md`; `docs/physics.md` §4.5, §5, §8, §9.

## Goal

Make the sim able to reproduce every impairment CableLabs lists, at the shapes and severities the published record shows, including the bad cases. A drop of 20 dB is in that record.

Every shape comes out of the physics from a physical fault (invariant 5). A field figure is something to check against, never something to draw. Sim output stays `Synthetic` (invariant 11). Passing means the sim can reach the published shapes and sizes. It doesn't mean its mix of faults matches any real network, since no source publishes one.

## The list, fixed before any result

From [PNM-HFC Table 24] and [ZHU2020 §II.B], plus Comcast's families since they cost little more. Targets come from `docs/reference/rxmer-impairments.md` §2, read by eye, so each carries a tolerance.

| Impairment | Source | Target | Fault that reaches it |
|---|---|---|---|
| Healthy | Public capture | Mean 40–47, scatter 0.79 ±0.15 | None |
| Ingress, LTE | Table 24, ZHU | Drops of 6, 10, 20 ±3, width 10 ±3 MHz | `DownstreamIngress` |
| Several ingress bands | ZHU | Each band at its own size | Several `DownstreamIngress` |
| Spike | ZHU | About 10 ±3, under 3 subcarriers | Narrow `DownstreamIngress` |
| Sweep comb | Comcast | About 17 ±3, narrow, regular | Several narrow `DownstreamIngress` |
| Suckout | Table 24, ZHU | Drops of 13, 18, 30 ±3, baseline 38 ±3 | `Suckout` |
| Roll-off | Table 24, ZHU | 21.5 ±4 falling gradually, under 2 dB per MHz | `FrequencySelectiveLoss`, low-pass with a skirt |
| Pullout | Table 24 | Low end falls, transmit rises | `FrequencySelectiveLoss`, high-pass with a skirt |
| Filter | Table 24 | A band missing on one drop | `FrequencySelectiveLoss`, band-stop |
| Ripple | Table 24, ZHU's wave | 5 ±2 p-p, period 7.5 ±1.5 MHz, one period dominant | Two `ConnectorReflection` |
| Ripple hidden | [HRANAC2019] | Flat when noise comes before the echo | The same, with weak optics |
| Water, non-periodic wave | Table 24, ZHU | 5 ±2.5 p-p, baseline falling 10+, no dominant period | `WaterIngress` |
| Low MER | Comcast, laser notes | 12.5 ±3 below healthy, on every modem | `ForwardOptics`, new |
| Flat loss | Table 24 | Flat fall, transmit rises | `BadDrop` |
| Tilt | Table 24 | Only matters when excessive | `AmpDrift` |
| Resonant peak | Table 24, ZHU | 17 ±1.5 in level, dip in RxMER | `ResonantPeak`, new |
| Edge roll-off | [HRANAC2019] | 9 ±6 at the edge, gone at Ncp 1024, Nrp 256 | Physics, no fault |
| Adjacency | Table 24, ZHU | Block's level moves, its neighbour's edge falls | `AdjacentLevelOffset`, new |
| Compression, distortion | ZHU, notes §11 | Flat fall as noise | `AmpDrift` with a compression onset |
| Phase distortion | Table 24, ZHU | Group delay rises at a roll-off's edge | From minimum-phase loss |
| CPD | Table 24 | Raised return floor, downstream untouched | `CommonPathDistortion`, new |
| Return ingress, bursts | Table 24 | A dip where it lands, bursts reading milder | `UpstreamIngress` |

A family that can't pass with defensible parameters stays failed and is reported. Nothing gets scripted to pass.

**Lab oracle, the one hard number.** Echo with 6.97 dB p-p of channel ripple and no other noise: mean 46.65 dB and σ 1.6 dB in RxMER [HRANAC2019 Test Cases 2B, 2C].

## Deliverables

1. **`docs/physics.md`:** a modem ceiling that ripples through the echo (§4.5), adjacent-channel leakage into OFDM (§4.5.1), four new faults, skirted loss and water on drops (§8), each with a worked example.
2. **Fault and work order schemas, version 3, with migrations (invariant 3).** Models trained on version 2 still load, since version 3 only adds what they never saw.
3. **`crates/sim/tests/field_shapes.rs`:** every row above.
4. **`fixtures/field/`:** the one real capture we found, under its license, read through our own decoder.
5. **`docs/rxmer-gallery.md`** from a real run.

## Tests

- Oracles for the new physics in `crates/sim/tests/oracles.rs`.
- Every row above, in `field_shapes.rs`.
- `fixtures/one_street` still gives a valid level budget.
- Round trip, golden samples and migration for the new versions.
- Determinism across all the new faults at once.

## Out of scope

- **Training data from any of this: task 17.** Field-sized severity ranges in `features::sampler`, faults drawn jointly since many captures hold several, `datagen spectra` at full resolution, and labels computed by running each row with and without its faults on the same seed. Severity is continuous, in dB and lost bit loading.
- **The spectrum model: task 17.** A 1-D CNN detector naming shape, band and severity per modem, neural only, with "don't know" as an answer. Its output feeds the graph model, then the sim verifier.
- **Weight-space editing: task 18.** A harness for an LLM to edit trained weights, with results on `calibrate` maps only, test maps sealed, and each edited model a new versioned artifact with its edit log.
- **Re-running selection.** Step 15's numbers are about the old sim.

## Acceptance

`cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace` green. `docs/rxmer-gallery.md` from a real run.

## Deferred

- **Observations with no field yet:** the histogram's clipping signature, the constellation's compression pattern, and hum. A new observation version, when something needs them.
- **Modem front ends.** The lab's RxMER didn't change from 0 to +10 dBmV, since attenuation raises noise figure at high input. Ours moves a little.
- **Modem spread in edge roll-off.** Some modems show it and others on the same signal don't.
- **Return ceiling.** Healthy OFDMA reads 54 dB. Comcast shows "over 40", so a CMTS ceiling probably belongs here, unsourced so far.
- **Reading the unread sources:** Fox et al. on water, SCTE 280, SCTE 294.
- **Real data.** Only public sources. CableLabs' and Comcast's labelled sets aren't released, and proprietary data isn't an option.

## Decisions made during the task

- **The scope became the CableLabs list,** at the user's direction, so spectrum rows and labels moved to task 17.
- **A suckout's mismatch wasn't one.** A 20 dB level notch only fell 7 dB in RxMER at a healthy modem level, against 18 dB in the field. Comcast's example sits on a 38 dB baseline, so that modem is about 14 dB lower. Put there, 20 dB of notch falls 17 dB. So the physics was right, and modem level decides.
- **Ripple needed the shaped ceiling and a low modem.** Both reflectors must be bad, about 3.5 dB return loss, and the modem's own noise must dominate. It's the only family where we had to pick an unusually bad part to hit the target, and the published ripple case had a mismatch whose size isn't stated.
- **Water runs far past the old ranges:** 40 dB per 100 ft and 70 % impedance variation over 70 ft of drop, against training's 6 dB and 12 %. Those are ours, chosen to reach the field's soaked-part loss of 20–30 dB.
- **`AdjacentLevelOffset` and `ForwardOptics` name no element.** A plant has one root, and they mean the same on either fixture. On an RPD, `ForwardOptics` does nothing since its optics are digital.
- **Version 2 models still load,** but the sim moved under them, so their scores are unmeasured until task 17 retrains them.
