# Task 14 — Design on a real map

**Build-order step:** after 13; closes the open "OSM ingest for a chosen region" line of v1 without choosing the region for the user.
**Depends on:** Tasks 08, 13
**Produces:** a hardened `design::osm`, a fiber route from the headend, `import_osm_area`, `import_osm`, `get_map`, `list_maps`, `set_headend` and `discard_map` tools, re-solving in `design_region`, and the map controls in the page.
**Load:** `CLAUDE.md`; `docs/DESIGN.md` "Design optimizer", "Reactivity"; `docs/tasks/08-design-and-records.md`.

## Goal

Draw an area on a real basemap, get its OpenStreetMap streets and homes, design it, then move the headend and pin nodes on the map and have the design re-solve. Everything the page does goes through MCP (invariant 8), so another client can do it too.

The streets are real and the plant is invented. Every result says so (invariant 11), and nothing here implies any operator's network is shown.

## Deliverables

1. **`design::osm` that survives real data.** `read_overpass` returns the map and an `OsmReport` of what it left out.
   - Streets: road values a cable would follow, including link roads, without driveways and the like. Parking aisles stay, since a route only follows one if a home needs it.
   - Homes: residential buildings and `yes`, without sheds, garages, shops. `building:flats` gives that many homes. An address node inside a building footprint isn't a second home. An apartment building with no unit count is one home, and the report counts how many.
   - Crossings: pieces of street within 200 ft are joined, nearest first, by a street with id `crossing-N`. They're stored in the map as inputs, and the report counts them.
2. **Fiber route.** `design::fiber::fiber_route(map, design)`: a Steiner tree along streets from the headend to every node, its length, its lines and the nodes it can't reach. Derived, never stored. The sim doesn't model fiber, so the headend changes this route and nothing in the RF design, and the page says so.
3. **Tools.**
   - `import_osm_area`: a lat/lon box, at most 40 km². The server fetches it from Overpass outside the twin's lock, then imports. Retries on 429 and 5xx within a 150 s deadline, then says plainly what failed. `--overpass-url` overrides the source.
   - `import_osm`: the same import from Overpass JSON a client already has, for a client with its own extract or no route out.
   - `get_map`, `list_maps`: streets and homes as GeoJSON, source and label, headend, last controls, every node's site, spans and whether it's pinned, the fiber route.
   - `set_headend`: moves it on the map, which is an input, and returns the new fiber route.
   - `design_region` re-solves: it replaces the map's nodes, and remembers the profile and controls so a reload keeps the pins.
   - `discard_map`: removes a map, its region and its nodes.
4. **Protection for records.** Re-solving or discarding is refused while any of the map's nodes has a `Built` element, a fault, a work order past `Proposed` or a migration. A re-solve can't silently destroy what someone recorded. `discard_map` takes `force` for someone who means it, so a map is never stuck.
5. **Persistence.** Removed nodes leave the store too, or they'd come back on restart. Design inputs and reports persist with the maps.
6. **The page.**
   - Design tab: find a place, draw a box, import, design. What it found and left out is on screen, including when a place has too few mapped homes to design from.
   - On the map: streets, homes, every node's spans, the fiber route, draggable headend, draggable nodes that pin where dropped, add a node by clicking, clear pins.
   - A basemap only while drawing or on an OpenStreetMap map, with attribution. Generated maps and the fixture keep the plain ground (invariant 11).
7. **`examples/osm_survey.rs`**, `examples/design_digest.rs`.

## Tests

- `osm`: on a hand-written extract, each rule above: driveways out, aisles in, links in, shed out, flats expand, address in a footprint dropped and outside kept, a crossing added within 200 ft and not beyond, report counts.
- A real extract under `fixtures/osm/` (ODbL, attributed): no invalid plant, every node's checks reported, and a count of homes served held by test.
- Fiber: hand-worked length on a small map, a headend moved along a street changes it, and a node on a disconnected piece is reported unreached.
- Amp placement: a long run after a tap that leaves the signal below threshold gets an amp rather than losing 200 dB, and an amp goes in ahead of a split.
- Generated designs are byte-identical to before (`design_digest`): `7298d7aca5f41752`. The model's training layouts must not move.
- Tools: import from text, re-solve replaces nodes with no leftovers, pins hold and survive `get_map`, refused with a fault, with a `Built` element and after a migration, `set_headend` changes fiber and no plant, `discard_map` removes everything, area over the limit refused, a stub Overpass server exercises fetch and its failure.
- Determinism: same extract, same controls, byte-identical design.

## Out of scope

Drawing plant by hand. Moving individual amps or taps. Aerial or underground by street: one choice per import. Parcel or address-list upload. Fiber in the sim: no optical budget, no latency. Self-hosted tiles or a configurable basemap.

## Acceptance

`cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace` green. In the browser, on a real place: draw, import, design, drag a node, drag the headend, reload and find it all there.

## Deferred

- **Apartment unit counts.** Most apartment buildings have no `building:flats`, so MDUs undercount, and the import says how many.
- **Sparse OSM.** Many US rural places have few mapped buildings. One test area had 3 homes on 99 streets. The page says so, and doesn't invent homes. Parcel and address-list upload is the fix (`docs/DESIGN.md`, v3).
- **Spans follow streets only in length.** A plant edge has two ends and no path, and a span can run around a corner, so the map draws it straight and some cut across a block. Its length and the fiber route do follow streets. A path on the edge is a schema change, so it waits for a version.
- **Very small nodes.** A piece of street more than 200 ft from any other gets its own node, however few homes it has. They're in the table, and a person can see them.
- **Basemap for a customer deployment.** Tiles come from OpenFreeMap and place search from Nominatim in the browser, which a locked-down network won't allow.

## Decisions made during the task

- **A home more than `max_drop_ft` from a street is now unserved,** not a `DropTooLong` issue. It was snapped within three times that, which real maps reach and generated ones don't. A drop can still run slightly over when a tap is shared along the street, and that's reported as an issue. Generated designs don't change.
- **Real roads exposed an amp placement bug.** After a tap that left the signal below an amp's threshold, the run to the next branch was laid with no amp at all, whatever its length, reading −169 dBmV at the next one. Generated streets are too short to show it.
- **Tiles and place search run in the browser.** They aren't twin operations and carry nothing about it, so they stay out of MCP. A client bringing its own map still has `import_osm_area` and `import_osm`.
- **A pin takes up to a full node's homes,** the nearest ones, and the rest repartition, so one drag can add or remove a node elsewhere. That's what "re-solve" means here, and the numbers on screen change with it.
- **A `Proposed` work order doesn't block.** Diagnosing a node can leave one and nothing can remove it, so the first version of this rule left a map stuck after one diagnosis: no re-solve and no discard. Nobody has acted on a proposal, and it goes with the node.
- **The headend changes fiber only.** The optimizer never read it. A draggable dot with no effect would be a fake control, so it now drives a real, reported derivation, and the page states what it doesn't change.
