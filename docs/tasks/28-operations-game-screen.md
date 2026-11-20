# Task 28 — Operations as a game screen

**Build-order step:** after 26, at the user's request: "Now let's work on the UI. How do we make it feel more like a sim game without
losing critical functionality?"
**Depends on:** Task 26 (the ops world and its tools), Task 13 (the page)
**Produces:** a full-window operations screen for a running world: the region is the stage, both worlds side by side, trucks that
drive, calls that ring, weather and night that show, disasters dropped from a toolbar onto the map, a running score, and scenarios
that play a demo day with one click. The ops tools gain what the screen needs; the page's other tabs are untouched.
**Load:** `CLAUDE.md`; `docs/tasks/26-operations-sim.md` (what the world is and isn't); `crates/twin/src/operate.rs`;
`crates/mcp-server/ui/index.html` (the Operate tab).

## Goal

Task 26's world is right underneath and hard to watch on top. Driven in a WebGL browser (headless Chromium with SwiftShader, the
first time this page's ops layers were seen drawn), the Operate tab showed:

- **The map isn't framed on the region.** It stays on the node picked in the header, so the region's outage is a speck at its edge.
- **Nothing is drawn but dots.** There are no streets (they draw on the Design tab only), no plant and no hub. The node in the header
  draws its own modems over the world's homes.
- **Trucks jump** once a second, from one tick's position to the next.
- **Everything else is a form in a 400 px sidebar.** That includes the truth switch, whose label is broken.

A city-building game feels alive because the city is the screen, time visibly passes, things move, and every control is one click
from the map. That's this task. It changes how the world is shown and driven, never what it does (invariants 4 and 5): the page
displays, and every rule stays in the crate.

## The screen

Operate opens a screen over the whole window. Leaving it returns to the tabs, and the world stays where it was, paused.

- **The stage.** One map per world, side by side with the cameras locked together: "From calls only" on the left, "With the twin"
  on the right. The same storm hits both, and the difference is what's being sold. The view switch shows one world at full width;
  narrow windows start there.
- **What the map draws:**
  - the region's streets
  - its plant, faint: spans (aerial solid, underground dashed), nodes, fiber
  - the hub
  - every served home
  - the basemap, only under real geography (invariant 11), as the Design tab does
- **Homes by how their household lives it:** up, slow, out, or no house power. Out homes glow, so a cluster reads from far out.
- **Life on the map:**
  - A new call rings out from its home as a ripple.
  - An open job is a pin at its target, coloured by crew.
  - A truck is a badge, coloured by crew and numbered, that drives its route smoothly.
  - A truck working on site pulses and shows its progress.
  - A driving truck's remaining route is drawn ahead of it.
- **The sky:**
  - Night falls and day breaks on the clock.
  - A storm darkens the sky and rains.
  - A heat wave warms the light.
  - All of it is read from the world's own environment, never invented by the page.
  - With reduced motion asked for, the rain and ripples don't animate.
- **The top bar:**
  - "Simulated: not field data" in the banner's colours (invariant 11), and the real-streets note when the streets are real.
  - The clock, with the day and a sun or moon.
  - Speed controls: pause and three speeds, plus "as fast as it goes". The bar says when the sim is behind the chosen speed rather
    than pretending to keep up.
  - The weather.
  - The seed, the intensity and the scenario.
  - Buttons for the board, the sources and leaving.
- **The toolbar of disasters** down the left: storm, heat wave, power cut, dig-in, car into the plant, fiber cut.
  - The three placed ones arm the cursor. A power cut shows its circle under the mouse.
  - Esc disarms.
  - A placed disaster flashes where it lands, in both worlds.
  - From the keyboard, Enter on an armed tool places it at the centre of the map, so no disaster needs a mouse.
- **The score, as a race,** on the right:
  - Customer-hours out, homes out now, truck rolls, rolls that found nothing, and calls abandoned, from calls beside with the twin,
    each with a pair of bars.
  - A chart of homes out over time for both worlds.
  - The full table of task 26's counters, one click away.
  - "In this world, under these rates. Not a measured saving." always under it (invariant 11).
- **The ticker** along the bottom: the story as it happens, newest first. Each line with a place has a button that flies both
  cameras there.
- **The board,** a drawer:
  - for each world, the call centre (waiting and abandoned calls)
  - open tickets
  - jobs: crew, target, truck, and why it was sent (a ticket, the twin's reading, or an escalation)
- **The inspector.** Click a truck, home, job or trouble for a card saying what an operator would know of it. A truck's card can
  follow it with the camera.
- **The audience view,** a switch labelled "Audience view: what's really wrong. The dispatcher never sees it". Off by default. On,
  each map marks the plant's open troubles, and power cuts as circles, and the ticker shows the truth lines in their own colour.
- **The end of a day.** At midnight a card gives the day's score for both worlds, with the caveat. It's dismissed to play on.

## Scenarios

A scenario is a demo day as data, so a presenter can press one button and talk:

- **What it is:** a seed, an intensity, and actions at world times. Its file is `fixtures/ops/scenarios.json`.
- **First scenario:** task 26's demo day, a storm at dawn and then a dig-in, a car and a fiber cut at noon. Its test reads it there.
- **Starting one:** `ops_start { scenario }` logs its actions at their times. A scenario is the user's actions written ahead, not
  scripted physics (invariant 5): each action makes faults through the same hazard draws as a button press.
- **Free play:** no scenario, and the toolbar does everything.

## The tools

All additive, all through MCP (invariant 8), none kept (`persist.rs`'s `NOT_KEPT`). `schema::ops` v1 isn't touched (invariant 3):
what the screen needs that the schema doesn't hold goes in the tools' JSON.

- **`ops_layout`:** what doesn't change during a run, so any client can draw the region:
  - every served home with its node
  - the hub
  - the plant's spans with installation, and the node sites
  - the fiber's lines
  - the streets
  - the region's bounds
  - whether the geography is real
- **`ops_state` adds:**
  - for a truck, when it left and arrives, or when its work started and ends, so a client can move it smoothly
  - every drive still going at the time asked from, or set off after it: truck, job, route, when it left and arrives
  - the sky now: hour, storm wind and rain and when it ends, heat and when it ends
  - for both worlds, homes out and trucks out, every 10 world minutes since the start, for the chart
  - the actions taken, with their times
  - truth entries gain the kind of trouble and, for a power cut, its radius
- **`ops_act` takes `at_s`,** a world time from the next step on, so an action can be written ahead. Without it, the next step as
  now.
- **`ops_scenarios`** lists the scenarios, and `ops_start` takes `scenario`.
- **The pair steps its two worlds on two threads.** They share nothing mutable, so this is the same fold, faster. A test holds a
  run to its single-threaded twin byte for byte.

## Time on the page

The world steps in whole minutes. The page shows a clock that runs smoothly at the chosen speed, never ahead of the newest state it
holds, and asks for the next stretch while showing this one.

- **Truck positions** are interpolated along their routes from the times in `ops_state`. That's display, like easing a camera; the
  world's own `field::position` is what the world uses.
- **Homes, calls and the score** change when the state that holds them is reached.
- **If the sim can't keep up,** the clock slows to what it gets and the top bar says so. Chunking can't change results
  (`one_advance_is_many`).
- **The speeds offered** are measured here and written down, so none of them is a promise the sim can't keep.

## Must survive

The user's condition. Each item is checked in the browser before this task is done.

- **The five other tabs** work as before, and the page loads with no console errors.
- **The Operate tab** stays as the door to the screen, with the seed, the intensity, the scenario and Start. With a world running, it
  shows the score table and opens the screen.
- **On the screen:**
  - play, pause and speed
  - either world alone, or both
  - the audience view, with its label
  - all six disasters, the placed ones by click and by keyboard
  - both worlds' scores with the caveat, and the full counters table
  - the board, the story
  - the rates with source links and confidence tags
- **Invariant 11 on screen:** "Simulated: not field data" is visible the whole time on the screen, with the real-streets note when
  it applies.
- **Without WebGL** the screen still runs: clock, controls, score, board and story, with the map's place saying why it's empty.
- **Keyboard and screen readers:**
  - every control is a labelled button
  - Space plays and pauses, 1 to 4 set the speed, Esc disarms or closes
  - focus is kept on entering and leaving
  - major events (an outage, a fix) are announced politely, at most every few seconds
- **Themes and sizes:** dark theme, and a window 760 px wide.

## Tests

- Rust:
  - the two-thread pair against one thread, byte for byte
  - `act` at a time against the same act made at that time
  - every scenario reads and runs a minute
  - `ops_layout` and the new `ops_state` fields on a small world
  - the demo day reads its scenario
- In the browser (headless Chromium with SwiftShader, scripted):
  - start the demo-day scenario and play
  - two frames a second apart show a driving truck in different places
  - a dig-in placed by a click on the map lands nearer that click than any other underground span would put it
  - the view switch, the audience view and the inspector
  - the end-of-day card
  - every "Must survive" item
  - console clean
  - screenshots kept

## Out of scope

- Sound.
- Money: truck-roll costs and credits need sourced rates first, and the counters stay customer-minutes and rolls.
- Building or editing plant inside the screen: the Design tab does that.
- Several people in one world.
- A phone layout beyond 760 px.
- Making the sim itself faster than two threads give.

## Decisions made during the task

### What was built (2026-11-19)

The screen, as specified, in `crates/mcp-server/ui/operate.js` and `operate.css`. The server serves them beside the page, which
keeps one file per concern instead of growing `index.html` past 2,000 lines. The Operate tab is now the launcher: scenario, seed,
intensity and Start, and with a world running, its score table and the door back to the screen.

The tools:

- **`ops_layout`** and **`ops_scenarios`**, both new.
- **`ops_start`** takes a `scenario`.
- **`ops_act`** takes an `at_s`.
- **`ops_state`** now returns both worlds under `worlds`, or the one asked for. Each world gains its drives (`trips`). Beside
  them it gives:
  - the sky
  - both series, from `series_since_s`
  - the actions taken
  - each truck's `from_t_s` and `to_t_s`
  - truth entries with their kind, radius and node

The ops crate gains `World::act_at`, `sky`, `series`, `trips_since`, `actions_taken`, `homes`, `inputs` and `calls_waiting`. It also gains
`Pair::act_at`, an `advance` on two threads, and `advance_in_turn` to hold it to. Scenarios are `ops::scenario`, a file shape of
their own: they're the user's actions, which `schema::ops` v1 doesn't hold, and that schema wasn't touched.

Decisions made building it:

- **The demo day's noon events are at 12:01, not 12:00.**
  - Task 26's test acted at 12:00, and an action logged then applies at the next step, so its dig-in, car and fiber cut happened
    at 12:01.
  - Each is drawn from the minute it happens, so at 12:00 they land on other cables, and the day turns over: 45,651
    customer-minutes from calls and 58,687 with the twin, which rolled 9 trucks and found nothing on 6.
  - At 12:01 the scenario gives task 26's recorded day exactly: 149,121 and 127,781.
- **How much that day rests on its draws.** The same stormy Monday on six seeds, and with its noon events a minute either side
  (`demo_days_across_seeds`, ignored, 1,600-home generated suburban region). Customer-minutes out:

  | Day | From calls | With the twin | Twin ÷ calls |
  |---|---|---|---|
  | Seed 26, as recorded | 149,121 | 127,781 | 0.86 |
  | Seed 1 | 39,601 | 38,488 | 0.97 |
  | Seed 2 | 359,838 | 232,425 | 0.65 |
  | Seed 3 | 307,087 | 186,683 | 0.61 |
  | Seed 4 | 373,631 | 181,169 | 0.48 |
  | Seed 5 | 473,814 | 271,055 | 0.57 |
  | Seed 26, noon a minute early | 45,651 | 58,687 | **1.29** |
  | Seed 26, noon a minute late | 135,241 | 130,729 | 0.97 |

  - The twin was ahead on 7 of the 8 days and behind on one.
  - Where it's far ahead, the reactive world rolled 18 to 22 trucks and 17 to 19 of them found nothing.
  - The day it's behind, it rolled 9 trucks and 6 found nothing: house-power outages it read as breaks, as task 19 measured it would.
  - In this world and under these rates. Eight days aren't a distribution, and the demo shouldn't be presented as one.
- **A generated suburban town is a strip.** `design::generate` lays one out as a chain of blocks: 9.3 km by 0.4 km at 1,600 homes,
  and a rural one 75 km long. An urban grid of 1,600 homes is 0.9 by 1.0 km.
  - The camera frames the homes and the nodes, and the hub only when it's near them. A generated map's hub can sit kilometres
    off, and framing it too shrank the town to a line.
  - The launcher's "generate a town" makes an urban grid.
  - The generator's shapes are task 14's, and aren't changed here.
- **Speed.** The pair on two threads runs task 26's demo day at 2.54 s of wall time a world hour, against 4.3 s on one thread:
  about 24 world minutes a second on average. Through the tools during an outage of 85 homes, five world minutes ran in about
  170 ms and a state read took 7 ms, so the server keeps up with 15; a slow browser is what falls behind. The speeds offered are
  1, 5 and 15 world minutes a second and "as fast as it goes", and the clock never runs ahead of the states it holds. When the sim
  falls behind the chosen speed, the top bar says so and shows the rate it gets.
- **The page asks for the next stretch while it shows this one,** about 0.6 s of play at a time. Homes and the score change
  when the clock reaches the state that holds them, and each call rings at its own minute.
- **Every drive is logged, because snapshots miss them.** In a 1 km town a drive takes a world minute or two, so most started and
  ended between two states the page read, and the truck was drawn at the hub and then at the job. The world now logs each drive
  as it sets it off (`Trip`: truck, job, route, times), output only, and `ops_state` returns those still going or set off since
  the time asked from. The page draws a truck from its latest drive by the shown time, and from the state between drives. A test
  holds every roll to a logged drive and a driving truck to its last one.
- **The clock slows to watch a drive.** At 5 and 15 world minutes a second a drive lasts a fraction of a second on screen. With
  "Watch drives" on (the default), the shown clock runs at 1.2 world minutes a second while a truck is on the road in a world on
  screen, and says so; "as fast as it goes" never slows. Display pace only: the world steps the same.
- **MapLibre owns a marker's element.** The first build set a truck's class on it, which took off MapLibre's own: trucks fell out of
  place and drew as bars across the map, and a click meant for the map opened the inspector. What's ours sits on a child, and
  markers stack trucks over troubles over job pins over rings, since a working truck stands on its own job's pin.
- **The story is classed for reading, not for rules.** Calls recede, since the map rings them; fixes stand out; the plant's truth
  has its own colour, and shows only in the audience view. Node ids read as "Node 8", as elsewhere on the page.
- **The audience view is off by default.** It's what makes the pitch, so the presenter turns it on when the audience should see
  what the dispatcher can't.
- **The two worlds' colours** are the reference palette's first two categorical slots (blue, orange). They passed its validator
  in both themes against the HUD's surfaces, with contrast at least 3:1 and colour-blind separation ΔE ≥ 24. They're never used
  for status: homes use the page's status colours, and those are paired with size and a key.
- **No disaster needs a mouse.** An armed tool offers "Drop it at the map's centre" in its hint, a button reached by Tab. This
  replaced the spec's Enter-to-place, because Enter on the armed tool's own button would toggle it.
- **A disaster with nothing to hit says so.**
  - The game found it: an urban grid's cable is all on poles, so a dig-in placed there did nothing at all, with no line in the
    story, while the page said an excavator had gone through a cable.
  - The world now writes a truth line when a dig-in finds no buried cable, a car finds no pole or pedestal, or a fiber cut finds
    no fiber plant.
  - The page's word for a disaster is what it does ("an excavator digs there, through the nearest buried cable if there is one"),
    and the audience view shows what it hit.
  - A test holds it on a region made all aerial with no fiber.
- **One bad state never stops the clock.** The first trips went out with their routes as `{lon_deg, lat_deg}` objects while
  trucks' routes are `[lon, lat]` pairs, so the first drive put a NaN position on a marker. MapLibre threw, and the exception
  ended the page's animation loop for good: the clock froze at the first truck roll. The tools now send trips in pairs, and a
  test holds it. The loop catches an error, says it once, and goes on.
- **The scenarios file is its own golden.** It's a fixture the twin reads, so `every_scenario_reads_and_its_first_action_lands`
  round-trips it and runs each scenario's first action, rather than a separate sample under `fixtures/schemas/`.
- **Which region runs.** Task 26 started `regions.values().last()`, which is the last by id, not the one designed last. On a fresh
  `--as-built` server that's the `one-street` fixture's region, whose map isn't loaded, so Start failed with "no map one-street".
  `ops_start` now takes the last region by id whose map is loaded, and refuses with "design an area first" when there's none. The
  launcher passes the region of the map on the Design tab when it's designed, and says which map it started. With no region, it
  offers to generate an urban grid and start on it.
- **Measured in the browser** (headless Chromium with software WebGL, 13 to 19 frames a second; a browser with a GPU wasn't
  measured): through the
  dawn storm "as fast as it goes" ran about 27 world minutes a second; after the noon fiber cut, with every node of the cut
  stepping each minute, about 7. So 15 world minutes a second keeps up most of a day and says it's behind during a large outage.

### Acceptance (2026-11-19)

Driven end to end in headless Chromium with software WebGL (SwiftShader), scripted with Playwright; screenshots are in
`docs/gallery/operations-screen.md`. Every "Must survive" item was checked:

- The six tabs load and switch with no console error, before the screen and after leaving it.
- **The launcher:**
  - takes the scenario, seed and intensity
  - with no region, generates an urban grid and starts on it
  - with a world running, shows the score and opens the screen
- **On the screen:**
  - Focus lands on the speed control, and leaving returns it to the launcher.
  - Space plays and pauses, and the keys set the speed.
  - A car crash placed by a click landed 15 px from the click, and its burst drew within a pixel of it.
  - A truck on the road moved between frames 0.7 s apart, with the clock slowed to watch it.
  - Space plays and pauses wherever focus is, and Space after clicking Storm sent no second storm. Keys 1 to 4 set the speed.
  - The inspector opens on a truck (and follows it), a job pin and a trouble by the keyboard, and a home by a click.
  - The inspector names a truck's job and why it was sent.
  - The board shows each world's call centre, tickets and jobs.
  - The sources list all 35 rates with their confidence and 8 source links.
  - The view switch and the audience view work.
  - The day ends on its card.
- The dark theme in a 760 px window starts on one world.
- With WebGL turned off, the clock runs, a power cut drops at the centre, and the map's place says why it's empty.
- **On a real map** (2026-11-20, `fixtures/osm/any_map/albany_center_ny.json`, imported and designed on the generic rules: 10 nodes,
  no issues, 1,582 homes passed):
  - Start picks that region.
  - The OpenStreetMap basemap loads under both worlds with its attribution.
  - The stormy Monday runs to past noon with no console error.
  - The top bar's label first read "Real streets from OpenStreetMap. The plant on them is invented, not any operator's network.",
    which pushed Leave onto a second row at 1,600 px. It now reads "Real streets, invented plant", with that sentence as its tooltip.

**What the screen showed about the world.** On the urban grid, the stormy Monday is no win for the twin:
- **The scenario alone,** run through the tools: 507 customer-hours out from calls against 489 with the twin. But the twin rolled 11
  trucks to 4, and 10 of its 11 found nothing. Of those 10, 8 were sent within 300 ft of a power cut that was on when it was sent.
- **The same day with a car crash added** at 07:46 (the browser check's): 858 against 921, the twin behind, with 16 rolls to 11.
- **Why:** the dawn storm's power cuts darken dense blocks of homes. Their households have no power to call with, so dispatch from
  calls waits. The twin reads a block of dark modems as a break and sends a line crew to the plant point its reading names, where
  it finds nothing, as task 19 measured house power would be misread.
- An operator can check the power company's outage map before rolling on a dark block; power companies commonly publish one, though
  that isn't checked here for any. The twin here doesn't, so with power cuts common it rolls more trucks, not fewer.
- Whether the twin should read a utility outage feed is the next decision for this demo. It's the user's call, since it adds an
  input the world doesn't have today.
- **The clock's keys work wherever focus is.**
  - The first handler ignored keys on any button, and the screen opens with focus on one. Space then clicked the focused ▶▶
    instead, so it never paused, and Space after clicking Storm would have called a second storm. The check that passed was
    pressing that button by accident.
  - Now Space and 1 to 4 belong to the clock anywhere on the screen. A button's own Space click is suppressed, and a map marker's
    Enter and Space stay its own.
  - A checkbox keeps Space, which ticks it, and lets digits through. A text field keeps both.
