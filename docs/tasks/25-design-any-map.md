# Task 25 — Design any map

**Build-order step:** after 24, at the user's request ("I want to be able to build a network for any map without issues").
Ahead of 18 and 19, which don't touch the designer.
**Depends on:** Tasks 14, 23, 24
**Produces:** a version 2 designer that, given any real area someone draws on the page, builds plant with no issue on any node it
builds, says where every home went, and never fails; a survey that measures it on real maps; the page reaching it by default.
**Load:** `CLAUDE.md`; `docs/as-built-plan.md`; `docs/tasks/23-as-built-designer.md` (its decisions); `docs/reference/design-practice.md`;
`docs/reference/plant-and-powering.md`; `docs/physics.md` §3, §7, §15.

## Goal

Draw an area on a real map, press Design, and get a network. Today that mostly works on generated maps and often doesn't on real ones.
The version 2 designer (task 23) was held to its bars on three generated families and two Broomfield extracts. On 28 real towns from
task 15 it leaves issues on built nodes, strands whole neighbourhoods, and fails apartment blocks.

All of it is synthetic: the streets are real, the plant is invented (invariant 11).

## Where it starts (measured 2026-11-18)

`cargo run --release -p design --example any_map -- FILE...` with the page's controls (120 homes per node, take rate 0.6, D3.0 share 0.3,
250 ft drops, seed 1, `generic` rules, greenfield, ceiling not lifted), on the 28 towns in `ml/data/osm/` and the two Broomfield extracts:
62,181 homes.

| | Low split | High split |
|---|---|---|
| Maps with an error | 0 | 0 |
| Maps with no issue on any built node | 18 of 30 | 11 of 30 |
| Issues on built nodes | 326 | 435 |
| Homes over the $3,000 ceiling, not built | 1,406 (2.3 %) | 1,226 (2.0 %) |
| Homes unserved | 422 | 422 |

The issues, by cause:

1. **A modem told to transmit under its floor.** `TxBelowFloor`, 14.2 to 17.0 dBmV per 1.6 MHz against 17 (§7.2), on most failing
   high-split maps. The drops stage picks a home's premises against the return path the RF walk estimated. Alignment then fits return
   pads and equalizers, which moves the loss by about a dB, and `drops::refit` re-checks flagged homes against the same stale estimate,
   so it keeps the template that fails.
2. **A cascade outside N/10 + 3 dB** [SCTE 183 §6.2]: 3.21 to 3.84 dB peak to valley against 3.2 at N = 2, one or two amps on some
   maps. Alignment fits pad and equalizer at two pilots; the bow between them is left.
3. **Apartment blocks.** Every unit of a building with `building:flats` is a home with its own drop from a street tap. A 268-unit
   building in Seattle Ballard puts 268 drops on the taps at one structure, no premises template fits, and that one node carries 285 of
   the map's 294 issues. Seattle, Culver City, Minneapolis and Northampton have buildings of 8 units or more (10, 1, 2 and 6 of them).
4. **A neighbourhood cut off from the hub's streets.** Northampton leaves 179 homes unserved: 77 structures whose streets don't meet the
   hub's within the extract, 618 ft away at the nearest. The road that joins them in reality is outside the box, or of a kind the
   importer skips. Version 1 serves them with a node fed by fiber; version 2 needs a street path from the hub.
5. **Station misses**, noted as `TooHot`, `TooFlat` or `TooTilted` at a structure, which leave a tap's port outside the target. These
   fail the Tap-port level bar more than they raise issues.

## What "without issues" means

A map ends one of four ways. The first two are bugs; the last two are said, never hidden.

| Outcome | Bar |
|---|---|
| An error or a panic | None, on any map |
| An issue on a node that's built | None. The "No issues" bar of task 23 (`check_design`, `check_physics` at −20, 20 and 45 °C with task 22's checks, `check_region`, link issues and alignment) |
| A home over the rules' cost ceiling | Built when the page's switch says so, and then held to the bar above like any other node; marked over the ceiling with its cost either way |
| A home unserved | Only when no structure is within a drop of it, or its street joins nothing the hub can reach within the rules' longest crossing. Each says which |

The other ten bars of task 23 are reported per map and must not get worse on the generated families, where they're met today.

## Changes

Each is in the designer. No check is loosened, no tolerance widened, no exemption added. A new rule value or part has a source in a
reference note, or is `Generic`, in a new design-rules version with its migration (invariants 3, 9).

1. **Refit reads the return path as aligned.** `drops::refit` takes each flagged home's return loss from the sim, as it already takes the
   forward level, so a template is judged on the plant as it is. Where none fits, the home takes the template that misses least, and the
   miss is a note.
2. **Return attenuation at the home,** if (1) leaves a modem under its floor: a return-path attenuator at the ground block, in the
   premises parts, where the rules allow it. Sourced or `Generic`.
3. **Alignment's bow.** Where an amp's output misses N/10 + 3, the designer tries the family's other equalizer and pad pairs against the
   whole band, not the two pilots, before anything else, and keeps the best that meets it.
4. **Apartment blocks.** A building with several units is served once: one feed from the street into the building, and a splitter tree,
   or an amp and splitter tree, inside it to each unit. What practice puts there, and its losses, come from a reference note written
   first from public documents. The schema holds it with the parts version 2 already has, or a new version if it can't.
5. **Joining a cut-off piece of street.** A component of the structure graph with homes, not joined to the hub's, is joined at its
   nearest point by a crossing priced as underground construction, up to the rules' longest crossing, and noted with its length. Past
   that, its homes are unserved and say why.
6. **The cost ceiling on the page.** A switch, "Build homes over the cost ceiling", on by default. Nodes over it are built, priced and
   marked as today's `build_over_ceiling` does.
7. **The page's default designer** is version 2 under `generic` rules. Version 1 stays selectable and frozen.

## Deliverables

1. `crates/design/examples/any_map.rs`: the survey above, one JSON line per map and profile.
2. The changes above, each with a unit test on the case that found it.
3. `fixtures/osm/`: small real extracts cut from the towns where each cause appeared (an apartment block, a cut-off piece, a tight
   station), with attribution in its README, so CI holds them.
4. A reference note section for apartment-block practice, and for return attenuation if (2) is built.
5. The survey's before and after in this file's decisions, on both profiles, ceiling on and off.

## Tests

- Each fixture under `fixtures/osm/` designs on both profiles with no issue on any built node, ceiling lifted.
- A home unserved carries its reason; a home built over the ceiling is in `over_ceiling`.
- `one_street` keeps its valid level budget. The version 1 digests and `7298d7aca5f41752` don't move. The version 2 pin moves only with
  its reason recorded here.
- Same seed, same bytes, on one real fixture.
- `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace` green.

## Acceptance

- The 30 maps above: no error, and no issue on any built node, on both profiles, ceiling on and off.
- **Then, once, on maps not looked at while fixing:** at least ten real areas fetched fresh through `import_osm_area` and designed
  through `design_region` over MCP, the page's path, of mixed kinds (dense, apartment-heavy, suburban, rural fringe, hilly, split by
  water or highway). The same bar. Whatever it finds is reported here as found, before any fix for it.
- The page: an area drawn, designed with the defaults, the Design tab showing no issue.

## Out of scope

Version 1's designer, which stays frozen. The legacy-then-upgrade mode, whose estates fail by design (task 23). Retraining task 24's models:
they were trained on plant the changed designer no longer builds exactly, which is recorded here, and the 23-hour selection isn't rerun
unless the user asks. Commercial buildings and business services.

## Deferred

- Streets outside the drawn box fetched for routing only, which would replace (5)'s crossing with the real road.
- A fiber-to-the-home answer for homes over the ceiling.

## Decisions made during the task

- **Where it ended (2026-11-18), the same 30 maps and controls:**

  | | Low split | High split |
  |---|---|---|
  | Maps with an error | 0 | 0 |
  | Maps with no issue on any built node, ceiling not lifted | **30 of 30** (was 18) | **30 of 30** (was 11) |
  | The same, ceiling lifted | **30 of 30** | **30 of 30** |
  | Issues on built nodes | **0** (was 326) | **0** (was 435) |
  | Homes unserved | 119 (was 422) | 119 (was 422) |
  | Homes over the ceiling, not built / built and marked when lifted | 893 / 1,048 | 940 / 1,150 |
  | Mean time a map, this machine | 45 s | 35 s |

  Built with the ceiling lifted: 62,062 of 62,181 homes. All 119 unserved have no structure within a drop of them (the survey
  counts each cause). The Tap-port level bar, a design practice and not an issue, misses on 8 low-split and 2 high-split maps, a
  few ports each: real maps missed it before this task too, and where the sim's last word must choose, a modem inside its limits
  wins over a port on its target.
- **Once, on maps not looked at while fixing:** twelve areas of about 1 km² fetched fresh through `import_osm_area` and designed
  through `design_region` over MCP with the page's controls and the ceiling switch on. No issue on any built node, on either profile:

  | Area | Homes | Nodes (low / high) | Unserved | Over the ceiling, built |
  |---|---|---|---|---|
  | Brooklyn, Park Slope | 3,060 | 22 / 22 | 0 | 0 |
  | Chicago, Lakeview | 1,830 | 14 / 15 | 0 | 0 |
  | San Francisco, Outer Sunset | 2,492 | 17 / 17 | 1 | 0 |
  | Pittsburgh, Squirrel Hill | 1,370 | 9 / 10 | 11 | 0 |
  | Boston, Charlestown | 1,493 | 11 / 13 | 3 | 0 |
  | Katy, TX | 930 | 9 / 10 | 0 | 0 |
  | Duluth, MN | 735 | 7 / 7 | 0 | 1 |
  | Gilbert, AZ | 663 | 4 / 6 | 0 | 0 |
  | Asheville, NC | 527 | 4 / 5 | 3 | 0 |
  | Miami Beach, FL | 350 | 4 / 4 | 2 | 0 |
  | Loveland, CO, rural fringe | 250 | 2 / 2 | 0 | 0 |
  | Pella, IA | 55 | 1 / 1 | 0 | 0 / 55 |

  Pittsburgh's first fetch timed out at the public Overpass server and was fetched again. Through the page, a generated rural map of 477
  homes, which built nothing before, built 40 nodes with no issue, every home marked over the ceiling at $8,289 a home.
- **(1) Refit reads the return path as aligned.** `drops::refit` takes each flagged modem's loss to its first active from the sim at the
  return band's edges, less what its drop and premises put below the port, and judges templates on that. It also kept the node's
  template record stale after a rebuild, which it now updates. Alone this barely moved the misses: no template fitted most of them.
- **(2) An attenuator, not a return-only one.** The spec said return-only attenuation at the ground block. The note sources return-only
  plug-ins only to 85 MHz [CS-TQ], nothing for high split's 204, and a filter whose stop band covers a channel is `FilterBlocksChannel`
  however shallow. So the parts are flat 3 and 6 dB in-line attenuators (`atten-generic-inline-*`, `Generic`), held as a `Filter` with
  no stop band, which no reader treats as more than its pass loss (checked: `upgrade.rs`, the sim, the page). Four templates, at the
  ground block with 50 ft of house cable and on the modem itself, with no share: zero-share templates are drawn last, in the rules'
  order, so they're a remedy and never a draw. The ones on the modem were needed where house cable tilted a modem past the 10 dB spread.
- **(3) Alignment's bow is the equalizer's shape.** Every equalizer in the sim was √f-shaped (§15, approx). Cable is a·√f + b·f, and tap
  through loss rises linearly, so each amp left about 1.5 dB of bow no choice of √f equalizer can remove; two amps exceeded N/10 + 3 at
  N = 2. The sweep as first written had nothing to choose between. Measured before building: a scratch library whose family equalizers
  carry the .500 feeder's two-term shape as a table cleared every flatness miss on the seven towns that had one. Built as new parts,
  `eq-generic-cable-2db` to `-20db`, the existing ones unchanged. **Pilot alignment keeps §15's definition:** `sim::align` picks among
  the equalizers a value and a shape state, and one read from a table only when held, by `align::align_holding`. Allowed to pick them,
  it flipped `amp-1` on the as-built fixture from `eq-generic-20db` to the table one, which would change that fixture and the pins and
  models of tasks 22 to 24; two pilots can't tell the shapes apart anyway. The designer's `feeder::sweep` tries every equalizer of the
  family on an amp outside the limit, re-aligns below it, and keeps the flattest.
- **(4) Apartment blocks** are groups (`Structures::groups`): units of one building at one structure, at least the fewest modems any
  group template holds (4), in runs of at most the most (8). Each group takes one tap port; the feeder judges its drop on the group
  templates (`serves`, `Behind::group_drops_ft`). Templates `mdu-4*` and `mdu-8*`: an 8- or 4-way after a ground block, alone, after a
  10 dB passive-return drop amp, or after `dropamp-generic-1218-active` (14.5 dB forward [ANX-MRA], 10 dB return gain [ANX-FRA]); or a
  unity-gain 4- or 8-port amp with active return in place of both [ABS-IPA3000]. 100 ft home runs. All `Generic` (design-practice.md §9,
  with CommScope's MDU practice [CS-DROP-MANUAL §10]). The feed a group, not a building, is a simplification the note says.
- **(5) Joining a piece** is `structures::LONGEST_JOIN_FT`, 1,500 ft, a constant [inferred] rather than a rule: a rules field is a
  schema version, deferred. A node whose coax crosses a join says so in its notes. Area splits now pool every unit, as `areas` does;
  pooling one entry a group made a large block unsplittable.
- **Faceplate cable simulators** (`cond-generic-cs-2db` to `-8db`, a `TapConditioner` with a rising table, [CS-TQ T-CS] via
  design-practice.md §2): the first taps after an amp tilted modems past 10 dB of spread, and the conditioners were all equalizers.
  They're tried after the shaped ones.
- **The sim's last word** (`drops::settle`): after refit and the sweep, each home the sim still flags has every conditioner on its tap
  tried, then every template it may take, the node read again each time, keeping whatever leaves fewer issues. The drops stage reads
  five check points and its own sum of losses, and the sim the whole band with every reflection; the last few misses were that gap.
  Templates are first fitted with `RESIDUAL_DB` (0.5 dB) of margin, and without it only if none fits.
- **Not done:** an unserved home doesn't carry its reason in the design (the survey reports them in aggregate). Task 24's models were
  trained on plant this designer no longer builds exactly; they weren't retrained. Version 1's designer is unchanged and still the
  frozen choice on the page.
- **The page:** the switch is on by default, `generic` rules are the default, and the cost box says whether homes over the ceiling were
  built.
- **Late changes, after the first full survey** (the numbers above are from the last run, on the final code): candidates are ranked
  by issues, then homes left out, then tap ports off target, then cost (a cheaper candidate that leaves homes out isn't better);
  the sim's last word tries a home's templates before its tap's conditioners, and keeps a conditioner that removes issues even if it
  takes a port off target; a 10 dB in-line attenuator on the modem; the sweep tries only equalizers within 4 dB of the pilots'
  choice, nearest first, stopping at the first that fits, and the taps below a swept amp are retuned with its choice held. A 600-home
  map designs in 9.9 s against master's 6.2 s on this machine (bound 30 s). Two tests in `design_v2.rs` changed their premise, not
  their bar: the small Broomfield extract now builds as one node under the ceiling, and a tighter tolerance may cost conditioners
  rather than actives. V2 pin `0662a19330be2c58`; v1 pins and `one_street` held.
- **Past the designer,** on the Seattle block over MCP: `get_level_budget`, `get_parts_list`, `mark_built`, `get_status_map`,
  `inject_fault` and `diagnose` all work on apartment groups. A suckout at one group's ground block marks its six modems affected,
  but the diagnosis names one unit's drop, not the shared feed: the diagnoser doesn't know groups. Not fixed here.
- **Not exercised:** drawing an area on the page with a WebGL map; the fresh areas went through the same MCP calls the page makes.
