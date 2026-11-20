# Task 27 — An SC-QAM modem's equalizer

**Build-order step:** before task 26's hazards, at the user's call ("Fix, then build"), from task 19's finding.
**Depends on:** Task 3 (§4.5), Task 19
**Produces:** an SC-QAM demodulator whose adaptive equalizer takes out an echo within its span, so a strong short echo no longer
takes a DOCSIS 3.0 modem offline; `docs/physics.md` §4.5 with its oracle; pins moved on purpose.
**Load:** `CLAUDE.md`; `docs/physics.md` §4.5, §5.1, §20.1; `docs/reference/impairments.md` §2.

## Goal

§4.5 kept an SC-QAM channel's residual at the level of its largest echo: no equalizer at all. Task 19 found what that does to an
open cut: the cut's echo through the isolation of the tap above it is about 15 dB down, so every DOCSIS 3.0 modem on that tap
loses its channels, while the 3.1 modems beside it keep 41 dB of RxMER. Real modems don't: DOCSIS assumes a cable modem works
through a dominant echo of −10 dBc at up to 0.5 µs, −15 dBc to 1.0 µs, −20 dBc to 1.5 µs and −30 dBc beyond, the downstream
channel its receiver is specified against [DOCSIS 3.0 PHY, via the IEEE 802.3 EPoC summary noll_01a_0312] [guideline]. The
receiver's adaptive equalizer is what makes that work, and its span sets the longest echo it can take out [Hranac, "How Adaptive
Equalization Works", Broadband Library] [guideline].

## The model

An echo of level `E` dB (`E = 20·log10 ρ`) and delay `τ` leaves an SC-QAM channel a residual

    E_res = E − C   for τ ≤ τ_span,    E_res = E   for τ > τ_span,     MER_isi = −E_res

with `C = 30 dB` and `τ_span = 1.5 µs`, both approx:

- **`τ_span`** is where the assumed channel's last step starts: beyond 1.5 µs DOCSIS assumes the echo is already −30 dBc, as it would
  if the equalizer couldn't reach it [inferred].
- **`C`** is what lets the assumed channel's worst echo, −10 dBc at 0.5 µs, leave 256-QAM its margin: a residual of −40 dBc is 40 dB
  of `MER_isi`, under the modem's own ~41 dB at a good level, so the channel works as DOCSIS says it must [inferred]. No source
  gives a CM's cancellation depth.
- The largest residual is kept, as before, and the rest is assumed cancelled. OFDM is unchanged: its cyclic prefix already does this.

## Deliverables

1. `docs/physics.md` §4.5 rewritten for SC-QAM, with the oracle below.
2. `sim::noise::inv_mer_isi_sc`, and `isi_sc` built on it.
3. The worked examples as tests (`crates/sim/tests/oracles.rs`).
4. Every pin the change moves, moved with a history comment that names this task; `one_street` keeps a valid level budget.
5. `docs/reference/impairments.md` §2 with the assumed channel and the equalizer's span, cited.

## Oracles

- −15 dBc at 0.29 µs (task 19's open cut through a tap's isolation): residual −45 dBc, **MER_isi 45 dB** (was 15 dB).
- −10 dBc at 0.5 µs, the assumed channel's worst: **40 dB**.
- −15 dBc at 2.0 µs, past the span: **15 dB**, as before.

## Tests

- The oracles.
- Task 19's case: an open cut no longer takes the DOCSIS 3.0 modems on the tap above it off (`crates/sim/tests/damage.rs`).
- Determinism and every other physics oracle unchanged.

## Out of scope

Retraining. Models from tasks 12 to 24 were trained on rows written under the old residual, and their reported numbers stand for
that physics; a short echo now marks a 3.0 modem's SC-QAM MER less, so the features they read have moved for 3.0 modems. A modem
whose equalizer adapts over time, and an equalizer's noise enhancement in a deep notch.

## Decisions made during the task

### Built (2026-11-19)

As specified. `sim::noise::inv_mer_isi_sc` and `SC_EQ_CANCEL_DB` / `SC_EQ_SPAN_US`, §4.5 and `impairments.md` §2 rewritten, the
oracle in `crates/sim/tests/oracles.rs`, and `a_cut_just_past_a_tap_leaves_that_taps_modems_up` in `crates/sim/tests/damage.rs`.

- **Pins moved:** the sim's two `one_street` digests (`bb42282f8209344b` to `9c8afd723a327e07`, `4ccfe14f3dd83669` to
  `05023d694cea75a9`), with history comments. The version 1 and version 2 design pins held.
- **One test made relative:** `flat_loss_lowers_rxmer_and_raises_transmit` held a flat loss's RxMER ripple under 1.0 dB, against a
  healthy curve whose own ripple is 1.012 dB; the drop's curve went from 0.999 to 1.000. It now holds the ripple to the healthy
  curve's plus 0.25 dB, which is what "a flat loss adds no ripple" means.
- **Task 19's numbers predate this,** and weren't rerun: they stand for the old residual.
- **The echo table is unverified.** It was read from search summaries; the PDFs those summaries came from (an IEEE 802.3 EPoC
  deck, DOCSIS 1.0 and 2.0 guides) couldn't be read, and the DOCSIS 3.0 PHY specification wasn't. `τ_span` and `C` were
  `[inferred]` and approx anyway; the table they're inferred from is now marked unverified too.
