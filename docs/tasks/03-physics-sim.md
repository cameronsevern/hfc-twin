# Task 03 — Physics sim

**Build-order step:** 3
**Depends on:** Tasks 01, 02
**Produces:** `crates/sim/`: forward path, noise and MER, return path, AC power and the echo and
pre-eq primitives, on both first profiles.
**Load:** `CLAUDE.md`; `docs/physics.md` §1–§7, §10, §11; `docs/reference/plant-and-powering.md`.

## Goal

Every number `docs/physics.md` §1–§7 computes, reproduced from plant, profile, library and
environment. These are pure functions: same inputs, same bytes (invariant 4). This crate takes no
RNG yet, because nothing in §1–§7.3 is random. Step 4 adds the fault overlay's contents, modem
state and telemetry on top of it.

## Deliverables

### 1. Formula modules, one per physics section

Each is a pure function of named, unit-carrying values, and each has a unit test against the
worked example in `docs/physics.md`.

| Module | Physics | Functions |
|---|---|---|
| `cable` | §1 | attenuation, temperature, delay |
| `passive` | §2, §11 | table lookup that errors outside the rated band, coupler through loss, `device_loss_db(element, in_port, out_port, f_mhz)`, `carries(element, f_mhz, direction)` |
| `noise` | §4.1–§4.5 | thermal floor, CNR, cascade, CTB, CSO, CIN, `mer_src_db`, the MER sum, ingress term, ISI term |
| `fec` | §4.6 | effective margin, OFDM and SC-QAM curves |
| `echo` | §5.1, §5.2 | cavity delay and level, ripple |
| `preeq` | §5.3–§5.5 | 24-tap SC-QAM inverse and metrics, OFDMA normalization, RxMER and response summaries, group delay |
| `ac` | §6 | constant-power solve, supply loading |
| `upstream` | §7.1–§7.3 | transmit limit, `P_tx`, thermal funneling, upstream SNR |

### 2. `transfer(a, b, f_mhz)` (§11)

Built on `PlantGraph::path_between`. Sums cable and device loss along any two points in either
direction, and returns the delay and the actives crossed. It reports passive loss only: an active's
gain depends on what's above it, which only the tree walk knows, so §3.2 and §7.1 use the walk.

### 3. The tree walk

`Walker` makes one pass from the root over a grid of frequencies, branching at each element, so its
cost is elements × frequencies and not modems × path length × frequencies. One pass gives every
modem's:

- `level_dbmv(f)`: §3.2, without echoes.
- `h(f)`: the complex response from cavities (§5.1) and active group delay (§5.6).
- `inv_plant(f)`: the linear noise terms above the modem (§4.5).
- Upstream local loss, and the excess loss above the first active (§7.1, §7.2).
- Per root port, the return noise at the node (§7.3).

A scalar question is a grid of one. There is one code path.

**How an active works.** A healthy active holds `out_level(active, f)` (§3.1) at 20 °C. Each amp's
forward gain was set at alignment, so it only passes on deviations:

    dev_out(f) = dev_above(f) − (loss(f) − loss_ref(f)) + agc(f) + drift(f)
    L_out(f)   = out_level(f) + dev_out(f)

`loss_ref` is the same section healthy at 20 °C. So a suckout on a span shows at every modem below
it, through any number of amps, which no other formula would give without a script (invariant 5).

**AGC.** One pilot at the band's top reads `dev` at the amp's input and corrects up to ±`agc_range_db`,
with a correction shaped like the section's cable loss, which is what a temperature change looks
like. So a temperature swing inside the range leaves the §3.1 targets exact, and the §3.2 whole-fixture
oracle holds. Any fault's shape mostly survives. An amp's own drift sits after its AGC, or the AGC
would hide it.

**The return path.** Unity gain back to the node (§7.1), so a modem's loss is only what lies below
its first active, plus any *excess* over the healthy 20 °C value above that, plus `return_atten_db`.

### 4. The overlay

`Overlay` is what a fault set becomes: point impairments on spans and ports, wet sections, amp
overrides and ingress. It's empty here and filled from a `FaultSet` in step 4. It is built now so the
walk has one shape, and the tests below exercise it by hand.

### 5. Physics design checks

`check_physics(graph, samples, overlay) -> Vec<PhysicsIssue>`, the §3.3, §3.4, §6.3 and §7.2 checks
that `PlantGraph::check_design` said would come later, plus `level_budget` for reading the numbers.

### 6. Latency

`examples/latency.rs` measures a full forward and return pass on the fixture and a 500-modem tree.
The result is recorded under "Decisions made during the task".

## Tests

Oracles, each from `docs/physics.md`:

- §1: 1.98 dB/100 ft at 750 MHz, 24.8 dB over 1,250 ft, 26.04 and 22.82 dB with temperature, 0.2337 µs.
- §2: 23 dB tap insertion at 750, 204 and 30 MHz (1.24, 0.89, 0.80 dB), a lookup outside the rated
  band is an error, 1.05 dB coupler.
- §3.2: 7.24 and 7.42 dBmV at the §3.2 modem. The whole-fixture ranges, spreads and totals at −20,
  20 and 45 °C.
- §4: −81.22 dBmV, 68.4 dB, 60.62 dB, 41.88 and 40.93 dB, 39.46 and 37.49 dB with ingress.
- §4.6: the flat and notch FEC oracles.
- §5: 0.3506 µs, −33.9 dB, 0.35 dB and 2.85 MHz, 5.69 dB and 2 MHz, peak at tap 10, MTR 29.0 dB,
  39.7/1.71/30 dB, 0.4/0.894/2.4, 350.6 ns.
- §6: 86.47 V, and the fixture's voltages by hand.
- §7: 48.01, 47.22, 44.21 dBmV. 29.52 and 23.50, 38.01 and 31.99. The whole-fixture table. −44.13 dBmV,
  52.13, 39.74 and 54.13 dB.

Beyond the oracles:

- A grid of one equals the same frequency in a larger grid, to the bit.
- `transfer` from the root's section matches the walk's loss on every modem.
- A suckout placed by hand in the overlay on span-1 appears at every modem, and on span-3 only below it.
- Temperature inside AGC range leaves output on target; past it, the level moves by the excess.
- Determinism: the same walk twice gives byte-identical output, and reordering the plant's lists
  changes nothing.

## Out of scope

Building an overlay from a `FaultSet`, modem state (§7.4), the observation records, randomness and
domain randomization are step 4. `r_loop` temperature dependence is deferred.

## Acceptance

- `cargo test -p sim` is green, with every oracle above asserted on both profiles where it has a
  value on both, and clippy clean.
- The fixture produces no `PhysicsIssue` at −20, 20 and 45 °C (the CLAUDE.md rule).
- No function in `crates/sim` reads a clock or an RNG.

## Deferred

- **`r_loop` with temperature.** `docs/reference/plant-and-powering.md` suggests about 10 % at 45 °C. §6.2
  ignores it, and it matters as much as §8's jitter, so step 12's sensitivity report should say.
- **Two-pilot AGC.** The one-pilot, cable-shaped correction is enough for §1.2. A pilot-loss fault
  needs the real thing.

## Decisions made during the task

- **The §3.2 whole-fixture oracle counts what's at a modem's input, not what it demodulates.** Its
  +13.97 dBmV is a D3.0 modem at 1218 MHz, above that modem's 1002 MHz rating. The signal is there,
  so `ModemBudget` keeps both: `level_*` and `total_input_dbmv` over the whole band, as the oracle
  says, and `demod_*` over what the modem demodulates, which §3.3's window applies to.
- **A minimum-phase notch has negative group delay at its centre.** §8 says only "so it has group
  delay". The sign follows from the Hilbert relation and is checked against the ordinary low-pass
  result, where delay rises toward the band edge.
- **Echoes between ordinary parts are computed down to −50 dB and fault echoes to −80 dB,** read at
  100 MHz with passives priced at 500 MHz. Fixed points, so a walk's answer at a frequency doesn't
  depend on what else is on its grid. That is tested to the bit.
- **Amp noise excludes the root.** The §4.5 oracle lists one CNR and one CIN for the one amp in that
  modem's cascade, so `MER_src` stands for the root.
- **Downstream ingress reads the signal at the entry point.** `Loss(entry → modem)` in §4.5 cancels,
  and amps gain signal and ingress alike, so this also holds across amps.

**Latency, measured:** a full forward (3,800 frequencies) and return (1,900) pass takes 7 ms at 8
modems, 100 ms at 108 and 520 ms at 508, on this machine in release. So about 1 ms per modem. Nothing
has been optimized, and frames are cloned at every branch. Step 13 sets the budget.
