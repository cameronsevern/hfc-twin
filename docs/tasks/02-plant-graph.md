# Task 02 — Plant graph

**Build-order step:** 2 (the fixture half is already in Task 01)
**Depends on:** Task 01
**Produces:** `crates/plant-graph/`: a read-only, indexed view of a validated plant that
answers every topology question the sim and the fault model will ask.
**Load:** `CLAUDE.md`; `docs/physics.md` §5.1, §6.2, §7.3, §8 and §11; `docs/reference/README.md`
only if a question about taps or powering comes up.

## Goal

`schema::plant::Plant` is flat lists, which are right for a file and wrong for walking.
This crate turns one into a tree you can ask questions of: the ports between two points,
the modems below a point, the amps in a cascade, what a supply reaches.

**The boundary: this crate never computes a dB, a volt or a delay.** It returns paths and
sets, with feet as the only number. `transfer(port_a, port_b, f_mhz)` from §11 is step 3,
built on `path_between` here. If a function needs a frequency, it belongs in the sim.

The session ends when `cargo test -p plant-graph` passes, with every oracle below asserted on
the one-street fixture.

## Deliverables

### 1. The graph

```rust
PlantGraph::new(plant: &'a Plant, library: &'a ComponentLibrary, profile: &'a Profile)
    -> Result<PlantGraph<'a>, GraphError>
```

`new` runs `ComponentLibrary::validate`, `Profile::validate` and `Plant::validate`, and
returns all their errors. So a `PlantGraph` always holds a valid tree, and no query has to
handle a malformed one. Queries return `Err` only for an id or port the caller got wrong.

The graph is an in-memory index, never serialized or stored (invariant 1). It is read-only:
an edit makes a new `Plant` and a new graph.

Dependencies allowed: `schema`, `thiserror`. No I/O outside tests.

**Canonical order.** The tree is indexed in sorted order: elements by id, an element's children
by the id of the port they hang from. Every query returns results in an order that doesn't depend
on how the file lists elements or edges. Two plants that differ only in list order are the same
plant, and step 13's recompute key needs them to behave identically.

No `HashMap` iteration can reach a result (invariant 4). Use `BTreeMap` or sorted `Vec`.

### 2. A tightened `Plant::validate`

`path_between` relies on edges pointing away from the root, and `validate` doesn't check that yet.
Add to `crates/schema`: an edge's `to.port` must have role `Input` and its `from.port` must not
(`PlantError::PortDirection`). This adds an error, not a field, so the schema version doesn't change.

### 3. Tree queries

- `root()`, `element(id)`, `edge(id)`, and `parent(id)`, which returns the parent element and the
  edge that reaches it.
- `children(id)`, and `ancestors(id)`, ordered from the parent up to the root.
- `modems()` and `amps()`, in canonical order.
- `cascade(id)`: the amps between the root and an element, nearest the root first, not counting
  the root or the element itself. A modem with a `cascade` of length 2 is N+2.
- `root_port(id)`: the root's RF port an element hangs from. Funneling is per port (§7.3).
- `below(location)`: the elements downstream of a `schema::fault::Location`, and `modems_below`
  and `amps_below` filtering it. For a point on an edge, that's the edge's far end and everything
  under it. For a port with role `Input`, it's the element and everything under it. For any other
  port, it's only what hangs from that port. A modem's `rf` port gives that modem. This is the
  reach of downstream ingress (§8).
- `common_ancestor(ids)`: the deepest element above or equal to all of them.

### 4. Paths

```rust
path_between(a: &Location, b: &Location) -> Result<Path<'a>, QueryError>
path_from_root(id) -> Result<Path<'a>, QueryError>   // root's RF port to the element's Input port; empty for the root
```

A `Path` is an ordered `Vec<Segment>`, and works between any two points, in either direction:

```rust
enum Segment<'a> {
    Cable { edge: &'a Edge, len_ft: f64, direction: PathDirection },
    Direct { edge: &'a Edge, direction: PathDirection },
    Device { element: &'a Element, in_port: &'a Port, out_port: &'a Port },
}
enum PathDirection { Downstream, Upstream }  // Downstream is away from the root
```

- `len_ft` is the part of the edge the path covers, so it's less than the edge's length when
  an endpoint is a point along it.
- A path crossing an element is a `Device` with the two ports it uses. The sim reads the loss from
  those ports' roles: `Input` to `Through` is insertion loss, `Input` to `TapPort` is tap loss,
  and `TapPort` to `TapPort` or `Through` to `TapPort` goes through tap isolation. That last pair
  is the off-path echo route (§5.1).
- A port endpoint leaves its element through that port's edge with no `Device` segment. If the
  path has to leave by another port, the element is crossed and the path starts or ends with
  a `Device`.
- A port with nothing connected is still a valid endpoint, since an open tap port is a reflector.
- Two points on one edge give one `Cable` segment. A zero-length `Cable` is left out, so a path from
  a point to itself is empty, and so is one from a point at an edge's end to the port there.
- `Path::cable_len_ft()` sums the `Cable` segments, which is the `d` of §5.1.
- `Path::reversed()` swaps the ends: reversed order, flipped directions, swapped ports.

### 5. Power

`power() -> PowerOverlay`, per §6.2, with no current or voltage:

```rust
struct PowerOverlay<'a> {
    feeds: Vec<PowerFeed<'a>>,      // one per supply, in canonical order
    unpowered: Vec<ElementId>,      // actives no supply reaches
    contested: Vec<ElementId>,      // actives more than one supply reaches
}
struct PowerFeed<'a> {
    supply: &'a PowerSupply,
    actives: Vec<ElementId>,        // every active this supply reaches
    segments: Vec<PowerSegment<'a>>,
}
struct PowerSegment<'a> {
    edge: &'a Edge,
    actives_beyond: Vec<ElementId>, // the actives whose current crosses this edge
}
```

The walk starts at `feeds` and goes outward in **both directions**, since a supply can sit
mid-cascade and feed both ways. It crosses a `Span` with `power_passing: true` and an element that
passes power, reading `power_passing` from its spec. Actives pass power. Drops, `Direct` edges and
modems don't carry it. A non-passing part is a cut, per §6.2. Segments with no active beyond them
carry no current, so they're left out.

Assumption to confirm: every active passes AC between all its ports. `ActiveSpec` has no field for
it. Real amps set this with fuses, so if a design needs it, it's a new schema version.

### 6. Design checks

`check_design() -> Vec<DesignIssue>`: a well-formed plant that isn't to spec. `validate` says
"this isn't a plant". This says "this plant isn't buildable or isn't allowed". Empty means no issue
found so far. Step 3 adds the level and voltage checks that need physics.

- `CascadeExceeded { modem, depth, max }`, at each modem, against `profile.max_cascade`.
- `PartNotQualified { component }`, from `qualifies`, once per part in `parts_list`.
- `Unpowered { element }` and `ContestedPower { element }`, from `power()`.

### 7. Fault references

`check_faults(&FaultSet) -> Result<(), Vec<FaultRefError>>`. Task 01's test does this by hand, and
step 4 needs it in the crate. It checks references and ranges only, nothing physical:

- A `Location::Span` names a `Span` or `Drop` edge, not `Direct`, with `0 ≤ pos_ft ≤ len_ft`.
- A `Location::Port` names a real element and one of its ports.
- `WaterIngress` names a `Span`, not a drop, and `pos_ft + affected_len_ft ≤ len_ft`.
- `amp_id` is an `Amp`, `modem_id` is a `Modem`, `psu_id` is a power supply.

## Tests

Oracles are hand-computed from the fixture in `docs/tasks/01-schemas.md`. Each runs on both plants,
with the root being `node-1` or `rpd-1`. Modem `n` hangs from drop `n`, and within each tap port
`tap-1` has the 80 ft D3.0 modem and `tap-2` the other.

**Paths**

| From | To | Expected segments | Cable |
|---|---|---|---|
| `amp-1/out` | `modem-4/rf` | span-2, tap-1 (`in`→`through`), span-3, tap-2 (`in`→`tap-2`), drop-4 | **820 ft** |
| root `rf-1` | `modem-8/rf` | 6 spans, 1 drop, and 6 devices: amp-1, tap-1, tap-2, amp-2, tap-3, tap-4 | **2,820 ft** |
| `modem-1/rf` | `modem-2/rf` | drop-1, tap-1 (`tap-1`→`tap-2`), drop-2 | **230 ft** |
| `modem-3/rf` | `modem-1/rf` | drop-3, tap-2 (`tap-1`→`in`), span-3 upstream, tap-1 (`through`→`tap-1`), drop-1 | **460 ft** |
| `modem-2/rf` | `tap-2/in` | drop-2, tap-1 (`tap-2`→`through`), span-3 downstream. The off-path route of §5.1 | **450 ft** |
| span-3 at 150 ft | `tap-2/in` | span-3 downstream. The §5.1 oracle's cavity | **150 ft** |
| span-3 at 150 ft | `tap-1/through` | span-3 upstream | **150 ft** |
| span-3 at 100 ft | span-3 at 250 ft | span-3 downstream | **150 ft** |
| `tap-4/through` | `term-1/in` | `Direct` only | **0 ft** |
| `tap-1/tap-1` | `tap-1/through` | tap-1 (`tap-1`→`through`) only | **0 ft** |
| `modem-2/rf` | `tap-1/tap-3` | drop-2, tap-1 (`tap-2`→`tap-3`). An open port | **150 ft** |

`path_from_root(modem-4)` carries 1,900 ft of hardline, the 1,200 ft span plus §7.2's 700 ft.

**Tree queries**

- `cascade`: `modem-4` is `[amp-1]`, `modem-8` is `[amp-1, amp-2]`, `amp-1` is `[]`.
- `modems_below`: root gives all 8, span-3 at 150 ft gives modems 3 to 8, `tap-2/in` gives modems 3
  to 8, `tap-2/through` gives modems 5 to 8, `tap-1/tap-1` gives modem 1, a point on drop-4 gives
  modem 4, and `amp-2/in` gives modems 5 to 8.
- `amps_below`: root gives `[amp-1, amp-2]`, which is the §7.3 oracle's M = 2.
- `common_ancestor`: modems 1 and 2 give `tap-1`, modems 3 and 4 give `tap-2`, modems 1 and 8 give
  `tap-1`, and modems 5 and 8 give `tap-3`.

**Power**

- Fixture: the supply feeds the root, and reaches the root, `amp-1` and `amp-2`. Segments are span-1
  `[amp-1, amp-2]` and span-2, span-3 and span-4 `[amp-2]`. Nothing is unpowered or contested.
- Feeding `amp-1` instead: span-1 `[root]` and spans 2 to 4 `[amp-2]`, which proves it walks both ways.
- A non-passing `tap-1`, with a library copy: `amp-2` is unpowered, and span-1 is `[amp-1]`.
- `power_passing: false` on span-1: `amp-1` and `amp-2` are unpowered.
- A second supply feeding `amp-2`: both amps and the root are contested. Add `power_passing: false`
  on span-4 and nothing is.

**Design checks**

- Both fixtures have none. `max_cascade` of 1 gives one issue each for modems 5 to 8 and for nothing
  else, since they're at N+2. A cable rated to 1002 MHz on the high-split plant gives `PartNotQualified`.

**Fault references**

- `fault_examples.json` and `faults.json` pass on both plants.
- Each rule above fails on a hand-broken fault: `pos_ft` 500 on the 400 ft span-2, water overrunning
  its span, a `Location::Span` on `direct-1`, water on a drop, a tap as `amp_id`, an amp as `modem_id`,
  an unknown `psu_id`, and an unknown port.

**Construction**

- `new` passes on both fixtures and returns the errors for a cycle, a bad library entry, an invalid
  profile and the wrong profile.
- `Plant::validate` rejects an edge that enters a `Through` port and one that leaves an `Input` port.
- Query errors: an unknown id, an unknown port and `pos_ft` past the end of an edge return `Err`,
  and never panic.

**Properties**

Over randomly generated valid trees, from a small seeded xorshift in the test so no new dependency
is needed, with fixed seeds:

- `path_between(a, b)` equals `path_between(b, a).reversed()`.
- `cable_len_ft` matches a separate breadth-first search that ignores edge direction.
- Consecutive segments connect: each starts on the port or element where the last ended.
- `modems_below` at the root's RF ports covers every modem, and a modem is below an element's `Input`
  port exactly when that element is the modem or one of its ancestors.
- **Order doesn't matter:** reversing and rotating the file's `elements` and `edges` lists leaves
  every query's result unchanged. The same plant built twice gives identical results (invariant 4).

## Out of scope

Any physics: loss, level, noise, delay, voltage, current. Those are step 3, as is `transfer` and
`carries`. Applying faults to a plant is step 4. Finding the reflector a fault pairs with is step 3
or 4, built on `path_between`. Editing a plant, the OSM importer and the optimizer are step 8. No
new field goes into the schema here. If one looks necessary, stop and ask, because that's a new
schema version.

## Acceptance

- `cargo test -p plant-graph` and `cargo test -p schema` are green, and
  `cargo clippy --all-targets -- -D warnings` is clean.
- Every oracle above is asserted on both fixture plants.
- No function in `crates/plant-graph` takes a frequency or returns a dB, a volt or a delay.
- The `fixtures/` files and golden files are unchanged, which proves no schema shape moved.

## Deferred

- **Homes passed.** `profile.max_homes_passed` can't be checked: `Plant` has no homes, and modems
  aren't homes passed. The optimizer needs them at step 8, as a new schema version.
- **A region over many plants.** Task 01 defers this too.
- **Reflector search.** "The nearest other mismatch" (§5.1) is a walk over `return_loss_db`, which
  needs the sim's rule for what counts as a mismatch.
- **Graph latency.** Step 3 measures the per-service-group budget, and this crate only avoids
  anything worse than sorted indexes and walks of tree depth.

## Decisions made during the task

- **Dev-dependencies:** the tests add `serde` and `serde_json` to read the fixtures. The crate itself
  has only the two dependencies above.
- **Extra accessors:** `plant()`, `library()`, `profile()`, `component(id)` and `actives()`, since step 3
  will need them and `power()` and `check_design()` already do.
- **Test strength:** beyond the oracles, 14 deliberate breaks were made to the code, such as dropping
  the child sort, not swapping ports downstream and walking power one way only. Each failed at least
  one test. This was a one-off check, not a committed test.
