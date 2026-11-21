---
id: "01a1fd4e-b75b-72e1-b3f9-166bf56bf92f"
title: "ISSUE-003: R1: no recursion"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "phase-2"
  - "power-of-10"
  - "r1"
  - "layout"
  - "closed"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T03:47:16-06:00"
description: "Replace each recursive walk with a bounded explicit stack, with depth and node limits on import."
---

# ISSUE-003: R1: no recursion

**Status/Phase:** Closed · Phase 2

## Description
semscope found 24 recursion findings. Most walk the layout tree in `src/dashboard.rs`. Others are in `src/grafana/v2.rs`, `src/grafana/import.rs`, `src/ui/layout.rs`, `src/app/template.rs`, `src/prom/tls.rs` and `src/app/state/refresh.rs`.

## Acceptance Criteria
- [x] `MAX_LAYOUT_DEPTH` and `MAX_LAYOUT_NODES` exist. The importer rejects a deeper or larger layout with a clear error.
- [x] One bounded walker replaces the recursive read-only walks.
- [x] The builders and the layout in `project_dashboard` use explicit stacks.
- [x] An ADR records the walker design.
- [x] `.semscope.cue` turns on NASA-P10-01.
- [x] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [The workflow guide](/development-workflow--01a1fd4e486d.md)

## Resolution
Done on 2026-11-21. semscope finds no recursion (24 before), and `.semscope.cue` turns on NASA-P10-01. `zmake check` passes: 565 unit tests, 30 CLI tests, line coverage 95.49%. The 20 snapshot tests pass with no change.

- **Design:** [ADR-0006](/decisions/adr-0006-one-bounded-layout-walker-replaces-recursion--01a1fd7e8997.md): `LayoutWalk`, `sections_mut`, frame stacks in each builder, and the limits `MAX_LAYOUT_DEPTH = 16`, `MAX_LAYOUT_NODES = 4096` and `MAX_LAYOUT_STEPS`.
- **Review fixes:** the first version cut repeat copies at the node limit with no message. It also stopped a flat layout of more than 8194 `--query` panels part-way. Now the footer reports a cut repeat, and more `--query` panels than the limit give a start-up error. A test covers each case.
- **False positives:** semscope matched two names, not calls: the `Drop` impl in `src/prom.rs` and `AppState::fetch_panels`. Both now have different names.

