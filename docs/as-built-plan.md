# As-built plan: every component in the plant, designed the way it's built

Written 2026-11-10, from [`docs/realism-review.md`](realism-review.md). It answers two requirements: the design process should behave as
closely as possible to what would be built in reality, and every component, fiber included, must be present in the plant.

The work is four tasks, each with its own spec. This file holds what's true across all four: the rules, the order, the decisions already
taken, and which task covers which component. Where a spec and this file disagree, stop and resolve it.

Everything these tasks produce is synthetic (invariant 11). A design that passes says the designer follows its rules and the sim follows its
physics. It doesn't say the result matches any operator's plant.

## Sequence

| Task | Spec | Builds | Gate before the next |
|---|---|---|---|
| 21 | [21-as-built-schemas.md](tasks/21-as-built-schemas.md) | Two reference notes. Component and plant schemas at version 2, a fiber plant schema, design rules as data. A second hand-built fixture with every component. Every reader compiles and reads them | Its acceptance, and the pinned digests unmoved |
| 22 | [22-as-built-physics.md](tasks/22-as-built-physics.md) | Physics for every new part, written into `docs/physics.md` with a worked oracle first. New observations, fiber faults with a location, a region-level optical pass | Every oracle, the §12 validation suite, and the latency budget |
| 23 | [23-as-built-designer.md](tasks/23-as-built-designer.md) | A designer that works from design rules: hub, fiber, node sites, express and feeder, taps to a port level, drops and premises, alignment, powering, fiber design, a priced objective, and a legacy-then-upgrade mode | Its structural bars, fixed in the spec before any design is run |
| 24 | [24-as-built-diagnosis-and-selection.md](tasks/24-as-built-diagnosis-and-selection.md) | Diagnosis candidates and features for the new parts, regenerated rows on the new designs, retrained models, selection re-run under task 15's and 17's rules | The report, whichever way it goes |

One task at a time, in order (`CLAUDE.md`). Specs 22 to 24 were written ahead of the build at the user's request. Each is reread against what
the task before it actually produced, and amended under its own "Decisions made during the task", before it's started.

## Rules across all four

1. **Version 2 is additive.** Every version 1 document is a valid version 2 document once its version string is bumped. New fields are
   optional and are left out of the JSON when absent. New kinds are new enum variants. `migrate` bumps the string, as `schema::fault::migrate`
   does, and a frozen `*.v1.json` golden for each schema stays readable forever (invariant 3). Version 2 closes when task 21 is accepted.
   A change to a closed schema, however small, is the next version with its own migration and frozen golden.
2. **Absent means "as before".** A plant with none of the new parts simulates to the same bytes as it does today. Every new mechanism is
   switched on by the presence of the part or field that causes it, never by a flag (invariant 5).
3. **Pinned digests prove it.** Task 21 starts by pinning, on the untouched tree, a digest of the sim's output on both `one_street` plants
   and of every generated design. They ignore `schema_version` strings and nothing else. A task that moves one has a bug.
4. **`one_street` doesn't change.** Not its files beyond the version string, not its level budget, not one oracle in `docs/physics.md` §1 to
   §12. New oracles are computed on the new fixture, `fixtures/one_street_as_built/`.
5. **Physics first.** A new mechanism is an equation and a hand-worked example in `docs/physics.md` before it's code, and the example is the
   test (`CLAUDE.md`, testing rules).
6. **Sources first.** A fact about plant, optics or practice comes from a reference note that cites a public document. What can't be sourced
   is `Generic` in the library, `approx` in `docs/physics.md`, and listed under the note's Gaps. No operator's internal specification is
   used or asked for, and the notes name no operator as the subject.
7. **The old designer stays.** `design_region` as it is today is kept, frozen, as the version 1 path, with its digest held. The shipped models
   were trained on its layouts, and they stay valid for those. The new designer is a second path, and task 24 trains on it.
8. **Fiber is plant.** Hubs, cables, closures, optics and links are stored, carry a record status, and can be faulted at a location. This
   reverses task 14's decision that the fiber route is derived and never stored. The router stays, as the fiber designer's first step.
9. **The sim owns what it emits** (invariant 2). An observation field is added in task 22, when the sim can compute it, never in task 21.
10. **Speed is part of the physics task.** Premises roughly triple the elements below a tap. Task 22 holds a measured budget on
    `crates/sim/examples/latency.rs`.

## Decisions taken

The review ended with five open decisions. These are the defaults this plan takes. Any of them can be overturned before task 21's schema
lands, and the first is expensive to change after that.

| Decision | Taken | Why |
|---|---|---|
| Fiber stored or derived | Stored, in a new `schema::fiber` | Required: fiber must be present, with records and faults |
| Generated designs moving | The version 1 designer is frozen and kept. Version 2 designs are new layouts, and task 24 retrains | Keeps every shipped model's training layouts and scores meaningful |
| Order against tasks 18 and 19 | This program goes first | The user asked for it to start. Task 19's fiber events take their location from `schema::fiber` when 19 is built, and whichever of 19 and 22 lands second takes the next fault schema version |
| Structures | In: poles, strand, pedestals, vaults, conduit and cabinets, as records and parts-list lines with no RF | "All components" |
| Sources | Public documents only, operator-neutral | `docs/realism-review.md`, tags |

## Where each component lands

Every row of the review's section 3. "Rules" means `schema::design_rules`.

| Component | Schema, task 21 | Physics, task 22 | Designer, task 23 |
|---|---|---|---|
| CMTS or vCore, ports, service groups | `fiber`: `Cmts`, `ServiceGroupBinding` | Source MER table, upstream receiver ceiling | One port per service group |
| Forward transmitter, return receiver, optical amp, splitter, mux | `fiber` elements and component classes | Link budget, analog CNR, clipping, return curve | Chosen per link from rules |
| CIN switch, pluggable optics, timing source | `fiber` elements and component classes | Digital link margin, latency | One switch port and optic pair per RPD |
| Fiber cable | `fiber`: `FiberCable` with count, type, installation, path | Loss by wavelength and length, cut at a position | Sheaths sized along the route |
| Splice closures, splices, connectors, panels | `fiber` elements, and hops on a link | Loss per event, reflection | Closures at branches and reel ends |
| Node optics and RPD optic | `Fitted` modules on the root | Received power, per-port output | Fitted with the node |
| Per-port pads and equalizers | `PlugIn` class, `Fitted` on an active | Alignment, residual | Alignment pass |
| Multi-output and single-output amps | `outputs` on `ActiveSpec` | Per-port level, return combining loss | Type by role, branch at an output |
| Pads, equalizers, cable simulators | `PlugIn` class | Loss shapes | Alignment pass |
| Taps of 2, 4 and 8 ports | Library entries | As today | Size by homes at the location |
| Terminating taps, tap plug-ins, in-line equalizers | `terminating`, `PlugIn`, `LineEqualizer` | Tilt at the port, end-of-line reflection | End of line, and where tilt needs it |
| Splitters, directional couplers | As today, more values | As today | Couplers for unequal branches |
| Power inserter | As today | Port current limit | Every supply feeds one |
| Tap-port terminators, open ports | `Fitted` on a tap | Open-port reflection | A state on every port |
| Hardline by size | Library entries | As today | Size by role |
| Connectors, splices, adapters | `Connector` class, `fittings` on an edge | Loss and a reflector each | At every cable end |
| Supply, batteries, cabinet | `Battery` class, `batteries` on a supply, `Structure` | Loop resistance with temperature. Standby is task 19's | Sited and sized |
| Drop by size, aerial or buried | Library entries, `installation` on a drop | Buried drops follow ground temperature | Size by length |
| Ground block, in-home splitters and cable, drop amp, filters | New element kinds and classes, `House` edge | Premises path, in-home echoes, coupling | Premises drawn from rules |
| More than one device per home | `Cpe` kind | Shares the splitter | Drawn from rules |
| Poles, strand, pedestals, vaults, conduit | `Structure`, `support` on an edge | None | Placed along routes |

## Risks

| Risk | Mitigation |
|---|---|
| Practice can't be sourced from public documents | The notes say what's unsourced, the values are `Generic`, and task 23's bars are structural, not numbers from memory |
| The schema is wrong in a way only the physics shows | Task 21 builds the hand-built fixture with every part, and task 22 opens by rereading the schema against the physics it has to carry |
| Premises make the sim too slow for diagnosis | Task 22's latency budget, measured before and after |
| Four large tasks drift from each other | One at a time, each spec reread and amended before it starts, digests held throughout |
| Shipped models meet plants they never trained on | From task 21 the twin answers with the no-ML floor on any plant that has version 2 parts, and says why, until task 24 |
