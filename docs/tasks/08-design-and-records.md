# Task 08 — OSM ingest, layouts, design optimizer, parts lists and plant records

**Build-order step:** 8
**Depends on:** Tasks 02, 03
**Produces:** `schema::region`, a new schema with its own version; `crates/design`; plant-record tools on the MCP server.
**Load:** `CLAUDE.md`; `docs/DESIGN.md` "Physics simulation", "Reactivity"; `docs/physics.md` §3, §6.

## Goal

Streets and homes in, a valid design out, as a set of elements marked `Designed` with a parts list derived from them. Classical,
deterministic, no AI. It's both a product function and the source of layout diversity for steps 10 to 13.

## Deliverables

1. **`schema::region`, version 1.** `StreetMap` (streets, homes, headend, and where it came from), `DesignControls` (homes per node, cascade limit,
   take rate, pins) and `Region` (its nodes). This resolves two things Tasks 01 and 02 deferred, homes passed and a region over many plants,
   without touching `Plant`'s schema. Round-trip and golden tests like the others.
2. **`layout`.** A seeded generator by family: `UrbanGrid`, `Suburban`, `Rural`. All synthetic, and labeled so.
3. **`osm`.** Reads Overpass JSON into a `StreetMap`. Tested on a small hand-written extract.
4. **`partition`.** Splits homes into nodes at the homes-per-node target and `max_homes_passed`.
5. **`route`.** A Steiner-tree heuristic over the street graph (Takahashi–Matsuyama), rooted at the node.
6. **`place`.** Walks the tree: clusters homes into taps within drop reach, picks tap values from the level budget, inserts amps
   when the budget runs out. Each decision uses the sim's own cable and passive functions.
7. **`design_region`.** Runs all of it and checks with `check_design` and `check_physics` at −20 and 45 °C. It splits a node that
   can't meet its cascade limit, and adds supplies where voltage falls short. The result carries whatever issues remain.
8. **Plant records.** Elements are `Designed`, the parts list derives from them, and `mark_built` moves them to `Built`.

## Tests

- Every family at several seeds: no topology or physics issue on any node, every home on a tap port within drop reach, homes per
  node under the target, cascade under the limit.
- Determinism: same input, byte-identical plants.
- The parts list adds up to the plant by hand on a small map.
- A tighter cascade limit or homes target produces more nodes.
- A pinned node stays where it was pinned.
- The sim runs on a designed plant and every modem comes up operational.
- The diagnoser finds a fault on a designed plant, not only on the fixture.
- `osm` reads its fixture and skips what isn't a street or a home.
- Region schema round trip and golden.

## Out of scope

Aerial or underground choice beyond what the map says. WASM for interactive what-ifs: this runs server-side, and step 13 measures
whether it's fast enough. Real parts: step 9.

## Acceptance

`cargo test --workspace` green, clippy clean.

## Decisions made during the task

- **Slack stands in for a pad.** Close to an active, a tap can be too hot at the top of the band, and tilt too steep, even at the highest
  value. The real fix is an input pad or an equalizer, and the schema has no such part. Up to 600 ft of extra hardline on that span
  takes its place, and it's in the parts list.
- **A return amp's thermal control holds unity against temperature** [SCTE 283 §7.2.1.4.11]. The walk first counted temperature above the first
  amp as excess, and that piled up along a cascade. Only faults and switches count now. Nothing at 20 °C changes.
- **`check_physics` holds a D3.0 modem to what it transmits on,** its SC-QAM channels, not the whole return band.
- **More generic taps:** 29, 26, 20, 14, 11 and 8 dB, with insertion loss interpolated from §2.1 and isolation at value + 15 dB. All `Generic`.
- **The map's demo region isn't chosen,** and it's an open question in `docs/DESIGN.md`. `osm` reads any Overpass extract, tested on a hand-written one. Generated
  layouts stand in until someone picks a town.
- **Corrected at step 13: homes were being lost.** Generated streets crossed without a shared point, so the street graph came apart into pieces, and a node
  only served the piece its root was on. The rest counted as passed and got no drop: 1,158 modems for 4,108 homes at full take. Nothing reported it, because
  `unserved` only counted homes too far from any street. The generator now joins streets where they meet (`join_streets`), a home a node can't reach goes to a
  node of its own, and `every_served_home_gets_a_modem_at_full_take` holds it. Designs went from about 30 homes per node to about 150, at the same 0 issues. Real OSM ways
  are taken as drawn: shared nodes join, and a bridge stays apart.
