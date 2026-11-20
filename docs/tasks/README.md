# Task specs

One file per step of the build order in `docs/DESIGN.md`. Written one step ahead
of the build, not all up front, because later steps change as earlier ones land.

| Step | Spec | Status |
|---|---|---|
| 1 | [01-schemas.md](01-schemas.md) | done |
| 2 | [02-plant-graph.md](02-plant-graph.md) | done (fixture is in 01) |
| 3 | [03-physics-sim.md](03-physics-sim.md) | done |
| 4 | [04-faults-and-telemetry.md](04-faults-and-telemetry.md) | done |
| 5 | [05-diagnosis-no-ml.md](05-diagnosis-no-ml.md) | done |
| 6 | [06-mcp-server-and-min-ui.md](06-mcp-server-and-min-ui.md) | done |
| 7 | [07-emulator-adapter-validation.md](07-emulator-adapter-validation.md) | done |
| 8 | [08-design-and-records.md](08-design-and-records.md) | done |
| 9 | [09-component-library.md](09-component-library.md) | done |
| 10 | [10-daa-and-migration.md](10-daa-and-migration.md) | done |
| 11 | [11-work-order-lifecycle.md](11-work-order-lifecycle.md) | done |
| 12 | [12-training-and-model-selection.md](12-training-and-model-selection.md) | done, bar missed on calibration ([report](../model-selection.md)) |
| 13 | [13-reactivity-and-lifecycle-demo.md](13-reactivity-and-lifecycle-demo.md) | done |
| 14 | [14-map-design.md](14-map-design.md) | done |
| 15 | [15-held-out-maps-selection.md](15-held-out-maps-selection.md) | done, GNN won by the rule, not wired in ([report](../model-selection.md)) |
| 16 | [16-rxmer-impairment-sim.md](16-rxmer-impairment-sim.md) | done, every listed impairment passes ([gallery](../rxmer-gallery.md)) |
| 17 | [17-spectrum-model.md](17-spectrum-model.md) | done, not ready by the rule: `LowMer` F1 0.697 and fall error 2.21 dB miss ([report](../model-selection.md)) |
| 18 | [18-weight-space-editing.md](18-weight-space-editing.md) | done 2026-11-19 on task 17's model: the harness, two edits (one kept) and one read of task 15's reserve areas. Editing didn't beat training: the fall error fell 0.17 dB on both `calibrate` and the reserve areas, but healthy false alarms rose 0.59 points on the reserve areas, past noise |
| 19 | [19-damage-events.md](19-damage-events.md) | done 2026-11-19: schema version 5, physics §20, the six events in the sim, `diagnosis::outage`, the sampler, `datagen maps-events` and a gallery. On the test maps one part of three passes: names the break 76.3 % (bar 90 %, shorted cuts trip their supply), house power called a plant break 15.0 % (bar 10 %), forward told from return 100 % |
| 21 | [21-as-built-schemas.md](21-as-built-schemas.md) | done: schemas at version 2, `schema::fiber`, design rules, two notes, `one_street_as_built`. Pinned digests and `7298d7aca5f41752` hold. The sim refused version 2 parts until 22, which prices them all |
| 22 | [22-as-built-physics.md](22-as-built-physics.md) | done: `docs/physics.md` §13 to §19, `sim::optics`, `sim::align`, fiber faults, observation, environment, profile and fault versions, `get_optical_budget`. Pins hold. Latency: both bars hold: +1.2 % at 8 modems, full homes 2.19× bare |
| 23 | [23-as-built-designer.md](23-as-built-designer.md) | done: `design::v2` on rules as data, rules v2 and region v3, `get_design_cost`, `list_design_rules`, fiber plant on the page. Every bar met on every built node; built over the ceiling, both extracts hold too, rural misses two checks. VOGEL2019's run reproduced to its fifth tap. The upgraded estate meets its bar. Pin `f22e294c97482f3e`, old pins hold |
| 24 | [24-as-built-diagnosis-and-selection.md](24-as-built-diagnosis-and-selection.md) | selection read, closing: by task 15's rule the GBDT stays (the GNN misses "when the sim is wrong" by 0.1 point), and it's wired in for plant with version 2 parts. `DESIGN.md`'s bar missed on this plant (top 3 74.0 %, ECE 10.5 %), beating the floor by 16.9. Fiber bars met; inside the home missed (16 % against 80 %). The spectrum model passes all ten parts of task 17's rule. Two floor tests stay ignored with their numbers. Report in `docs/model-selection.md` |
| 25 | [25-design-any-map.md](25-design-any-map.md) | done: on 30 real towns (62,181 homes) and twelve fresh areas, no issue on any built node, both profiles, ceiling on or off; no errors; unserved 422 → 119. Apartment blocks fed a group at a time, attenuators at the modem, cable-matched equalizers chosen by a sweep, cut-off pieces joined, faceplate cable simulators, and the sim's last word on each flagged home. Page defaults to `generic` rules with homes over the ceiling built. V2 pin moved; v1 pins hold |
| 26 | [26-operations-sim.md](26-operations-sim.md) | built 2026-11-19: the crate `ops`, `schema::ops` v1, sourced rates, five MCP tools and an Operate tab; two worlds on one seed, from calls and with the twin. On the demo day the twin rolled 4 trucks to 6 and cut customer-minutes out 14 %, in this world and under these rates. Acceptance not met: 11 nodes for a day, not 20 to 40 for a week, and the map layers not yet seen drawn |
| 27 | [27-scqam-equalizer.md](27-scqam-equalizer.md) | done 2026-11-19, ahead of 26's hazards: an SC-QAM modem's equalizer takes out an echo within 1.5 µs by 30 dB, so an open cut no longer takes 3.0 modems off the tap above; the sim's two `one_street` pins moved |
| 28 | [28-operations-game-screen.md](28-operations-game-screen.md) | built 2026-11-19: the Operate tab opens a full-window operations screen, both worlds side by side on synced maps, trucks that drive their routes, calls that ring, night and weather from the world's own environment, a disaster toolbar, the score as a race, the board, the inspector, and scenarios as data (`fixtures/ops/scenarios.json`); `ops_layout`, `ops_scenarios`, and the pair on two threads (a world hour in 2.5 s, was 4.3). Driven in headless Chromium with software WebGL, light and dark, 760 px and with no WebGL. The twin won the stormy Monday on 7 of 8 seed or timing variants of the suburban region, and on the urban grid rolled nearly three times the trucks, 8 of its 10 empty rolls sent into power cuts |
| 29 | [29-one-app.md](29-one-app.md) | built 2026-11-20: the whole page laid out like the operations screen (map as stage, one top bar with the modes, tool strip, side panel, status line), Operate a mode not a dialog, and Design as three numbered steps from a place to a running town |

Steps 21 to 24 are one program, from [`docs/realism-review.md`](../realism-review.md), and [`docs/as-built-plan.md`](../as-built-plan.md)
holds what's true across them. Their specs were all written up front, at the user's request, which is the exception to the rule above. So
each is reread against what the step before it produced, and amended, before it's started.

Template for a new spec: Goal · Deliverables · Tests · Out of scope · Acceptance · Deferred.
