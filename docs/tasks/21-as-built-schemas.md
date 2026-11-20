# Task 21 — Sources and schemas: every component, fiber included

**Build-order step:** first of four in [`docs/as-built-plan.md`](../as-built-plan.md).
**Depends on:** Tasks 01, 02, 08, 09, 14
**Produces:** `docs/reference/design-practice.md` and `docs/reference/fiber-optics.md`; `schema::component` and `schema::plant` at version 2;
`schema::fiber`, `schema::design_rules` and `schema::region` version 2; library entries for every new class; `fixtures/one_street_as_built/`;
`plant-graph` reading all of it; pinned digests.
**Load:** `CLAUDE.md`; `docs/as-built-plan.md`; `docs/realism-review.md` §3; `docs/tasks/01-schemas.md`; `docs/tasks/02-plant-graph.md`;
`docs/reference/plant-and-powering.md`; `docs/reference/daa-remote-phy.md`.

## Goal

The plant graph can hold everything a built network has, from the CMTS port to the device in the home, and nothing that reads a plant
breaks. No new physics and no new designer: the sim says plainly that it can't price a new part yet, and task 22 teaches it.

The schema changes once, for everything. So this task has to get the shape right for parts whose physics and placement come later, and it
proves the shape by building one street by hand with every part on it.

## Before anything else: pin the digests

On the tree as it is, before any other edit, add tests that hold:

1. **Sim output on `one_street`.** `simulate` on both plants, over a short environment series (cold, mild, hot, rain), healthy and with every
   fault in `fault_examples.json` and `fault_examples_rpd.json` applied one at a time. Full detail, with randomization on in one run and off
   in another.
2. **Every generated design.** What `crates/design/examples/design_digest.rs` hashes today.

Both hash the JSON with every `schema_version` key removed, using a hash that doesn't depend on the Rust release, such as
`sim::rng::hash_str`, since `DefaultHasher` makes no such promise. `design_digest` also keeps printing its old value over the designs with
their version strings set back to `"1"`, which must stay `7298d7aca5f41752`. That shows the designs themselves didn't move.

Record the values in this file under "Decisions made during the task". They hold through tasks 21 and 22.

## Deliverables

### 1. Two reference notes

In the existing notes' form: about 400 lines, every claim cited `[DOC §section]` and tagged, ending with **Implications for hfc-twin** and
**Gaps**. Add both to `docs/reference/README.md`'s table and its Sources. Start from documents already in that list (SCTE 183 §16, TR-DCA,
R-PHY, SCTE 15, 32 and 74, SCTE 279, TR-TAR, SCTE 300), then public vendor design guides, construction manuals and data sheets. Read a PDF
directly when a fetch's summary comes back thin, as task 09 found. A question no public document answers goes under Gaps, and its value in
the library or the rules is `Generic`.

**`design-practice.md`** has to answer:

- Tap-port design level and how it's stated, the allowance for the drop and the house, and the end-of-line rule.
- Tap families: port counts, values, terminating taps, plug-ins that equalize or attenuate at the tap.
- Amplifier types by role, their output configurations, and the pad and equalizer values a station takes.
- Cable sizes by role, and drop size by length.
- How branches are made: an amp's own outputs, couplers, splitters.
- Node size, port use and segmentation.
- Powering practice: where supplies and inserters go, port limits, what a supply carries.
- Structures: pole spacing, pedestal placement, strand, conduit, expansion loops.
- In-home wiring: ground block, splitter loss and isolation, cable, drop amps, filters, what devices a home has.
- What an upgrade in place keeps and replaces.

**`fiber-optics.md`** has to answer:

- Fiber attenuation by wavelength, splice and connector loss and return loss.
- Cable construction: fiber counts, types, reel lengths, closures.
- Analog forward and return links: transmitter and receiver parameters, how CNR follows received power and modulation depth, clipping.
- Digital links for DAA: pluggable optic power and sensitivity, wavelength multiplexing, what sits in the hub.
- Architectures: star, ring, how many fibers a node takes.
- What a node or RPD reports about its optics.

### 2. `schema::component`, version 2

Additive (plan rule 1). New classes, each with a spec carrying only what the notes and `docs/physics.md` will need. Units in every field
name, optical power in dBm, wavelength in nm, fiber length in feet and loss per 1000 ft.

| Group | New classes |
|---|---|
| Coax | `PlugIn` (pad, equalizer, cable simulator, return pad, return equalizer, tap conditioner, each with `value_db` and a `family`), `LineEqualizer`, `Connector` (hardline pin, hardline splice, housing adapter, drop F) |
| Premises | `GroundBlock`, `DropAmp`, `Filter` (shape, band edges, depth, stop-band return loss), `Cpe` (a device that reports nothing) |
| Power | `Battery` |
| Structures | `Structure` (pole, pedestal, vault, cabinet), `Strand`, `Conduit` |
| Fiber | `FiberCable`, `SpliceClosure`, `PatchPanel`, `FiberConnector`, `OpticalPassive` (splitter, mux, demux), `OpticalTx`, `OpticalRx`, `OpticalAmp`, `Transceiver`, `Switch`, `Cmts`, `TimingSource` |

New optional fields on existing specs:

- `ActiveSpec`: `outputs` (per output: name, loss from the main output, return combining loss), `plugin_family`, a noise figure table and a
  CIN table by frequency, gain flatness, AGC pilots.
- `NodeSpec`, `RpdSpec`, `Cmts`: a source MER table by frequency. `RpdSpec` and `Cmts`: an upstream receiver ceiling.
- `TapSpec`: `terminating`, `plugin_family`.
- Every part with a `return_loss_db`: an optional table by frequency, and for a tap one per port class. The single number stays as the
  fallback.
- `CableSpec`: a size `family`, so a connector can be checked against its cable, and `ampacity_a` [SCTE 32].
- Any part: an optional tolerance on its stated values, which is what §8's randomization will draw within. No existing entry states one, so
  no existing plant's draws change.

A part with no RF path has no rated band. `LibraryError::BadRatedBand` now asks `ComponentSpec::has_rf_path()` where it named
`PowerSupply`. `qualifies` treats such a part as it treats a supply.

### 3. `schema::plant`, version 2

Additive. The tree stays one node's service group.

- **`ElementKind`:** `LineEqualizer`, `GroundBlock`, `DropAmp`, `Filter`, `Cpe`. A splitter or coupler in a home is the existing kind with a
  premises part.
- **`Element.fitted: Option<Fitted>`**, what's installed in or on it:
  - On an active: input pad, input equalizer or cable simulator, a pad per output port, return pad and equalizer, each a `PlugIn` by id, and
    an optional set point. On a `Node`, its forward receiver and return transmitter. On an `Rpd`, its optics.
  - On a tap: a conditioner, and a state for each tap port without a drop: `Terminated { component_ref }` or `Open`. A port with neither a
    drop nor a state means what it means today.
  - On a `GroundBlock`: the home's `coupling_db`, how well its wiring keeps ambient signals out.
- **`Element.structure`** and **`Plant.structures`:** `Structure { id, kind, component_ref, geo, record_status }`.
- **`EdgeKind::Span`** gains `fittings: Vec<Fitting { pos_ft, component_ref }>`, `support` (a `Strand` or `Conduit` by id) and `path`, the
  points between its ends. **`EdgeKind::Drop`** gains `fittings`, `path` and an optional `installation`. **`EdgeKind::House { cable_ref,
  len_ft, fittings }`** is cable inside a home.
- **`Edge.record_status: Option<RecordStatus>`.** Cable is plant too. `None` follows the element the edge enters.
- **`PowerSupply`** gains `batteries` and `structure`.
- **`validate`** adds, as new errors: a modem is below a `Drop`, through `House` edges and premises parts only; a `House` edge is below a
  `GroundBlock`; `fitted` matches its element's kind and each id's class; a plug-in's family is its host's; a terminating tap has no
  through edge; a fitting's position is on its edge; structure references exist.
- **`parts_list`** counts fittings, plug-ins, modules, batteries, structures, and supports by the foot.

### 4. `schema::fiber`, version 1

One fiber plant per region. Stored, with a record status on everything (plan rule 8).

```
FiberPlant { schema_version, id, origin, hubs, elements, cables, links, bindings, structures }
Hub              { id, geo, record_status }
FiberElement     { id, kind, component_ref, geo, hub: Option, structure: Option, record_status }
FiberCable       { id, component_ref, from, to, len_ft, installation, path, support: Option, record_status }
OpticalLink      { id, service_group, direction: Forward | Return | Both, wavelength_nm, hops }
ServiceGroupBinding { service_group, cmts, port }
```

`FiberElement.kind`: `Cmts`, `OpticalTx`, `OpticalRx`, `OpticalAmp`, `OpticalPassive`, `Switch`, `Transceiver`, `TimingSource`, `PatchPanel`,
`SpliceClosure`, and `NodeEnd { service_group }`, which is where a link meets that node's plant.

A link is a path of hops: `Fiber { cable, strand }`, `Splice { at, loss_db: Option }`, `Connector { at, component_ref }`,
`Through { element, port_in, port_out }`. A budget is a sum over hops, and a cut on a cable reaches every link with a hop on it, so one cut
can take several nodes.

`validate`: hops are contiguous; a link starts at something that transmits and ends at something that receives; a strand is within its
cable's count; no two links use one strand in one direction on one wavelength; every binding and reference exists. `parts_list` as for a
plant.

### 5. `schema::region`, version 2, and `schema::design_rules`, version 1

`Region` gains an optional reference to its fiber plant. `StreetMap.headend` stays as the hub's site.

`DesignRules` is the operator's design specification as a data file, as a profile is for an architecture and the policy table is for
repairs. It holds every rule the first note sources: tap-port target and tolerance, premises allowance, end-of-line rule, tap family, cable
and amp by role, node size and port rules, powering rules, structure spacing, premises templates and their shares, fiber rules (fibers per
node, standard counts, reel length, link margin), and a cost table by part and by foot of construction. Each value carries its basis:
`Sourced { note }`, naming the section of the reference note, or `Generic`. Ship `fixtures/design_rules/generic.json`. Nothing reads it for
a decision until task 23.

### 6. Library

`fixtures/library/generic.json` gets at least one `Generic` entry per new class, and what the fixture below needs: taps of 2 and 8 ports,
a terminating tap, more coupler values, a multi-output amp, a larger hardline, series 11 drop [SCTE 74], pads and equalizers in a
family's steps. `fixtures/library/sourced.json` gets a `Datasheet` entry wherever a public sheet was read, with `source_url`.
`docs/library.md` records, per sourced part, which fields the sheet doesn't state, and lists the classes that stayed `Generic`.

No existing entry changes, since `docs/physics.md`'s oracles are computed on them.

### 7. `fixtures/one_street_as_built/`

The same street as `one_street`, built by hand with every part, on both profiles. Every element `Built`, both plants `Synthetic`. This is
the plant task 22's oracles are computed on, so keep it small enough to work by hand.

- A hub: a CMTS port and analog optics for low-split; a switch port and an optic for high-split. Two fiber cables joined in one splice
  closure, a patch panel at the hub, connectors at both ends, a link each way.
- The root with its optics and its plug-ins. One multi-output amp and one single-output amp, each with pads and an equalizer. A coupler and a
  splitter. A line equalizer.
- Taps of 2, 4 and 8 ports, one terminating, one with a conditioner. Unused ports terminated, and one left open.
- A connector at every cable end, and one hardline splice.
- A power inserter fed by a supply with batteries, in a cabinet.
- Homes: a bare modem on a drop; a ground block, a two-way splitter, in-home cable, a modem and a `Cpe`; one with a drop amp; one with a
  filter; one on a long series 11 drop; one buried drop.
- Poles and strand on the aerial spans, and one buried span in conduit with a pedestal.

### 8. Readers

- **`plant-graph`:** indexes the new kinds and edges. New queries: the premises a device is in (derived from the tree, never stored), an
  edge's fittings, a link's hops in order, the links that cross a cable. A region-level check says whether every service group has a link
  each way. `check_design` on one plant is unchanged. Everything stays in canonical order.
- **`sim`:** compiles, and is byte-identical on plants without new parts. On anything with an RF or power effect that it has no physics
  for yet (a new kind of element, a `House` edge, a fitting, a plug-in, a port state, a fitted optic), it returns a new
  `PhysicsError::Unsupported` naming it. It never treats one as transparent. Structures and supports have no such effect, and are ignored.
- **`design`:** the current designer is untouched and becomes the version 1 path (plan rule 7).
- **`twin`, `mcp-server`:** stored documents migrate on load. `get_plant` and `get_parts_list` return the new parts. A new read-only tool,
  `get_fiber_plant`. The page draws new kinds and shows them in the inspector, with no new controls. A plant with version 2 parts is
  diagnosed by the no-ML floor, and the answer says the model wasn't trained on such a plant (plan, risks).
- **`features`, `proposal`, `diagnosis`, `datagen`, `emulator`:** compile, and behave as before on plants without new parts.

## Tests

- The pinned digests, unmoved.
- Round trip, golden sample and exported JSON Schema for every new and changed schema.
- Frozen `plant.v1.json`, `component_library.v1.json` and `region.v1.json` parse, migrate, and equal the current samples apart from the
  version.
- Every file under `fixtures/` from before this task parses and validates unchanged.
- `Plant::validate` and `FiberPlant::validate`: the new fixture passes, and one hand-broken variant fails for each new error.
- Parts lists by hand: `one_street` is as task 01 says, and the new fixture's coax, premises, structure and fiber counts match a table in
  this file.
- `plant-graph`: the new queries on the new fixture, against hand-listed answers, and order-free under shuffled lists.
- `sim` on the new fixture returns `Unsupported`. Task 22 turns this test over.
- `qualifies`: every new generic part qualifies on a shipped profile or has no RF path.
- Library: every `Datasheet` part has a link, and the generic file claims none.
- Twin: a stored version 1 node loads, and a plant with version 2 parts gets the floor with its reason.

## Out of scope

Any physics for a new part: task 22. Any change to what the designer builds: task 23. Observation, fault and environment schemas, which
change when the sim can produce or use the new fields: task 22. Editing a fiber plant or a home through the page. Importing real plant
records.

## Acceptance

`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` green. The pinned digests and
`7298d7aca5f41752` hold. A reviewer can take each row of "Where each component lands" in the plan and find its class, its place in the
plant and its instance on the new fixture. Both notes say what they couldn't source.

## Deferred

- **Structural return loss of cable** as plant input. SCTE 15 gives a limit, not a profile along a reel.
- **More than one hub per map,** and route diversity. One headend stays the design input.
- **Power crossing between nodes.** A supply still belongs to one plant.
- **Multi-dwelling buildings** as their own structure, with risers and lock boxes.

## Decisions made during the task

### Pinned digests

Pinned on the tree at `40d0133` before any other edit, with the uncommitted plan and review files present and no code changed.
FNV-1a (`sim::rng::hash_str`) over JSON with every `schema_version` key removed. They hold through tasks 21 and 22.

| Digest | Test | Value |
|---|---|---|
| Sim on `one_street` low-split: healthy and the 13 coax examples one at a time, randomization off and on (seed 3), full detail, a 4-step series at −20, 20, 45 and 12 °C with 25 mm/h rain on the last | `crates/sim/tests/pinned.rs` | `bb42282f8209344b` |
| Sim on `one_street` high-split: the same, plus the 5 RPD examples | `crates/sim/tests/pinned.rs` | `4ccfe14f3dd83669` |
| Every generated design: both profiles, three families, seeds 1 to 8, 300 and 1,000 homes | `crates/design/tests/pinned.rs` | `3caa35431bc358fd` |
| `design_digest` over the designs with version strings set back to `"1"` (`DefaultHasher`, as before) | `crates/design/examples/design_digest.rs` | `7298d7aca5f41752` |

All 66 sim runs returned observations; none errored.

### Versions and migration

- **`one_street` stays a version 1 file.** Its plants aren't edited, not even the version string, and every reader brings them forward
  with `schema::plant::migrate`. `every_file_is_on_the_current_schema_version` now asserts exactly that. The libraries are version 2,
  since they hold version 2 classes. Frozen `plant.v1.json`, `component_library.v1.json` and `region.v1.json` migrate to equal today's
  samples, as `fault_set.v1.json` does. Version 2's parts have samples and goldens of their own (`plant_as_built.json`,
  `component_library_as_built.json`), so the main samples stay what version 1 migrates to.
- **Maps share region's version.** `StreetMap` didn't change, but it shares the module's `SCHEMA_VERSION`, so generated maps now say `"2"`.
  `region::migrate_map` brings stored ones forward. The pinned digests ignore version strings, so this moves none of them.
- **The policy table's exported JSON Schema widened** without a version bump of `work_order`: `PolicyEntry::component_class` is a
  `ComponentClass`, which gained variants. No policy file changes shape and every version 3 file still parses. If a strict reading of
  invariant 3 wants a bump for an enum it borrows, that's `work_order` version 4 with an identity migration; flagged rather than done.
- **The twin migrates on load:** the library and `one_street` in `load_one_street`, and every stored plant, region and map in the
  server's `load_into`. A stored document on a version nothing knows fails with `TwinError::Migration` or the store's error.

### Schema choices

- **`Fitted::Passive` covers taps, splitters and couplers,** not taps only. A capped splitter leg or an unused coupled port is the same
  question as an unused tap port, so all three take port states. Only a tap takes a conditioner. The fixture caps `sp-1`'s second leg.
- **`ActiveFitted` is one struct for nodes, RPDs and amps.** Optics are checked against the root's kind: analog optics on a `Node` only,
  transceivers on an `Rpd` only.
- **A support is a part, not a record:** `support: Option<ComponentId>` names a `Strand` or `Conduit`, counted by the foot as the span's
  length. A strand run from pole to pole isn't its own record.
- **Structures are listed per document.** A pole the coax plant and the fiber plant share appears in both with the same id, and nothing
  checks yet that the two agree (`pole-1` in the fixture). That's a region-level check for task 23, when the designer places both.
- **`RoutePoint { geo, structure }`** is a path point, so a span names the poles it passes.
- **`PowerSupply.record_status`** was added, optional, as on an edge: invariant 11 says every plant element records it, and a supply
  had none.
- **Batteries are a bank:** `BatteryBank { component_ref, per_string, strings }`, so task 19's standby can read string voltage and
  capacity. The parts list counts `per_string × strings`.
- **`Tolerance`** carries ± bounds on losses, gain, loop resistance, return loss, noise figure and a modem's ceiling: what §8 draws now and
  what task 22's spec says it will draw.
- **Premises rules.** A home's parts are `GroundBlock`, `Splitter`, `DirectionalCoupler`, `DropAmp`, `Filter`, `Terminator`, `Modem`
  and `Cpe`. A modem or customer device must reach a `Drop` through `House` and `Direct` edges and those kinds only. A modem that
  doesn't still reports version 1's `ModemNotOnDrop`, and `NotThroughPremises` too when an element is in the way. `GroundBlock` and
  `DropAmp` must be below a drop. A `Filter` may also sit on a tap port, outside a home, as tap-port filters do. A `House` edge has a
  ground block at or above its upstream end.
- **An amp's `outputs` name its ports.** `OutputSpec.port` is the port id on an element built from the part, where the spec said "name".
- **Fiber elements have `ports`,** which the spec's shape didn't list, so a `Through` hop and a binding name a port that can be checked.
  An element no link passes through needs none.
- **A node end's part is the node's,** a `Node` or `Rpd` part. The spec gave `FiberElement` a required `component_ref`; for a node end
  the only sensible part is the node itself, and `check_region` checks it against the plant's root. The fiber parts list leaves node ends
  out, since the coax plant counts the node.
- **Links.** A link starts at an `OpticalTx` or `Transceiver`, or at a node end if it carries return; it ends at an `OpticalRx` or
  `Transceiver`, or a node end if it carries forward. Hops must join end to end, and two fibers in a row need a splice or connector
  between them. A strand is used once per direction per wavelength, and a `Both` link holds both directions.
- **Splices have no part.** `Hop::Splice` has an optional measured loss and no `component_ref`, as the spec shaped it, so the parts list
  can't count them; `FiberPlant::splices()` does, once per pair of strands joined at an element. Connectors are counted once per
  element and strand, however many links cross them.

### Readers

- **What the sim refuses** (`sim::support`), naming each as `what: where`: a version 2 element kind, anything `fitted`, a `House`
  edge, a fitting, a buried drop, and a version 2 field with an RF effect set on a version 1 class (`outputs`, `nf_table`, `cin_table`,
  `gain_flatness_db`, `agc_pilots_mhz`, a return loss table, `source_mer`, `us_rx_ceiling_db`, `terminating`, `leg_isolation_db`). A
  stated `tolerance` is refused only when randomizing. Every entry point checks: `simulate`, `Walker::new`, `Overlay::build`,
  `solve_ac` and `transfer`. **Ignored:** structures, supports, paths, record statuses, size families, ampacity, plug-in families and
  batteries. Batteries matter only to standby, which the sim doesn't model (task 19). A drop marked `Aerial` is what version 1 assumed of
  every drop. A plant carrying every ignored part simulates to the same JSON as without them, randomized or not
  (`sim/tests/as_built.rs`).
- **The version 1 designer stays frozen** with a larger library: it places only 4-port through taps with no version 2 field
  (`design::optimize::version_1_tap`), which was every tap the library held when it was written. Migration's replacement search keeps a
  tap's ports and termination, a splitter's legs and a coupler's value, and never picks a part with a version 2 field. Before, a 2-port
  tap of the same value would have been the replacement for a 4-port one by id order, and an invalid plant. The design digests hold.
- **`plant-graph`** adds `premises_of` and `premises` (a home is everything below one drop, derived), `fittings`, `FiberGraph` with
  `hops`, `links_on_cable` and `links_for`, and `check_region`, which reports a missing node end, binding or link each way, a node end
  whose part isn't the root's, and a region that names another fiber plant. `check_design` on one plant is unchanged.
- **The twin** keeps fiber plants by id, validates them on `add_fiber_plant`, persists them in the meta document, and serves them read
  only through `get_fiber_plant` with the parts list, splices and each region's `check_region` issues. A plant with any version 2 part,
  RF-neutral ones included, isn't shown to the proposal model: `diagnose` answers with the no-ML floor and says why in
  `model.withheld_reason`. The floor itself needs the sim, so on the as-built plant `diagnose` and the status reads return the sim's
  `Unsupported` error; a plant whose only version 2 parts are RF-neutral gets the floor.
- **The page** draws the new element kinds, house cable and structures, and its popup shows what's fitted, fittings, support,
  installation, structure and batteries. Nothing edits them. Not seen in a browser: the served twin has no way to load a version 2 plant,
  and the status reads would refuse the as-built one until task 22. The inline script parses.
- **`crates/design/examples/realism_survey.rs`**, the review's uncommitted file, needed one match arm for `House` to compile. Nothing
  else in it changed.

### Library and rules

- **Every new class has at least one `Generic` entry,** 123 in all, appended to both libraries after version 1's entries, none of which
  changed. Modular roots and amps are copies of version 1's generics with a plug-in family. Pads come in 1 dB steps (0-20), forward
  equalizers in 2 dB (0-20), return pads and equalizers in 2 dB, as one generic family; the steps are typical, and `docs/reference/
  design-practice.md` §4 has the families on the market. Taps of 2 and 8 ports and the faceplate tap carry `plugin_family`, so none is a
  version 1 tap. `.750` hardline and series 11 drop are plant-and-powering.md's fits to the SCTE 15 and 74 ceilings; fiber is G.652.D's
  maximum. In-home 4- and 8-way splitters sit at SCTE 153's ceilings.
- **`Datasheet` entries are one per class a read sheet covers,** 20 new ones in `sourced.json`: the fiber parts the fiber note read,
  and a ground block, drop amp, filter, home splitter, series 11 drop, coupler, line equalizer, pad and battery. The hardline tap and the
  amp and node sheets are read and cited but not made into entries, since an entry would be mostly fields the sheets don't state.
  `docs/library.md` lists, per part, what isn't from the sheet.
- **Two schema fields came from reading sheets,** before anything was frozen: `LineEqualizerSpec.duplex`, since a feederline equalizer
  sits behind a diplexer and only qualifies on its own split, and `PlugInSpec.return_loss_db`, which a pad sheet states.
- **Generic point-of-entry filter.** The sourced filters pass 5-1002 MHz and stop from 1125 MHz, which cuts a 1218 MHz downstream. The
  fixture's `filter-generic-poe` stops above 1250 MHz so it sits harmlessly on both profiles; nothing sourced says such a part exists.
  `filter-generic-poe-1002` has the sourced shape, and the rules' MoCA home uses it. `qualifies` reads a rated band, not a stop band, so
  it can't refuse the sourced one on the high-split profile; task 23's check should.
- **The rules file.** Sourced: the premises allowance (100 ft of series 6 and a 4-way home at 1218 MHz, design-practice.md §1), tap
  families with the lowest value terminating (§2), feeder cable and line extender (§4, §5), drop size by length and the 250 ft span (§5),
  four node ports and N+3 (§6), 15 A per port and one battery string (§7), 200 ft poles and an expansion loop each pole (§8), the
  premises templates' parts (§9), 80 % supply load and three 12 V batteries (plant-and-powering.md §12), and every fiber rule but fibers
  per node (fiber-optics.md §2-§4, §8). `Generic`: the tap-port target and tolerance, which no public document states; the end-of-line
  minimum; express cable and amp; homes per node; pedestal spacing; every premises share; part prices; coax construction per foot; and
  fibers per node, which the note only infers. Prices are placeholders to rank designs, not estimates.

### The as-built fixture

`fixtures/one_street_as_built/`: the same street, 28 elements and 27 edges per plant, on both profiles, with a fiber plant and a region.
The root feeds a power inserter on `.750` express (a splice at 300 ft on the second span), a two-output bridger, and from its first
output taps of 4 ports (23 dB, a conditioner), 8 ports (17 dB, ports 3-7 terminated, 8 open), a line extender, a line equalizer and a
2-port 4 dB terminating tap. From the bridger's second output, a 12 dB coupler feeds a splitter (one leg capped) to a 20 dB tap, and a
buried span in conduit to a 17 dB tap in a pedestal. Seven homes: a bare D3.0 modem; a ground block, 2-way splitter, modem and set-top on
house cable; a drop amp; a filter on the ground block; a 300 ft series 11 drop; a buried drop; and a second bare D3.0 modem. Poles every
200 ft, strand on every aerial span, a supply with three batteries in a cabinet feeding the inserter. Every element `Built`, both plants
`Synthetic`. The fiber plant has a hub with a CMTS port, a 1310 nm forward transmitter (through a 1×8 coupler, and the receiver rated for an HFC
link, since task 22 §16: the generic parts as first fitted overloaded both receivers) and a return receiver for low-split, a core, switch,
optic and timing source for high-split, a patch panel, jumpers, and per node two cables joined in one closure, with a link each way.
Both node ends are at one site, so the closure feeds two drop cables: three outside cables, two on each node's links. Hub jumpers are
recorded `Underground`, as `Installation` has no indoor case.

**Parts list, counted by hand** (`crates/schema/tests/as_built.rs` asserts it). Low-split; high-split swaps the root and its optics.

| Part | Low | High | | Part | Both, units |
|---|---|---|---|---|---|
| Root, modular | 1 node | 1 RPD | | `pad-generic-0db` / `-2db` / `-4db` | 3 / 2 / 1 |
| `frx-generic`, `rtx-generic-1310` | 1, 1 | 0 | | `eq-generic-0db` / `-4db` / `-10db` | 1 / 1 / 1 |
| `sfp-generic-10g-lr` | 0 | 1 | | `rpad-generic-0db` / `-2db` | 1 / 2 |
| Parts-list lines | 48 | 47 | | `req-generic-0db` / `-2db`, `cond-generic-2db` | 1 / 1, 1 |

| Coax, units | Both | | Premises, units | Both | | Structures and power, units | Both |
|---|---|---|---|---|---|---|---|
| Bridger, line extender | 1, 1 | | `modem-generic-d30` / `-d31` | 2 / 5 | | Poles | 16 |
| Taps: 23 dB 4-port, 17 dB 8-port, 4 dB 2-port terminating, 20 dB and 17 dB 4-port | 1 each | | Ground blocks | 3 | | Pedestal, cabinet | 1, 1 |
| Line equalizer, 12 dB coupler, 2-way splitter, power inserter | 1 each | | Home splitter, set-top, drop amp, filter | 1 each | | Supply | 1 |
| Terminators: 2 elements and 15 port states | 17 | | `conn-generic-f`: 7 drops and 6 house cables, both ends | 26 | | Batteries | 3 |
| `.750` pins, `.750` splice, `.500` pins | 4, 1, 18 | | | | | | |

| By the foot | Runs | Feet | | Fiber plant | Count |
|---|---|---|---|---|---|
| `cable-generic-750` | 2 | 1,200 | | Hub parts: CMTS, transmitter, 1×8 coupler, receiver, core, switch, optic, timing source, panel | 1 each |
| `cable-500-p3` | 9 | 1,900 | | Closure | 1 |
| `drop-rg6`: 6 drops of 585 ft and 6 house cables of 131 ft | 12 | 716 | | Poles | 4 |
| `drop-generic-rg11` | 1 | 300 | | SC/APC mated pairs: 3 per link, 4 links, and 2 at the coupler | 14 |
| `strand-generic`, coax | 10 | 2,850 | | Splices | 4 |
| `conduit-generic` | 1 | 250 | | Jumpers 3 × 15 ft simplex, 1 × 15 ft duplex; 48-fiber 1,700 ft; 12-fiber 2 × 1,300 ft | |
| | | | | Strand, fiber: 3 runs, 4,300 ft | 17 lines |

### For task 22's spec

- **Where optics live.** A node's analog optics and an RPD's transceivers are `fitted` on the plant's root; the fiber plant reaches the
  node at its `NodeEnd`, whose part is the node's own. `sim::optics` needs the fiber plant, the node end and the root's fitted optics
  together. The spec's "each node's fitted optics" reads right; it should say the receiving end of a forward link is the root's
  `forward_rx` or transceiver, not a fiber element.
- **A link's direction and a bidirectional optic.** The fixture uses a `Forward` and a `Return` link on two strands for the RPD, each
  ending at the same hub `Transceiver`. A single-fiber bidi optic would be one `Both` link; `TransceiverSpec.rx_wavelength_nm` is there for
  it, and no source read gives one.
- **`FiberFaultSet`'s `EventLoss { location }`** needs a fiber location type, which `schema::fiber` doesn't have yet: a point on a cable
  (`cable`, `pos_ft`), or an element. Splices have no part, so "a bad splice" is located by element and the two strands it joins.
- **Tolerance.** `Tolerance` now holds return loss, noise figure and a modem's ceiling as well as losses, gain and loop resistance. A
  splice has no part to state one on; its allowance and spread belong in the rules (`FiberRules.splice_loss_db`).
- **The power inserter's loss is 0 dB today** (`sim::passive`), and the as-built fixture puts one in the express line. Task 22 should give
  it the loss its spec states, or state that 0 is the model.
- **Stop bands and qualification.** A filter that stops inside the band plan qualifies today. §13 or `check_physics` should flag it.
- **The fixture's homes hold every case §13 names** except two devices behind one splitter where both report: `stb-2` is a `Cpe`, which
  reports nothing, so the §13 oracle "two devices see each other through its isolation" can price the echo but only one side observes it.

### Closing fixes, after review

- **`work_order` is version 4:** an identity migration, `policy_table.v3.json` and `work_order.v3.json` frozen, the shipped policy
  read through `migrate_policy`, and stored work orders migrated in the server's `load_into`.
- **Fields added for task 22, all optional** (component schema, still version 2): `ActiveSpec.gain_table` (§15's gain by frequency, since
  `max_gain_db` and `gain_flatness_db` don't give a shape); `PowerInserterSpec.insertion_loss`, `return_loss_db`, `port_i_rated_a`
  (§14, `check_physics`); `SpliceClosureSpec.splice_loss_db` and `PatchPanelSpec.splice_loss_db`, with their spread in the part's
  `tolerance.loss_db` (§16); `DropAmpSpec.return_nf_db` (§13); `FilterSpec.transition_mhz` (§13); `CableSpec.r_loop_k_temp`, so loop
  resistance only moves with temperature where a part says how (§19, rule 2); `OpticalRxSpec.agc_range_db` (§16). The sim refuses
  `gain_table`, an inserter's loss or return loss and `r_loop_k_temp` until task 22 prices them. New generic `pi-generic-1218` states its
  loss; closures and panels allow 0.05 dB a splice [fiber-optics.md §3, SCTE 270's worked budget]; `.750` and series 11 take 0.004 per °C,
  marked `Generic` (plant-and-powering.md's Implications call it a standard metal value no source there gives).
- **Fixture:** home 8 on `tap-2`'s third port has a ground block, a 2-way splitter and two reporting D3.1 modems; the inserter is
  `pi-generic-1218`. Per plant now 32 elements and 31 edges, and the table above changes to: terminators 16 (15 → 14 port states),
  ground blocks 4, home splitters 2, `modem-generic-d31` 7, `conn-generic-f` 34, `drop-rg6` 16 runs and 867 ft, `pi-generic-1218` in
  place of `pi-generic`. Parts-list lines stay 48 and 47.
- **ATX-ANW1544** carries no "Confidential and Proprietary" footer in its text, where the four ATX documents not cited do; it stays.
- **Persistence was not exercised.** No Postgres answers on 5432 and the variable isn't set. The only database running belongs to another
  project, so it wasn't used.
- **Fix 2, finished: §17 to §19 reread line by line.** One more field: `ActiveSpec.ref_composite_dbmv`, the total composite power a part's
  distortion and CIN figures are quoted at, since §17 has CIN follow the composite power the plan loads and §4.4 said only that the
  datasheet states a loading, with no field for it. The sim refuses it until §17 prices it. Nothing else is missing: source MER and the
  receiver ceiling are on `RpdSpec` and `CmtsSpec` (and `NodeSpec`), NF and CIN tables on `ActiveSpec`, `coupling_db` on the ground block,
  loop resistance against temperature on `CableSpec`. Ambient sources, solar and wind load, and a connector fault's couplings are the
  environment and fault schemas' own next versions, task 22's. **Version 2 closes here.**
