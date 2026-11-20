# UX review of the web UI

Reviewed 2026-11-05 on branch `feat/gnn-selection`, using the server from `.claude/launch.json` (`mcp-server --http 127.0.0.1:8484 --model ml/artifacts/proposal_gbdt.json`, in memory, no Postgres). The page is
`crates/mcp-server/ui/index.html`. Screenshots are in [`docs/ux-review/`](ux-review/). The findings below are the review as it was written. They were then fixed in [task 20](tasks/20-ux-fixes.md); see [Status after task 20](#status-after-task-20).

## Verdict

Every flow works from start to finish: monitor, environment, workbench, what-if, diagnose, dispatch → complete → verify,
migrate, generate a map, OpenStreetMap search → draw → import → design, pins, added nodes, moving the headend, refusals and
discard. The synthetic labelling (invariant 11) is the strongest part of the page: the banner is always on screen, the
basemap appears only under real geography, and imported maps list exactly what was left out.

As an operator tool it isn't ready yet. It reads like an engineer's console for the twin, and four things would hurt
anyone using it for real:

1. **It shows a blank page, with no message, when WebGL or the unpkg CDN is unavailable**, and the parts that don't
   need the map die with it.
2. **Work orders exist only in the page.** A reload or a node switch loses them, and nothing in the UI lists them again.
3. **Some displays go stale and stay misleading**: the RxMER overlay after a new diagnosis, the what-if result after
   you change the fault, and errors that disappear before anyone reads them.
4. **It doesn't scale past a demo.** At 462 nodes the region table is an unsorted wall that pushes the node's own
   modems off screen, and one unguarded button marks every node in the twin built.

None of these is deep. Most of the fixes are small, local changes to `index.html`. The server already has a tool for
the work order list (`list_work_orders`).

## Status after task 20

Every finding is fixed or answered. Each fix was rechecked against its original repro in a browser with WebGL (headless
Chromium with SwiftShader) and one without (the Browser pane). Both were checked at 1440 and 375 px, in light and dark.

**Server** (`crates/twin`, `crates/mcp-server`):
- MapLibre 4.7.1 is vendored and served at `/vendor/`.
- `set_environment`, `generate_layout` and `design_region` refuse out-of-range values, naming the field.
- `get_map` reports what locks a map (`locked`).
- `get_region_status` counts open work orders.
- `localize_ingress` reports `upstream_degraded`.
- No tool was added.
- `crates/twin/tests/arguments.rs` covers the checks. The existing twin and mcp-server tests pass, and clippy is
  clean.

| Finding | Status |
|---|---|
| H1 blank without WebGL or CDN | Fixed. MapLibre comes from the server. Without WebGL the map area explains why, and everything else works: [screenshot](ux-review/after-no-webgl-diagnose.jpg). |
| H2 work orders vanish | Fixed. The Diagnose tab lists the node's orders from `list_work_orders`, newest first, each with its next step. They survive a reload. The region table counts open orders. |
| H3 stale RxMER overlay | Fixed. Each diagnosis re-plots. An inject, clear, repair or environment change marks the diagnosis out of date and drops the overlay. The plot title has the read time. |
| H4 "Mark every node built" | Fixed. It's now "Mark this map's N nodes built" and "Mark this node built", each confirmed in place and saying it's one-way. No tool to un-build was added (out of scope in task 20). |
| H5 doesn't scale | Fixed. Problems sort first. Rows are grouped by map, with a filter and "Problems only" (on by default above 30 nodes), and the region box collapses. The node picker is grouped by map, readable, and narrowed by "Find a node": [screenshot](ux-review/after-11-monitor-scale.jpg). |
| H6 no validation | Fixed in the page (bounds, a message beside each field, cleared on edit) and on the server (plain refusals). Raw serde text is rephrased. |
| M1 swallowed clicks | Fixed. A tab switch no longer locks buttons. |
| M2 errors far away and wiped | Fixed. Each action has its own message slot beside it, with `role="alert"`, kept until dismissed or the action runs again. |
| M3 Workbench stale and silent | Fixed. What-if clears on any change. Inject confirms. Faults are listed by name, target and parameter, each with Remove. Position along a span is a field. A bench click no longer changes the plot: [screenshot](ux-review/after-02-bench-injected.jpg). |
| M4 migration | Fixed. The plan says what applying clears, the apply is confirmed in place, profiles are shown by name, and a stale id is pointed out. |
| M5 statistician wording | Fixed. One plain lead sentence, the Workbench's fault names, a close-call note in the answer and the order, and the order says where to go. The statistics sit under "Details": [screenshot](ux-review/after-13-diagnose-closecall.jpg). |
| M6 no legend or inspection | Fixed. A legend, a popup on click, injected faults drawn, zoom buttons, and no red in the node palette. |
| M7 map and node mixed | Fixed. Separate Map and Node sections. Choosing a map moves the header to its node, and choosing a node moves the map. Profiles have readable names. |
| M8 dark contrast | Fixed. Dark text on the amber banner and on light-blue buttons, and the fiber colour follows the theme: [screenshot](ux-review/after-14-dark.jpg). |
| M9 keyboard and a11y | Fixed. ARIA tabs with arrow keys. Focusable rows. Node and headend markers move with the arrow keys and Enter. Esc cancels map modes. The plot has a text summary. Status dots carry a shape and a word. There's an `h1`. |
| M10 refused controls offered | Fixed. Locked maps say why, and their controls are disabled. In-page confirmations replace `confirm()`: [screenshot](ux-review/after-07-design-locked.jpg). |
| L1 long imports | Fixed. Elapsed seconds and "Stop waiting", plus a size hint. |
| L2 place search | Fixed. Up to five results to pick from, and a hint when there are none. |
| L3 Monitor copy | Fixed. Units on every column. D3.0 modems are marked. Environment is labelled as whole-twin, and time of day was added. Cache numbers are moved to a footnote. |
| L4 ids and wording | Fixed. Readable map and node names, plurals, the optics sentence, no § references, the "Unpin" tooltip, and a note when generated homes differ from the request. |
| L5 narrow widths | Fixed. The layers box folds closed below 760 px, and the Diagnose columns stack: [screenshot](ux-review/after-15-narrow.jpg). |
| L6 map jumps on mark built | Fixed. |
| L7 switch test | Answered. It was not physics: 18 MHz falls between the node's upstream channels, and 30 and 150 MHz register and are traced below amp-2. The default is now 30 MHz. The hint says gaps show nothing. The result tells "nothing to find" apart from "not below an amp". |

**Still open:**
- The fixture node ids (`sg-one-street-lowsplit`) name a profile and can go stale after a migration. The page shows
  the real profile beside the id, but renaming nodes is a twin change, not a page change.
- A real place is named by its coordinates rather than the searched name, because the twin doesn't store a name for a
  map.
- Touch dragging on a phone, a real screen reader and Postgres persistence weren't tested.

## How it was tested

- The Browser pane's preview has WebGL disabled, which became finding 1 ([screenshot](ux-review/00-no-webgl-blank.jpg)).
  Everything else was driven in headless Chromium with software WebGL (SwiftShader) over the DevTools protocol, at
  1440×900, 768×1024 and 375×812, in light and dark color schemes.
- Real place: Lafayette, Colorado (Nominatim), a 0.1 km² box around Simpson and Cleveland Streets, imported through
  Overpass.
- Not covered: Postgres persistence (`--database-url` / `docker compose up`; the only Postgres running belongs to
  another project), touch dragging on a real phone, a real screen reader, and an Overpass outage.

## Findings, by severity

Severity is the cost to an operator: **High** means they lose work, are misled, or can't use the page. **Medium** means
they're confused or slowed down. **Low** is polish. Line numbers refer to `crates/mcp-server/ui/index.html`.

### High

**H1. Without WebGL, or offline, the whole page is blank and gives no message.** *All pages.*
Repro: open the page in a browser without WebGL (the Browser pane, many VDI or Citrix NOC desktops, GPU disabled), or
with unpkg.com unreachable. You get the banner, an empty node picker and empty tables. The console says "Failed to
initialize WebGL". `new maplibregl.Map(...)` (l. 267) throws at the top level of the only script, so no MCP call runs at
all. Monitor, Diagnose, Workbench and Migrate don't need the map, but they die with it.
[Screenshot](ux-review/00-no-webgl-blank.jpg).
Fix: create the map in a `try`. On failure, show "The map needs WebGL, which this browser doesn't offer. Tables and
diagnosis still work." and keep loading everything else. Serve MapLibre from the server rather than unpkg, so an
air-gapped NOC can use the page.

**H2. Work orders vanish on reload or node switch, and the UI can't find them again.** *Diagnose.*
Repro: inject a fault → Diagnose → a "Proposed" order appears. Reload the page, or pick another node and come back.
The Diagnose tab is empty, but the region table still shows the fault, and the order still exists on the server. The
page only shows `lastAnswer.work_orders[0]` (l. 637) and clears it in `loadNode` (l. 471). It never calls
`list_work_orders`, so an order can't be dispatched, completed or verified once the page forgets it. That's a dead end
in the main lifecycle.
Fix: render the Diagnose tab's order card from `list_work_orders` for the node, with every open order and its status.
Add a region-wide "Open work orders" list. That's the NOC's main queue.

**H3. After a second diagnosis, the RxMER plot still shows the first one's hypothesis.** *Diagnose.*
Repro: inject Amp drift → Diagnose → pick modem-6 under "Predicted against observed". The overlay is red, "simulated
under AmpDrift". Clear the fault, inject a Suckout, Diagnose again. The plot still says "simulated under AmpDrift" and
still draws the old observed and old simulated traces. The new top answer is Suckout.
[Screenshot](ux-review/11-diagnose-suckout.jpg).
`diagnose` sets `lastAnswer` but never calls `plotModem()` (l. 626–651).
Fix: after each diagnosis, re-plot the selected modem, or close the plot. Also re-plot after inject, clear and
environment changes. Put a "computed at" time in the plot title.

**H4. "Mark every node built" acts on the whole twin, asks nothing, can't be undone, and locks every map.** *Design →
This node.*
Repro: with a generated map and a real map loaded, click "Mark every node built". All 20 nodes on every map, plus the
fixture nodes, flip to Built in about 0.3 s (l. 1014–1018). From then on, "Re-solve this map" and every node drag are
refused on every map ("can't re-solve this map: … has built elements"). No control moves a node back to Designed.
[Screenshot](ux-review/34-resolve-refused.jpg).
The button sits under the heading "This node", so its scope looks local.
Fix: scope it to the current map ("Mark this map's 16 nodes built"), ask for confirmation with the count, and move it
next to the map controls. Add a way back to Designed, or at least say that it's one-way.

**H5. The region table and node picker don't scale.** *Header, Monitor.*
Repro: generate a few maps (or set Homes per node to 0, see H6). With 462 nodes, the node picker is a 462-item native
select of machine ids. The Monitor tab opens with a 462-row region table: it's sorted by string (`sg-10` before
`sg-2`), unhealthy nodes aren't moved up, there's no filter, and long ids wrap onto two lines. The selected node's modem
table and environment controls sit *below* all 462 rows.
[Screenshot](ux-review/28-monitor-many-nodes.jpg).
The task 13 demo region alone is 20–80 nodes, and a real hub has hundreds.
Fix: sort by severity (down, then affected, then faults) and then by natural order. Add a filter box and a "problems
only" toggle. Group rows by map. Make the region table collapsible, or move it to its own tab, so the node's modems
come first. Replace the picker with a searchable combobox.

**H6. Inputs aren't validated, so nonsense values are silently accepted.** *Design, Monitor.*
Repro, each from the page:
- Homes per node `0` → "Generate and design" makes **439 one-home nodes**.
- Take rate `5` (500 %) is accepted.
- Homes left blank → `Number('')` = 0, which gives a 29-home map.
- Air temperature `200` °C and rain `-5` mm/h are accepted. Every modem turns amber.
- Homes `-5` and seed `1.5` are refused, but with raw serde text: "bad arguments: invalid value: integer `-5`,
  expected usize".

[Screenshot of 200 °C](ux-review/04-monitor-hot.jpg).
Fix: set `min`, `max` and `step` on the inputs (take rate 0–1, homes per node ≥ 10, temperature −40 to 60, rain ≥ 0).
Check values before calling a tool and say what's wrong next to the field. Map server argument errors to plain
language. Validate on the server too, since other MCP clients get the same nonsense through.

### Medium

**M1. Clicks right after switching tabs are silently ignored.** *All tabs.*
Repro: click the Diagnose tab, then immediately click "Diagnose this node". Nothing happens and no error appears. A
tab switch runs `guarded(syncGeography)` (l. 592), which disables every button while it runs (l. 584). A click in that
window lands on a disabled button. I hit this on the first try.
Fix: don't use the global busy lock for tab switches, or disable only the controls that the running action affects.

**M2. Errors appear in one place, far from the cause, and are wiped by the next action.** *All pages.*
The only error display is one red line in the header (l. 75). `guarded` clears it at the start of every action,
including a tab switch (l. 585). Repro: inject a fault on a real map's node → Design → "Re-solve this map". The refusal
("… has a fault injected") shows in the header, and switching to Workbench to clear the fault wipes it. Errors use raw
server text ("can't re-solve this map: osm_39.9962_-105.0929_39.9987_-105.0886/sg-0 has built elements."). There's no
`aria-live`, so screen readers never hear about failures.
Fix: show the error next to the control that caused it, keep it until it's dismissed or the state changes, and
rephrase it for an operator, saying what to do next ("Node 0 is marked built. Re-solving would overwrite built records.
Unbuild it or design a new map."). Add `role="alert"`.

**M3. The Workbench keeps stale results and confirms nothing.** *Workbench.*
- What-if: run it for Connector reflection on amp-1, then change the class to Water in cable and the target to span-5.
  The card still says "8 of 8 modems would read affected", now under the wrong fault.
  [Screenshot](ux-review/06-workbench-wrong-target.jpg). Clear `#fOutcome` in `fillTargets` and `targetChanged`.
- Inject gives no confirmation. The outcome card is cleared (l. 611), and the only sign it worked is the word
  "AmpDrift" under "On this node".
- The fault list shows schema kinds, with no target or parameters. An injected **Amp failed** is listed as **AmpDrift**
  (l. 425). Faults can't be removed one at a time.
- The position along a span that a map click sets (`posFt`, l. 349) is invisible and can't be edited.
- Clicking a modem while a span-only fault is selected shows an error, and also replaces the RxMER plot with that modem
  (l. 341).

Fix: list each fault as "Amp failed, amp-2" or "Water in cable, span-5 at 172 ft, 120 ft long", with a remove button
for each. Show a brief "Injected" confirmation. Add a "Position along span, ft" field.

**M4. Applying a migration asks nothing, clears faults without saying so, and leaves a misleading node name.** *Migrate.*
Repro: on `sg-one-street-lowsplit`, Plan → "Apply to the twin". It applies at once. Only afterwards does it say "Its
faults were cleared". The plan never mentioned that. The node keeps the name `sg-one-street-lowsplit` while running
`d31_highsplit_1218_rpd`.
Fix: put "Applying clears this node's N faults and marks M parts Designed" in the plan card, and confirm before
applying. Show the profile next to every node name, or display a name that doesn't embed the profile.

**M5. The Diagnose tab speaks statistician, not NOC.** *Diagnose.*
[Screenshot](ux-review/08-diagnose.jpg).
- Labels: "posterior", "misfit 0.02", "χ²/dof", "raw posterior, not calibrated", "81 simulations". The fault names
  are schema kinds ("AmpDrift", "WaterIngress", "FrequencySelectiveLoss", which breaks mid-word at 375 px), while the
  Workbench calls the same faults "Amp drift" and "Water in cable".
- The two columns ("Without the model" and "With the model") can disagree, and nothing says which to trust. On a
  healthy node with 0 modems affected, the left column's top answer is **BadDrop 23 %** and never offers Healthy. The
  right column says Healthy 49 %. [Screenshot](ux-review/30-narrow-diagnose.jpg).
- Near-ties are hidden. A suckout gave span-4 53 % against span-5 47 %, and the order sends the truck to one with no
  hint to check both.
- The work order for "Suckout on span-4" names element **amp-2**, which reads as the wrong place.

Fix: lead with one answer in plain words ("Most likely: amp-2 has drifted −8 dB (99 %). 4 modems affected."), use the
Workbench labels, and move the model comparison and fit statistics behind a "Details" disclosure. When the top two are
within ~10 points, say so in the work order ("Check span-4 first, then span-5"). Name the element the crew should go
to, and give its location.

**M6. The map has no legend, no labels and no way to inspect an element.** *All map views.*
Purple and blue dots (Designed and Built record status), orange dots (power supply or RPD), green, amber and red modems,
and dashed lines (fiber or crossings) are never explained. Amps aren't labelled, so "amp-1" in a dropdown can't be
matched to a dot. Clicking an amp or tap on Monitor does nothing. Only modems and Workbench targets react (l. 338–351).
Faults themselves aren't drawn. Only the work-order pin is. Node 1's colour in `PALETTE` is red (`#d9534f`, l. 656),
the same as "down" and the fault ring. There are no zoom buttons.
Fix: add a collapsible legend in the layers box. Show a popup on click with id, kind, record status and key levels.
Draw active faults on the fault layer. Drop red from the node palette. Add `NavigationControl`.

**M7. The Design tab mixes map scope and node scope, and the header can disagree with the map.** *Design.*
The top half acts on the map ("This map", "Re-solve"). The bottom half (This node, Design checks, Parts, Level budget)
acts on the node in the header. After importing Lafayette, the header still showed
`generated-Suburban-1-1600/sg-0 · Synthetic · 108 designed` while the map showed real streets.
[Screenshot](ux-review/20-design-imported.jpg).
The Profile select keeps `d31_lowsplit_1002_iccap` while the header's node runs high-split, and it truncates to
"d31_lowsplit_1002_icca" at 400 px.
Fix: split the tab into "Map" and "Node" sections with their own headings, or move the node half to a "Records" tab.
When a map is chosen, switch the header to one of its nodes. Give profiles readable names ("Low-split 1.0 GHz, I-CCAP"
and "High-split 1.2 GHz, RPD").

**M8. In dark mode the safety banner and primary buttons have low contrast.** *All pages, dark scheme.*
In dark mode the banner is white on `#d99a3c`, about 2.4:1, and primary buttons are white on `#6ea4ee`, about 2.6:1.
Both are below the 4.5:1 that WCAG AA requires. In light mode the banner is white on `#b26a00`, about 4.2:1. The
banner is how invariant 11 is enforced on screen, so it needs to stay readable. Fiber lines (`#111820`) disappear on the
dark ground. [Screenshot](ux-review/33-dark-design.jpg).
Fix: in dark mode, use dark text on the amber banner and buttons, or darken the fills. Give fiber a theme colour.

**M9. Keyboard and screen-reader support is minimal.** *All pages.*
None of the 516 clickable table rows (`tr.pick`) can take focus. Node and headend markers can't be reached by keyboard,
so pinning has no keyboard path. The tabs are plain buttons with no `role="tab"` or `aria-selected`. Escape doesn't
cancel "Draw an area" or "Add a node" (I checked: the button stayed "Cancel drawing"). The RxMER canvas has no text
alternative. Modem and node status is shown by colour dots alone. There's no `h1`.
Fix: make rows buttons or give them `tabindex="0"` with Enter handling. Use the ARIA tabs pattern. Let Escape cancel
map modes. Add a status word or icon beside each dot. Give the plot an `aria-label` that summarises min and mean RxMER.

**M10. The page offers controls whose action is always refused.** *Design.*
On a map with built nodes, "Re-solve this map", "Add a node", "Clear pins", "Unpin" and node dragging all stay enabled.
Each fails, and a dragged marker snaps back. Discard uses two native `confirm()` dialogs, and the second one contradicts
itself: "… has a fault injected. Clear it first. Discard anyway?"
Fix: disable these controls and say why ("Locked: node 0 is built"). Replace `confirm()` with an inline confirmation
that states the consequence once.

### Low

**L1. Long operations give little feedback.** Importing a 0.1 km² area took **39 s**, with only "Asking OpenStreetMap.
This can take a minute." and no spinner, elapsed time or cancel. A 40 km² box would take far longer. A re-solve only
disables buttons. Fix: add a spinner with elapsed time and a Cancel button (use `AbortController`), and suggest a
typical area size, such as "a few blocks, under 1 km²".

**L2. Place search is brittle.** "Old Town Lafayette, Colorado" found nothing. "Lafayette, Colorado" zoomed to the
whole city, so you have to zoom in by hand before drawing. Nominatim is called with `limit=1` (l. 776). Fix: show the
top five results to pick from.

**L3. The Monitor tab has gaps in copy and units.**
- Column headers lack units: Level Δ, Tx Δ, Margin and NMTER are all dB.
- Margin shows "–" for DOCSIS 3.0 modems with no explanation, and DOCSIS version isn't shown.
- "1 answers served from cache, 2 computed" is a developer metric on an operator screen.
- Environment is global, but it sits inside the node's panel.
- Simulated time (`set_environment` supports `t`) isn't exposed.
- For a D3.0 modem the plot title still says "Observed" in blue while saying no RxMER exists.

[Screenshot](ux-review/03-monitor-rxmer-ofdm.jpg).

**L4. Ids and wording.** Maps are named `osm_39.9962_-105.0929_39.9987_-105.0886` and nodes
`generated-Suburban-1-1600/sg-0`. Use the searched place name and "Node 0". Other copy problems:
- "1 nodes over 101 homes" (l. 887).
- "…calibrate it.: 0.9 dB mean" (l. 1038).
- "§7.3" citations in operator text.
- "Unpin" on an added node removes the node itself. Label it "Remove" there.
- Node numbers change on every re-solve, which makes them a poor handle.
- Asking for 1,600 homes gave 1,385, and the page doesn't say why.

**L5. Narrow widths work but are cramped.** At 375 px there's no horizontal scroll and the map stacks above the
panel. The layers box covers about a third of the 50vh map, the active tab scrolls inside about 300 px, and the
two-column Diagnose cards squeeze. [Screenshot](ux-review/29-narrow-375.jpg). Fix: collapse the layers box into an
icon button below 760 px, and stack the Diagnose columns.

**L6. Marking a node built re-fits the map.** `drawPlant()` defaults to `fit=true` (l. 1013), so the view jumps.
Pass `false`.

**L7. The ingress switch test can't tell "no ingress" from "not below an amp".** Before any ingress, and after
injecting Upstream ingress at the Workbench defaults on drop-7 (18 MHz, 20 dBmV), it says "No amp's return leg changed
the noise, so it isn't below an amp." `get_status_map` reports `upstream_degraded: false`, even with 45 dBmV in a
what-if. The UI should say "No ingress detected at the node". Whether ingress through a drop should register at these
levels is a physics question, outside this review (see follow-ups).

## Invariant 11: is synthetic always labelled?

Mostly yes, and this is done well.

- The banner "Simulated plant and telemetry. Not field data." is on every view, at every width, and in both schemes.
  Under a real map it adds "Real streets from OpenStreetMap. The plant on them is invented, not any operator's
  network."
- Generated maps get no basemap ([screenshot](ux-review/26-design-built.jpg) for comparison with
  [a real one](ux-review/21-design-real-designed.jpg)). Their info card says "Generated streets and homes, not a real
  place."
- `Synthetic` tags appear on the header node, design results, migration plans and verified labels. The RxMER plot title
  ends "Simulated data." Imported maps credit OSM and list what was left out ("7 buildings that aren't homes, 4
  addresses inside a building already counted").

Gaps:
- Dark-mode banner contrast (M8) weakens the main label.
- The work-order card and the diagnosis cards have no tag of their own, so a screenshot cropped below the banner loses
  the label. Add the `Synthetic` tag there, as on the design card.
- The record-status colours (Designed in purple, Built in blue) are the on-map form of the designed, built or imported
  provenance, but no legend explains them (M6).
- When the header node and the shown map differ (M7), the header's `Synthetic` tag describes a different plant from the
  one on screen.

## What works well

- The lifecycle runs in a few clicks: diagnose → Dispatch → "Mark the repair done" → "Verify from telemetry" →
  "Cleared: fresh telemetry shows every modem it affected back to normal", and the label appears under Verified labels.
- What-if says "Nothing was changed."
- Refusals protect records: re-solve and discard are refused with faults or built elements.
- The headend's text explains honestly that it only moves the fiber route.
- Pins snap to streets, and re-solves take about 2 s.
- The 40 km² limit is checked before any network call: "1321.1 km² is over the 40 km² one import can ask for. Draw a
  smaller box." ([screenshot](ux-review/36-design-area-too-big.jpg)).

## Suggested order of fixes

1. H1 (load without WebGL), then H2 (work order list from `list_work_orders`), then H3 (re-plot after diagnose). These
   are small changes, and they remove the dead ends.
2. H4 and M10: scope and confirm the destructive buttons, and disable controls that can't succeed.
3. H6 and M2: validate inputs, and keep errors next to their cause.
4. H5: sort, filter and collapse the region view.
5. M5, M6 and M7: plain-language diagnosis, a map legend and inspection, and the map/node split in Design.
6. The rest as polish.

## Follow-ups outside this review

- The switch-test and ingress question in L7. *Resolved in task 20: it's a channel gap, not physics.*
- The server accepts the same out-of-range values as the page (H6). *Done in task 20.*
