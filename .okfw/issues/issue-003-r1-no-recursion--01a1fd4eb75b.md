---
id: "01a1fd4e-b75b-72e1-b3f9-166bf56bf92f"
title: "ISSUE-003: R1: no recursion"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "open"
  - "phase-2"
  - "power-of-10"
  - "r1"
  - "layout"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T02:52:36-06:00"
description: "Replace each recursive walk with a bounded explicit stack, with depth and node limits on import."
---

# ISSUE-003: R1: no recursion

**Status/Phase:** Open · Phase 2

## Description
semscope found 24 recursion findings. Most walk the layout tree in `src/dashboard.rs`. Others are in `src/grafana/v2.rs`, `src/grafana/import.rs`, `src/ui/layout.rs`, `src/app/template.rs`, `src/prom/tls.rs` and `src/app/state/refresh.rs`.

## Acceptance Criteria
- [ ] `MAX_LAYOUT_DEPTH` and `MAX_LAYOUT_NODES` exist. The importer rejects a deeper or larger layout with a clear error.
- [ ] One bounded walker replaces the recursive read-only walks.
- [ ] The builders and the layout projection use explicit stacks.
- [ ] An ADR records the walker design.
- [ ] `.semscope.cue` turns on NASA-P10-01.
- [ ] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [Development workflow](/development-workflow--01a1fd4e486d.md)
