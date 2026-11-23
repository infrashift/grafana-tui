---
x-okfw-id: "01a1feaa-f30f-718f-9116-e68ae69a2aee"
title: "ISSUE-011: The SVG export writes NaN coordinates for NaN or huge values"
type: "Issue"
x-okfw-kind: "issue"
tags:
  - "issue"
  - "bug"
  - "export"
  - "closed"
x-okfw-created: "2026-11-21T09:12:58-06:00"
x-okfw-timestamp: "2026-11-21T09:37:00-06:00"
description: "A NaN bar gauge value or data near f64::MAX gives NaN coordinates in the SVG export."
---

# ISSUE-011: The SVG export writes NaN coordinates for NaN or huge values

**Status/Phase:** Closed · debt

## Description
The SVG export can write `NaN` as a coordinate. ISSUE-009 found two cases with new assertions:
- A NaN series value in a bar gauge gives `width="NaN"` in `render_bar_gauge_row`.
- Data near ±1e308 gives infinite y bounds. `map_y` then gives NaN rows.

A browser skips an element with a NaN coordinate, so the panel loses a bar or a line with no message.

## Acceptance Criteria
- [x] A bar gauge with a NaN value draws no bar, or an empty bar, with a finite width.
- [x] Y bounds stay finite for data near ±f64::MAX.
- [x] The export coordinate assertions reject NaN, and a test covers each case.
- [x] `make check` passes.

## Related
[ISSUE-009](/issues/issue-009-r5-assertion-density--01a1fd4eb779.md)

## Resolution
Done on 2026-11-21.

- `bounds_fraction` in `src/export.rs` maps a value to its place between the bounds with halves, so the difference stays finite near `f64::MAX`. NaN gives 0. `map_x` and `map_y` use it. For normal values the result is the same bit for bit, because the result of a divide by 2 is exact. The 20 snapshot tests pass with no change.
- `calculate_y_bounds` computes the padding from halves and clamps each bound to `f64::MIN` and `f64::MAX`.
- A NaN bar gauge value draws an empty bar, as in the TUI. The value label still shows "NaN".
- `write_rect` and `AxisLabels::claim` now reject NaN in their assertions.
- New tests: `nan_bar_gauge_values_draw_an_empty_bar`, `data_near_the_float_limits_keeps_finite_coordinates` and `data_near_the_float_limits_gives_finite_bounds`.

