# Realism review: the sim and the designer

Reviewed 2026-11-10 at commit `40d0133` on `master`. Read for it: `crates/sim`, `crates/design`, `crates/plant-graph`, the plant, component,
region, environment and fault schemas, both libraries and profiles, `docs/physics.md`, the reference notes, and the specs for tasks 08, 09,
10, 14, 16 and 19.

The numbers come from `cargo run --release -p design --example realism_survey`, added with this review. It designs generated maps of 600
homes at seed 1 and the Broomfield OpenStreetMap extract, on both profiles, and reads a few healthy nodes through the sim. Add `-- sourced` for
the data-sheet parts. **Every plant and reading here is synthetic** (invariant 11).

Claims about real plant carry one of two tags:

- **[note]**: stated in `docs/reference/`, and cited there to its source.
- **[practice]**: common field practice, written from memory, with no note behind it. These are leads. None should reach
  `docs/physics.md`, a profile or the library until a reference note sources it from a public document.

Nothing below was changed in the sim or the designer. This is the review, and section 4 proposes the work. The plan that followed is
[`docs/as-built-plan.md`](as-built-plan.md), with a spec for each of tasks 21 to 24. It took a default on each decision listed at the end of
this file.

## Verdict

The fault physics is the strong part. A fault is physical parameters, its signature is computed, noise is priced where it enters, and every
equation has a hand-worked oracle. None of that needs redoing.

What's weak is everything a fault is seen against, and the plant it sits on:

1. **Healthy plant is too uniform and too clean.** Every active is aligned exactly, every modem of a generation is identical, and the
   upstream has no noise but its amps' own. Across a whole design, healthy RxMER means sit within about 2 dB from the 5th to the 95th
   percentile.
2. **The home doesn't exist.** A modem hangs straight off its drop. The notes put 70 % of problems in the home or yard.
3. **The estimates that change a diagnosis most are modelled most simply.** The sensitivity report ranks tap and active return loss first and
   second. Each is one number per part, flat in frequency, and connectors, splices and open tap ports aren't in the plant at all.
4. **Fiber and optics are two constants** on the node part. Nothing between the headend and the node exists as plant.
5. **The designer builds a valid plant, not a plausible one.** It meets every limit the sim checks, and gets there in ways no field design
   does: 60 to 88 % of low-split taps at the highest value, 7 to 28 % of hardline laid as slack to stand in for a pad, one cable size,
   one-output amps, and a two-way splitter at every branch.

## 1. The sim

### Findings, in the order I'd fix them

| # | Finding | What it costs | Size | Known already |
|---|---|---|---|---|
| S1 | No premises: modem directly on the drop | The largest fault population has nowhere to live, and modem levels have no natural spread | Large, schema | No |
| S2 | Alignment is exact, and every active shares one set point | No level or response error to tell a fault from | Medium, schema | No |
| S3 | Return loss is flat in frequency; connectors, splices and open ports are missing | Echo structure on healthy plant is too sparse and too regular | Medium | Partly: sensitivity report |
| S4 | Source MER is the spec's floor, as a staircase | A 0.7 dB step at 1002 MHz in every healthy high-split curve | Small, moves oracles | No |
| S5 | Modems are identical within a generation | No population spread | Small | Partly: task 16 deferred |
| S6 | Upstream has no baseline noise and no receiver ceiling | Healthy SNR is one number per node type | Medium | Partly: task 16 deferred |
| S7 | Optics are constants, and fiber isn't plant | Every analog node has the same optical noise at any distance | Large, schema | Task 10 deferred |
| S8 | One air temperature for everything, and no wind or sun | No span-to-span or intermittent behaviour | Small to medium | Physics §10 says so |
| S9 | Amp noise and distortion don't vary with frequency or loading | RxMER on deep cascades is optimistic | Small | `docs/library.md` |
| S10 | The channel plan loads 40 to 60 % of the band | Full-band capture, composite power and total input are off | Small | No |
| S11 | Powering is an ideal source on resistors at 20 °C | Supply counts, not diagnosis | Small | Notes |

**S1. No premises.** `Plant::validate` requires a modem to sit on a drop (`crates/schema/src/plant.rs:613`), and the walk goes tap port,
drop, modem. There's no ground block, splitter, in-home cable, second outlet, drop amp or filter.

- The notes put 70 % of problems in the home or yard [note: PNM-HFC Tables 41, 42], and an in-or-near-home call assumes one reflector is in
  the home [note: PNM-HFC §8.12.1]. The scope rule "one modem in a home but not another is in-home" [note: §8.8.9.3] has nothing to act on,
  since a home has one modem.
- Modem levels cluster at their design values, so `datagen` fakes the spread: half the modems get a `BadDrop` of 0 to 14 dB, flat and with no
  reflection (`crates/datagen/src/spectra.rs:33`, `:209`). In-home loss isn't flat, and it comes with a short echo cavity.
- Home ingress has no entry point of its own.

*Suggest:* premises as plant. A drop ends at a ground block, then a splitter, in-home cable and one or more devices, each a part with loss,
isolation and return loss. A bare modem on a drop stays valid, since some homes are wired that way.

**S2. Alignment is exact.** At 20 °C with no fault, every active's output equals the profile's target at every frequency. The walk carries
`ref_loss_db`, the loss alignment "saw", and an amp outputs target plus the deviation from it (`crates/sim/src/walk.rs:596`–`:627`). That's an
equalizer matching cable, tap insertion and splitter loss exactly, with continuous settings. `check_amps` tests gain and input at the top
frequency only (`crates/sim/src/budget.rs:285`).

- A real station has an input pad, an equalizer and gain and slope controls [note: SCTE 183 §16.4], in discrete values [practice]. An equalizer
  follows cable's shape, and passive loss isn't cable-shaped, so a residual is left.
- The guideline for response is N/10 + 3 dB peak to valley after N amps [note: SCTE 183 §6.2]. The sim's is 0 dB at any N.
- `out_level_dbmv(profile, f)` gives the node and every amp one level and one tilt (`walk.rs:222`).
- Randomization moves an amp's gain by ±1 dB flat, a passive by ±0.5 dB flat and cable by ±3 % (`crates/sim/src/faults.rs:502`). Alignment then
  absorbs the passive and cable draws. Nothing has a shape in frequency.

*Suggest:* alignment as stored plant input. Each active carries its pad and equalizer values and its set point, chosen from the part's plug-in
values by an alignment pass that does what a technician does. Output is then computed from input, pad, equalizer and the part's gain, and the
residual is physical. A wrong pad becomes a parameter change, where today it has to be an `AmpDrift`.

**S3. Reflectors.** A part has one `return_loss_db` at every port and frequency (`crates/schema/src/component.rs`). Only devices reflect.

- The sensitivity report (`ml/artifacts/sensitivity.json`) moves one estimate at a time: tap return loss changes 25 % of answers and active
  return loss 19 %, the two largest. Cable loss is third at 18 %. Noise figure, power draw and loop resistance change under 1 %.
- Hardline's own structural return loss is specified at 30 dB to 1002 MHz and 24 dB above [note: SCTE 15 §15.5]. The sim's cable reflects
  nothing.
- Loose seizure screws and unterminated tap ports are named echo sources [note: PNM-HFC §I.5.2], and isolation is specified with every port
  terminated, which the field doesn't always do [note: PNMP §5.2.1.2]. The plant has no connector, and a tap port has no state. In the survey,
  46 to 78 % of tap ports have no drop.
- Echoes are single bounces between pairs in one section, and an amp isolates completely (`walk.rs:677`, `:818`). That's a fair first order.

*Suggest:* return loss as a table by frequency and by port class. Connectors and splices as parts on an edge's ends, with a small loss and a
return loss each. A tap port state: drop connected, terminated or open.

**S4. Source MER is a staircase.** `mer_src_db` returns PHY Table 41's minimums: 50 dB below 600 MHz, 47 dB to 1002 MHz, 45 dB above
(`crates/sim/src/noise.rs:50`). On the high-split profile the first OFDM channel runs 835 to 1025 MHz, so the step falls inside it.

- Measured on the healthy high-split fixture: `modem-2` reads 41.9 dB at 1001.9 MHz and 41.2 dB at 1002.1 MHz, and the other three modems
  step by the same 0.7 dB. That step is the channel's whole standard deviation, 0.23 dB, against 0.01 dB on the second channel.
- A requirement's floor is standing in for a device's value. A model can learn the step as what healthy looks like.

*Suggest:* source MER as a table on the node, RPD or CMTS part, continuous in frequency, with Table 41 kept as a check on it. It moves §4.5's
oracle, so `docs/physics.md` changes first.

**S5. Modems are identical.** `modem-generic-d31` is every D3.1 modem: 44 dB ceiling, 10 dB noise figure, one return loss. Randomization draws
nothing for a modem. Task 16 already defers the front end's noise figure rising with input and the spread in edge roll-off.

*Suggest:* several modem parts per generation, and a per-modem draw on ceiling and noise figure in §8's randomization.

**S6. Upstream has no baseline.** Return noise is each return amp's thermal noise plus whatever a fault injects (`walk.rs:652`), then a
constant for analog optics (`crates/sim/src/telemetry.rs:806`).

- Healthy upstream SNR in the survey: 38.5 to 39.6 dB on every analog node, because the fixed 40 dB optics term dominates. 46 to 57 dB on RPD
  nodes, set by the amp count alone. Every channel on a node reads the same.
- Task 16 defers a receiver ceiling: healthy OFDMA reads 54 dB where a published example shows "over 40".
- Return lasers have a usable input window: 35.5 dB MER at 7 dBmV total and 52.5 dB at 26 dBmV in one test [note: TR-MIDSPLIT §6.1.1.1]. The
  sim's optics term doesn't move with load.

*Suggest:* a burst receiver ceiling on the RPD and CMTS part. A per-premises ingress level with a time-of-day schedule, funnelled through that
home's own path loss, as a property of the home and not a fault. A return laser curve against total input.

**S7. Optics are constants.** `optics_cnr_db` 48 and `us_optics_snr_db` 40 sit on the node part (`component.rs:312`). No length, received
power, wavelength or load enters.

- The notes say link length sets the size of the gain from DAA: strong beyond 25 km, much lower within 8 km [note: TR-DCA §6.2.3]. Every
  migration comparison today reports the same gain at any distance.
- On an RPD, `CinLinkCut` is a switch. Task 10 deferred `ExtraOpticalLoss` because it needs an optical budget as plant input, or it would be
  scripted.

*Suggest:* section 3's fiber plant, and a link budget in `docs/physics.md`: received power from launch power, fiber loss by wavelength, splices
and connectors, then CNR on an analog link and margin on a digital one. Received optical power becomes an observation the sim can produce.

**S8. Environment.** Every aerial span and every drop is at `temp_c`, and every buried span at `ground_temp_c` (`crates/sim/src/cable.rs:31`).
Loop resistance has no temperature term, though the notes estimate +10 % at 45 °C. A connector fault is a fixed return loss, where the notes
say rapid reflection swings may mark a loose connector [note: PNM-HFC §8.11.4].

*Suggest:* wind and solar load in the environment series. A temperature and wind coupling on connector faults, as rain coupling works on water
today, so intermittency comes out of the series.

**S9. Amp noise and distortion.** Forward noise figure is one number, where the BLE120's sheet gives 9 dB at 1218 MHz and 7 dB at the low edge
(`docs/library.md`). CIN is one number per amp from the output at the top of the band, at 1 dB per dB (`noise.rs:39`), where the notes expect
nearer 2. CTB and CSO are stored and unused. The sensitivity report gives these under 5 % of answers, so this ranks low.

**S10. Loading.** Both profiles carry 32 SC-QAM channels from 600 to 792 MHz and one or two OFDM blocks, which is 40 % of the low-split band
and 60 % of the high-split one. Nothing is carried below 600 MHz on either. A full-band capture there is noise, and distortion and optics can't
depend on composite power. `level_budget` integrates total input as
if the whole band were loaded (`budget.rs:201`), which is conservative. *Suggest:* fill the plans, and compute composite power from them.

**S11. Powering.** An ideal source at its rated voltage, resistive spans at 20 °C, actives that pass power between all ports
(`crates/plant-graph/src/power.rs:167`), and a supply attached to an element with no inserter. No port or fuse limit. Task 19 already covers
standby time and shorts.

### Smaller points

- `HEALTHY_ECHO_FLOOR_DB` drops healthy echoes below −50 dB (`walk.rs:27`). That's fine per echo, and hides the many-small-echoes floor on
  dense plant.
- Off-path echoes exist only for a fault's mismatch (`build_off_path`), never for an ordinary neighbour such as an open port two taps down.
- Measurement noise and RxMER scatter are independent draws per value, and scatter is switched on only for `datagen`'s spectrum rows.
- Modems never go offline by choice. Task 19 has the utility outage. A subscriber unplugging is the commoner case [practice].

## 2. The designer

### What it builds today

Generic library, low-split. High-split builds the same shape, with 42 to 75 % of taps at 29 dB.

| Map | Homes per mile | Nodes, homes each | Amps per node | Cascade | Taps at 29 dB | Tap ports with a drop | Slack | Supplies |
|---|---|---|---|---|---|---|---|---|
| Urban grid | 149 | 4 × 151 | 4.8 | N+2 | 60 % | 54 % | 22 % | 4 |
| Suburban | 64 | 4 × 131 | 8.5 | N+2 | 61 % | 40 % | 11 % | 4 |
| Rural | 8 | 20 × 15 to 30 | 12.2 | N+6 | 88 % | 23 % | 8 % | 51 |
| OpenStreetMap extract | 17 | 7 × 20 to 78 | 12.3 | N+6 | 63 % | 22 % | 13 % | 26 |

Every design has 0 issues. Slack is span length beyond 1.5 times the straight line between a span's ends, so it's an estimate.

**D1. Taps aim at the modem's floor.** `choose_tap` tries values from the highest down and takes the first that keeps every modem inside the
window (`crates/design/src/place.rs:178`). So it picks the highest value that leaves the worst modem just above −4.5 dBmV. The median modem's
lowest level in band is −1.1 to +2.1 dBmV. The rural map is the exception at +5.1 dBmV, because 29 dB is the highest value there is.

- Field practice sets taps to the same nominal level at the port [note: SCTE 183 §16.4], with room for the house behind it [practice].
- Values of 8 and 11 dB are never chosen on the generic parts, and 29 dB is 60 to 88 % of taps. A field run steps down toward the end of the
  line [practice], and the notes' examples show 23, 20, 14, 11 and 8 dB.

**D2. Slack cable stands in for a pad.** Close to an amp even the highest tap is too hot or too tilted, so up to 600 ft of extra hardline is
added to that span (`place.rs:36`), a decision task 08 recorded. It comes to 7 to 28 % of all hardline, and all of it is in the parts list.
The parts that do this in the field are equalizers, plug-ins and higher tap values [note: SCTE 183 §16.4 names in-line equalizers].

**D3. One cable, one amp, one output.** `DesignParts` holds one hardline and one amp. An amp has ports `in` and `out` (`place.rs:346`).
Hardline between actives runs 700 to 1,400 ft on .500. SCTE 15 defines sizes to 1160 [note], and a station's bridger feeds several outputs
[note: SCTE 183 §16.4]. Express runs on larger cable than tapped feeder [practice].

**D4. Every branch is a two-way splitter.** `branch` adds one per extra child, chained (`place.rs:662`): 26 to 81 per design, and 4 to 24 of
them are the first device after an amp. The library's directional coupler is never placed. A branch at an amp is that amp's second output in the field, and an
unequal branch takes a coupler [practice].

**D5. Taps.** Up to four homes within 160 ft share a tap, and every tap has four ports, so rural taps serve one home on a four-port part. An
end of line is a through tap and a terminator, 34 to 95 per design, where a terminating tap is a part of its own [note: PNMP §3]. A tap sits
anywhere along a centerline, since the map has no poles or pedestals.

**D6. Nodes.** Homes are bisected on latitude and longitude (`crates/design/src/partition.rs`), which ignores streets, rails and rivers. A
node sits at its homes' centroid, snapped to a street. A group over its cascade limit is bisected again, and a detached piece of street gets
its own node, so the extract yields nodes of 20 to 78 homes on low-split and 2 to 78 on high-split. A root's ports are whatever the route tree has there, with no balancing: a
median of two of four in use on three of the four maps. Fiber doesn't enter the choice of site.

**D7. Powering.** One supply feeds each root, and `repair_power` adds another at the lowest amp (`crates/design/src/optimize.rs:158`). No
power inserter is placed, and a supply never serves two nodes, since it belongs to one plant file. The generic amp draws 60 W and needs 60 V.
On the sourced parts the same maps need one supply per node, so most of the 51 and 26 above is the generic part.

**D8. Routes and drops.** A Steiner tree over street centerlines, with aerial and buried feet costing the same. A drop is the straight line
from home to tap, up to 250 ft, always on one cable. A span has two ends and no path, which task 14 deferred.

**D9. Fiber.** `fiber_route` is 49 lines: a Steiner tree from one headend to the node sites, returning feet, lines and unreached nodes
(`crates/design/src/fiber.rs`). No cable, fiber count, splice, optic or budget, and no equipment at the headend.

**D10. No cost, so no choice of architecture.** The rural map gets coax at 8 homes per mile: N+6, 12 amps for every 30 homes. The stated
objective is the fewest actives and the least cable, and nothing is priced, so the designer can't see that fiber deeper would be cheaper, or
that nobody would build this [practice].

**D11. Everything is greenfield.** A Tier 1 estate midway through a rollout is legacy plant upgraded in place: amp sites fixed by an older
design frequency, modules and faceplates swapped [practice]. `design::migrate` converts a plant and reports what fails. A generated estate should
come from designing at legacy rules and then upgrading, since that's what leaves real plant marginal at the top of the band.

## 3. Components

What a built network has, against what the plant graph can hold and what the designer places.

| Group | Component | Plant schema v1 | Designer | What the sim would read from it |
|---|---|---|---|---|
| Hub | CMTS or vCore, its ports and service groups | No | No | Source MER, upstream receiver ceiling |
| Hub | Forward transmitter, return receiver, optical amp, splitter or mux | No: two constants on the node part | No | CNR from modulation depth and received power, clipping |
| Hub | CIN switch, pluggable optics, mux, timing source | No | No | Link margin, latency, where a timing fault is |
| Fiber | Cable: sheath, fiber count, type, aerial or buried | No | Route feet only | Loss by wavelength and length, where a cut is |
| Fiber | Splice closures, splices, connectors, patch panels, slack | No | No | Loss per event, a dirty connector's reflection |
| Node | Optical receiver and return transmitter, or RPD and its optic | One `Node` or `Rpd` element | Yes | Optical input level, per-port output |
| Node | Per-port pads and equalizers, segmentation | No | No | Per-port level and noise funnel |
| Amp | Multi-output and single-output types | Ports allowed, no per-port spec | One type, one output | Per-port level, return combining loss |
| Amp | Input pad, equalizer or cable simulator, return pad and equalizer, AGC pilot | No | No: slack cable | Alignment residual, misalignment |
| Passive | Taps of 2, 4 and 8 ports | `Tap`, 4-port generics | 4-port only | As today |
| Passive | Terminating taps, tap plug-ins, in-line equalizers | No | No | Tilt at the port, end-of-line reflection |
| Passive | Splitters, directional couplers | Yes | Two-way splitter only | As today |
| Passive | Power inserter | Yes | Never placed | Where current enters, port limit |
| Passive | Terminators: end of line, and on tap ports | End of line only | End of line only | Open port reflection, isolation |
| Cable | Hardline by size | Yes | One size | As today |
| Cable | Pin connectors, splices, housing adapters | No: a `Direct` edge | No | Small loss and a reflector each |
| Power | Supply, batteries, cabinet, utility service | Voltage and rating only | One per node | Standby, sag |
| Premises | Drop by size, aerial or buried | One `Drop` edge | One size | As today |
| Premises | Ground block, splitters, in-home cable, drop amp, filters | No | No | Loss, in-home echoes, ingress entry |
| Premises | More than one device per home | No | No | In-home scope |
| Structure | Poles, strand, guys, pedestals, vaults, conduit | No: `Installation` on a span | No | Nothing in RF. Records and parts list |

## 4. Proposed sequence

The build order's own rules apply: schemas before the code that reads them, the sim before what consumes it, a hand-built fixture before the
optimizer. One step is specced at a time.

| Task | Builds | Why here |
|---|---|---|
| 21 | Two reference notes from public sources: design practice, and fiber and optics. Component and plant schemas at version 2 with every row of section 3, a fiber plant schema, and design rules as a data file. Migrations whose defaults change nothing. A second hand-built fixture with every component on it | The schema changes once, for everything. `one_street` and its oracles don't move |
| 22 | Physics for the new parts, each with a worked oracle: premises path, connector and splice, pad and equalizer alignment, optical link budget, upstream baseline and ceiling, return loss by frequency, source MER from the part. New observations and fiber faults with a location | The sim reads the new plant before any designer emits it |
| 23 | The designer rebuilt on rules as data: hub, fiber, node sites, express and feeder, taps to a port level, drops and premises, alignment, powering, fiber design, a priced objective, and a legacy-then-upgrade mode. `realism_survey` targets fixed before any result | Emits what 21 defines and 22 can read |
| 24 | Regenerate the maps, retrain, and re-run selection | The training layouts will have moved |

**Decisions this needs from you:**

1. **Fiber becomes stored plant.** Task 14 made the fiber route a derivation, never stored. Fiber with record status, splice locations and
   faults is an input, so it has to be plant. The requirement that fiber be present settles this, and it reverses that decision.
2. **Generated designs will change.** Task 14 held `design_digest` at `7298d7aca5f41752` so the models' training layouts wouldn't move. They
   will, and every model retrains (task 24).
3. **Order against tasks 18 and 19.** Task 19's `AnalogFiberCut` names a node and a direction. With fiber as plant it gains a location, so
   either 19 waits for 21's schema or its fiber events migrate later.
4. **How far structures go.** Poles, strand and conduit carry no RF. They can be records and parts-list lines only, or left out of v2.
5. **Sources.** Operators' design specifications aren't public. The notes would rest on SCTE and CableLabs documents, vendor design guides and
   data sheets, and say where practice is still unsourced.

## Status after task 23

Task 23 built `design::v2`, the designer on rules as data ([`docs/tasks/23-as-built-designer.md`](tasks/23-as-built-designer.md)). The
version 1 designer is unchanged and its digests hold, so the "before" numbers above are still what version 1 builds. The "after" numbers
are `realism_survey`'s version 2 lines: generic library, `fixtures/design_rules/generic.json`, the same maps at seed 1. **Every plant and
reading here is synthetic** (invariant 11), and every price is `Generic` but two construction rates.

The section 2 table again, version 2, low split:

| Map | Homes per mile | Nodes, homes each | Amps per node | Cascade | Taps at 29 dB | Tap ports with a drop | Slack | Supplies | Cost per home passed |
|---|---|---|---|---|---|---|---|---|---|
| Urban grid | 215 | 3 × 155 to 257 | 7.0 | N+3 | 21 % | 46 % | 0 % | 3 | $489 |
| Suburban | 71 | 4 × 117 to 140 | 6.8 | N+3 | 13 % | 49 % | 0 % | 4 | $2,067 |
| Rural | Nothing built: coax costs $4,236 to $16,367 a home against the rules' $3,000 ceiling, and every home is reported as over it | | | | | | | | |
| OpenStreetMap extract | 46 | 3 × 34 to 65, and 170 homes over the ceiling | 4.7 | N+2 | 29 % | 46 % | 1 % | 3 | $2,174 |

High split builds the same shape, with 20 to 30 % of taps at 29 dB; it builds every suburban home, and leaves 170 on the extract over the
ceiling. Homes per mile rise because the hardline has no slack in it and the routes follow structures. Every bar task 23 fixed is met on
every built node of every map, on both profiles, and No issues, Alignment and Fiber on the two fully loaded profiles too.

These numbers are after the closing fixes, which changed two things the review didn't name. The tap port is held at one level, at the
top of the band, as SCTE 183 §16.4 and the one worked run state it, and the bottom of the band is bounded by the modem's own window and
spread behind its premises: the first rules held a level at 54 MHz and a port tilt limit too, both `Generic`, and they refused
VOGEL2019's own run. And the premises templates use a ground block and a 4-way at SCTE 129's and SCTE 153's ceilings, where the older
generic parts read over them (task 23's decisions, the closing fixes).

**Against real nodes.** The note's five real node areas give actives per mile against homes passed per mile [ULM2016 Table 3,
design-practice.md §6]: 5.0 at 37, 4.9 at 57, 5.9 at 112, 7.6 at 187, 7.4 at 274. The only outside check on the designer's density, and a
read-out, not a bar:

| Design | Homes per mile | Actives per mile | Real nodes either side |
|---|---|---|---|
| Urban, low split | 215 | 7.5 | 7.6 at 187, 7.4 at 274 |
| Urban, high split | 218 | 6.1 | 7.6 at 187, 7.4 at 274 |
| Suburban, low split | 71 | 3.7 | 4.9 at 57, 5.9 at 112 |
| Suburban, high split | 71 | 4.2 | 4.9 at 57, 5.9 at 112 |
| OpenStreetMap extract, both | 46 | 4.5 | 5.0 at 37, 4.9 at 57 |
| Version 1, urban, low split | 149 | 4.7 | 5.9 at 112, 7.6 at 187 |

Urban low split and the extract sit with the real nodes; suburban and urban high split sit about a fifth under them, at fewer actives a
mile than any real area. Version 1 sat a fifth under too. The real areas are legacy N+2 and N+3 plant, so a high-split design's longer
reach on fewer amps isn't wrong by this alone. The rural map, at 10 homes a mile, is under the sparsest real area and isn't built.

Finding by finding:

- **D1. Taps aim at the modem's floor: fixed.** Taps are chosen for the rules' level at the port, 14 dBmV at the top of the band within
  3 dB, at −20, 20 and 45 °C, with every modem behind them inside its window and spread; the Tap-port level bar holds on every map. 8 and
  11 dB values now come up (39 of 136 urban taps), and 29 dB falls from 60 to 88 % of taps to 13 to 30 %. The median modem's lowest level
  in band goes from −0.6 and −1.1 dBmV to +2.5 and +1.7 (urban, suburban, low split). The 95th percentile of the highest is 12.2 and
  12.8, as version 1's 12.5 and 13.3: with no level held at 54 MHz, the bottom of the band runs as hot as the modem's window allows.
- **D2. Slack stands in for a pad: fixed.** No slack is laid. The survey's estimate (span length over 1.5 times the straight line between
  its ends) reads 0 % of hardline on the generated maps and 1 % on the extract, where a span follows a winding street; it read 8 to 22 %.
  The No stand-ins bar holds: every span's length is its path's within 1 ft, and the only spans at the 3 ft minimum are the rules' jumpers
  between housings at one structure. Line equalizers, tap conditioners and station set points do what slack did: 1 to 10 line equalizers
  a map.
- **D3. One cable, one amp, one output: fixed.** Express and feeder roles each have a cable and an amp, and the express cable is a
  candidate (.750 and .500). Amps use their outputs: the Branches bar holds, so no splitter is the first device after an amp with an
  output free. Hardline between actives runs 294 to 571 ft urban and 537 to 822 ft suburban (10th to 90th percentile), against 715 to 832
  and 792 to 1,025. Amps per mile, 7.5 urban against version 1's 4.7, are now set beside the real nodes above.
- **D4. Every branch is a two-way splitter: fixed.** Urban, low split: 5 line splitters and 6 couplers, against 26 splitters. The splitter
  counts in the survey's version 2 lines leave out the ones inside homes (218 urban), which version 1 had none of.
- **D5. Taps: fixed.** Families of 2, 4 and 8 ports, taps sized to the homes at their structure, terminating taps at the ends of lines
  where the rules say (the End of line bar), and every port with a state: a drop, a terminator or open where the rules allow it. Taps sit
  on poles and pedestals. Tap ports per home passed rise, 1.29 against 1.13 urban.
- **D6. Nodes: fixed.** Areas are grown over the street graph from candidate sites, a site is weighed by the cost of the fiber to it and
  the actives it needs, and the node count is a candidate (one fewer, as planned, one more). A candidate whose cascade fails N/10 + 3 dB
  isn't kept while another passes. A root's ports in use: median 4 of 4, against 2, and a node has at most the rules' 30 actives
  [Sourced, design-practice.md §6]. The large extract builds three nodes of 34 to 65 homes at the ceiling; the rest of it is over.
- **D7. Powering: fixed, but for one point.** Every supply feeds an inserter, none is over 80 % of its rating, no inserter port is over
  its rating, and every active is above its minimum voltage (the Powering bar); the designer itself holds 5 V over it, a `Generic` rule.
  3 to 4 supplies a generated map, against 4. Still open: a supply serves one node, since it belongs to one plant file.
- **D8. Routes and drops: fixed.** Structures on one side of the street at the rules' setback and spacing; spans follow paths along them,
  aerial and buried priced apart; drops run from the tap's structure to the home, which has a ground block, in-home cable, splitters and
  devices by the profile's templates. Median drop 57 ft urban, 74 ft suburban.
- **D9. Fiber: fixed.** The fiber plant is stored plant: sheaths of the rules' counts along the structures, closures at branches, node
  drop-offs and reel ends, splices, a panel, connectors and jumpers, and hub equipment. Each link's optics are chosen from the rules' list
  and checked by `sim::optics`: received power at least 3 dB over the receiver's minimum and under its maximum, the forward composite
  index within 0.30, the return laser under its clip. The list now runs to long reach: ER and ZR optics for an RPD, and 1550 nm with a hub
  EDFA for an analog node [fiber-optics.md §6, §7]. Urban, low split: 0.5 sheath miles, 15 cables, 2 closures, 6 links. A cut on a shared
  sheath takes down the nodes behind it.
- **D10. No cost, so no choice of architecture: answered.** Six candidates are built and priced, and the cheapest with no issue is kept.
  Coax at 8 to 10 homes per mile costs $4,068 to $18,686 a home, so the designer builds none of the rural map and says so for every home,
  rather than an N+6 cascade. A control builds over the ceiling and still marks it. The prices are `Generic` and say so wherever a cost is
  shown; the ceiling is `Generic` too.
- **D11. Everything is greenfield: fixed.** The second mode designs at 750 MHz under `legacy-750.json`, where the legacy design meets
  every bar under its own rules, then upgrades in place to high split under the target rules' moves: modules and faceplates swapped,
  RF set-tops and televisions taken off the coax and the in-home splitters they leave with one leg taken out (SCTE 300's upgrade
  expense lines, design-practice.md §10), each station set where its own taps fit and the faceplates re-valued for the new band. On
  the urban grid: 975 changes (24 modules, 207 faceplates, 471 set-tops and televisions, 273 premises parts), nothing the moves can't
  replace, no issue of any kind, and every structural bar met; 13 of 414 tap-port checks sit outside the tolerance at the top of the
  band, as an upgraded estate runs there. The lowest modem level in band at the 5th percentile is +1.1 dBmV, against +0.5 greenfield
  and −8.3 before the set-tops came off: the estate an operator would accept, with the premises work that takes. No booster was
  needed: once the splitters came out, no modem was over its transmit limit.

What the ceiling hides. Built over the ceiling, both extracts hold every bar on both profiles, the large one's sparse node included
once the walk looked at each pole of a long lifted run for an amp. The rural map makes 37 nodes of 6 to 21 homes at about $8,300 to
$8,400 a home, every node with optics that reach, and holds every bar but two checks: one amp of 188 at 3.75 dB at N = 2 (low split),
and one tap of 706 0.1 dB under at 45 °C (high split), what alignment leaves on 1,100 ft spans. None of it is built at the rules'
ceiling, and all of it is in task 23's decisions.

## Status after task 24

Task 24 diagnosed and selected models on version 2 plant ([`docs/tasks/24-as-built-diagnosis-and-selection.md`](tasks/24-as-built-diagnosis-and-selection.md),
report in [`docs/model-selection.md`](model-selection.md)). **Every number is synthetic.** The diagnoser reads the new parts: connectors at
fittings, faults inside a home, a plug-in set wrong, and fiber at the region. On the 8 test maps through the hybrid, the GBDT, which task
15's rule keeps, places 74.0 % of faults in its top 3 against the floor's 57.2 %, and calls 45 % of healthy nodes faulty. A cut on a shared
cable is named every time and no coax fault is blamed on the fiber; a fault inside a home is put in that home 16 % of the time, against a bar
of 80 %. Rerun on as-built plant, the sensitivity report ranks hardline and drop loss, active and tap return loss, and modem implementation
MER first, and each moves far more answers than on version 1.
