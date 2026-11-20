# Task 13 — Reactivity, the workbench, the lifecycle demo and the full UI

**Build-order step:** 13, the end of v1
**Depends on:** Tasks 06, 08, 10, 11, 12
**Produces:** `twin::cache`, persistence on Postgres, the full map page, the lifecycle demo, `Dockerfile` and `docker-compose.yml`.
**Load:** `CLAUDE.md`, invariants 1, 7, 8, 11; `docs/DESIGN.md` "Reactivity" and "Impairment workbench".

## Goal

The demo surface, on a proven core: design a region, mark it built, watch it run, inject a fault, diagnose it, issue the work order, prove the fix from telemetry, then migrate a node. All through MCP, on a region of 20–80 nodes.

## Deliverables

1. **Incremental recompute.** Per service group, keyed by a hash of that group's plant in canonical order, profile, the library entries it actually uses, environment, faults and seed. Changing one node recomputes one node. Reordering a file's lists recomputes nothing. The cache holds only derivations, so dropping it changes no answer (invariant 1).
2. **Region tools.** `get_region_status` across every node, computed in parallel, and `get_recompute_stats`.
3. **Model beside the floor.** `diagnose` runs the no-ML diagnoser, and when a model is loaded, the hybrid too, and returns both with their answers side by side. The work order comes from the hybrid's verified answer when it has one.
4. **`localize_ingress`.** The switch test as a tool (§7.3).
5. **Workbench.** Every fault class as parameters, on any element clicked on the map, with what-if and inject.
6. **The full page.** Design, monitor, workbench, diagnose and migrate panels over design, fault and telemetry layers, and a per-subcarrier plot. It's still only an MCP client, and the banner stays.
7. **Persistence.** Inputs only, as JSONB in Postgres: plants, faults, work orders, evidence, maps, regions, environment. Derived values never go in (invariant 1). No cloud SDK (invariant 7).
8. **Compose.** Postgres and the server, from one command.
9. **The demo.** `crates/mcp-server/tests/demo.rs` drives the whole lifecycle through `tools/call` on a region of at least 20 nodes, and records how long each step took.

## Tests

- Cache: one node changed, one recomputed. List order changed, none. Environment changed, all. A part only one node uses, changed, recomputes only that node. Cached answers equal fresh ones, to the byte.
- The demo above.
- Postgres: state survives a restart, against a real server, when `HFC_TEST_DATABASE_URL` is set.
- By hand: the page in a browser, and `docker compose up`.

## Out of scope

WASM for interactive what-ifs. The milestone offered it, and it's only needed if a server round trip is too slow, which the demo's timings will say. Auth, multi-tenancy, sharding.

## Acceptance

`cargo test --workspace` green, clippy clean, and `docs/DESIGN.md`'s v1 list reviewed line by line, with each item marked for what's true of it.

## Decisions made during the task

- **"Affected" means off the node's own normal.** A healthy plant under domain randomization has modems reading past a threshold, as a real one has modems off design. Reading those as affected meant a repair could never read clean, so `Twin::who` subtracts a cached baseline: the same node with no faults on a mild dry day. A field system would have history here. Weather still shows, since the baseline doesn't move with it.
- **The diagnoser scores hypotheses on every core.** A full pass is about 1 ms per modem, and corrected designs have 50–90 modems, so a diagnosis took 28 s on one thread. It takes about 5 s on 16. `DiagnoserConfig::threads` doesn't change the answer, and a test holds that. No WASM: `what_if` is one simulation, well under a second.
- **No basemap.** The region's generated, and real streets under it would pass it off as a real place (invariant 11). MapLibre itself still loads from unpkg, so the page needs internet. Task 14 adds one only under an OpenStreetMap map, whose streets are real.
- **Migration capacity is the typical modem's, not a sum.** Channels are shared, so adding modems' rates gave 41 Gbit/s on one node.
- **A migrated layout can fail, and the plan says so.** Spacing designed for low-split runs short at 1.2 GHz and 204 MHz: 29–36 issues per node on the demo region, mostly transmit limit and amp input. That's what a plant-side re-space would have to fix, and the plan reports it by kind. `apply_migration` applies anyway, since the twin models the outcome and doesn't forbid it. Moving amps to fix it is a later step.
- **A quiet node needs to earn a work order.** On healthy designed nodes the floor names a fault up to 0.99 and the model's top answer can too, at a calibrated 2–5 %, and each of those raised an order. An order now needs someone reading worse than the node's own normal, or a calibrated confidence of at least even odds. The floor's posterior doesn't count, since it isn't a confidence. The even-odds line is a judgement, not a measurement. A real fault that leaves nobody affected and scores low gets no order.
- **Persistence saves after each tool that writes,** and `every_tool_is_classified` fails if a new tool isn't declared as reading or writing.

