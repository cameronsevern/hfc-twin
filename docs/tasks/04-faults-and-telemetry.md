# Task 04 — Faults, modem state, telemetry, features and sweep

**Build-order step:** 4
**Depends on:** Task 03
**Produces:** in `crates/sim`, the fault overlay, §7.4 modem state and the §9 telemetry generator. In
`crates/features`, the feature layer and the parameter sweep.
**Load:** `CLAUDE.md`; `docs/physics.md` §5, §7.4, §8, §9; `docs/reference/mac-operations.md`.

## Goal

Same plant + environment + faults + seed gives byte-identical observations in the §9 schema, and
every fault's signature comes from its physical parameters going through the walk (invariant 5). No
line of this crate says what a fault looks like in telemetry.

## Deliverables

1. **`rng`.** A seeded generator passed explicitly, derived per element, per time step and per use,
   so adding a modem doesn't move any other modem's draws. Binomial and Gaussian draws. No `rand`
   dependency, no clock.
2. **`Overlay::build(graph, faults, sample)`.** One arm per §8 row. Onset scales severity. A
   `Schedule` multiplies it by the hour of `t`. Rain raises `k_w,eff`. Water's impedance steps are
   seeded point reflectors, so its non-periodic waves come out of ordinary cavities.
3. **`randomize`.** §8's domain randomization, seeded, into the overlay's offsets. Off by default.
4. **`state`.** The §7.4 fold: T3 and T4 counters, retries, partial service, reinitialization.
5. **`telemetry::simulate(scenario) -> Vec<StepObservation>`.** One `ModemObservation` per modem, one
   `ActiveObservation` per active and one `ServiceGroupObservation` per step. Capability masks use
   `effective(profile, modem)`. `Detail::Summary` decimates subcarriers for speed.
6. **`crates/features`.** `features_for(step, graph, reference)`, a pure function: MER margin, level
   drift against a design reference computed from the healthy plant, deviation from siblings, pre-eq
   reflection delay in feet, codeword rates, normalized arrays.
7. **`sweep`.** A seeded sweep over fault class, location and severity, writing labeled feature rows
   as JSON lines. It's a binary, and step 12 reuses it at scale.

## Tests

Oracles from §7.4: 7.6e−6 exhaustion at p_loss 0.5, 1,500 T3 expiries capping a 300 s step, about 30 s
to reinitialize on total downstream loss.

Signatures, which are what the physics must produce and no code may script:

| Fault | Must be true of the emitted telemetry |
|---|---|
| Connector on span-3 at 150 ft, RL 10 | Pre-eq and channel-estimate ripple at 3 and 4 and below; cavity length recovered to within 20 ft from ripple; modems 1 and 2 clean or nearly so; RxMER barely moves |
| Suckout on span-2 | RxMER dip at the same subcarriers across every modem below; level notch at f0 |
| Amp-1 drift −2 dB | Level step of 2 dB at modems 1–4; below amp-2, AGC takes it up; nothing above amp-1 |
| Amp-2 failed | Modems 5–8 lose signal and reinitialize; modems 1–4 unaffected |
| Water on span-3 with rain | Loss grows with rain; ripple non-periodic; modems 1, 2 clean |
| Upstream ingress on a drop | `us_snr` falls across the service group; modem levels unchanged |
| Downstream ingress on span-5 | RxMER dip inside its band only at modems 5–8 |
| Power sag past `v_min` | `v_in` falls, the amp goes down, modems below it lose service; none above |
| Bad drop on modem-4 | Modem-4 only |
| Frequency-selective loss | Band missing at modems below |

Also:

- Determinism: same seed, byte-identical JSON. A different seed changes draws, not physics.
- The healthy fixture: every modem operational, no uncorrectables, no T3s, D3.0 modems have no OFDM
  fields, RxMER near §4.5's oracle.
- Upstream-only faults raise T3 and leave T4 flat (§7.4).
- Every emitted record validates against the schema by serde round trip, with no `NaN` or infinity
  in the JSON.
- Features: the same physical fault on both profiles gives comparable margin and drift values.

## Out of scope

Diagnosis. Sim calibration. FDX. RPD faults (step 10).

## Acceptance

`cargo test --workspace` green, clippy clean, no `fixtures/` shape changes.

## Deferred

- **SC-QAM downstream MER as a field.** §9 lists RxMER for OFDM only, so a D3.0 modem's downstream shows
  through levels and codewords. Real ones report channel SNR. A new observation schema version if the
  diagnoser needs it.
- **Burst timing beyond duty cycle,** and FBC in sweeps: `Detail::Summary` leaves FBC out for speed.

## Decisions made during the task

- **`Detail::Summary`** keeps one subcarrier in 8, or 400 kHz, which still resolves echoes up to 1.25 µs. The
  decimated axis starts at index 0, so `f = zero + (first + i)·spacing` still holds.
- **A modem that isn't registered reports only counters and channel state** (§11's validity at emit).
- **Ranging survives at QPSK**, modeled as 6 dB below 16-QAM's `thr` (approx). One exchange fails if
  the request or reply does.
- **Reading `Unusable` for a channel:** SC-QAM at `P_uncorr ≥ 0.5`; OFDM when all data profiles fail or
  mean RxMER falls below 8 dB (approx). A profile "fails" above a 1 % rate, standing in for vendor limits.
- **Measurement noise** is separate from physics, seeded, 0.25 dB in the sweep (approx), and readings
  round to 0.1 dB as devices report.
- **Faults in unloaded spectrum show nothing** outside FBC, so the sampler draws frequencies from the
  channel plan.
- **The off-path echo can be stronger than the on-path one.** A 10 dB connector 150 ft below a 23 dB tap
  shows −25.9 dB at that tap's own modems, against about −30 dB through the cavity below. That's §5.1's
  point about high-value taps, and it means "modems above the fault are clean" is wrong for reflections.
