# Task 22 — Physics for the new parts

**Build-order step:** second of four in [`docs/as-built-plan.md`](../as-built-plan.md).
**Depends on:** Task 21
**Produces:** `docs/physics.md` §13 to §19 with worked oracles; the sim pricing every part task 21 added; a region-level optical pass;
observation, environment and fault schemas at their next versions; fiber faults with a location; a measured latency budget.
**Load:** `CLAUDE.md`; `docs/as-built-plan.md`; `docs/realism-review.md` §1; `docs/physics.md`; `docs/reference/design-practice.md`;
`docs/reference/fiber-optics.md`; `docs/reference/plant-and-powering.md` §5 to §7; `docs/reference/impairments.md` §2, §12.

## Goal

The sim computes what each new part does, from its parameters, and every finding S1 to S10 of the review is closed or explicitly deferred.
A plant with none of the new parts still simulates to the same bytes (plan rules 2 and 3).

**Before any code:** write each section below into `docs/physics.md` with a hand-worked example on `fixtures/one_street_as_built/`, and
only then implement it. The example is the test. Task 21 closed by rereading its schema against these sections, so the fields should be
there. If one still isn't, that's a new schema version with its migration and frozen golden, never an edit to version 2 (plan rule 1).
The fixture isn't frozen the way `one_street` is: where an oracle needs a case it lacks, extend it, and update task 21's hand-counted
table.

A formula marked [practice] below is the expected form, written from memory. Confirm it against a source in the notes, or mark it approx in
`docs/physics.md` and list it for the sensitivity report.

## Deliverables

### 1. `docs/physics.md`, new sections

**§13 Premises.** The walk continues past the tap port: drop, ground block, splitter legs, in-home cable, device.

- Each part adds its loss by frequency, and is a reflector with its own return loss, so cavities form inside the home and between the
  home and the tap port under §5.1 unchanged.
- In-home cable sits at a fixed indoor temperature, a named constant. A buried drop follows `ground_temp_c`.
- Two devices behind one splitter see each other through its isolation, as tap ports do in §5.1.
- A filter is a shaped loss with a stop-band mismatch, the same mechanism as §8's frequency-selective loss, here as a healthy part.
- A drop amp has gain and a noise figure each way and is powered in the home, so it's outside §6. Its noise enters §4.5 and §7.3 at its
  own place.
- Upstream, the modem transmits through the same parts (§7.2).
- **Oracle:** the fixture's splitter home, level and transmit power by hand at one frequency each way, and its in-home echo's delay and
  level.

**§14 Fittings, port states and return loss by frequency.**

- A connector or splice on an edge is a small loss and a reflector at its `pos_ft`. A power inserter in the line has the insertion loss
  its part states, where §6.2 only spoke of its resistance.
- Return loss is read from the part's table where it has one, by port class on a tap, and from the single number otherwise.
- An open tap port reflects, and a terminated one reflects at its terminator's return loss. Either reaches other ports through isolation,
  which generalizes §5.1's off-path echo from faults to ordinary plant.
- More reflectors in a section mean many more pairs. The per-pair floor stays, and this section states how much echo power it discards on
  the fixture. Discarded echoes aren't summed into a noise term, for the reason §4.5 gives.
- **Oracle:** two reflectors of 7 dB return loss, 100 ft apart with 1 dB of loss between them, give an echo 16 dB down and 234 ns late
  [SCTE 183 §9.1].

**§15 Alignment.** Replaces "a healthy active holds its target" where an active has `fitted`.

- Output is input, less pad and equalizer, plus the part's gain at that frequency, per output port. An equalizer's and a cable
  simulator's loss shapes come from their value [practice: an equalizer of value E loses about `E·(1 − √(f/f_hi))` plus insertion].
- AGC acts at the part's pilots within its range (§3.4), on the gain, not on the output directly.
- `sim::align(plant, library, profile) → fitted values`: the pass a technician makes, working out from the root, choosing from the plug-in
  values the part's family has, to bring each output to its set point at the pilots. Pure and deterministic. The residual at every other
  frequency is whatever's left.
- Return: pads and equalizers set unity gain to the reference (§7.1), and a multi-output amp's combining loss is in the path and in the
  noise funnel [SCTE 183 §16.4].
- An active without `fitted` behaves exactly as today.
- **Oracle:** the fixture's first amp: input by hand, plug-ins chosen, output at three frequencies, residual. And a check, not an oracle:
  response through the fixture's cascade stays inside N/10 + 3 dB [SCTE 183 §6.2].

**§16 Optical links.**

- Received power is launch power less fiber loss at the link's wavelength over its length, less every splice, connector and passive on
  its hops. A splice with no measured loss takes the loss stated by the part it's made in.
- A link's far end is on the coax plant: a forward link is received by the root's fitted receiver or optic, and a return link is launched
  by its fitted transmitter or optic. The fiber plant's `NodeEnd` is where the two documents meet.
- Analog forward: CNR from received power, modulation depth per channel, the transmitter's relative intensity noise, shot noise, and the
  receiver's thermal noise [practice: `CNR = (m²/2)·(R·P)² / (B·(RIN·(R·P)² + 2q·R·P + i_n²))`]. It replaces the constant `optics_cnr_db`
  when the node has a link, and the constant stays as the fallback when it doesn't.
- Clipping: a noise-like term that rises steeply once composite modulation passes the transmitter's limit, from the loaded channel plan.
  Source the form, or mark it approx.
- Analog return: SNR against total input, rising on the noise side and falling on the clipping side. Anchors: about 35.5 dB at 7 dBmV and
  52.5 dB at 26 dBmV for one tested laser [TR-MIDSPLIT §6.1.1.1].
- Digital: margin is received power less the optic's sensitivity. Below it, or above its overload, the link is down, and the RPD falls
  through `sim::rpd` as a cut does today.
- Latency: 1.5 µs per 1,000 ft [TR-DCA §11.9.3]. Reported, and not yet fed to ranging.
- **Oracles:** the fixture's forward link budget and CNR by hand, the return anchors within a stated tolerance, and the digital link's
  margin.

**§17 Source MER, receiver ceiling and loading.**

- `MER_src` is read from the table on the RPD, or on the CMTS behind an analog node. Without one, PHY Table 41's floors as today.
- An upstream receiver ceiling combines in linear with §7.3's SNR, where the part states one.
- Composite power is summed from the channel plan with tilt. CIN and optical modulation depth read it, where the part has the table or
  the link exists.
- Noise figure and CIN are read from the part's table by frequency where it has one.
- **Oracle:** the §4.5 modem again, with a source table and a noise figure table, by hand.

**§18 Upstream baseline.**

- The environment carries ambient sources: a band, a level per MHz at zero coupling, and an hourly schedule. A home's `coupling_db` says
  how far below that its wiring sits. The product enters the return path at the ground block and funnels under §7.3.
- This is a property of healthy homes, and a fault that opens a shield lowers coupling at its own place.
- **Oracle:** three homes with different coupling under one ambient source, summed at the node by hand.

**§19 Environment.**

- An aerial span's temperature is air temperature plus a solar term from the sample. Loop resistance rises with temperature (§6.2).
- A connector fault takes optional couplings to temperature and wind, as water takes one to rain, so its return loss moves with the
  series and intermittency isn't scripted.
- **Oracle:** the §6.2 feed at 45 °C, and a coupled connector at two samples.

### 2. Schemas, each additive with its migration and frozen golden

- **`observation`:** optical received power at a node and at the hub's return receiver, an RPD's optic status, output level per port. Each
  under a capability flag, and each named in `docs/reference/collection-management.md` with the device that reports it.
  `observation::model_can_read` lets a model trained on the earlier version load, as `fault::model_can_read` does.
- **`environment`:** ambient sources, wind and solar load.
- **`fault`:** `Misalignment` (a wrong pad or equalizer at an active, as deltas), and the couplings on a connector. The next free version.
- **`FiberFaultSet`,** a new document with its own version: `FiberCut { cable, pos_ft, repair_s }`, `EventLoss { location, loss_db }` for a
  bend, a bad splice or a dirty connector, and `TransmitterDrift { element, power_delta_db, rin_delta_db }`. A fiber location is a point on
  a cable, an element, or a splice at an element, named by the strands it joins, since a splice has no part. Held per region, since one
  fault can reach several nodes. Task 19's `AnalogFiberCut` becomes `FiberCut` when 19 is built.

### 3. The sim

- The walk prices every kind task 21 added. `PhysicsError::Unsupported` is left with nothing to name, and its test from task 21 turns over.
- `sim::optics`: from a fiber plant, each node's fitted optics, the library, fiber faults and a sample, each node's link state: received
  power, CNR or margin, up or down, latency. Pure. `Scenario` takes its node's link state as an input, and the twin's recompute key includes it, so a change on a
  cable recomputes the nodes whose links cross it and no others.
- `sim::align`, as §15.
- §8's randomization also draws within the tolerance a part states: a modem's ceiling and noise figure, a plug-in's value, a connector's
  return loss, a splice's and a fiber connector's loss. A part that states none draws nothing, and each draw has its own stream by element
  id, so existing plants don't move.
- `check_physics` adds: inserter port current, plug-in values within what the family has, optical margin, transmit power through the
  premises, and a filter whose stop band covers something the devices below it use, which `qualifies` can't see from a rated band.
- New profile files with the band fully loaded, beside the two shipped, which don't change.
- Several generic modem parts per generation, with different ceilings, noise figures and return loss, each stating a tolerance. The two
  existing modem parts don't change.

### 4. Tools

`get_optical_budget` for a node. `inject_fault` and `clear_faults` take fiber faults at the region. `get_level_budget` reports through the
premises. The page shows a node's optical numbers and lets a fiber fault be placed on a cable, through those tools (invariant 8).

The server can start on `fixtures/one_street_as_built/`, with its fiber plant and region. Task 21 couldn't see its page changes in a
browser, since nothing could load a version 2 plant. They're checked here, on that fixture, along with this task's.

### 5. Latency

`crates/sim/examples/latency.rs` gains a node with full premises on every home. The baseline is the tree before this program, `40d0133`,
exported with `git archive` and run on the same machine. Measured there on 2026-11-10: 5.9 ms at 8 modems, 63 ms at 108 and 314 ms at 508.
`ComponentLibrary::get` is a linear scan, and the library is seven times the size it was, so anything outside `PlantGraph`'s own index
that looks parts up in a loop is the first thing to check. Measured against that baseline, and again after:

- A plant without new parts is no more than 5 % slower.
- With a ground block, splitter, in-home cable and two devices on every home, a full pass is no more than 2.5 times the same homes with
  bare modems. A part with one way through doesn't need its frame cloned, which is where to look first.

Both figures go in this file with the machine they were measured on.

## Tests

- Every oracle in §13 to §19, in `crates/sim/tests/oracles.rs`, against the hand-worked numbers.
- The pinned digests from task 21, unmoved.
- `docs/physics.md` §12's validation suite, unchanged and passing, with the published anchors above added to it and to
  `docs/validation.md`.
- Each mechanism alone: a plant with only that part added differs from the base plant only where that part can reach.
- A cut on a shared cable takes down every node whose link crosses it and none other. A return-only cut leaves RxMER and takes ranging.
- Migration gain depends on link length: the same node at two fiber lengths reports two gains, larger at the longer [TR-DCA §6.2.3].
- `sim::align` is deterministic, and on the fixture picks the plug-ins worked by hand.
- Determinism across everything new at once: same inputs and seed, same bytes.
- Round trip, golden and migration for each schema touched. A model artifact from before this task still loads.
- `one_street` still gives a valid level budget.
- The latency bars.

## Out of scope

Placing any of these parts: task 23. Diagnosis candidates for them, and retraining: task 24. The emulator and adapter carrying the new
observations. Optical second- and third-order distortion. Fiber effects beyond loss and reflection: dispersion, nonlinearity, polarization.
PTP quality as a function of the network between hub and RPD.

## Acceptance

`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` green. Every new equation has a section, an
oracle and a test. Every value without a source is marked approx and named in a list at the end of this file, for the next sensitivity
report. The latency bars hold. The review's S1 to S10 each point at the section that closed it, or at Deferred here.

## Deferred

- **Modem front ends:** noise figure rising with input level, already deferred by task 16.
- **Hum** from an overloaded supply, and standby and shorts, which are task 19's.
- **Structural return loss** along a cable.
- **Latency into ranging and the scheduler.**
- **Fiber faults read per step.** A scenario takes one link state (`Scenario::link`), and the twin reads the region's fiber faults at its
  own time, so a fiber fault's onset, ramp or repair is read once per scenario, not at each sample of the series. Task 19's damage
  events (a cut that's later spliced, a loss that grows) need the link state recomputed per step: `sim::optics::node_link_drawn` already
  takes a time, so it's `simulate` that has to call it per sample instead of taking one link.

## Decisions made during the task

### Progress (one line per section, so a restart picks up from here)

- **§13 done.** `docs/physics.md` §13 and its oracles; `sim::cable::Exposure` (air, ground, indoor at `INDOOR_TEMP_C`), ground block,
  filter (`sim::passive::filter_shape`), `Cpe`, drop amp and leg-isolation echoes in the walk; `oracles.rs` `section_13_*` pass; pins
  hold. The oracle hangs the fixture's homes on `one_street`'s taps, since the whole as-built plant can't simulate until §14 to §16.
  Next: §14.
- **§14 done.** `docs/physics.md` §14; `ReturnLoss::Table` and `ReturnLoss::Behind`, `sim::passive::part_return_loss` (tables, a tap's
  by port class), fittings as healthy points in `cross_edge`, port states as reflectors behind the passive's loss at its input, the
  inserter's loss and mismatch, and `ModemView::discarded_echo_power`; `oracles.rs` `section_14_*` pass; pins hold. The as-built
  fixture's discarded-echo figure waits for §16, when it first walks whole. Next: §15.
- **§15 done.** `docs/physics.md` §15 (and §14's terminating tap); fitted amps in `cross_active` (input plug-ins and gain, per-port
  outputs, AGC against the aligned input via `out_dev_ref_db`, a root's set point, return residuals and combining loss in the funnel),
  line equalizers and tap conditioners in `cross_passive`, `Walker::capture`, `sim::align`, and `transfer` now crossing fittings and
  conditioners. The fixture holds `align`'s choice on both profiles (amp-2 differs by profile), and task 21's count follows. `oracles.rs`
  `section_15_*` pass; pins hold. Still refused: the root's optics (§16) and `r_loop_k_temp` (§19). Next: §16.
- **§16 done.** `docs/physics.md` §16 and its oracles; `sim::optics` (budget, SCTE 270 §25.7's four terms and a Gaussian clipping term
  derived from zero-light clipping, digital margin, latency); `Overlay::optics_cnr_db`, `Scenario::link`, `rpd_state`'s `link_down`;
  `design::migrate::plan_migration_with_link`. The fixture's forward now reaches its node through a hub 1×8 coupler and its return lands
  on `rrx-generic-hfc`, since the generic parts overloaded both receivers; three generic parts added, task 21's count follows. §14's
  discarded-echo figure is in. `section_16_*` and `migration_gain_depends_on_link_length` pass; pins hold. Still refused:
  `r_loop_k_temp` (§19). The twin passes no link yet (the tools step). Next: §17.
- **§17 done.** `docs/physics.md` §17 and its oracles; source MER from the RPD's table or the bound CMTS's (carried on `NodeLink`), an
  analog node's own RF term, the upstream receiver ceiling combined with the optics, `nf_table` and `cin_table` by frequency, and
  `sim::noise::composite_dbmv` with CIN following it where a part states `ref_composite_dbmv`; `agc_pilots_mhz` joins the priced list
  (§15 reads it). `section_17_*` pass; pins hold. Still refused: `r_loop_k_temp` (§19), and `gain_flatness_db` if a part states it,
  which no section prices. Next: §18.
- **§18 done.** Full workspace tests and clippy run first, before any schema change: green apart from a task 21 twin test that still
  expected `amp-1`'s pre-§15 equalizer, now fixed. Environment version 2 (`ambient_sources`, `wind_m_per_s`, `solar_w_per_m2`) and fault
  version 4 (`ConnectorReflection::couplings`), each with migration, frozen `environment.v1.json` and `fault_set.v3.json`, and goldens
  for the new fields; `one_street`'s files stay at their versions and read through migration. `docs/physics.md` §18;
  `sim::faults::ambient_at`, ambient entering at a ground block and at an opened connector. `section_18_*` pass; pins hold. Next: §19.
- **§19 done.** `docs/physics.md` §19; solar gain on aerial exposure (`SOLAR_GAIN_C_PER_W_M2`, approx), `cable::r_loop_ohm_per_kft`,
  `ac::solve_ac_at` (the sim solves at each sample, `check_physics` keeps each active's lowest voltage over its samples),
  `faults::connector_rl_db` at the fault's own temperature and the wind. `gain_flatness_db` is a rating, not an RF field, so
  `unsupported` names nothing on the as-built fixture: task 21's refusal test turned over (sim and twin). `section_19_*` pass; pins
  hold. Next: full workspace tests and clippy, then the schemas, the sim's remaining items, tools and latency.
- **Checkpoint, then schemas.** Full workspace tests and clippy green after §19. Observation version 2 (`ServiceGroupObservation::optics`,
  `ActiveObservation::out_levels`, `model_can_read`, `migrate_version`; the proposal models read through `model_can_read`), profile
  version 2 (`plant_reports`, four flags, since `CapabilitySet` sits inside the closed component schema), fault version 4's
  `Misalignment` (cause `AmpDrift` until task 24), and `schema::fiber_fault` version 1. Frozen version 1 observation and profile files,
  goldens for every new field, each new reading named with its device in `collection-management.md`. `sim::optics` reads fiber faults
  (`node_link_at`, `region_links`; no light is `None`), the sim emits the optics and per-port readings under the flags, a modem with
  every upstream lost can't register, and a dark return leaves the CMTS hearing nothing. `sim/tests/fiber_faults.rs` passes; pins hold.
  Next: the twin and tools.
- **The sim's remaining items.** Randomization draws within a stated tolerance (modem ceiling and noise figure, plug-in values, fitting
  return loss, splices and fiber connectors in `sim::optics::node_link_drawn`, and the version 1 amp and passive draws within their
  part's range), each on its own stream, so task 21's tolerance refusal is gone and its test turned over. `check_physics` adds inserter
  port current, a misalignment beyond the family, a filter in the way, transmit power into a tuner through the home
  (TR-MIDSPLIT §6.2's 21 dBmV at the tuner, our arithmetic), and `optics::link_issues` the optical margin. Two fully loaded profiles
  (`*_full.json`, 99 % of the downstream, 86 % and 93 % of the upstream) and four generic modems with tolerances. `sim/tests/checks.rs`,
  `tolerance.rs` pass; pins hold. Next: the twin and tools.
- **The twin and tools.** `Twin::link` (a node's region, fiber plant and fiber faults at the twin's time, with its own draws) feeds
  every sim the twin runs and the cache key, so a cut on a cable recomputes the nodes whose links cross it and no others
  (`twin/tests/as_built.rs`). `get_optical_budget`; `inject_fault`, `list_faults` and `clear_faults` take fiber faults at a region;
  `get_level_budget` reads each home from its tap port; fiber faults persist in the meta document. `twin::load_as_built` and the
  server's `--as-built` start on `fixtures/one_street_as_built/` (`.claude/launch.json` has `twin-as-built`). In the browser pane on
  that fixture: the Design tab's checks, home loss and Optics table, a cut on `fiber-2` from the Workbench taking the low-split node
  down and leaving the RPD normal, clearing it, and Diagnose answering. The pane has no WebGL, so the map and its popup (task 21's
  rows) weren't seen drawn; the GeoJSON they read carries every new kind, fitted values, fittings and support. Next: latency.
- **Latency, measured.** See "Latency" below. Next: final workspace tests and clippy, and the report.
- **Closing fixes, after review.** Fix 3: fiber faults read once per scenario recorded under Deferred. Fix 1: the frame pool; the 8-modem
  bar now holds (+1.2 %), pins hold. Fix 2: the map drawn in headless Chromium with SwiftShader (as `docs/ux-review.md` did) on the
  `--as-built` server's low-split node: spans, drops, house cable, actives, passives (equalizer, inserter, terminator), premises
  (ground blocks, filter, drop amp, CPE), 18 structures and the supply all drawn, light, dark and 375 px, console clean. Two things
  were wrong and are fixed in `ui/index.html`: structures drew over the plant and a click took the first hit, so a click on an amp, an
  inserter or a span between poles opened the pole; structures now draw beneath, and a click takes a device before the structure it
  hangs from and any point before a line. And fitted values printed as one JSON line that wrapped the label column letter by letter;
  the popup now has a row per slot (input pad and equalizer, return pad and equalizer, each output's pad and equalizer, forward
  receiver, return laser, conditioner, ground block coupling), each tap port open or terminated with its part, and the set point.
  Seen: amp-1's eight fitted rows, span-2's three fittings and its strand, tap-1's conditioner and terminated ports, node-1's optics.

### Latency

Intel Core i9-9900K at 3.60 GHz, 16 threads, Linux 7.2, release build, idle apart from the run. The baseline is `40d0133` from `git
archive`, built in its own directory; both trees' `latency` examples run alternately as fresh processes, 15 rounds, first pass each, as
the baseline's example measures it (median, range):

| Plant | `40d0133` | Now | Change | Bar |
|---|---|---|---|---|
| 8 modems, no new parts | 5.86 ms (5.76-6.37) | 5.93 ms (5.85-7.05) | +1.2 % | 5 %: holds |
| 108 modems, no new parts | 63.7 ms (61.8-65.8) | 54.0 ms (52.2-58.2) | −15.2 % | 5 %: holds |
| 508 modems, no new parts | 315.6 ms (309-324) | 254.9 ms (249-262) | −19.2 % | 5 %: holds |
| 108 homes, full premises against bare | | 112.0 ms, **2.08×** (2.00-2.12) | | 2.5×: holds |
| 508 homes, full premises against bare | | 556.8 ms, **2.19×** (2.17-2.22) | | 2.5×: holds |

Full premises: ground block, 2-way splitter with leg isolation, house cable with F connectors at both ends of every run, a modem and a
set-top on every added tap port; the fixture's own eight modems stay bare. Four changes got there from 6.9×: the walk doesn't descend a
subtree nothing reads (a set-top's leg), the last branch read takes its frame rather than a copy, echoes a home's reflector takes part in
are synthesized by chunks (`docs/physics.md` §14, approx, within 1e-6), and frames reuse buffers (below). Every change leaves a plant
without new parts at the same bytes: the pins hold.

**The 8-modem bar, after review.** It first measured +14 to +21 %. Warm, or with glibc's heap trimming off (`MALLOC_TRIM_THRESHOLD_`),
the gap was 4 to 7 %: most of it was the allocator giving heap back between the passes a measurement makes (the AGC pilot pass, the
forward and the return) and faulting it in again. A dropped frame now leaves its buffers in a small per-thread pool (`walk::Spare`, 32
kept), and the next frame copies into them, so a pass reuses the last pass's memory. Measured by the same alternating method, 15 runs
each: +1.2 %. Bare passes on larger plants got faster too, which is why the full-premises ratio rose from 2.03× to 2.19× while the
full-premises time fell.

### Decisions

- **The fixture's optics.** The generic forward transmitter (10 dBm) overloads the node receiver (+1 dBm) on the fixture's 3,000 ft, and
  the generic return receiver is SCTE 174's RFoG one (−21 to −12 dBm), which a node's 3 dBm laser overloads on any short link. So the
  forward runs through a hub 1×8 coupler (`osplit-generic-1x8`, SCTE 270's typical 50/50 loss, our arithmetic) and the return lands on
  `rrx-generic-hfc` (−15 to +3 dBm, from memory, `Generic`). Task 21's count follows.
- **Clipping** is derived, not sourced as a formula: the Gaussian tail beyond zero light (SCTE 270 App. F's clipping at zero, SCTE 183
  §16.2's Gaussian composite), with all of it taken as in band (approx).
- **The return laser model** is §16's four terms at the laser's own drive, so NPR curves come out of RIN, shot, thermal and clipping.
  `rtx-generic-1310-npr` is fitted to TR-MIDSPLIT's two points; the existing `rtx-generic-1310` (OMI 10 %) clips at 26 dBmV, which is
  consistent with its stated 22.6 dBmV clip, so it stays as it is.
- **Drive.** Transmitters are driven at their reference input per 6 MHz (approx: the CMTS's drive isn't modelled), so the per-channel
  index is the part's. On the full low-split profile the fixture's forward transmitter (3.58 % per channel) then clips: µ = 0.32, about 37
  dB. A full load needs a lower index, which no generic part states; task 23 picks transmitters.
- **No light** is `None` in every link result and observation, since JSON has no infinity.
- **CMTS tables** reach the walk on `NodeLink` (from the fiber plant's binding), as the link state does, rather than as a new plant field.
- **New capability flags** sit on the profile (`PlantReports`, profile version 2): `CapabilitySet` is embedded in the closed component
  schema's modem parts.
- **RPD optic status** is in observation version 2's `OpticsObservation`, not in `schema::rpd`, so one schema moved, not two.
- **`Misalignment` maps to `AmpDrift`** in `CauseClass::from` and the sampler until task 24 decides whether it's a cause of its own.
- **"Plug-in values within what the family has"**: which family and slot a fitted part may fill was already checked when a plant is read
  (task 21), so the check reads a misalignment's deltas against the family's range.
- **"Transmit power through the premises"** is TR-MIDSPLIT §6.2's concern: a modem's transmit into a tuner through the home's splitter,
  against 21 dBmV per 6.4 MHz at the tuner (our arithmetic on the source's two cases). Transmit power against the modem's own limit
  through the home's losses was already checked, since §13's walk crosses the home.
- **`gain_flatness_db`** is a rating, not an RF field: no shape to price, and `gain_table` carries shape.
- **Registration needs ranging.** A modem with every upstream lost (a return link down) can't register again; `sim::state` let it before,
  which no plant could reach until a return could be cut. No pinned output moves.
- **Fiber faults** are read at the twin's time, once per scenario, not per step (approx: a ramp across a long series reads its start).
- **Ambient sources** act on the return band only, and a home with no ground block, or one with nothing fitted, picks up nothing.
- **Temperature at a fault** is the edge's into its element for a port location (indoor on house cable), a span's own for a span.
- **Not built:** RF output from an optical receiver outside its window (`check_physics` flags it),
  modem return-loss draws (a modem's tolerance draws its ceiling and noise figure, as the spec lists).

### Values without a source (approx), for the next sensitivity report

| Value | Where | Section |
|---|---|---|
| `INDOOR_TEMP_C` 20 °C | `sim::cable` | §13 |
| Open port return loss 0 dB | `sim::walk` | §14 |
| Filter edge 2 MHz without `transition_mhz` | `sim::passive` | §13 |
| A fitted root holds its set point | `sim::walk` | §15 |
| √f plug-in shape | `sim::passive` | §15 |
| Fiber loss held flat past a cable's stated wavelengths | `sim::optics` | §16 |
| EDFA at constant output, whatever the input | `sim::optics` | §16 |
| A pluggable launches mid-range | `sim::optics` | §16 |
| Transmitters driven at their reference per 6 MHz | `sim::optics` | §16 |
| Clipping power all in band | `sim::optics` | §16 |
| `rtx-generic-1310-npr`'s fit: 80 MHz loaded, −6 dBm received | library, `oracles.rs` | §16 |
| `rrx-generic-hfc`'s input range, −15 to +3 dBm | library | §16 |
| Node RF output held whatever the received power | `sim::walk` | §16 |
| The composite read at the profile's design tilt for a set-point station | `sim::walk` | §17 |
| A transmitter whose AGC holds composite isn't modelled | `sim::optics` | §17 |
| Ambient level and a home's coupling (inputs; the fixture's 60 dB is `Generic`) | environment, plant | §18 |
| No pickup without a fitted ground block | `sim::walk` | §18 |
| `SOLAR_GAIN_C_PER_W_M2` 0.02 | `sim::cable` | §19 |
| No conductor self-heating | `sim::ac` | §19 |
| Connector couplings linear in temperature and wind | `sim::faults` | §19 |
| A modem's highest transmit read at the tuner band's bottom | `sim::budget` | checks |
| Chunked echo synthesis for a home's reflectors (numerical, under 1e-6) | `sim::walk` | §14 |
| Four generic modems' ceilings, noise figures, return losses and tolerances | library | §3 |
| Fiber faults read once per scenario | `twin` | §16 |

### Review findings S1 to S10

| Finding | Closed by |
|---|---|
| S1 No premises | §13 (task 21's schema, this task's walk), `checks`: tuner and filter |
| S2 Alignment exact, one set point | §15 `sim::align`, set points, residuals; `Misalignment` |
| S3 Return loss flat; connectors, splices, open ports missing | §14 |
| S4 Source MER a staircase | §17, where a part states a table; the generic parts state none, so healthy curves keep the step until one does (Deferred, task 23's parts) |
| S5 Modems identical | Four generic modems with tolerances, §8's draws within them (`tolerance.rs`) |
| S6 No upstream baseline or receiver ceiling | §18 ambient sources, §17 receiver ceiling, §16 return laser against total input |
| S7 Optics constants, fiber not plant | §16 `sim::optics`, fiber faults, `get_optical_budget`; migration gain by link length |
| S8 One temperature, no wind or sun | §19 |
| S9 Amp noise and distortion flat | §17 noise figure and CIN tables, CIN against composite; no generic part states them yet |
| S10 Loading 40 to 60 % of the band | §17 composite; the `*_full.json` profiles |
