---
x-okfw-id: "01a1fd4e-b756-7211-8758-8b8624bb488e"
title: "ISSUE-002: R4: functions of 60 lines or fewer"
type: "Issue"
x-okfw-kind: "issue"
tags:
  - "issue"
  - "phase-1"
  - "power-of-10"
  - "r4"
  - "closed"
x-okfw-created: "2026-11-21T02:52:36-06:00"
x-okfw-timestamp: "2026-11-21T03:17:16-06:00"
description: "Split each production function longer than 60 lines, with no change to behavior."
---

# ISSUE-002: R4: functions of 60 lines or fewer

**Status/Phase:** Closed · Phase 1

## Description
semscope found 41 functions longer than 60 lines. The longest are `render_graph_panel` in `src/ui/panels/graph/mod.rs` (466 lines) and in `src/export.rs` (325 lines), `draw_ui` (288) and `main` (276). Move the logic that the two graph renderers share into one module first.

## Acceptance Criteria
- [x] The shared graph logic is in one module. The TUI and the export use it.
- [x] Each production function has 60 lines or fewer.
- [x] Context structs replace the `too_many_arguments` allows.
- [x] `.semscope.cue` turns on NASA-P10-04. `too_many_lines` and `excessive_nesting` are at `warn`.
- [x] The snapshot tests pass with no change.
- [x] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [The workflow guide](/development-workflow--01a1fd4e486d.md)

## Resolution
Done on 2026-11-21. semscope finds 0 functions over 60 lines (41 before). `zmake check` passes: 547 unit tests, 30 CLI tests, line coverage 95.44%. The 20 snapshot tests pass with no change.

- **Work split:** four agents split four groups of files in parallel worktrees: graph and export, `ui/`, `main` and the importers, and `app`, `prom` and `annotations`. Each group had no shared file, so the merges had no conflicts.
- **Shared graph logic:** `src/ui/panels/graph/view.rs` holds `GraphView`: bounds, events, cursor values, series colors and threshold values. The TUI and the SVG export use it. `LegendMode` keeps the one difference: the TUI legend lists hidden series, and the export legend does not. The export also uses `cluster_badge` and `nice_grid_step` from the graph module.
- **Context structs:** `TextStyle` (export), `VariableQueryContext` (variables) and `Settings`, `Timing`, `Look`, `DashboardContent` and `LoadedDashboards` (main) replace long argument lists. Two `too_many_arguments` allows are gone. `AppState::new` keeps its allow, because many tests call it.
- **Function count:** semscope counts 839 functions (683 before). The R5 average falls, so ISSUE-009 needs more assertions.
- **Recursion:** the splits keep each recursive call in its original function. ISSUE-003 removes them.
- **Disk:** four parallel Rust builds filled the disk. Use `CARGO_INCREMENTAL=0` for parallel builds.

