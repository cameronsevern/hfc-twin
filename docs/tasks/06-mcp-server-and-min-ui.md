# Task 06 — MCP server, then a minimal map UI

**Build-order step:** 6, the end of v0
**Depends on:** Task 05
**Produces:** `crates/twin`, the operations; `crates/mcp-server`, the only API over them, with stdio and HTTP
transports; and a minimal map page that is itself an MCP client.
**Load:** `CLAUDE.md`, invariant 8 and 10; `docs/DESIGN.md` "What AI does and does not do".

## Goal

Hand-built plant in, work order out, queried through MCP and shown on a map. Anything the page does, any MCP client
can do, because the page does nothing but call tools.

## Deliverables

1. **`crates/twin`.** `Twin` holds library, profiles, policy, each node's plant, its injected faults and its work
   orders, the environment sample and a seed. Observations aren't stored: each call derives them (invariant 1). Every
   result says it's `Synthetic` (invariant 11).
2. **Tools.** `list_service_groups`, `get_plant`, `get_parts_list`, `get_level_budget`, `get_design_issues`,
   `set_environment`, `inject_fault`, `list_faults`, `clear_faults`, `get_modem_telemetry`, `get_status_map`,
   `what_if`, `diagnose`, `list_work_orders`, `get_modem_identity`. Each has a JSON Schema from `schemars`.
3. **Transports.** JSON-RPC 2.0 with `initialize`, `ping`, `tools/list` and `tools/call`, newline-delimited over stdio
   and as POST `/mcp` over HTTP. `GET /` serves the page and nothing else is routed. There's no REST.
4. **Identity gate.** The twin is keyed by plant location. `get_modem_identity` answers only when the server was
   started with `--allow-identity`, and even then gives a pseudonym, since raw addresses never enter the twin
   (invariant 10).
5. **The page.** MapLibre over the plant's GeoJSON. Modems colored from `get_status_map`. A fault form, a diagnose
   button, ranked hypotheses and a work-order pin. A permanent banner says the data is simulated.

## Tests

- JSON-RPC: `initialize` returns capabilities, `tools/list` has every tool with an object schema, unknown methods and
  bad params give the right errors, and notifications get no reply.
- The v0 loop through `tools/call` only, on both profiles: inject a connector fault, read status, diagnose, get a work
  order on the right span, clear it, and status goes clean.
- `what_if` leaves state untouched.
- The gate refuses by default.
- The same calls give byte-identical results (invariant 4).
- By hand: the page in a browser, recorded under "Decisions".

## Out of scope

Auth beyond the identity gate (v3). Persistence: Postgres enters at step 13. The full map UI is step 13.

## Acceptance

`cargo test --workspace` green, clippy clean, the page works in a browser, and `docs/DESIGN.md`'s v0 bar is met.

## Decisions made during the task

- **Hand-written JSON-RPC rather than an SDK crate.** MCP's tool surface is four methods. `axum` carries HTTP and nothing
  else in the server knows about it.
- **The page was driven in a real browser pane through MCP alone:** connector on span-3 at 6 dB, then 89 simulations,
  then `ConnectorReflection span-3` at the top and a work order pinned near tap-2. The first screenshot confirmed the
  layout and the OSM base map. Later readings were from the DOM, because the pane was hidden and doesn't render then,
  so the plant lines and pin on the map were counted (96 plant features, 5 layers), not seen.
- **Both fixtures name one street,** so `load_one_street` renames their service groups in memory. The files are untouched.

## v0 status

Every item in `docs/DESIGN.md`'s v0 list holds on the synthetic fixture: one street to a work order, queried through MCP
and shown on a map, on both profiles with both modem generations on one tap, with every `docs/physics.md` oracle passing,
byte-identical reruns, and at least 90 % top component on connector faults.
