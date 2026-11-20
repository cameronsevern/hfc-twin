# hfc-twin — rules for working in this repo

Read `docs/DESIGN.md` once for context. The build order in that doc is the
roadmap; `docs/tasks/` holds one spec per step. Work on the lowest-numbered
unfinished task unless told otherwise. Do not start a later step to "get ahead";
later steps depend on earlier ones being right.

`docs/reference/` holds notes on how DOCSIS and HFC plant work, distilled from the
CableLabs and SCTE documents. Its README says which file to load for which task; load
only what the task needs. Don't state a DOCSIS fact from memory when a note covers it.

## Invariants (never violate; if a task seems to require it, stop and ask)

1. **The plant graph is the single source of truth.** Everything downstream
   (design validity, sim results, telemetry, diagnosis) is a pure, deterministic
   derivation of the plant graph + architecture profile + component library +
   environment + fault set + seed.
   No derived value is ever stored as if it were an input.
2. **The sim owns the observation schema.** The telemetry/feature schema is
   defined by what the sim emits. Adapters map vendor data *into* that schema.
   Never add a field to the schema because a vendor exposes it; add it because
   the sim can produce it.
3. **Schemas are versioned, never edited in place.** A schema change is a new
   version with a migration. Every model artifact records the schema version it
   was trained on.
4. **Deterministic given a seed.** Same inputs + same seed = byte-identical
   output. All randomness goes through a seeded RNG passed explicitly. No
   `rand::thread_rng()`, no system time in simulation code.
5. **Physics is parameterized, never scripted.** A fault is physical parameters
   (return loss, position, gain delta). Its telemetry signature is *computed* by
   the sim, never looked up or hand-authored. If you find yourself writing
   "if fault == Suckout then dip MER at f0", stop.
6. **Rust core.** Plant graph, profiles, sim, fault model, diagnosis engine, and
   the MCP server are Rust crates in this workspace. Python is allowed only for
   ML training/evaluation (step 12) and must consume the Rust-generated data
   files; it never reimplements physics.
7. **Four infrastructure primitives only:** Postgres, S3-API object storage via
   `opendal`, a Postgres-backed queue, containers. No cloud SDKs in application
   code. Cloud-specific code lives only under `infra/`.
8. **The MCP server is the API.** No parallel REST API. The web UI is an MCP
   client. Anything the UI can do, an external MCP client can do.
9. **Specs are sourced.** Every component-library entry carries a `source_url`
   and a `spec_confidence` (`datasheet` | `generic` | `operator_supplied`).
   Numbers from memory are tagged `generic`, never `datasheet`.
10. **Subscriber identity is pseudonymous by default.** Modem MACs and
    addresses are hashed at ingest; the twin is keyed by plant location. Only the
    adapter's own secured store holds the addresses it needs to reach a modem.
11. **Synthetic is never presented as real.** Every plant element records whether
    it was designed, built or imported, and every plant and observation records
    whether it is `Synthetic` or `Field`. Results, demos and docs say which they are.

## Testing rules

- Every physics function has a unit test against a hand-computed value from
  `docs/physics.md` (the worked examples there are the test oracle).
- The hand-built street fixture (`fixtures/one_street/`) must always produce a
  valid level budget. A change that breaks it is a bug, not a fixture update.
- Property test for determinism: run the sim twice with the same seed, assert
  byte-identical output.
- Schema tests: every schema has a round-trip serde test and a golden JSON
  sample under `fixtures/schemas/`.

## Style

- `cargo fmt`, `cargo clippy -D warnings`, no `unwrap()` outside tests.
- Units in type names or field names, always: `len_ft`, `freq_mhz`,
  `level_dbmv`, `loss_db`, `temp_c`. A bare `f64` named `level` is a bug.
- Frequencies in MHz, lengths in feet, levels in dBmV, impedance 75 Ω,
  temperatures in °C. Convert at the boundary, never mid-calculation.
- Comments explain physics, not code. Cite `docs/physics.md` section numbers.

## What to do when a task spec is missing or ambiguous

Write the spec first (`docs/tasks/NN-name.md`, same template as the others),
then implement it. Do not implement from the design doc alone.
