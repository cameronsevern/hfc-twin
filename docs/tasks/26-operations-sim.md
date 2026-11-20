# Task 26 — Running the network day to day

**Build-order step:** after 19, at the user's request: "build a network and then have things happen to it, disasters, wear and tear,
etc, and then I want to watch people call in to complain and then see trucks go out to fix things." Step 13's lifecycle demo, made a
world that runs on its own clock. It's a demo first: "Watching it all play out will serve as a demo to communicate what is possible."
**Depends on:** Tasks 11 (work order lifecycle), 13 (reactivity, the map), 19 (damage events, `diagnosis::outage`), 22 to 25 (plant
as built, designed on any map)
**Produces:** a world that runs a designed region forward in simulated time: wear and weather and disasters happen to the plant as
physical faults; households notice what their own modem shows and call; a call centre takes the calls; a dispatcher who sees only
what an operator would sends trucks along real streets; techs find and fix what's there, or don't; the twin's telemetry proves the
fix. All of it through MCP tools and on the page, paired against the same world run without the twin.
**Load:** `CLAUDE.md`; `docs/DESIGN.md` (Reactivity, Build order 11 and 13); `docs/tasks/11-*.md`, `13-*.md` and `19-damage-events.md`
(their decisions); `docs/physics.md` §7.4, §8, §20; `crates/twin/src/lifecycle.rs`.

## Goal

Someone draws an area, designs it, and presses play. Over a simulated week, connectors corrode and amps drift, a storm brings
limbs down on the aerial plant and water into the closures, a truck hits a pedestal, a backhoe finds a buried span, and the power
company loses a neighbourhood for an afternoon. Homes go dark or slow on the map. Some call; most don't, at first. Tickets pile up
on the call centre's board, trucks leave the hub, drive the streets, stop, and the homes come back, or the truck finds nothing and
rolls again. A counter keeps score: customers out and for how long, calls, truck rolls, repeat rolls.

The same seed runs twice, once as an operator works today (wait for calls, send a tech to the first house that called) and once with
the twin (it sees the outage in telemetry, reads where, and sends the truck to the plant before most people call). The difference is
the pitch, and it's shown as "in this world, under these rates", never as a measured saving.

**All of it is synthetic (invariant 11),** and the page says so the whole time. Real streets, invented plant, invented households,
simulated everything. Later the same tools take real telemetry and real tickets in place of the sim's; that's why tickets get their
own schema now.

## The world

A fold, like the sim (invariant 4): the world at sim time T is a pure function of the designed region, the rules files, the seed, and
the log of what the user did, each action stamped with the sim time it took effect. No wall clock in the crate.

- **The step is fixed in sim time,** one minute. Display speed changes how many steps run per second of wall time, never what a step
  does. `advance(to: T)` in one call gives byte-identical state to the same advance in many calls.
- **Work is scheduled, not polled.** Timed things (a battery running out, a repair finishing, a truck arriving, a call ending) are
  events on a queue keyed by sim time. A node's telemetry is recomputed only when its inputs change (a fault starts, grows, is
  repaired; the hour's temperature or rain changes; a supply goes on battery) and, while any modem on it isn't operational, every
  step, so §7.4's timers run.
- **Modem state carries across steps.** `sim::telemetry::simulate` starts every modem operational, so the world needs a call that
  takes the states and the previous sample's time in and gives them back: additive, no schema change. The modems' draws are keyed by
  seed, time and modem, so it can match one long run byte for byte; a test holds it to that.
- **An event at world time T** is a fault with `Onset::Ramp { start: T, end: T }`, and its `repair_s` counts from T (`sim::faults`
  reads both that way today). A test holds an event at T to the same event at 0, shifted by T.
- **Fiber faults** enter as the node's link; the world recomputes the region's links whenever its fiber fault set changes.
- **Latency is measured before the loop is built:** a step of every node of a 30-node region, and of only the nodes that changed.
  The design must keep a step under the wall time a demo speed gives it; what that is gets written down here, measured.

## What happens to the plant

Nothing scripted (invariant 5): every hazard makes faults of kinds that already exist, with parameters, and the sim computes what
they do. A hazard is a process with a rate, and a rate is data.

| Hazard | Makes | Driven by |
|---|---|---|
| Wear | `ConnectorReflection` that worsens over days, `AmpDrift`, `WaterIngress` on spans, corroded taps (`DamagedDevice` with CPD and no blow) | Time, temperature swings, rain |
| Weather | `SpanCut` and `DamagedDevice` on aerial spans in wind, `WaterInDevice` and `WaterIngress` in rain, `UtilityOutage` in storms | The environment series, whose storms the user can call |
| Vehicles | `DamagedDevice` or `SpanCut` at structures beside roads | Road class, by traffic |
| Digging | `SpanCut` on underground spans | Daytime, weekdays |
| The power company | `UtilityOutage` over an area | Storms, and a base rate |
| Fiber | `FiberCut`, `EventLoss` | Construction, a base rate |

- **Disasters on demand,** a palette as in a city-building game: a storm (wind and rain over the region for some hours), an ice
  storm, a heat wave (the plant's temperature), a car into a pedestal at a chosen place, a dig-in at a chosen span, a power cut over a
  drawn circle, a fiber cut. Each is a user action in the log, and what it does is drawn from the same processes at raised rates.
- **Rates are sourced data (invariant 9),** in `fixtures/ops/rates.json`, each with `source_url` and `spec_confidence`. Searched for,
  not recalled: SCTE and CableLabs reliability and PNM papers, FCC outage reports (NORS summaries), utility reliability indices
  (SAIFI, SAIDI), public storm damage reports. What has no source is `generic` and says so on the page.

## Households

Each home passed that's served gets a household, keyed by plant location (invariant 10), with nothing real about it.

- **It reacts to what its own modem shows,** never to the fault list: offline, partial service, or a codeword error rate that would
  stall video. How likely it is to call per minute, by how bad and how long, and by hour of day, is a rate in the rules file. Some
  never call. A neighbour already out makes a call more likely to say "the whole street".
- **A call** has a time, a household, what it says (no service, slow, intermittent) and how long it takes. It isn't a diagnosis.
- **House power** (task 19's confounder) turns the modem off and the household doesn't call about the internet until the power is
  back, if then.

## Operations

- **The call centre:** agents on shifts, a queue, a handle time per call; a call either joins an open ticket at the same node or
  opens one. Waiting and abandoned calls are counted.
- **The dispatcher sees what an operator sees:** calls, tickets, and the twin's telemetry, diagnoses and work orders. **Never the
  fault list.** A truck sent to the wrong place finds nothing, closes the job "no trouble found", and the ticket stays open.
- **Two policies, one world:**
  - *Reactive,* as most dispatch works without a twin: a ticket becomes a truck roll to the first home that called; at the home the
    tech checks the drop and the premises, then works up the plant from there.
  - *With the twin:* the twin's diagnosis and `diagnosis::outage` run on what telemetry shows (in sim time: a reading is ready a set
    number of minutes after the step it reads, so a 13 s diagnosis never sits in a step), and its work order sends the truck to the
    point it names. Calls still come in and are tied to the order.
- **Trucks and techs:** a fleet at the hub, with shifts. A truck drives the region's streets (the OSM graph the design was built on)
  at speeds by road class, works the job for a time by what it finds, and comes back or goes on. Bucket trucks for aerial work and
  splicing crews for fiber are their own kind.
- **On site, the tech finds what's there.** If the fault is within reach of where the truck stopped (the element, its spans, the home),
  the tech finds it and fixes it in a time drawn from the rules; the fault is removed from the plant, which recomputes, and
  `twin::lifecycle` verifies from telemetry that the symptoms cleared. If not, "no trouble found".
- **What's counted:** customer-minutes out, by cause; calls, answered and abandoned; truck rolls, repeat rolls, "no trouble found";
  time from onset to first call, to dispatch, to fix. Reactive and twin, side by side, on the same seed.

## Schemas

Calls, tickets, trucks and jobs get their own versioned schema, `schema::ops` version 1, with golden samples and round-trip tests
(CLAUDE.md), outside the observation schema (invariant 2): a ticket isn't something the plant emits. Shaped so an operator's real
ticketing and dispatch feeds can map into it later.

## The API and the page

The MCP server is the API (invariant 8); the page is one client of it.

- Tools: start a world from the twin's current region, seed and policy; advance it to a sim time; read its state since a time (what
  changed: homes, calls, tickets, trucks, jobs, counters); take an action (a disaster, a policy, a staffing change); read the counters
  for both policies.
- **The page gets an Operate mode** beside Design: play, pause and speed; the clock and day; the disaster palette; homes coloured by
  service; calls as they come in; tickets and work orders on a board; trucks moving along the streets to their jobs; the counters,
  reactive beside twin. A banner says Synthetic for as long as it runs.

## Build order inside the task

Each stage is usable and tested before the next starts.

1. **The clock and one hazard.** The resumable sim call and its test; events at T and their test; the latency measurement; the
   world as a fold, with wear on connectors; `advance` and `state` tools. Rates file with sources, first entries.
2. **Households and calls.** The call model, the call centre, `schema::ops` v1.
3. **Trucks and repair, reactive.** Street routing, the fleet, jobs, "no trouble found", repair through `twin::lifecycle`.
4. **The twin's policy,** and the paired counters.
5. **The rest of the hazards and the disaster palette.**
6. **The page,** driven in the browser and shown working, end to end.

## Tests

- One `advance` to T and many give byte-identical state; the same seed and log give the same run.
- The resumable sim call matches `simulate` over the same samples byte for byte.
- An event at T matches the same event at 0, shifted.
- A household reacts only to its modem's observation: with the fault list hidden from it, the same calls.
- The dispatcher never reads the fault list: a test world where the twin's reading is wrong rolls a truck to the wrong place and gets
  "no trouble found".
- A repair removes the fault, and verification closes the work order only when telemetry clears.
- `one_street` runs a quiet week with wear only, and its level budget and pinned telemetry don't change when nothing happens.
- Schema round trips and golden samples for `schema::ops`.

## Acceptance

- A region of 20 to 40 nodes runs a simulated week at a speed a demo can watch, measured, on the page, with a storm called halfway.
- Every number on the page traces to a rate with a source or a `generic` tag.
- The reactive and twin runs share a seed and everything else, and the page shows both.
- The page is driven in the browser and shown working: a storm, the calls, the trucks, the repairs.

## Out of scope

Real telemetry and real tickets (that's the pilot, v2). Billing, churn and money: the counters are customer-minutes and truck rolls.
Anyone's real staffing or rates. Multi-region. Mobile apps for techs.

## Open, for the user

- **§4.5's SC-QAM echo residual** (task 19's finding): today an open cut takes DOCSIS 3.0 modems off on the taps above it, so in this
  world every cut draws calls from homes that should be fine. Fixing it is a physics change that moves pinned results and the rows
  earlier models trained on. Whether to fix it first is the user's call; it blocks the hazard stage, not the clock.

## Decisions made during the task

### What was built (2026-11-19), and how it differs from the plan

The user's call on the open question was "Fix, then build": task 27 changed §4.5's SC-QAM residual first, so an open cut no longer
takes 3.0 modems off the taps above it. Then every stage was built, and the page driven.

- **The crate `ops`:** `World` and `Pair`, a one-minute step, hazards drawn from the seed and the time alone so both worlds see the
  same ones, households, the call centre, jobs and trucks routed on the design's street graph (`design::route::StreetGraph`), and
  counters. `schema::ops` version 1 holds the rates, calls, tickets, jobs, trucks, counters and feed, with a golden sample.
  `fixtures/ops/rates.json` has 35 rates: 4 published, 1 inferred from a published figure, 30 `generic` and saying so. Every
  published one was read at its source; four that came from search summaries and couldn't be read there were made `generic`.
- **The sim, additively:** `sim::telemetry::simulate_from` carries modem states across calls, and a ramp of no length is a step at
  its start, so an event at world time T is `Onset::Ramp { start: T, end: T }`. Tests hold a run split anywhere to the run in one,
  byte for byte, and an event at T to the event at 0 shifted (`crates/sim/tests/resume.rs`).
- **Tools:** `ops_start`, `ops_run`, `ops_state`, `ops_act`, `ops_rates`. A world lives in memory only and a restart starts none.
- **The page:** an Operate tab with play, speed, the disaster palette (storm, heat wave, power cut, dig-in, car into the plant, fiber
  cut; the last three of those placed by a click on the map), the two worlds' counts side by side, the board, the story, the rates
  with their sources, and map layers for homes by service, recent calls, jobs, trucks with their routes, and, on request, what's
  really wrong. Driven in the in-app browser end to end; that browser has no WebGL, so the map layers themselves weren't seen
  drawn, and a placed disaster was driven through the tools instead of a click.

Decisions made building it:

- **Verification is the world's own:** a ticket closes when telemetry shows its homes back, rather than through `twin::lifecycle`,
  which works on the twin's single fault list. Both worlds see modems online and offline, as any operator's CMTS does; what only
  the twin has is where.
- **The dispatcher never reads the fault list.** On site, a crew finds what's within reach of where it stopped: a home's premises,
  drop, tap and the tap's input span; a plant point and its spans; a supply's whole run, a short on any power-passing span its
  current reaches; a node's fiber. Nothing within reach is "no trouble found".
- **An escalation ladder for both:** a point found clean, then up the tree to the node, then each supply's run, then the fiber; with
  every rung tried in a day, trucks stop until the trouble resolves. Without it, both worlds rolled trucks in a loop.
- **From calls:** a truck to the first caller's home; with 3 calls on a node's ticket and that truck back, a line crew to what the
  callers share, then up the ladder.
- **With the twin:** `diagnosis::outage` on every step it simulates, acted on 5 world minutes later, only while the node is still
  out and not within 30 minutes of a fix. Nodes wholly dark together that share a fiber cable get one splicing crew, and that job
  covers all of them. Without that, a cut through a shared cable sent a line crew to each node first.
- **The phone line knows a known outage,** and says so with no agent: with the twin once it has a truck on the node, from calls once
  the node's ticket has 3 calls. Callers who give up redial, and households still out call back. Without the message one agent took
  400 calls in a day and lost 80 % of them.
- **The demo's intensity multiplies the plant's hazards only.** The power company's rate is sourced and only weather raises it;
  multiplied, a storm's power cuts swamped the story.
- **A fault takes the same time to fix in both worlds,** its draw keyed to its place among the plant's troubles (which come in the
  same order in both, being drawn from the seed and the time alone), not to the job, so the pair differs in dispatch and not in luck.
- **Operators see modems, not house power.** A home with no house power reads as out to the dispatcher and the twin, as it does to
  any CMTS; only its household and a crew at its door know it's the power. First written reading the household's view, which let
  the twin skip house-power outages it couldn't have told from a break; with it fixed the twin rolls on them and finds nothing,
  as task 19 measured it would.
- **A job is called off** when its node has no home out for 10 minutes before a truck reaches it.

The demo day (`cargo test --release -p ops -- --ignored a_demo_day`, a 1,600-home generated region, intensity 50, a storm at dawn,
then a dig-in, a car and a fiber cut at noon), in this world and under these rates:

| | From calls | With the twin |
|---|---|---|
| Customer-minutes out | 149,121 | 127,781 |
| Truck rolls | 6 | 4 |
| No trouble found | 3 | 2 |
| Calls abandoned | 11 | 0 |
| The fiber cut fixed, world minute | 976 | 807 |

The twin's two that found nothing were house-power outages it read as breaks, as it must from modems alone.

The dig-in sat under the fiber cut's outage and was fixed later with the twin than from calls, which found it by luck at a caller's
tap. A world hour takes about 4.3 s of wall time for both worlds, so 5 world minutes a second keeps up and 60 doesn't.

Tests (`crates/ops/tests/world.rs`): one advance is many, for both worlds of a pair; a dig-in is called in and fixed from calls
alone, and its ticket closes only on telemetry; the twin dispatches before calls do; placed events land where placed; a wrong reading
(house power over a node) rolls a truck that finds nothing; a fault no modem feels draws no call; a quiet week on `one_street`
breaks nothing; a rates file missing a rate is refused. Plus `crates/sim/tests/resume.rs` and the `schema::ops` goldens.

**Acceptance, not met:** the spec asks for a week on a region of 20 to 40 nodes at a demo's pace, on the page, with a storm halfway;
what ran is 11 nodes for a day, at about 4.3 s a world hour, so a 30-node week would take tens of minutes at full speed. The map
layers (homes, calls, trucks on their routes, jobs, what's really wrong) and placing a disaster by clicking the map have never been
seen working: the in-app browser has no WebGL and the Chrome tab tried was hidden. A household's reaction is tested by a fault no
modem feels, not by hiding the fault list from it, since it never reads that list beyond its own house's power.

Not done: shifts are by the hour and not by person; calls aren't spread over a whole call centre; the twin acts on hard breaks only
(a soft fault like a drifting amp is found when people call); the rates marked `generic` are judgement; and the map layers need a
browser with WebGL to be seen.

Task 28 drove the page in a WebGL browser for the first time and rebuilt it as an operations screen; the map layers this task
never saw are seen there (`docs/tasks/28-operations-game-screen.md`).
