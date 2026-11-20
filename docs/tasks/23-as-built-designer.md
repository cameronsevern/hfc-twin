# Task 23 — A designer that builds what the field builds

**Build-order step:** third of four in [`docs/as-built-plan.md`](../as-built-plan.md).
**Depends on:** Tasks 21, 22
**Produces:** `design::v2`, a second design path driven by `DesignRules`; a fiber design; a priced objective; a legacy-then-upgrade mode;
`design_region` and the page reaching it through MCP; `realism_survey` held to bars fixed below.
**Load:** `CLAUDE.md`; `docs/as-built-plan.md`; `docs/realism-review.md` §2; `docs/reference/design-practice.md`;
`docs/reference/fiber-optics.md`; `docs/tasks/08-design-and-records.md`; `docs/tasks/14-map-design.md`; `docs/physics.md` §3, §6, §13 to §19;
the "Decisions made during the task" of `docs/tasks/21-as-built-schemas.md` and `docs/tasks/22-as-built-physics.md`.

## Goal

Streets, homes, a hub site and a design-rules file in. Out comes a region a field designer would recognize: every part from task 21 placed
where practice puts it, levels set the way a design sets them, and a fiber plant from the hub to every node. Classical and deterministic,
with no AI (`docs/DESIGN.md`).

The current designer isn't edited. It stays as the version 1 path with its digest held (plan rule 7). `design_region` takes the new path
when it's given rules.

Everything it emits is `Designed` and `Synthetic`. Real streets with an invented plant are labelled so, as today.

## Schemas first

Every schema is closed at the version tasks 21 and 22 left it (plan rule 1), so what this task needs is a next version, each with its
migration and a frozen golden of the one before:

- **Design rules, version 2.** Version 1 holds what the note could source, and nothing has read it for a decision yet. Before any stage
  is written, list every value the stages below read, and add what's missing: the cost ceiling per home passed, the tilt limit at a tap
  port, which side of a street plant runs on, the alternatives a candidate set tries, hub and node optics by link type and loading, the
  premises templates each profile allows, and the moves an upgrade in place may make. A value with no source is `Generic`, as before.
- **Region, version 3.** `DesignControls` gains the rules id and the mode.
- **Fiber plant,** only if a link has to store something it can't today, such as a hub pad that sets a transmitter's drive.

`one_street`, both libraries' existing entries and the as-built fixture's meaning don't change.

## How it works

Each stage is its own module and a pure function of what's above it. Every decision reads the sim's own functions, at both ends of the
temperature range, as `place` does today. A rule's value comes from `DesignRules`, never from a constant in the code.

| Stage | Does | Replaces |
|---|---|---|
| `structures` | Poles along aerial streets and pedestals along buried ones, at the rules' spacing, on the side the rules say. Deterministic from the map | Positions anywhere on a centerline |
| `fiber_route` | The router task 14 built, from the hub over streets | Unchanged |
| `nodes` | Groups homes by distance along streets, so a rail line or river divides them. Sizes groups by the rules' homes and actives per node. Sites each node at a structure where coax and fiber cost least together, and balances homes across its ports | Bisection on coordinates, and a centroid |
| `backbone` | Express runs from node ports to pockets of homes, on the rules' express cable, amps by role. A branch at an active uses its outputs. An unequal branch elsewhere takes a coupler of the value the budget wants | One cable, one-output amps, a two-way splitter at every branch |
| `feeder` | Taps at structures, sized by the homes there. Values set by working out from each active to hold the rules' tap-port level within tolerance, at the highest and lowest design frequencies. A conditioner or line equalizer where tilt at the port exceeds the rules. The next active goes where the budget ends. A terminating tap ends a line | Highest tap that keeps the modem above its floor, and slack cable |
| `drops` | Drop size by length, aerial or buried with its span. Every tap port gets a state. A premises drawn from the rules' templates and shares, by seed and home id | A bare modem on one cable |
| `fittings` | A connector at every cable end, splices at reel length, strand or conduit on every span, and each span's path along its street | None of these |
| `align` | `sim::align` (task 22) chooses each active's plug-ins, and `sim::align::aligned` writes them to the plant. The stages above it budget with the part's gain and the family's steps, so what they place is what alignment can reach | Exact alignment by construction |
| `power` | Supply sites at structures near the centre of their load, each feeding a power inserter. Checked at both temperatures against the supply's 80 % rule [SCTE 183 §16.12], inserter port ratings and each active's minimum voltage | A supply on the root, and another at the worst amp |
| `fiber` | Sheaths sized along the route from the rules' fibers per node and standard counts. Closures at branches and reel ends. A link each way per node with strands and wavelengths assigned. Hub optics, CMTS ports and bindings, or a switch port and optic pair for an RPD. Per link, the transmitter, receiver and any hub coupler are chosen so that received power sits inside the receiver's window, the composite index stays clear of clipping at the profile's loading, and a return laser's drive stays under what its part clips at. `sim::optics` checks each | A route length |
| `cost` | Prices parts and construction from the rules' table | Nothing |

**Parts the designer places state what the sim can now read.** Task 22 found that the generic parts state no source MER, noise figure
or CIN tables, so findings S4 and S9 stay open on any plant built from them, and that the generic forward transmitter clips at about
37 dB on a fully loaded plan. So the library gains, for the roles the rules name: a root and amps that state `source_mer`, `nf_table`,
`cin_table` and `ref_composite_dbmv`, and transmitters that state an index for the loading they're meant for. From a sheet where one was
read, `Generic` otherwise (invariant 9). No existing entry changes.

**Premises have to fit the profile.** Task 22's checks flag a point-of-entry filter that stops part of the downstream, a drop amp that
overdrives a modem on a short drop off a high tap, and on a high-split return a modem transmitting into a set-top's tuner through the
home's splitter [TR-MIDSPLIT §6.2]. A template is drawn only where the rules allow it on that profile, and a drop amp only where the
level calls for one.

**The objective is cost.** For each area the designer builds a small, fixed set of candidates (node count one step either side of the rules'
target, and the rules' alternative cable sizes), drops any with an issue, and keeps the cheapest. It reports every candidate's cost, so the
choice can be checked.

**When coax isn't the answer.** Where cost per home passed is over the rules' ceiling, the designer says so for those homes and doesn't
build them a deep cascade. Designing a different access technology for them is out of scope.

**Legacy, then upgraded.** A second mode designs under a legacy rules file and a legacy profile, both data, then upgrades in place to the
target profile through `design::migrate`: sites stay, modules and faceplates change, and the rules say which moves are allowed when
something no longer fits. This is how a generated estate gets the marginal levels an upgraded one has. Both rule sets carry their basis,
and an unsourced legacy rule is `Generic`.

## Bars, fixed before any design is run

Structural, so none rests on a number from memory. Each is checked on the three generated families and the two OpenStreetMap fixtures, on
both profiles, at −20 and 45 °C. No issues, Alignment and Fiber are also checked on the two fully loaded profiles task 22 added, where
composite power and optical clipping bind. `realism_survey` reports them, and a test holds them.

| Bar | Check |
|---|---|
| No issues | `check_design`, `check_physics` with task 22's additions (inserter ports, plug-ins, filters, tuners, optical input, laser drive), `plant_graph`'s `check_region` and `sim::optics::link_issues` are empty on every node not flagged as over the cost ceiling |
| No stand-ins | Every span's length is its path's length within 1 ft. No slack. No span at a minimum length that isn't a jumper the rules name |
| Tap-port level | At every tap, the level at the port is within the rules' tolerance of the rules' target, at both design frequencies and both temperatures |
| End of line | Every line ends in a terminating tap or a terminated through port, as the rules say |
| Every port has a state | A drop, a terminator, or open where the rules allow it |
| Alignment | Every active has plug-ins its family has. Response through any cascade is inside N/10 + 3 dB [SCTE 183 §6.2] |
| Branches | No splitter is the first device after an active that has an unused output |
| Powering | Every supply feeds an inserter. No supply over 80 % of rating, no inserter port over its rating, every active above its minimum voltage |
| Fittings | Every hardline and drop end has a connector that fits its cable |
| Fiber | Every node has a link each way with margin at or above the rules'. No strand is used twice. Every sheath is one of the rules' counts. A structure the coax and fiber plants share has one id and one position in both |
| Parts state their tables | Every root and amp a design places states a source MER or noise figure table as its class allows. So a healthy designed high-split node has no step in RxMER at 1002 MHz |
| Parts list | On one small map, every line of the coax, premises, structure and fiber parts lists matches a count made by hand |
| Deterministic | Same inputs, byte-identical region and fiber plant |
| The old path | The version 1 digest and `7298d7aca5f41752` hold |

Read-outs beside the bars, not bars themselves: the tap-value histogram, tap-port fill, actives per mile, hardline between actives,
supplies per node, cost per home passed, and the same lines the review quoted, before and after.

If a bar can't be met with defensible rules, it stays failed and is reported. Nothing is loosened after seeing a result.

## Deliverables

1. `design::v2` with the stages above, and `design_region` choosing it when rules are given.
2. `fixtures/design_rules/generic.json` filled in from the note, and a legacy rules file and legacy profile for the second mode.
3. Tools: `design_region` takes a rules id and a mode. `get_design_cost`. `list_design_rules`. `get_map` returns the stored fiber plant and
   each span's path. Protection for records from task 14 covers the fiber plant too.
4. The page: rules and mode on the Design tab, spans drawn along their paths, the fiber plant with its closures, premises in the inspector,
   and the cost table. All through MCP (invariant 8). The Browser pane has no WebGL, so the map is checked in headless Chromium with
   SwiftShader, as task 20's review was.
5. `realism_survey` reading both paths, and a new pinned digest for the version 2 path.
6. `docs/realism-review.md` gets a "Status after task 23" section: each finding D1 to D11, fixed, answered or still open, with the survey's
   numbers before and after.

## Tests

- Every bar above.
- A pinned node, a moved hub and a changed rule each re-solve, and a pin holds.
- A tighter tap-port tolerance, or a smaller node size, changes the design the way it should.
- The sim runs on a designed region and every modem comes up. The no-ML diagnoser finds a fault on a designed node.
- A fiber cut on a designed region's shared sheath takes down the nodes behind it.
- Legacy-then-upgrade: sites don't move, parts that no longer qualify are replaced, and what still fails is reported.
- Rules round-trip, and a rule with no basis fails validation.
- Speed: a 600-home map designs in under a time measured and written here, so a re-solve on the page stays usable. A level budget
  through full homes costs about twice one over bare modems (task 22), and every candidate is checked by the sim, so the candidate set
  stays small.

## Out of scope

Designing fiber to the home or any other access technology. Rings and route diversity. Hand-drawing plant on the page, or moving a single
tap or amp. Parcel or address-list upload. Importing an operator's design rules from their own documents. Retraining: task 24.

## Acceptance

`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` green. Every bar reported as met or failed,
as it fell. In the browser, on a real place: import, pick rules, design, see the fiber and a home's wiring, cut a sheath, re-solve after
moving the hub.

## Deferred

- **A better search than a fixed candidate set.** It's a heuristic, and makes no claim to be optimal.
- **Real pole and pedestal data** where a map has it.
- **Costs from a public price list,** if one turns up. Until then the table is `Generic` and says so wherever a cost is shown.
- **Multi-dwelling buildings.**

## Decisions made during the task

### What each stage reads, listed before any stage was written

Version 1 of the rules held what the note could source; nothing had read it for a decision. Each stage's values, with what version 2
adds marked **new** (all optional in the schema, so version 1 files migrate by their version string; `DesignRules::for_design` names
what a file lacks before the designer starts):

| Stage | Reads |
|---|---|
| `structures` | `structures.pole_spacing_ft`, `pedestal_spacing_ft`, `strand_ref`, `conduit_ref`; **new** `side`, `setback_ft`, `pole_ref`, `pedestal_ref` |
| `fiber_route` | Nothing: the router as task 14 built it |
| `nodes` | `node.max_homes`, `rf_ports`, `max_cascade`, the controls' homes per node; **new** `node.max_actives`; **new** per profile `root_ref`; the cost table's fiber construction and amp prices, to weigh a site |
| `backbone` | `roles` (express and feeder cable and amp); **new** `branching.splitter_ref`, `branching.couplers`; **new** per profile `station` (highest and lowest output and tilt a station is set to) |
| `feeder` | `tap_port` (target at both frequencies, tolerance); **new** `tap_port.tilt_tolerance_db`; `tap_families`; **new** `tap_families.parts`; `end_of_line`; **new** `conditioning.conditioners`, `conditioning.line_equalizers`; the feeder role's amp |
| `drops` | `drops`, the controls' longest drop, take rate and D3.0 share; `premises`; **new** per profile `premises` (the templates allowed); **new** `premises_parts` (modems, CPE by device, in-home cable, what an unused port gets, the terminator, the level below which a drop amp is drawn) |
| `fittings` | **new** `fittings.connectors`, `fittings.hardline_reel_len_ft`; `structures.strand_ref`, `conduit_ref` |
| `align` | The station set points `backbone` and `feeder` chose; the parts' own families |
| `power` | `powering` (supply, 80 %, port rating, batteries); **new** `powering.inserter_ref`, `powering.min_margin_v` |
| `fiber` | `fiber` (fibers per node, counts, reel, margin, splice and connector allowances); **new** `fiber.cable_refs`, `closure_ref`, `panel_ref`, `connector_ref`, `jumper_ref`, `duplex_jumper_ref`, `jumper_len_ft`; **new** per profile `optics` (CMTS or core, switch, timing, and the transmitters, receivers, couplers and optics to choose from, by link type; the profile carries the loading) |
| `cost` | `costs`; **new** `costs.plug_in_families`, `costs.ceiling_usd_per_home` |
| Candidates | **new** `candidates.node_count_steps`, `candidates.express_cables` |
| Legacy, then upgraded | **new** `upgrade.moves`; the controls' **new** `mode` names the legacy rules and profile |

A sourced value's note must read `file.md §n` (`RulesError::BadNote`), and a file with version 2's sections must price every part and
plug-in family the designer can place from them (`RulesError::MissingCost`). A rule with no basis doesn't parse: `basis` is required.

### Progress

- **Schemas.** Design rules version 2 (`schema::design_rules::migrate`, frozen `design_rules.v1.json`, new golden
  `design_rules_v2_fields.json`). Region version 3: `DesignControls::rules_id` and `mode` (`DesignMode::Greenfield`,
  `LegacyThenUpgrade`), frozen `region.v2.json` and `design_controls.v2.json`; maps share the version and didn't change. The fiber plant
  stays at version 1: a forward transmitter's drive is set by choosing a transmitter that states an index for its loading, so no link
  stores anything new. Next: the library's new parts, the rules files and the legacy profile.
- **Library, rules files, legacy profile.** Appended to both libraries, no existing entry changed, all `Generic`: a root, an RPD and two
  amps that state their tables (`node-generic-1002-tables`: source MER 44 dB with the optics excluded [CS-OM4120]; the amps' noise
  figures and power from CS-BLE120 and CISCO-HGBT, minimum voltage from CS-BLE180 and the HGBT lockout; each reference composite summed
  from the sheet's own output levels, our arithmetic; CIN, the RPD's and the CMTS's source MER generic, since no sheet states them), a
  CMTS stating a source MER, `ftx-generic-1310-full` at 2.2 % per channel (the index a forward receiver sheet quotes, inside J.186's
  0.30/√N at 156 channels), 4-port faceplate taps for every value, conditioners to 12 dB, line equalizers of 8 and 12 dB, fiber of 6 to
  144 counts, a television, `cable-generic-500` (`.500` with its size family stated, so a pin connector can be checked against it) and
  a `.500` splice. For the second mode: a 750 MHz node, bridger and line extender and 1 GHz 4-port taps, and
  `fixtures/profiles/d30_lowsplit_750_legacy.json` (DOCSIS 3.0, 5-42/54-750 MHz, 32 SC-QAM). `fixtures/design_rules/generic.json` is
  version 2 with entries for the two `_full` profiles; `legacy-750.json` is new. Both validate against both libraries and are complete
  (`schema/tests/design_rules.rs`). Plug-ins are priced by family, and modems and customer devices aren't priced: they aren't plant a
  design builds. The version 1 digests hold. Next: `structures`.
- **Stages `structures` to `fiber`, built and probed together.** `design::v2::{structures, nodes, backbone, feeder, drops, fittings,
  power, fiber}`, each a module, each reading `Kit` (the rules and parts resolved once). `backbone` drives `feeder` in one walk from
  each node port, since where an amp goes depends on whether the next structure's taps fit; they stay separate modules. `fittings`
  is called by the walk as it lays each span, so the levels count the pins and splices. Order: `power` runs before `align`, so the
  inserter's loss at an amp's input is aligned out (the table lists them the other way). The route stage is `route::steiner_tree`,
  task 14's router, run over the structures instead of the street points, so every pole the fiber passes is the coax plant's own.
  Decisions made while building, before any bar was measured:
  - **Tap-port tolerance 3.0 dB, not task 21's 2.0.** Decided while writing `feeder`, before any design ran: the note's own
    implication is at least half a tap step plus the sheets' tap-loss tolerance, 2.5 dB or more (design-practice.md, Implications),
    and the bar holds both ends of −20 to 45 °C, over which a feeder's cable swings 13 % of its loss. With 2.0, half a step leaves
    0.5 dB for temperature: four dB of cable after an amp. `Generic`, and flagged in the report.
  - **Link margin is headroom over the receiver's minimum,** for repairs and ageing, which only lose light (fiber-optics.md §3); a
    link above the receiver's maximum fails outright.
  - **Express** is the cable from an active's output to its first tap; an amp on an express line, or where the line branches, is
    the express role's; one on a tapped line, the feeder role's.
  - **Construction is priced once per run,** whatever is lashed to it or pulled through it: fiber in a node's area overlashes
    the coax strand [SCTE 180 §8.11.6 via design-practice.md §8] and costs its cable, and a node's site is weighed that way.
  - **A tap port's level holds within tolerance at three temperatures** (20 °C, where alignment is done, and both ends); the
    walk reads every loss with the sim's own functions, the return path too, so a home's template is the first drawn whose modem
    sits in the window and transmits inside its limit.
  Probed on the three generated families at 600 homes: plants validate, `check_region` is clean, analog links take the 1×8 hub
  coupler and read −1.9 to −2.1 dBm and 51 dB CNR, and `check_physics` leaves one to three issues a map, at the window edges where
  alignment's plug-in steps (§15's residual) move a level the walk set exactly. Next: `cost`, then the candidates, then the bars.
- **`cost`, the candidates and the ceiling, the bars, and closing the gap to the sim.** `v2::cost` prices from the rules' table
  (parts each or by the foot, plug-ins by family, construction once per run, structures once each, hub equipment with ports by
  the share of ports a design uses, since a chassis serves more than one region). `v2::design` builds every candidate (the rules'
  express cables × node-count steps), drops nodes over the ceiling (their homes said so, `OverCeiling`), keeps the cheapest with no
  issue, and reports every candidate's cost. `v2::bars` measures each bar as the spec words it, on the sim's own levels
  (`Walker::capture` and `transfer` to each tap port). Getting the bars met took fixing the walk, not the bars or the rules
  (beyond the tolerance above, decided before):
  - Power crosses only spans in the plant graph, not a direct mount, so housings at one structure are joined by the rules' jumper
    (`fittings.jumper_len_ft`, `Generic` 3 ft, added to version 2 before it closed), which the stand-ins bar names as the one
    span shorter than a run. The node's inserter sits on its first port, inside the walk's budget; other zones' inserters go at an
    amp's input, where alignment takes up their loss.
  - The station of each active is set to put its first taps nearest the target, not at the edge of the window, so alignment's
    plug-in steps (§15's residual) have room; an amp's station is never set below what its largest pad reaches from its input.
  - After alignment, `feeder::retune` reads every tap's input from the sim and re-values the taps outside the target, and moves a
    station's output or tilt half a dB where its taps are at the end of their values and all miss the same way; `drops::refit`
    draws again any home whose modem the sim flags, against its port as built. A few rounds at most; each change is local.
  - The lookahead plans the devices a fork will get (`plan_branch`, the same choice `branch` makes), counts the jumpers, calls
    for an amp only when the signal runs short (not when the next taps are too hot or too flat, which an amp makes no better),
    and won't place one whose equalizer can't bring its response inside N/10 + 3 dB [SCTE 183 §6.2].
  - A tap's value and conditioner must leave its modems room to transmit (§7.2): an equalizer loses its whole value on the
    return, so a conditioned tap can cost a modem 10 dB of headroom.
- **Tools and the page.** `design_region` runs the version 2 designer when `controls.rules_id` is given, and its second mode with
  `controls.mode`; it stores the plants, the region and the fiber plant (service groups prefixed with the map, in all three), and
  answers with each node's issues, the cost, every candidate and the homes over the ceiling. `get_design_cost` works the cost out
  from the records and the rules each time (`cost::cost_of_records`: each span's and sheath's path gives the runs it follows), so a
  cost is never stored; the candidates and the ceiling's homes are kept as derived, like the cache. `list_design_rules` lists each
  file with its profiles, targets, ceiling, moves, and how many values are sourced. `get_map` draws spans along their paths and the
  stored fiber plant (sheaths, closures, node ends). A fiber fault on the region or a built fiber record locks the map as a node's
  records do, and a re-solve or discard takes the fiber plant with it. The page: rules and mode on the Design tab, spans along their
  paths, the fiber plant with closures and node ends, a home's whole wiring in its popup, and a cost card with every candidate. All
  through MCP. Twin and server tests pass; `get_design_cost` and `list_design_rules` are in the classifier's read-only list.
- **Map check, headless Chromium with SwiftShader,** as task 20's review did, on the `twin` server. On a real place (the small
  Broomfield extract, imported through `import_osm`): rules picked, designed through the Design tab's button, one node of 38 homes
  built and 31 homes over the $3,000 ceiling said so, $2,460 a home with every candidate shown; the sheath from the hub to the node
  end drawn; a home's popup listing its ground block, splitter, modem and set-top and its 108 ft drop; a cut on the sheath took the
  node's link down and locked the map; cleared; the hub moved and the map re-solved (fiber 1,728 → 2,768 ft of sheath). On a
  generated urban map, high split: four nodes, seven sheaths branching at three closures, no design issues; the same map legacy then
  upgraded: five nodes, 683 issues, which is the point. Light, dark and 375 px; console clean.
- **Closing: the bars as they fell, the pins, and what stayed failed.** Measured with `realism_survey` and held by
  `crates/design/tests/design_v2.rs`, on the three generated families at 600 homes and both OpenStreetMap fixtures, both profiles:
  - **Every bar is met on every node that's built**, at −20 and 45 °C, and No issues, Alignment and Fiber on the two `_full` profiles
    too. The rural family builds nothing at the rules' ceiling (coax at $4,243 to $16,805 a home), and every one of its homes is
    reported as over it, so its bars hold with nothing to check; the test also holds that every home is built, over the ceiling or
    unserved. The large extract builds one node on low split and two on high, 248 and 204 of its 313 homes over the ceiling.
  - **Parts state their tables:** a designed high-split node's RxMER steps 0.05 dB at 1002 MHz, against 0.78 dB with the staircase
    parts in the same plant. `one_street` keeps its 0.7 dB step: its parts didn't change.
  - **Parts list:** on a 60-home map every line of the coax, premises, structure and fiber lists matches a count from the records.
  - **The old path:** the version 1 digests hold, `bb42282f8209344b`, `4ccfe14f3dd83669` and `3caa35431bc358fd`, and
    `design_digest` prints `7298d7aca5f41752`. **The new pin:** `every_version_2_design_is_pinned`, low and high split on the three
    families at 300 homes, schema versions stripped: `7e87cd5bc3f69b3b`.
  - **The diagnoser** finds a drifted amp and a suckout on a designed high-split node, class and part. A bad drop it doesn't find, and
    that test is ignored with its reason: the no-ML diagnoser groups siblings by a modem's parent, which is now the home's ground block,
    so no modem has a sibling; and the node's healthy prediction already has 37 modems over `who`'s thresholds (the faulted home's
    modem, healthy, reads 0.70 dB of channel-estimate ripple against the 0.25 dB that counts as affected), so a lone modem doesn't
    stand out and no drop is proposed. Both are the diagnoser's
    features and thresholds, written for a modem on its drop. That's task 24's, not this task's.
  - **With the ceiling lifted** (`bars_hold_with_the_ceiling_lifted`, ignored, and the survey's rural "no ceiling" lines), the nodes the
    ceiling skips are built and fail: on rural, 14 (low split) and 27 (high split) nodes 15 to 47 km from the hub have no optics that
    reach, since the generic rules list only a 10 km LR transceiver and 1310 nm DFB transmitters; on high split 7 modems are over their
    transmit limit and one tap is 0.6 dB over its tolerance; and 171 to 175 line equalizers sit at amps' inputs, where the amp check
    passes with one and not without. On the large extract, its sparse nodes have two hot drops and a tap 15.7 dB over target at the
    lower frequency (low split), an amp over its gain and one amp at 7.5 dB peak to valley (high split). Those
    nodes aren't built at the rules' ceiling, and the No issues bar exempts nodes over it, as written. A node's notes now say why no
    optics fit, with the route's length and splices.
  - **Line equalizer in front of an amp.** Where an amp goes in anyway, its own input equalizer takes the line equalizer's work
    (design-practice.md §1, §4), and the line equalizer stays only where the amp check fails without it. On every map measured, it
    did fail without it, so no design moved.
- **Departures from this spec, and why.**
  - The upgrade isn't through `design::migrate`, as the second mode's paragraph says: that conversion never picks a part with a
    version 2 field (`docs/as-built-plan.md`, rule 7), so `design::v2::upgrade` is its version 2 counterpart, with the same report.
  - `power` runs before `align`, the router runs over structures, the tap-port tolerance is 3.0 dB, and the rules gained a jumper
    length: each is recorded above with its reason.
  - `panel-generic-144` was appended to both libraries, so one panel holds every strand a region uses and strands stay contiguous.
  - The rural family is measured with the ceiling lifted only beside the bars, not as one, since the No issues bar exempts nodes over
    the ceiling.
  - The no-ML diagnoser test is on a drifted amp and a suckout, not a bad drop; the bad-drop test is kept, ignored, for task 24.
- **Speed.** Measured on this machine in the test profile, every candidate built: 5.3 s for the 600-home urban grid on low split and 3.6 s for the suburban map on high
  split, six candidates each. The test holds under 30 s, room for a
  slower machine; a re-solve on the page is that plus the sim's check of the kept design.
- **For task 24's spec** (not edited here):
  - The no-ML diagnoser's sibling grouping and `who` thresholds predate premises parts: group a modem with the others on its tap, not
    its parent, and read ripple against the design's own, as the bad-drop test above shows.
  - The generated rural family builds nothing at the generic ceiling. Its rows would be empty unless rows lift the ceiling, which
    builds plant that fails Fiber for want of long-reach optics, or the family is dropped. That choice belongs before any row.
  - The legacy-then-upgraded estate has 683 issues on the urban grid, mostly transmit-into-tuner and transmit limit, because the moves
    don't add or move an amp or filter a set-top. Rows from it are most modems out of spec, so the spec should say whether that's
    the estate it wants.
  - Version 2 nodes are larger and fewer (3 to 4 on 600 generated homes, 1 or 2 on the extract), so a map yields fewer nodes a row.
- **Closing fixes, round two** (from the plan session's check of task 23). Design rules version 2 and region version 3 stayed open
  for these. One at a time, each compiled and tested before the next.
  - **Fix 1, two checks against sourced designs.**
    - `realism_survey` prints, beside each design's homes and actives per mile, the note's five real node areas [ULM2016 Table 3,
      design-practice.md §6] and the two either side of the design's density. A read-out, not a bar.
    - `crates/design/tests/vogel.rs` builds VOGEL2019's worked tap run (design-practice.md §1): a node at 54 dBmV at 1002 MHz,
      PIII 625, six 4-port tap locations 150 ft apart, a 100 ft drop and the paper's premises (ground block, 4-way, 50 ft) at the
      note's figures, under the generic rules with the paper's cable, source, premises and device target. The test's own copy of
      the library gives the data-sheet cable its size and the ground block and 4-way the SCTE figures the note prices them by
      (0.05 and 8.6 dB at 1002 MHz): the library's generic ones read 0.5 and 10.0 dB, over both standards' ceilings. Existing
      entries don't change, so that's reported here, not fixed. The paper's device target is a floor (its devices sit 0.1 to 4.4 dB
      above it); on 3 dB steps the feeder's nearest-the-target is that floor stated half a step higher.
    - **On the generic rules as they stood**, the feeder took 29, 23, 20, 14 and then an amp at the fourth tap, and the second tap's
      26 was refused. What refused it: the low-frequency target (14 / 6 dBmV, so a port held within 3 dB at 54 MHz too) and the
      port tilt limit (2 dB). Both `Generic`, both contradicted by the notes: taps are nominally set to one level [SCTE 183 §16.4],
      the one worked run states one level per device, North America sets no tilt limit at the modem, and tilt can read negative at
      the end of a run [PNM-3.1 §5.2.6.13, plant-and-powering.md §5]. **Corrected:** rules version 2 makes `at_lo_dbmv` and
      `tilt_tolerance_db` optional (a tilt limit needs a level at the bottom to measure from; `validate` says so), and both rules
      files drop them. The port is held at the top of the band within 3 dB; the bottom is bounded by what the sim holds at the
      modem.
    - **What the tilt rule had been standing in for**, found when the bars ran without it, and each fixed in the walk rather than
      in a rule: (1) the spread and window are the modem's, behind its premises, not the port's: a tap's value now fits only if some
      allowed template keeps a modem inside the window and the 10 dB spread limit (PHY Table 45, physics §3.3) on the structure's
      shortest and longest drops (`drops::serves`, shared with the drops stage); (2) with the return path known, the modem's
      transmit limit and floor are held there too; (3) a far tap at the bottom of its values sits near the floor of the window and
      alignment's residual takes it under, so the walk keeps half a pad step (0.5 dB, pads in 1 dB steps, physics §15) inside the
      window's floor at the two temperature extremes; (4) an amp's output now carries the residual alignment leaves (§15) to the
      next active, so the walk judges the cascade's response, not each amp's, against N/10 + 3 dB, and keeps the same half step for
      the plug-ins' edge error. Every bar holds again on all five maps and both profiles.
    - **The run now:** 29, 26, 20, 17, 11, then an amp, then 29, against the paper's 29, 26, 23, 20, 14, 8. The first five are each
      within one step; devices at 1002 MHz 2.5, 1.0, 2.4, 0.8, 1.3, 3.3 dBmV against the paper's 4.4, 3.1, 1.7, 0.1, 0.6, 1.1. **Not
      met:** no active inside the run, and every device within 1.5 dB. The sixth device behind the paper's 4-way would read about
      14 dB more at 54 MHz than at 1002, past the 10 dB DOCSIS allows at the modem, which the sim holds and the paper (one level per
      device) doesn't show; so the designer puts an amp before it. And on these parts the paper's own values put its devices 1.6 to
      3.9 dB under the paper's levels: premises at the SCTE ceilings are a dB over what the paper's levels imply, the node's inserter
      costs 0.5 dB, and through losses differ. Devices sit up to 2.2 dB under the paper's. `the_designer_follows_vogels_tap_run_to_its_fifth_tap`
      holds what's met; `the_designer_reproduces_vogels_tap_run` holds the criteria as fixed, ignored with that reason.
    - **Two walk changes from that list didn't hold, and were changed again before fix 2.** Carrying each amp's residual down
      the line (into the levels, or into the next amp's check alone) met Alignment, but made every deeper amp look worse:
      areas overflowed the cascade limit of 3 and split, and the urban map built 7 nodes of 60 to 153 homes at 4.1 actives
      a mile against the real nodes' 7.4 to 7.6 [ULM2016]. So the walk judges each amp on its own input again, less half a
      pad step below the first amp, and the design's own Alignment check, run by the sim, counts as an issue on its node
      when the candidates are compared: a candidate whose cascade fails N/10 + 3 dB isn't kept while one passes. On the
      400-home urban map the 2-node candidates fail it and a 3-node one is kept. The half-step margin on the tap window
      applies only at its bottom and only at the temperature extremes; on both sides it shrank every fit and made things
      worse.
    - **A planner bug found on the way:** an area pinned by the controls that overflowed couldn't split, so all its homes
      went unserved (252 of 400 with one pin). It now splits like any other, and the part whose homes sit nearest the pin
      keeps it.
    - **The node-size test** now asks for 80 homes a node, not 120: the candidates step a node either side of the target
      and the cheapest without an issue is kept, so a target within a step of the count kept needn't move it, and on that
      map 120 and the default both keep the same 3-node design. 80 builds 4 nodes of 90 to 116.
  - **Fix 2, sparse plant. Partly done.**
    - **Optics by reach, done.** Appended to both libraries, `Generic`, figures as `fiber-optics.md` gives them: a 10G ER
      (40 km) and ZR (about 80 km) SFP+ [§6, a vendor's sheet], a 1550 nm externally modulated forward transmitter at a
      standard and a full-load index [§5's example sheet], and a 1550 nm DWDM return transmitter at 7 dBm [§5]. The rules'
      analog optics gain `hub_amps` (version 2, optional), tried after none and only after a transmitter in the amplifier's
      band, and the fiber stage puts the amplifier between the transmitter and the coupler. `generic.json` lists the ER and
      ZR optics after the LR, the 1550 nm parts after the 1310 nm ones, the existing 17 dBm EDFA as a hub amp, and the
      second return receiver; all priced, `Generic`. On the rural map with the ceiling lifted, every node now has optics
      that reach: the Fiber bar holds.
    - **The control, done.** `DesignControls::build_over_ceiling` (region version 3, default off, so `design_region` behaves
      as before): the nodes over the ceiling are built and still listed, with their homes, in `over_ceiling`. The survey's
      rural read-out and the ceiling-lifted tests use it.
    - **Sparse nodes, not done.** With the control on, every bar holds on the small extract, and on the large one every node
      but one sparse node: 40 homes, 9 amps, where the node's port splits straight into two runs of about 1,380 ft. The walk
      carries the line past the nine empty poles on one (each about 180 ft apart) without an amp, so the first taps arrive
      too low and are fitted best effort: two homes hot at 54 MHz and under their transmit floor, three taps off target, and
      on high split an amp over its gain. The lookahead at an empty pole should have called for an amp before that; why it
      doesn't is the next thing to find, not found yet. On rural: one amp at 3.75 dB at N = 2 (low split), and two modems
      under their transmit floor (high split). `bars_hold_with_the_ceiling_lifted_on_the_extracts` and `..._on_the_rural_family`
      are ignored with those reasons.
  - **Fix 3, the upgraded estate. Not started.**
  - **Fix 4, bookkeeping, as far as the fixes went.** The version 2 pin moves from `7e87cd5bc3f69b3b` to `81b0636b2f0c7581`:
    the rules dropped the port's low-frequency target and tilt limit, the walk holds the modem's window, spread and transmit
    range behind its premises, a cascade failing Alignment counts against its candidate, and long-reach optics are listed.
    It's re-recorded here, before fix 2's sparse node and fix 3, so the tree stays green; both will move it again, and it's
    recorded again then. The version 1 pins don't move. The diagnoser test now holds an amp drift on six seeds (class and
    part, all six); a suckout on the same node is classed right on one seed of six and its span named on four, and that
    test is ignored for task 24 beside the bad drop.
- **Closing fixes, round three** (the plan session's second check: finish fix 2, do fix 3, two additions).
  - **A, the Vogel run's sixth tap.** Worked on the sim's own input at the fifth tap. A plain terminating 8 dB tap leaves the
    device 14.8 to 15.9 dB hotter at 54 MHz than at 1002 across −20 to 45 °C, past the 10 dB DOCSIS allows at the modem. A
    conditioner holds that: 6 dB of one gives 7.7 to 10.7 dB, 8 dB gives 6.0 to 8.9 dB. But the faceplate family's
    conditioners are shaped to 1218 MHz, so on a 1002 MHz plant one costs its 0.3 dB and about 0.9 dB of slope at the top:
    with 8 dB the port reads 15.9 dBmV at 45 °C against a 20.0 target, 4.1 dB under, past the 3 dB tolerance. The designer
    does try them: on every tap, the plain values first and then conditioners from the smallest, terminating taps included,
    and the family takes one. What rules it out here is the tap-port tolerance at the hot end with the family's slope band,
    not the order of preference, which stays (operators ask for unconditioned taps near the end of a line, design-practice.md
    §1). The ignored Vogel test says so.
  - **B, premises at the standards' ceilings.** Appended to both libraries, `Generic`: `gb-generic-scte129` (SCTE 129: 0.05 dB to
    1218 MHz, 30 dB return loss) and `splitter-generic-4way-home-scte153` (SCTE 153 class 2 for a 1218 MHz part: 8.8 dB at 600 to
    1002 MHz, 8.9 to 1218; the note gives no class 2 figure below 600 MHz, so 55 MHz takes class 3's 7.8 dB), both from
    design-practice.md §9, and priced as the older two. Both rules files' premises templates name them. `docs/library.md` has a
    task 23 section saying the older two are over those ceilings (the ground block ten times SCTE 129's; the 4-way at class 3's
    ceiling, a 1794 MHz part's, on an entry rated to 1218) and stay because the as-built fixture's oracles are worked on them.
    Correcting the first report: the old 4-way isn't over every class, it's over its own. The Vogel test keeps its own class 1
    copy, 8.6 dB at 1002 MHz, since the paper's plant is a 1002 MHz one. Every version 2 test and bar holds on the new parts.
  - **Fix 2, the sparse node: found and fixed.** The walk's lookahead was right; it was never asked. `lift`, which moves a fork onto
    an active's free outputs so no splitter is the first device after an output to spare (the Branches bar), hands each output a
    branch carrying the whole shared path to the fork: on the large extract, nine empty poles, about 1,700 ft. The walk took
    that branch in one step, so none of those poles was looked at for an amp, and the first taps arrived at 13 dBmV and were
    fitted best effort. A lifted branch is now walked one run at a time, each pole looked at a run ahead as any empty
    structure is, and the amp goes at the last pole before the line can't go on. (Looking at the whole remaining path from the
    first pole put amps 152 ft from the node with their input pads at 20 dB and outputs 2 to 4 dB hot; one run ahead is what
    every other empty structure does.) Built over the ceiling, both extracts now hold every bar on both profiles, and
    `bars_hold_with_the_ceiling_lifted_on_the_extracts` runs and passes. No other version 2 design moved.
  - **Fix 2, rural: stays failed.** Built over the ceiling at 8 to 10 homes a mile, every bar holds but two checks: one amp of 188
    at 3.75 dB peak to valley at N = 2 against 3.2 (low split), and one tap of 706 at 10.91 dBmV at 45 °C against 11.0 (high
    split). Both are what alignment leaves on spans of about 1,100 ft, beyond what the walk sees: the second amp's response adds
    to the first's, and a far tap's level after alignment sits 0.6 dB under the walk's at the hot end. The way to hold the first,
    carrying each amp's residual into the next one's check, split urban nodes to 60 to 153 homes in the second round (recorded
    above), so it isn't defensible here either. `bars_hold_with_the_ceiling_lifted_on_the_rural_family` is ignored with that
    reason. Rural isn't built at the rules' ceiling.
  - **Fix 3, the upgraded estate.** The bar, as fixed before any move was added: nothing service-breaking, so no `TxLimit`,
    `TransmitIntoTuner` or `LevelTooHigh` and no issue on an active, a supply or a link; the structural bars (stand-ins, port
    states, fittings, powering, fiber, alignment) hold; `LevelTooLow` and the Tap-port level bar may miss at the top of the band
    and are counted. Before: on the 600-home urban grid, 572 issues (383 transmit into a tuner, 116 over the transmit limit, 53
    short and 20 hot at 258 MHz), Tap-port level 153 of 414 outside; every structural bar already held.
    - **The legacy design meets every bar under its own rules**, on the 600-home urban grid (`the_legacy_design_meets_every_bar_under_its_own_rules`).
      To get there, the three 750 MHz legacy actives this task appended gained tables (noise figure and CIN interpolated from the
      1002 and 1218 MHz table parts to 750 MHz, source MER 44 dB, composite reference scaled by bandwidth; `Generic`, our
      arithmetic): they had none, so the legacy design failed Parts state their tables. On the 400-home map one tap check of 294 is
      0.2 dB under at 45 °C, on a station boxed in by its near taps at their highest value; that test is ignored with the reason.
    - **Set-tops replaced.** A new move, `ReplaceSetTops` (`Sourced`, design-practice.md §10: replacing set-tops is one of SCTE
      300's upgrade expense lines): where the target's return band reaches the tuner band (54 MHz up), RF set-tops and
      televisions on the coax come off it, replaced by devices on the home's network; an in-home splitter left feeding one
      device comes out with them (`RemovePremisesParts`, the same expense line's "removing home amps, splitters and filters"),
      its two house cables joined. On the 600-home map: 471 devices and 273 splitters. That took transmit-into-tuner to none,
      and with the splitters' loss gone, transmit limit and the top-of-band shortfall too.
    - **Faceplates re-valued.** The hot bottom of the new band (30 modems over 15 dBmV at 258 MHz) came from near taps already at
      their highest value under a station set to the rules' top output. The upgrade now sets each station where its own taps
      sit inside the tolerance, if any level inside the rules' station range does (`feeder::center_stations`), and re-values
      the faceplates from the sim's levels; the report records each re-valued plate as its swap. The greenfield pipeline uses
      the same station step after its retune; on the pinned maps it moved nothing (their digest was the same before and after).
    - **Boosters not added.** SCTE 279 puts boosters only where spacing is very long (§10). Once set-tops and splitters came out,
      no modem on either map was over its transmit limit and no amp short of input, so no estate measured called for one, and
      inserting an amp into an existing run (power, alignment, cascade) wasn't built for a case that doesn't arise here.
    - **After:** on the 600-home urban grid, no issues of any kind, every structural bar met, Tap-port level 13 of 414 outside
      (all at the top of the band, the only frequency the rule now holds). On the 400-home map, the same, with 27 of 294 outside.
      `legacy_then_upgraded` holds the bar.
  - **Fix 4, bookkeeping.** The version 2 pin moves once more, from `81b0636b2f0c7581` to `f22e294c97482f3e`: what moved it is
    the premises templates' ground block and 4-way at the standards' ceilings (B); the walk's lifted-branch step and the station
    centring move none of the pinned maps. The version 1 pins hold: `bb42282f8209344b`, `4ccfe14f3dd83669`, `3caa35431bc358fd`,
    and `design_digest` prints `7298d7aca5f41752` then `3caa35431bc358fd`. `docs/realism-review.md`'s status section and
    `docs/tasks/README.md` are current. Ignored tests, each with its reason: the full Vogel run, rural built over the ceiling, the
    legacy design on the 400-home map, and the bad drop and suckout for task 24.
