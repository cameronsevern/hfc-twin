# Task 19 — Physical damage events

**Build-order step:** after 17. Whether it goes before or after task 18 (weight-space editing) is the user's call.
**Depends on:** Tasks 04, 16, 17
**Produces:** one physical cause setting all its effects together, hard breaks that take out what they should, and a reading of an outage from which modems vanished.
**Load:** `CLAUDE.md`; `docs/reference/impairments.md`; `docs/reference/mac-operations.md`; `docs/reference/plant-and-powering.md`; `docs/physics.md` §5, §6, §7.4, §8.

## Amended before the build (2026-11-18)

Written after task 17, before tasks 21 to 24 changed what it builds on. Reread against them, and changed so:

- **Fault schema version 5,** from version 4 (task 22 added connector couplings as version 4). Version 4 files migrate unchanged.
- **No `AnalogFiberCut`.** Task 22's region-level fiber faults already cut a cable (`FiberCut`, every strand through the point, with a
  repair time) and lose light at a point (`EventLoss` at a cable, an element or one splice). A cut of the whole sheath is `FiberCut`;
  one direction only is `EventLoss` on the one splice of that direction's strand, at a loss past every link's margin. The sampler
  draws both, so "tells forward from return" is still measured, on faults that exist.
- **`AnalogNodeFailure` keeps two modes, power and the RF section.** A laser that fails is task 22's `TransmitterDrift` on the
  node's return laser or the hub's forward transmitter, at a power drop past the link's margin.
- **A utility outage is one event over an area,** a new kind, `UtilityOutage`: every plant supply inside it loses its AC input (its
  batteries carry it for a time worked out from the batteries the plant records and the load, §6, then everything it feeds loses
  power), and every home inside it loses house power. A small area over homes and no supply is the confounder, modems with no
  house power and no plant fault. (Amended during the build from two kinds, `UtilityOutage` by supply and `PremisesPowerLoss` by
  modem: an outage covers both together, and the twin is keyed by place, not by modem.)
- **Premises are in the plant now** (task 21), so `DamagedDevice` and `WaterInDevice` may name a ground block, splitter or drop amp
  as well as a tap, amp or node.
- **The decision rule runs on task 15's test maps,** which no question of this task has read. All synthetic.

## Goal

A car hits a tap, or water gets into a node. Today the sim can't represent either as one event. `WaterIngress` only applies to a span, `AmpDrift` and the RPD faults stop a single device, and nothing cuts a cable. Training rows draw a reflection, ingress and CPD separately, even though one defect commonly shows them together [PNM-HFC §9.7, §6.2.1].

This task adds those events as physical parameters (invariant 5). Which modems drop, in what order, and what the survivors' curves look like all come out of the sim. All of it is `Synthetic` (invariant 11).

## What's known, and what isn't

**No public source was found that gives telemetry from a car hitting a tap or water in a node.** So nothing here can be checked against a real capture of those events, and a pass means the sim follows its physics, not that it matches them.

| Claim | Source | Tag |
|---|---|---|
| Cut cable, power failure or a failed active make modems vanish | [PNM-HFC §VII.3 Table 44] | [guideline] |
| One defect commonly shows echo, ingress and leakage together, plus CPD when corroded | [PNM-HFC §9.7, §6.2.1] | [guideline] |
| A water-soaked tap loses about 30 dB by 750 MHz, and return loss falls to about 7 dB when water-damaged | `impairments.md`, `rxmer-impairments.md`, [SCTE 183 §9.1] | [field] |
| Leaking closures have causes listed but no RF signature | [PNM-HFC §7.2.5.2] | Gap |
| A charged three-battery string gives about 90 minutes of standby | [SCTE 183 §16.12] | [guideline] |
| A vehicle crash took out service for about 13,000 Comcast customers in Mountain View | padailypost.com/?p=7118, a news report with no telemetry | [field] |
| A severed fiber took out Cogeco service in Hamilton | forterieradio.ca/?p=236, a news report with no telemetry | [field] |
| Corroded or loose connectors bring high upstream noise, and Tx power spikes before incidents, at an Austrian ISP | Heiler et al., arXiv 2203.06989 | [field], not checked for this spec beyond its abstract and results |
| T3s, lost lock and drops after a damaged pedestal | One forum post | Unconfirmed, not used |
| The same crash also takes out utility power | Plausible for a pole hit | [inferred], the shares are unknown |

Anything tagged [inferred] stays tagged in `docs/physics.md`. Public sources only, since real data from an operator isn't an option.

## Events

Each is a new fault kind in fault schema v5, with a migration from v4 (invariant 3). A one-direction or whole fiber cut is task 22's
fiber fault (see the amendment above). Each changes the plant graph before the sim runs (`docs/physics.md` §8), and none touches telemetry.

| Event | Parameters | What it does to the plant |
|---|---|---|
| `SpanCut` | `span_id`, `pos_ft`, `end` (open or shorted), `repair_s` | Breaks the RF path at the point. The live side sees a full reflection, so an echo reaches modems upstream under §5.1. AC stops past the cut, so actives there lose power under §6 |
| `AnalogNodeFailure` | `node_id`, `mode` (power or RF section), `repair_s` | Power: the node and everything it powers stop. RF section: no forward out and no return in, the node powered. The RPD faults stay as they are |
| `UtilityOutage` | `center`, `radius_ft`, `repair_s` | Every supply inside loses its AC input: its batteries carry its load for the time §6 works out, then it stops, and with it everything it feeds. Every home inside loses house power, with no plant fault: the confounder when no supply is inside |
| `WaterInDevice` | `element_id`, `loss_db_at_1ghz`, `return_loss_db`, `rain_coupling_per_mm_per_hr`, `failed_after_s` | Water loss and mismatch inside a tap, amp or node, not along a span. It can end in the device failing outright |
| `PowerShort` | `span_id`, `pos_ft`, `fault_resistance_ohm`, `repair_s` | A low-resistance path across the hardline. Current follows from §6's loop model, and it trips the supply or a fuse if the rating says so. Everything on that supply loses power |
| `DamagedDevice` | `element_id`, `through_loss_db`, `return_loss_db`, `ports_lost`, `ingress_power_dbmv`, `cpd_second_order_dbmv_per_mhz` | The car-into-tap case when the through path survives: one cause with reflection, loss, ingress and CPD at one point, all applied together |

**Ranges.** The sampler spreads them from minor to total, as task 17's does. **No source gives shares, so this says nothing about prevalence.** The joint draw in `DamagedDevice` (how ingress moves with return loss) has no source either, so it's [inferred] and tagged.

**Physics first.** Before any code, `docs/physics.md` gets:
- The open and shorted end as a reflector at about 0 dB return loss, with a hand-worked echo at a modem upstream.
- Short-circuit current and trip against supply and fuse ratings, with a hand-worked case.
- Water inside a device as loss and mismatch at a point.
- How long a standby supply carries its load, and what happens when a utility outage hits a supply with no standby.

Each worked example is the oracle for its test.

## Modem state over time

A break isn't one snapshot. Modems past it go through §7.4 in order: MAPs stop, T4 runs out at about 30 s, the channel is unusable, reinit, then repeated failed attempts. Modems on the live side stay up and may see a new echo. A repair brings them back through `reregistration_s`, which sets a recovery curve. Events therefore run on the sim's existing steps, and rows must cover before, during and after, unlike task 17's single step.

## Reading a hard break

A modem that's gone has no curve, so the spectrum model can't help. A hard break is read from which modems vanished across the tree, plus T3, T4 and registration state.

**`diagnosis::outage`:** a no-ML reading. From modem states on the plant graph, it names the highest point in the tree that explains the vanished modems, with the modems that contradict it. It's keyed by plant location (invariant 10) and is a pure function of its input (invariant 1). It says "don't know" when the vanished modems don't fit a subtree.

**The confounder:** subscribers lose house power too. A utility outage takes modems down in a pattern that follows the power company and not the plant, so the sampler includes outages with no plant fault at all, and the reading has to say "not a plant break" for them. How that pattern falls on real streets is [inferred]: draw it by street area.

## Decision rule, fixed before any result

On task 15's test maps, or step 15's three reserve areas if a test map has already been read for this question. All synthetic. These bars are a first proposal, so the user can change them **before** any result.

| Part | Bar |
|---|---|
| **Names the break** | Where a `SpanCut`, a `FiberCut` or an `AnalogNodeFailure` takes out at least 5 modems, the point named is within one element of the true one at least 90 % of the time |
| **Doesn't over-call** | A utility outage with no plant fault is called a plant break at most 10 % of the time |
| **Tells forward from return** | A forward-only and a return-only fiber cut are told apart at least 95 % of the time |
| **Survivors still read** | The spectrum model isn't retrained here. Its read-outs on survivors' curves are reported per event, and are not part of the rule |

## Deliverables

1. `docs/physics.md`: the sections above, with worked examples.
2. `schema::fault` v5 and migration, with golden samples and a round trip for every event.
3. The events in `sim`, and `docs/reference/*.md` updated with what was checked.
4. `features::sampler`: `sample_damage_event`, sources in comments, one cause drawn jointly.
5. `diagnosis::outage`.
6. `datagen maps-events`: time-series rows marked `Synthetic`, with schema versions.
7. A gallery of each event at minor and total: which modems drop, their state over time, and survivors' curves.
8. `docs/model-selection.md`: a new section from the real run, whichever way it goes.

## Tests

- Physics: every event against its worked example in `docs/physics.md`.
- A cut leaves every modem upstream online, and every modem past it offline within the §7.4 timers, with the fixture's AC tree recomputed.
- A return-only fiber cut leaves RxMER intact while ranging fails. Forward-only loses both.
- An open and a short at the same point give the §5.1 echo with opposite signs, and only the short can trip a supply.
- A standby supply carries a utility outage for its modelled time, and a non-standby one doesn't.
- `DamagedDevice` with each effect off gives the same result as the existing fault at that point, so nothing new is hiding in the composite.
- `one_street` still gives a valid level budget with no event, and its existing telemetry doesn't change by a byte.
- Same seed, same bytes, across a whole event from onset through repair.
- `diagnosis::outage`: worked trees, including a contradicting modem and a pattern that fits no subtree.

## Out of scope

Retraining the GNN or spectrum model on these events, and wiring any of it into the twin. Weight-space editing. Fiber plant beyond a single link: rings, redundancy and splice points. Fire, lightning and surge, which have no notes yet. Upstream OFDMA curves.

## Acceptance

`cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace` green. The rule applied as written, even if the result is uncomfortable. Nothing in the docs reads as if an event was checked against a real one.

## Deferred

- **A model over time and across the tree,** if the no-ML reading falls short. This is also where intermittent drops and partial bonding could be read, since task 17's snapshot can't.
- **Labels for flat falls.** Task 17 found that its low background modems count as healthy while the same fault counts as `LowMer`, and a flat fall needs a baseline to read. Whether a per-modem history or neighbours supply that belongs with the model over time.
- **Real outage tickets,** if a public set ever turns up.

## Decisions made during the task
### The bars, fixed 2026-11-19 before any result

The user left the bars to this session. They're the proposal above, made exact where it left room. Nothing had been read when they
were written: no event had run on a map.

- **Plant and maps.** Task 15's 8 test maps, which no question of this task has read, designed by today's version 2 designer (task 25)
  with fiber, greenfield and legacy then upgraded, at every node with no design issue, on task 24's two fully reporting profiles
  (`*_full`): the reading uses the node's optics where the plant reports them, the shipped profiles report none, and on a digital node
  no modem can tell which strand went dark. On analog nodes the read is also reported with the optics withheld. All synthetic.
- **The read step.** Each event starts at t = 0 and is read once, at t = 600 s, from the modem states and the node's optics where the
  plant reports them. (First written as 300 s, and moved before any map ran: in the sim's §7.4 a modem that loses only its return
  reinitializes after 17 missed ranging opportunities 15 s apart, about 255 s, so at 300 s it's caught mid-reinit and reads as a
  forward loss; `crates/diagnosis/tests/outage.rs` shows it.) Rows also hold steps before, at 0, 60, 120, 300 and 450 s, and one after
  repair where the event has one.
- **Names the break.** Of `SpanCut`s, `FiberCut`s and `AnalogNodeFailure`s that leave at least 5 modems not operational at the read
  step, at least 90 % have the point named within one element of the true one. The true point of a cut is its span, of a node failure
  the node, and of a fiber cut the node it cuts off (the region's fiber reading names the cable, task 24). "Within one element" is at
  most one hop in the node's tree, counting elements and spans alike. A "don't know" is a miss.
- **Doesn't over-call.** Of utility outages over homes with no plant supply inside (house power only, the confounder) that leave at
  least 5 modems dark, at most 10 % are called a plant break. A "don't know" isn't a plant break.
- **Tells forward from return.** Of fiber losses on one direction's strand past the link's margin (task 22's `EventLoss` on its
  splice), at least 95 % are read as that direction. The loss is 40 to 60 dB, a strand broken or pulled at a splice [inferred]: an
  analog link counts as up while any light arrives (§16), and at 30 dB a return still carries 22 dB of SNR, enough to range.
- **"Past the link's margin" is read from the outcome:** a strand loss counts in the direction part where it leaves at least 5
  modems not operational at the read, as the other two parts count a break. (Added before the test read, once the training map
  showed an analog return that still ranged at 40 dB; the rows report how many strand losses this leaves out.)
- **Sizes.** At least 200 events in each of those parts, so each share has an interval narrower than about ±4 points; intervals are a
  bootstrap over nodes. A part with fewer than 200 fails on size, and there's no second run to top it up once shares are seen.
- **Reported, not ruled:** the other events (water in a device, a short, a damaged device, a utility outage that reaches a supply),
  the spectrum model's read of survivors' curves, events taking out fewer than 5 modems, and the reading at other steps.


### Where it stood (2026-11-18): deliverables 1 to 3 built, 4 to 8 not

Built and tested: the fault schema at version 5 (work orders at version 6, since they carry fault kinds, as task 22 did), with
version 4 files migrating unchanged and frozen goldens of the old versions; `docs/physics.md` §20 with its oracles; and all six
events in `sim` (`crates/sim/tests/damage.rs`, 12 tests: the §20.3, §20.4 and §20.6 oracles, a cut, an open against a short,
which one trips, an outage with and without standby, house power by home, a damaged device with one effect, the node's two modes,
a repair, same seed same bytes, and no event changing nothing). The new kinds map to `CauseClass::Unknown` in work orders until
`diagnosis::outage` reads them. Not built: the sampler, `diagnosis::outage`, `datagen maps-events`, the gallery and the run, so the
decision rule hasn't been read and no test map has been opened.

- **A cut is a 300 dB loss, not a removed edge.** The plant graph stays the input (invariant 1); the walk adds `CUT_LOSS_DB` and the
  AC solve opens the span, so the same plant with the event repaired gives the same bytes.
- **The cut test's span is past an amp.** A cut a few dozen feet past a tap reflects back into that tap's ports through its
  isolation and can take its modems out, which is physics (§20.1), not a bug, so the test that "everything before the cut stays
  up" cuts a span after an amp, which is directional.
- **Reflection sign is carried through the walk.** An echo is the product of its two reflectors' signs; every existing reflector
  is positive, so no existing telemetry moved.
- **Below a supply's rating a short's drop on shared spans isn't modelled,** and fuses rated below the supply aren't read (§20.4).
- **The table's parameters were settled in the build:** `SpanCut`'s end is `end`, `WaterInDevice` takes its loss at 1 GHz and one
  return loss (the mismatch, as §8's span water does, with no separate impedance variation), and `PowerShort` sits on a span at a
  position, as a cut does.

### The reading and the sampler, settled on a training map before the test maps were read (2026-11-19)

`diagnosis::outage` and `sample_damage_event` were built against `crates/diagnosis/tests/outage.rs` and run on one training map
(Ames, its 4 designs, 360 events) to find what they got wrong. Nothing here read a test map; the test run came after, once.

- **A cut opens the AC as well as the RF,** and a supply can feed amps on either side of where it enters (§6.2). So the reading's
  points are each element's subtree, each powered span's subtree with every active whose current crosses it, and each supply's
  whole run (a new verdict, `SupplyRun`, for a supply that trips or stops).
- **Nothing below a break can be up,** so a point with any modem up inside it is ruled out. The tolerance (a tenth of the modems
  gone, at least one) is only for modems gone outside the point: ones already down, and ones an echo takes above a cut. First
  written allowing modems up inside too, which let a house-power outage over most of a leg read as a cut.
- **A run of taps with no homes** gives every point along it the same modems, so the break is on one of its spans and nothing in the
  modems' states says which. Cable is cut by the foot, so the reading names the longest such span, and the sampler draws a span to
  cut, or to short, in proportion to its length [inferred]; first drawn uniformly, which picked jumpers of a few feet inside housings
  as often as street spans. With every modem gone the node is named, since one thing at the head explains all of it.
- **Found, and not changed here: the sim takes DOCSIS 3.0 modems off the live side of an open cut.** §4.5 keeps an SC-QAM modem's
  residual at its largest echo, and an open end's echo through a tap's isolation is about 15 dB down, so a 3.0 modem on the tap above a
  cut loses its channels, while the 3.1 modems beside it keep 41 dB of RxMER. A real 3.0 equalizer would take most of that echo out:
  DOCSIS assumes a modem works through a micro-reflection of −10 dBc at 0.5 µs. Changing §4.5 moves pinned results from earlier tasks,
  so it's left as a follow-up, and the cuts it affects show as modems gone outside the point named.
- **A shorted cut trips its supply** (§20.4) and takes out everything the supply feeds, so from the modems it reads as the node or the
  supply's run, never the span. How often a cut cable's conductors short is unsourced; the sampler draws half, and the rule's
  "names the break" is reported split by open and shorted.

### The run (2026-11-19)

**One part of three passes.** On the 8 test maps, read once: names the break 76.3 % (bar 90 %), house power called a plant break
15.0 % (bar 10 %), forward told from return 100 % (bar 95 %). Fiber cuts and node failures are named every time, by construction;
open cuts 77.0 % and shorted cuts 12.1 %, since a short trips its supply. `docs/model-selection.md` has the tables and why.

- **Built:** `diagnosis::outage` (12 tests), `features::sampler::sample_damage_event` (ten kinds), `datagen maps-events`,
  `events-rule` and `events-gallery`, `docs/gallery/damage-events.md`, and the results in `ml/artifacts/events/results_events.json`.
- **Water in a node** had nowhere to go: a node has no RF input. It acts at each RF output now (§20.3), found when the test run
  stopped on it; every row written before the change is one the change doesn't touch.
- **Not done:** `docs/reference/*.md` wasn't changed, since nothing new was checked against a source. The decision rule's misses
  aren't chased: the bars stand as fixed, and the test maps are read for this question.
- **Follow-ups found:** §4.5's SC-QAM echo residual is too harsh for a strong short echo, and supplies report nothing the reading
  could use to place a short.
- **Since:** task 27 changed §4.5's residual (2026-11-19). The run above was under the old one and wasn't repeated.
