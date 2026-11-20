# Task 05 — Diagnosis without ML

**Build-order step:** 5
**Depends on:** Task 04
**Produces:** `crates/diagnosis`: who is affected, what the cause is and how to fix it, using the plant
as the prior and the sim as the likelihood, with no training data. It's the floor every model must beat.
**Load:** `CLAUDE.md`; `docs/DESIGN.md` "Telemetry and fault model"; `docs/reference/impairments.md` if a class is hard
to separate.

## Goal

Features in, ranked hypotheses out, each with its physical parameters, a posterior, an affected-modem
count and simulated telemetry beside what was seen. The top one becomes a work order from the policy
table. Nothing learned, nothing scripted: a candidate is scored by simulating it.

## Deliverables

1. **`who`.** Which modems look wrong, from features alone, with thresholds in `DiagnoserConfig` (the
   sim doesn't hold severity thresholds, §5.3).
2. **Candidates, from topology.** Faults live on components, and one on a span affects what's below it.
   Spans rank by how well their downstream set matches the affected set. Each class proposes
   parameters from the features: cavity length from echo delay, gain from level drift, frequency from
   the deepest bucket.
3. **Scoring.** `fit(observed, predicted)` is a χ² over relative features, with deliberately wide σ so
   domain randomization and measurement noise don't dominate. Each candidate runs through
   `simulate`, with no noise and no randomization.
4. **Refinement.** Coordinate descent on the top candidates' main parameters.
5. **Posterior.** `prior × exp(−χ²/2T)` over hypotheses including "healthy", with `T` scaled by how
   well the best hypothesis fits, since a misfit means the likelihood is too sharp.
6. **Compound faults.** If the best single fault leaves a clear residual, try adding a second.
7. **Upstream ingress.** Modem data doesn't localize it (§7.3). The diagnoser says which root port and
   returns a switch-test plan. `localize_by_switching` runs that plan against a measurement closure.
8. **Work orders.** `work_order(hypothesis, policy)` from a template, with a deterministic id.

## Tests

- The v0 bar: on the fixture, with domain randomization and measurement noise on, the faulty component
  ranks first in at least 90 % of seeds and positions for a connector fault, on both profiles.
- Per class: the class and component accuracy table, from `examples/accuracy.rs`, recorded below.
- Healthy plant: the top hypothesis is "healthy" with no work order.
- Determinism: same inputs, byte-identical diagnosis.
- A compound of two bad drops comes back as both.
- The work order's action comes from the policy table, so editing the table changes the order
  without any code.
- `localize_by_switching` finds an ingress leg against the sim.

## Out of scope

ML (step 12), the lifecycle after proposal (step 11), active sensing (later).

## Acceptance

`cargo test -p diagnosis` green, clippy clean, the bar above asserted.

## Results

On the fixture, 10 draws per class with `sample_fault`'s wide severity ranges, domain randomization and 0.25 dB
measurement noise. Synthetic. `cargo run --release -p diagnosis --example accuracy 10`:

| Profile | Class | Top class | Top component | Top-3 component |
|---|---|---|---|---|
| Low-split | WaterIngress | 80 % | 50 % | 70 % |
| Low-split | ConnectorReflection | 80 % | 80 % | 90 % |
| Low-split | Suckout | 90 % | 50 % | 100 % |
| Low-split | FrequencySelectiveLoss | 70 % | 70 % | 80 % |
| Low-split | AmpDrift | 100 % | 100 % | 100 % |
| Low-split | UpstreamIngress | 80 % | n/a | n/a |
| Low-split | DownstreamIngress | 60 % | 60 % | 60 % |
| Low-split | PowerSupplySag | 100 % | 100 % | 100 % |
| Low-split | BadDrop | 100 % | 100 % | 100 % |
| High-split | WaterIngress | 80 % | 50 % | 60 % |
| High-split | ConnectorReflection | 90 % | 80 % | 90 % |
| High-split | Suckout | 90 % | 50 % | 100 % |
| High-split | FrequencySelectiveLoss | 80 % | 50 % | 90 % |
| High-split | AmpDrift | 100 % | 100 % | 100 % |
| High-split | UpstreamIngress | 80 % | n/a | n/a |
| High-split | DownstreamIngress | 70 % | 60 % | 70 % |
| High-split | PowerSupplySag | 80 % | 80 % | 100 % |
| High-split | BadDrop | 100 % | 100 % | 100 % |

Ten draws is a small sample, so read these as rough. The asserted v0 bar is narrower: a connector at 4–10 dB of
return loss, 30 seeds and positions, each profile, top component in at least 90 %. It passes.

**Why some rows are low.**

- Top component at 50 % with top-3 at 100 %: span-1 and span-2 have the same modems below them, with only amp-1
  between. Loss on either looks the same from every modem, so nothing in the telemetry separates them. That's a
  fact about the plant, and only top-3 means anything for these.
- Connector above 10–12 dB: it's about as reflective as the ordinary 16–18 dB parts beside it.
- Downstream ingress: draws with low power leave nothing to find.

A run takes 50–100 simulations, about 1.5 s on this 8-modem fixture. Scale is step 13's problem.

## Decisions made during the task

- **A cavity needs an up-facing and a down-facing reflector.** The first pass paired every reflector with every
  other, which counted each cavity about four times and gave a healthy N+2 modem −21 dB NMTER. An echo goes down,
  back up off something facing down, and down again off something facing up. Fixed in `walk.rs`.
- **Healthy NMTER still reads −24 to −31 dB** at the fixture's generic 16–18 dB return losses, since cable barely
  attenuates a cavity at 16 MHz. That sits at PNMP's monitor band. Those return losses are SCTE 183's field
  example, and real parts may do better at low frequency, so features compare against design rather than an
  absolute threshold, and step 12's sensitivity report should rank this one.
- **Pre-eq taps, averaged in power over SC-QAM channels, are the sharpest evidence for a cavity.** Domain
  randomization doesn't touch them, and they give position to within tens of feet.
- **Level buckets are compared with each modem's mean taken out, at a quarter weight.** A plant's tolerances move
  every bucket alike, and without this any fault could "explain" jitter better than a true echo explains itself.
- **Parameter variants merge.** One answer per cause and place, with their posteriors summed, so a top 3 isn't
  three versions of one answer.
- **Who is affected and quiet are separate.** A plant with no modem past a threshold still gets candidates if
  healthy doesn't fit, because a sag shows only in active voltages.
