---
id: "01a1fd4e-b76f-765e-a570-3450b793e282"
title: "ISSUE-007: R10: casts and indexing"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "open"
  - "phase-6"
  - "power-of-10"
  - "r10"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T02:52:36-06:00"
description: "Replace the as casts and the indexing in production code with checked forms."
---

# ISSUE-007: R10: casts and indexing

**Status/Phase:** Open · Phase 6

## Description
Clippy found 134 `as` casts, 77 indexing expressions and 25 string slices in production code.

## Acceptance Criteria
- [ ] `src/num.rs` holds the checked conversions. It is the one place that uses `as`.
- [ ] Production code has no indexing that can panic.
- [ ] The cast, indexing and `allow_attributes_without_reason` lints are at `warn`.
- [ ] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [Development workflow](/development-workflow--01a1fd4e486d.md)
