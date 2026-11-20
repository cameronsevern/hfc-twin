# Task 20: Fix the page's UX review

**Build-order step:** not in the build order. This is a repair pass on the page from steps 6, 13 and 14, asked for after the UX review.
**Depends on:** Tasks 06, 13 and 14.
**Status:** done; see the status section of `docs/ux-review.md`.
**Produces:** a page that a NOC analyst, field tech or designer can use without losing work or being misled, plus matching server checks on the arguments.
**Load:** `CLAUDE.md`, `docs/ux-review.md` (the findings and repro steps), `docs/tasks/13-reactivity-and-lifecycle-demo.md` and `docs/tasks/14-map-design.md`.

## Goal

Every finding in `docs/ux-review.md` is fixed or deliberately answered. The page stays an MCP client and nothing more
(invariant 8). It adds no tool that the page alone would use. The banner and the synthetic labels stay, and they get
stronger (invariant 11).

## Deliverables

1. **Loading (H1).**
   - MapLibre is served by the MCP server from `/vendor/`, built into the binary, so the page doesn't need unpkg.
   - If the map can't start (no WebGL), the page says so. Tables, diagnosis, the workbench and migration still work
     without it.
2. **Work orders (H2).** The Diagnose tab lists the node's orders from `list_work_orders`, newest first, each with
   its next step. A reload or a node switch loses none of them. The region table shows open orders per node.
3. **No stale displays (H3, M3).**
   - The RxMER plot is redrawn after a diagnosis, an inject, a clear or an environment change.
   - The what-if card clears when the fault, the target or a parameter changes.
   - Inject confirms what it did.
   - The fault list names each fault the way the Workbench does, with its target and a remove button.
   - A map click on a span sets a position the user can see and edit.
4. **Destructive actions (H4, M4, M10).**
   - "Mark built" acts on this node or this map, never the whole twin. It confirms with a count and says it's one-way.
   - Applying a migration confirms first and says beforehand that it clears faults.
   - Controls that the server would refuse (re-solving, pinning or adding nodes on a map with built nodes, faults or
     orders) are disabled, with the reason shown.
   - In-page confirmations replace `confirm()`.
5. **Validation and errors (H6, M1, M2).**
   - Inputs have bounds. The page checks them before calling a tool and reports beside the field.
   - The server refuses the same out-of-range values in plain words (`set_environment`, `generate_layout`,
     `design_region`).
   - Errors show beside the action that caused them, stay until replaced, use `role="alert"`, and are rephrased for an
     operator.
   - A tab switch doesn't disable other buttons.
6. **Scale (H5).**
   - The region table sorts problems first, then in natural order. It has a filter and a "problems only" toggle, and
     it collapses.
   - The node picker is grouped by map, labelled readably and searchable.
7. **Words (M5, L3, L4).**
   - Diagnosis leads with one plain sentence, uses the Workbench's fault names, and keeps the statistics and model
     comparison under "Details".
   - Near-ties are called out in the work order.
   - Units are on every column, the copy bugs are fixed, and profiles and maps get readable names.
8. **Map (M6, M7).**
   - A legend, a popup on click for any element, active faults drawn on the fault layer, zoom buttons, and no red in
     the node palette.
   - The Design tab separates the map from the node, and choosing a map moves the header to one of its nodes.
9. **Access and themes (M8, M9, L5).**
   - AA contrast for the banner and buttons in both schemes.
   - ARIA tabs, focusable rows, and Escape to cancel map modes.
   - A text summary of the plot, and status words beside the colour dots.
   - A collapsible layers box on narrow screens.
10. **The switch test (L7).** `localize_ingress` also reports whether the node's upstream reads degraded. The page
    tells "nothing to find" apart from "not below an amp".

## Tests

- `crates/twin/tests/arguments.rs`: out-of-range values for environment, layout and design controls are refused with
  a message naming the field. In-range values pass.
- `localize_ingress` reports `upstream_degraded`.
- Existing `mcp-server` tests still pass. No tool is added, so `every_tool_is_classified` still holds.
- By hand in a browser:
  - every repro in `docs/ux-review.md`;
  - the page without WebGL;
  - widths of 375 and 1440;
  - light and dark schemes.

## Out of scope

- A tool to move Built records back to Designed. Marking built stays one-way, and the page says so.
- Physics of ingress coupling through a drop. If the switch test still can't see the default ingress, that's reported
  as a separate question, not changed here (the `sim` crate has other work in flight).
- Auth and multiple users.

## Acceptance

Every finding in `docs/ux-review.md` is marked fixed, or answered with a reason, in that file's status section.
