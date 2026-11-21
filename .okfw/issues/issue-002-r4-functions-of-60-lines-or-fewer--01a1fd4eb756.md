---
id: "01a1fd4e-b756-7211-8758-8b8624bb488e"
title: "ISSUE-002: R4: functions of 60 lines or fewer"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "open"
  - "phase-1"
  - "power-of-10"
  - "r4"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T02:52:36-06:00"
description: "Split each production function longer than 60 lines, with no change to behavior."
---

# ISSUE-002: R4: functions of 60 lines or fewer

**Status/Phase:** Open · Phase 1

## Description
semscope found 41 functions longer than 60 lines. The longest are `render_graph_panel` in `src/ui/panels/graph/mod.rs` (466 lines) and in `src/export.rs` (325 lines), `draw_ui` (288) and `main` (276). Move the logic that the two graph renderers share into one module first.

## Acceptance Criteria
- [ ] The shared graph logic is in one module. The TUI and the export use it.
- [ ] Each production function has 60 lines or fewer.
- [ ] Context structs replace the `too_many_arguments` allows.
- [ ] `.semscope.cue` turns on NASA-P10-04. `too_many_lines` and `excessive_nesting` are at `warn`.
- [ ] The snapshot tests pass with no change.
- [ ] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [Development workflow](/development-workflow--01a1fd4e486d.md)
