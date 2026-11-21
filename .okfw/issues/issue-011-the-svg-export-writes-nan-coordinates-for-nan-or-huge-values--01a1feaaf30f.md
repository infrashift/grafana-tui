---
id: "01a1feaa-f30f-718f-9116-e68ae69a2aee"
title: "ISSUE-011: The SVG export writes NaN coordinates for NaN or huge values"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "open"
  - "bug"
  - "export"
created: "2026-11-21T09:12:58-06:00"
timestamp: "2026-11-21T09:12:58-06:00"
description: "A NaN bar gauge value or data near f64::MAX gives NaN coordinates in the SVG export."
---

# ISSUE-011: The SVG export writes NaN coordinates for NaN or huge values

**Status/Phase:** Open · debt

## Description
The SVG export can write `NaN` as a coordinate. ISSUE-009 found two cases with new assertions:
- A NaN series value in a bar gauge gives `width="NaN"` in `render_bar_gauge_row`.
- Data near ±1e308 gives infinite y bounds. `map_y` then gives NaN rows.

A browser skips an element with a NaN coordinate, so the panel loses a bar or a line with no message.

## Acceptance Criteria
- [ ] A bar gauge with a NaN value draws no bar, or an empty bar, with a finite width.
- [ ] Y bounds stay finite for data near ±f64::MAX.
- [ ] The export coordinate assertions reject NaN, and a test covers each case.
- [ ] `make check` passes.

## Related
[ISSUE-009](/issues/issue-009-r5-assertion-density--01a1fd4eb779.md)
