# Task 10 — DAA depth and migration planning

**Build-order step:** 10
**Depends on:** Tasks 04, 05, 09
**Produces:** fault and work-order schemas at version 2, with migrations; `schema::rpd`; RPD faults through sim, features and diagnosis; `design::migrate`.
**Load:** `CLAUDE.md`, invariants 3 and 5; `docs/reference/daa-remote-phy.md`; `docs/DESIGN.md`, "Migration comparison".

## Goal

Why the twin is useful during a rollout. A node's before and after on one model: achievable modulation and capacity each way, transmit headroom, the parts that must change and the modems that lose service. And an RPD is a first-class active with its own ways to fail.

## Deliverables

1. **Fault schema v2, work order schema v2.** `RpdFailure`, `CinLinkCut`, `GcpLoss`, `TimingLoss`, `RfmMisalignment`, as parameters. Version 1 files go through `migrate`, with a frozen `*.v1.json` golden for each (invariant 3).
2. **`schema::rpd`, version 1.** `RpdObservation`: `top_level_state`, `gcp_status`, `ptp_result`, `rf_output_up`, `drawing_power`.
3. **`sim::rpd`.** RPD state from fault parameters, time and the normative recovery defaults. Whether RF is on is derived, and the modems fall through §7.4 on their own. GCP loss has no RF effect until retries run out, the diagnostic trap the notes call out.
4. **Diagnosis.** RPD candidates, scored like every other. Separates a dead RPD from a cut link by whether it's drawing power.
5. **`design::migrate::plan_migration`.** For one node: the plant converted to the target profile, parts that no longer qualify, every modem's before and after, and any new design issues. Removing analog optics is reported as a what-if, separately, because it rests on an estimate (`docs/DESIGN.md`).

## Tests

- Migration: v1 goldens parse and migrate, old fixtures keep their meaning.
- RPD signatures: each class's RF and state over time. GCP loss holds RF until retries exhaust.
- Diagnosis: each class comes back right on the high-split fixture, including GCP loss with healthy RF.
- Migration planner, by hand on the fixture: upstream capacity from 122.88 Mbit/s of SC-QAM to that plus OFDMA, less transmit headroom at 204 MHz, root replaced, and no part flagged when every part qualifies. A 1 GHz tap in the plant is flagged. Determinism.

## Out of scope

A CIN model: `ExtraOpticalLoss`, `CinCongestion`, late MAPs. `L2tpLoss` and core failure across many RPDs. Virtual splitting and combining. Hub-shelf RPDs.

## Acceptance

`cargo test --workspace` green, clippy clean, frozen v1 goldens unchanged.

## Deferred

- **`L2tpLoss`, `CoreFailure`, `ExtraOpticalLoss`, `CinCongestion`.** The last two need a CIN and optical budget as plant input, or they'd be scripted.
- **A migration as a dated event on the time series,** with features re-baselined at it. `apply_migration` swaps the plant, and the design reference follows, so features stay relative. The event itself has no record yet.
- **R-OSSI and R-DTI aren't in our source set.** So GCP's per-retry time, holdover limits and the time before a Core holds RF off are ours, and marked approx in `sim::rpd`.

## Decisions made during the task

- **A root that's off emits no upstream measures, and a root with no amps has its own 10 dB receiver as the noise floor.** Upstream noise used to be exactly zero in both cases, which gave infinite SNR: on an RPD failure, and on every healthy N+0 node. RPD diagnosis found this, through a NaN ranking. §7.3's oracle has amps and doesn't move.
- **`drawing_power` is what separates a dead RPD from a cut fiber.** Both look the same to a Core, and the notes name AC telemetry as the evidence.
- **Capacity is PHY rate,** without overhead, summed per modem. It isn't simultaneous throughput.
- **The coax fault examples stay on both plants,** with a separate file for RPD examples, since only the high-split root is an RPD.
