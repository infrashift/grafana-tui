---
x-okfw-id: "01a1fd4e-b760-7262-aab0-7aed9ae7d121"
title: "ISSUE-004: R2: bounded loops"
type: "Issue"
x-okfw-kind: "issue"
tags:
  - "issue"
  - "phase-3"
  - "power-of-10"
  - "r2"
  - "closed"
x-okfw-created: "2026-11-21T02:52:36-06:00"
x-okfw-timestamp: "2026-11-21T04:01:08-06:00"
description: "Give each production loop a bound that a reader can see."
---

# ISSUE-004: R2: bounded loops

**Status/Phase:** Closed · Phase 3

## Description
semscope found 6 loops with no bound. The float tick loops in `src/export.rs` and `src/ui/panels/graph/autogrid.rs` do not stop when the step is too small.

## Acceptance Criteria
- [x] The tick generators are in one module with `MAX_TICKS`.
- [x] Each `loop` and `while` has a bound, except the event loop. The event loop has a semscope suppression with a reason.
- [x] `.semscope.cue` turns on NASA-P10-02. `while_float` is at `warn`.
- [x] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [The workflow guide](/development-workflow--01a1fd4e486d.md)

## Resolution
Done on 2026-11-21. semscope finds no loop with no bound, and `.semscope.cue` turns on NASA-P10-02. Clippy `while_float` is at `warn`. `zmake check` passes: 576 unit tests, 30 CLI tests, line coverage 95.58%. The 20 snapshot tests pass with no change.

- **Ticks:** `src/ui/panels/graph/ticks.rs` holds each tick generator of the TUI and the export. `interior_ticks` runs `for _ in 0..limit` with `MAX_TICKS = 4096`. It stops when `tick + step` does not grow `tick`, and it gives no ticks for a zero, NaN or infinite step. Before, a step under half an ulp of the tick hung the program.
- **New limits:** `MAX_TICKS = 4096`, `MAX_STEP_CHANGES = 1100` (the time step doubles; no test can reach it) and `MAX_STDERR_DRAIN_BYTES = 16 MiB`. `MAX_ERROR_CHAIN = 16` also bounds `error_chain`, which ends the text with ": …" past the limit.
- **Behavior changes:** an export time range with more than 4096 day ticks (about 11 years) gets a wider time step. A command annotation provider that writes more than 16 MiB to stderr fails with "stderr passed the 16 MiB drain limit". Before, only the timeout stopped it.
- **Deviation:** the TUI event loop in `run_app` has `// semscope: allow(NASA-P10-02)`, as [ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) records.

