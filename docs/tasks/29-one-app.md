# Task 29 — One app, laid out like the operations screen

**Build-order step:** after 28, at the user's request. They tried to design a real place and couldn't see what to do: "Can you make it
more obvious what I need to do to design? It also feels like operate is its own separate mini app within the app. We want one
cohesive experience." Then: "I like the design of operate, lets use that everywhere".
**Depends on:** Task 28 (the operations screen), Task 25 (real maps), Task 13 (the page)
**Produces:** one page laid out the way task 28's screen is, with the map as the stage, one top bar, a tool strip, a side panel and
a status line. Operate is a mode of it, not a screen over it. Design walks through numbered steps from a place to a running town.
No tool, schema or physics changes.
**Load:** `CLAUDE.md`; `docs/tasks/28-operations-game-screen.md`; `crates/mcp-server/ui/index.html`, `operate.js`, `operate.css`.

## Goal

The user, on a fresh load with a real place in mind, typed "aurora, co", pressed Find and got stuck:

- **Nothing said what came next.** The path is find, zoom in, draw a box, import, design, then run. The Design tab shows all of it
  at once as one long form, with no step marked as the current one.
- **The button they reached for looked live and wasn't.** "Design this map" is drawn in the accent colour while disabled. Its
  reason, "Choose or import a map first", shows only on hover.
- **Find frames the whole town.** Aurora came up 40 km across, and a box drawn there is over the import limit.
- **Red text sat under the form.** The design checks of the node picked in the header (an as-built fixture with known issues) read
  as errors about what they were doing.
- **Operate was its own app.** It opens a full-window dialog with its own top bar and a Leave button, and its launcher is a form in
  a sidebar.

The operations screen is the part the user likes: the map is the screen, controls sit around its edge, and the next thing to do is
one click away. This task gives the whole page that layout, and puts Design's steps in it.

## The layout, every mode

- **One top bar:**
  - the name, and "Simulated: not field data" as a badge in the banner's colours (invariant 11). It replaces the banner row, and
    the real-streets note sits beside it when the streets shown are real.
  - the mode switch, a segmented control: Design, Monitor, Workbench, Diagnose, Migrate, Operate. It's still a tab list for
    keyboards and screen readers, with arrow keys between modes.
  - the node picker, in the modes that work on one node (Monitor, Workbench, Diagnose, Migrate, and Design's node card).
  - the mode's own controls. In Operate those are task 28's clock, speed, Watch drives, weather, view switch, audience view, Board
    and Sources.
- **The stage:** the map, full height. Operate's stage is its two worlds side by side.
- **A tool strip** down the left, in the modes that place things on the map:
  - Design: find a place, draw an area, import this view, generate a town, add a node.
  - Operate: task 28's disasters, unchanged.
  - In the other modes there is no strip, and the stage takes the width.
- **A side panel** on the right holds the mode's content. These are the old tabs' sections, every control and id kept, styled the
  way the score panel is.
- **A status line** along the bottom. In Operate it's the ticker; elsewhere it holds the RxMER plot when one is open.
- **Layers and legend** float on the map as a card, styled like the operations screen's cards.

## Design, as steps

The Design panel opens with three numbered steps. The current step is open with one primary button; a step that's done shows a
one-line summary and a way to change it; a step not yet reachable says what it waits for.

1. **Pick a place.**
   - A search box. Find flies to street level on the hit, not the whole town's box.
   - **Import this view** is the primary button. It imports the map's view as the area. That's the same `import_osm_area` call the
     drawn box makes, so no new tool. While the view is bigger than the limit, the button is disabled and says by how much to zoom in.
   - Draw an area stays as the way to pick part of the view. Cable runs, aerial or underground, sits beside it.
   - **Or generate a town:** the layout family and home count, then Generate. It's an alternative, not a fourth step.
   - The maps already imported or generated stay in a picker.
2. **Design the network.**
   - The primary button is Design this map, or Re-solve this map once nodes exist.
   - The profile, rules, mode, homes per node, take rate and seed sit under "Settings", closed by default, with their values
     summarised on one line.
   - The result follows: nodes, issues and cost. So does the editing help: drag a node, add one, clear pins, mark built, discard.
3. **Run it.**
   - The scenario, seed and intensity, from the old Operate tab.
   - **Run this town** starts the world and switches to Operate.

The node card (records, design checks, parts, level budget, optics) is about the node in the header, not the steps. It sits below
them, closed by default, titled with the node's name.

**A disabled button looks disabled** everywhere on the page, and says why on a line under it, not only on hover.

The page opens in Design.

## Operate, as a mode

- No dialog and no Leave button. The tab is the mode, so its controls join the top bar, its disasters fill the tool strip, and the
  score fills the side panel.
- Before a world runs, the panel holds step 3's form, so Operate can be started from either place.
- **Leaving the mode pauses the clock.** A world plays only while it's being watched, and coming back resumes at the speed it had.
  That's the rule task 28 had for leaving the screen.
- **Space and the keys 1 to 4 are the clock's only in Operate.** In other modes they do what they did.
- The game's two maps are kept; the side-by-side comparison needs two. They resize when the mode is shown again.

## Out of scope

- Any change to what a tool does, a schema, the world or the physics.
- New MCP tools. Import this view sends the view's bounds through `import_osm_area`.
- Redesigning the old tabs' tables and forms beyond moving them and restyling them.

## Must survive

Every item is checked in a browser with WebGL before this task is done.

- **Every mode works as its tab did:**
  - Monitor: the region table, the environment, modems, and the RxMER plot.
  - Workbench: injecting a fault by click and by form, What if, the switch test and fiber faults.
  - Diagnose: diagnosing, work orders, predicted against observed, and labels.
  - Migrate: planning a migration and applying it.
  - Design: generating, importing, designing, re-solving with pins, marking built and discarding.
- **Everything on task 28's must-survive list,** now without the dialog. The keys are the clock's in Operate only, and leaving the
  mode pauses.
- **Invariant 11:** the page says it's a simulation once, up front, and the records and tool results keep their origin (see the
  decisions below).
- **No WebGL:** each mode says why its map is empty, and the panels still work.
- **Dark theme, and a window 760 px wide:** the panel goes under the stage, the tool strip becomes a row, and the top bar wraps.
- **The page loads with no console error,** and every mode can be reached by keyboard.

## Tests

- Rust: none change. No tool changes, and `persist.rs` still holds.
- In the browser, scripted in headless Chromium and checked once in Chrome with a GPU:
  - **From a cold load, by clicks alone:** find "aurora, co", zoom in, import the view, design it, run it, and play in Operate.
  - The same with a generated town.
  - Each mode's must-survive items.
  - Screenshots go in `docs/gallery/`.

## Acceptance

- A first-time user, with a place in mind and nothing else, can get from a cold load to a town running in Operate by following the
  primary button at each step.
- Every must-survive item holds.
- fmt, clippy and the workspace tests are clean.

## Decisions made during the task

### What was built (2026-11-20)

- **One shell.** `index.html` now has:
  - a top bar with the "Simulated" badge, the modes as a segmented tab list and the node picker. Operate adds its clock, speed,
    Watch drives, weather and real-streets note.
  - a tool strip: Design's (find, import this view, draw, generate a town, design, add a node, run it) or Operate's disasters.
  - the stage: the map, or Operate's two worlds.
  - a side panel with the mode's content.
  - Operate's ticker along the bottom.
  
  `body[data-mode]` decides what shows. The old tabs' sections moved into the panel with every id kept; the dialog's own ids
  (`game`, `gLeave`, `gTitle`) and the launcher's (`oOpen`, `oScore`, `oClock`) went with it.
- **Operate's view switch, audience view, Board and Sources moved to the panel,** above the score. In a shared top bar there wasn't
  room for them beside the mode switch.
- **The steps follow the page's state:**
  - Step 1 is done when a map is chosen and no place is being picked.
  - Step 2 is done when that map has nodes.
  - Step 3 is current after that.
  
  "Change" on step 1 goes back to picking a place. The strip marks the next action.
- **Import this view takes up to 2 km².** The page says "under 1 km², about a minute". Find lands at zoom 16 on the hit: a 1,600 px
  window then shows about 0.8 km². At zoom 17 a first run imported 0.2 km² of Aurora, CO, which was one node.
- **The page opens in Design, picking a place.** It used to open in Monitor on the node in the header.
- **Generate a town defaults to an urban grid.** Generated suburban maps are 9 km strips (task 28).
- **A disabled button is drawn as disabled:** dashed, muted, its reason on a line under it.
- **A lost map says so.** The user's import was lost to a restart of the server they were on. Design then showed "No map osm_…".
  Now a map the server no longer has sends the page back to step 1 with the reason: maps live in memory only.
- **"A simulation" is said once, up front.** The user found the repeated caveats annoying: "Let's just say its a simulation up
  front once and then call it good." Then: "For the sake of this implementation, everything will be synthetic. If I get to the point
  of needing to bring in real data, we'll make some updates."
  - A card on first load (`#intro`, remembered in the browser) says that everything is simulated, that streets and buildings can come
    from OpenStreetMap, and that no operator's network is shown.
  - The page no longer repeats it. Gone are the "Simulated: not field data" badge, the Synthetic and origin tags, "Real streets,
    invented plant", "Not a measured saving" and "Simulated data, read at".
  - This replaces task 28's must-survive item that kept the badge on screen the whole time.
  - Invariant 11 is kept where it carries information: every record and tool result still says Synthetic or Field, and the docs and
    gallery still say what's synthetic. The OpenStreetMap attribution stays on the maps, because the license asks for it.
  - When field data arrives, the labels come back wherever real and synthetic data meet.
- **A big area's reply is read whole.** The user's import failed with "sent an unreadable reply: response too big for
  into_string". That's ureq's 10 MB cap on `into_string`, and a few dense km² pass it. `overpass::fetch` now reads the reply through
  a reader capped at 256 MB, and says to draw a smaller area if a reply is bigger. Tested in
  `a_reply_over_ten_megabytes_is_read_whole`.
- **Splicers aren't sent back to a fiber they found clean.** Watching a run on a real map, the user saw the twin's truck 3 "stuck in
  a neighborhood".
  - It was the splicing crew, sent to Node 7's fiber again 60 s after finding it clean. The real trouble was an aerial cable brought
    down, 12 world hours before.
  - The cause was in `World::twin_reads`. Nodes wholly dark together on one fiber cable get a splicing crew, and that branch never
    asked `clean_lately`, which the node's own reading does.
  - Now, once splicers have found a node's fiber clean today, its reading goes up its ladder instead.
  - Tested in `splicers_who_found_a_fiber_clean_arent_sent_back_to_it`: house power out across two nodes on one cable. It failed
    before the fix.
- **Found and not changed:** Migrate's plan on the as-built high-split node fails with "built an invalid plant: GraphError {
  PlugInFamily … rpd-1 … pad-generic-2db }". It fails the same way on `master` for every target profile, so it predates this task.

### Acceptance (2026-11-20)

These checks ran in headless Chromium with software WebGL, scripted with Playwright:

- **Cold load to Operate by clicks alone.**
  - Find "aurora, co" and pick the Colorado hit. It lands at zoom 16.
  - Import the view live from OpenStreetMap: 0.8 to 0.9 km² and about 920 homes, in 41 to 62 s over two runs.
  - Design it: 7 and 9 nodes, no issues.
  - The welcome card shows on first load and not after a reload, and no caveat is left on the page.
  - Run it: the stormy Monday in Operate.
  - Play at Max to 08:30, then 5 world minutes a second.
  - No console error.
- **Leaving Operate pauses it:**
  - The clock held in Monitor, and Space there didn't start it.
  - Coming back resumed at the speed it had.
- **Monitor:** the region table, modems, the RxMER plot and the environment.
- **Workbench:** What if, inject and clear.
- **Diagnose:** an answer.
- **Operate:**
  - Before a run, the stage says none is running.
  - Free play on a generated town started from Operate's own Start.
  - The storm and a power cut placed by a click worked, as did the board.
- **Dark theme at 760 px:** the strip becomes a row, and the panel goes under the stage.
- **No WebGL:** each mode says why its map is empty, and Monitor's lists work.
- **Not yet done:** the same run in a browser with a GPU. Chrome's background tab wouldn't take screenshots.

Screenshots are in `docs/gallery/one-app.md`.
