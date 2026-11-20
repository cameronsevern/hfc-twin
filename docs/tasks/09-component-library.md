# Task 09 — Component library breadth

**Build-order step:** 9
**Depends on:** Task 08
**Produces:** `fixtures/library/sourced.json`, `docs/library.md`, and `crates/design/tests/library.rs`.
**Load:** `CLAUDE.md`, invariant 9; `docs/DESIGN.md` "Component library".

## Goal

Parts the optimizer can place that come from a public sheet, a link to each sheet, and a plain record of what no public sheet covered.

## Deliverables

1. A library with `Datasheet` parts read from the primary documents, each with `source_url`, and generics kept for the classes with no sheet.
2. A field-level provenance record, since `spec_confidence` applies to a whole part and some fields aren't on its sheet.
3. `DesignParts` swaps real parts into a design, and an optimizer rule keeps out a tap that blocks line power.

## Tests

- Every `Datasheet` part has a link, and the generic library claims none.
- The P3 625 fit is within 0.03 dB of its sheet at 5–1218 MHz.
- The BLE120 and XM3-915HP read as their sheets do, and the BLE120 qualifies on both profiles.
- Designs on the real parts come out with no issues, on every family and both profiles.

## Out of scope

Full breadth by vendor, operator-supplied parts from an invoice, and versioning a part's revisions.

## Acceptance

`cargo test -p design` green, and `docs/library.md` says which fields aren't from a sheet.

## Decisions made during the task

- **The library is only four sourced parts.** That's short of "populated from spec sheets" for both first profiles, and `docs/DESIGN.md`'s v1 list asks for that. Outdoor taps, drops, nodes, RPDs and modems stay `Generic` because
  no sheet was reachable or public. An operator's invoices or vendor access would close this.
- **The sheet was read directly in each case,** as an image of its PDF, since a fetch's text summary came back wrong or empty for these files.
- **The generic fixture stays,** so `docs/physics.md`'s oracles stay valid. Sourced parts are a separate file.
