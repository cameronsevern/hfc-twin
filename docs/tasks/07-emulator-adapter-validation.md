# Task 07 — Device emulator, adapter and validation

**Build-order step:** 7
**Depends on:** Tasks 01, 04
**Produces:** `crates/pnm-wire` (SNMP's BER, TFTP, PNM files), `crates/emulator` (sim-driven devices),
`crates/adapter` (the one v1 adapter), and `crates/sim/tests/validation.rs` with `docs/validation.md`.
**Load:** `CLAUDE.md`; `docs/reference/collection-management.md`; `docs/physics.md` §12.

## Goal

Telemetry travels the path it would from real hardware, and the sim gets its only outside check before anything is
trained on it. The adapter's interface gets shaped by a real protocol before anyone writes a second one.

## Deliverables

1. **`pnm-wire`.** SNMPv2c GET, GETNEXT and SET in BER. TFTP write with the block size option. RxMER, channel estimate, US
   pre-eq, spectrum, CCAP US RxMER and `DocsEqualizerData`, at the offsets in the reference notes, tested against their
   worked examples.
2. **`emulator`.** One UDP SNMP agent per modem and one for the CMTS. Setting a test's enable makes the modem check it
   has somewhere to upload and isn't already busy, then simulate, build the file and upload it as a TFTP client. Its
   `sysDescr` says it's simulated.
3. **`adapter`.** Discovers modems through the CMTS's table, hashes MACs on arrival, runs tests, takes files, walks status
   objects, and maps all of it into `ModemObservation` and `ServiceGroupObservation`. A budget limits how many modems run at
   once per CMTS. Origin comes from what devices report.
4. **Binding and pseudonyms.** `bind_plant` ties plant locations to keyed hashes. `AddressStore` is the only place a
   MAC or address is held, and it stays in the adapter crate (invariant 10).
5. **Validation suite.** §12, in tests, with the result table in `docs/validation.md`.

## Tests

- Codecs: exact bytes against a known SNMP GET and the notes' spec examples, garbage as errors, not panics.
- Contract: every field a modem reports crosses the wire and matches the sim to that field's encoding, on both
  profiles, with both generations.
- The adapter marks data `Synthetic`, and no MAC reaches the plant or an observation.
- A modem refuses a test with nowhere to upload, and refuses writes to read-only objects. A malformed datagram doesn't
  stop an agent. A silent device is a timeout, not a hang.
- Diagnosis from telemetry that crossed the wire gets a connector fault right.

## Out of scope

Real devices, vCore and other vendor adapters, plant-active transponders, UTSC, SNMP write security: v2.

## Acceptance

`cargo test --workspace` green, clippy clean, and `docs/validation.md` says plainly which rows pass.

## Deferred

- **The test-table OIDs are placeholders.** No source in `docs/reference` prints them, so they're under the right roots and all in
  `pnm_wire::oids`. Check them against the DOCS-PNM-MIB files first when a real modem is available. The file formats and object
  names are sourced; these arcs aren't.
- **What the adapter doesn't read yet:** PLC, NCP and partial channel objects, per-subcarrier upstream RxMER (a CCAP test), UTSC,
  FBC over SNMP on D3.0, and transponders. Those fields are `None`, as they'd be from a device that didn't offer them.
- **IPv6, and CCAP bulk configuration** via `docsPnmBulkDataTransferCfgTable`.

## Decisions made during the task

- **No SNMP or TFTP crate.** The subset is small, and these codecs are what a real device's bytes will first test.
- **The emulator's port column** exists only because many emulated modems share one address. It's under a private arc so it
  can't pass for a real object, and a real modem falls back to UDP 161.
- **`ofdma` ifIndexes are known to the adapter by convention** (4000 + k), and OFDM channels are matched by subcarrier zero. A
  real adapter reads `UsProfileIucList` for the first and keeps the second.
- **Pre-eq now folds the channel through the matched filter**, which is what a T-spaced equalizer sees. It came out of the
  validation suite and is in `docs/physics.md` §5.3. It moved the §5.3 oracle from 29.0 to 29.3 dB.
- **The band-edge group delay table is fitted:** 36 ns per active at the edges, from §12's N+5 row.
