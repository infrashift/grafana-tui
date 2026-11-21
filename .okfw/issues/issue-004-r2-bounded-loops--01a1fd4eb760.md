---
id: "01a1fd4e-b760-7262-aab0-7aed9ae7d121"
title: "ISSUE-004: R2: bounded loops"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "open"
  - "phase-3"
  - "power-of-10"
  - "r2"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T02:52:36-06:00"
description: "Give each production loop a bound that a reader can see."
---

# ISSUE-004: R2: bounded loops

**Status/Phase:** Open · Phase 3

## Description
semscope found 6 loops with no bound. The float tick loops in `src/export.rs` and `src/ui/panels/graph/autogrid.rs` do not stop when the step is too small.

## Acceptance Criteria
- [ ] The tick generators are in one module with `MAX_TICKS`.
- [ ] Each `loop` and `while` has a bound, except the event loop. The event loop has a semscope suppression with a reason.
- [ ] `.semscope.cue` turns on NASA-P10-02. `while_float` is at `warn`.
- [ ] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [Development workflow](/development-workflow--01a1fd4e486d.md)
