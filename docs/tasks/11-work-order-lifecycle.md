# Task 11 — Work order lifecycle

**Build-order step:** 11
**Depends on:** Tasks 05, 07
**Produces:** `crates/twin/src/lifecycle.rs`, and tools for the lifecycle, design and migration on the MCP server.
**Load:** `CLAUDE.md`; `docs/DESIGN.md` "Label strategy".

## Goal

A work order goes Proposed → Dispatched → Completed → Verified or NotVerified, and the last step is decided by telemetry, not by anyone saying it's done. A wrong diagnosis should fail here, visibly.

## Deliverables

1. **States.** Only those transitions, with anything else refused.
2. **Evidence at issue.** Who was affected when the order was written, kept beside it. It's a record of what was seen, so it's an input from then on, not a derived value.
3. **Simulated repair.** A crew works on what the order names. A fault on that component is gone, and a fault anywhere else is still there, however sure the diagnosis was.
4. **Verification.** Fresh telemetry: every modem affected at issue has cleared, and the plant no longer shows the fault's upstream or RPD symptoms. A verified order yields a label: cause, component, parameters, and that telemetry confirmed it, which is the primary label source (`docs/DESIGN.md`).
5. **Tools.** `dispatch_work_order`, `complete_work_order`, `verify_work_order`, `list_verified_labels`, and for steps 8 and 10: `generate_layout`, `design_region`, `mark_built`, `plan_migration`, `apply_migration`.

## Tests

- The right fix on both profiles: symptoms clear, and it's `Verified`.
- The wrong component: symptoms stay, and it's `NotVerified`, with no label.
- A compound fault with one of two repaired: `NotVerified` if its modems overlap, and the second order then clears it.
- Transitions out of order are refused, and an unknown id is an error.
- All of it through `tools/call`.
- Design to built through tools: a generated region becomes nodes, marked `Built`, and telemetry runs on it. A migration applied through tools moves a node's profile.

## Out of scope

Real dispatch and ticketing (v3). Fine-tuning on labels (v2): they're exported here, not used.

## Acceptance

`cargo test --workspace` green, clippy clean.
