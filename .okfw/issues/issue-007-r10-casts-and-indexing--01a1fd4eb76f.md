---
id: "01a1fd4e-b76f-765e-a570-3450b793e282"
title: "ISSUE-007: R10: casts and indexing"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "phase-6"
  - "power-of-10"
  - "r10"
  - "closed"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T04:51:59-06:00"
description: "Replace the as casts and the indexing in production code with checked forms."
---

# ISSUE-007: R10: casts and indexing

**Status/Phase:** Closed · Phase 6

## Description
Clippy found 134 `as` casts, 77 indexing expressions and 25 string slices in production code.

## Acceptance Criteria
- [x] `src/num.rs` holds the checked conversions. It is the one place that uses `as`.
- [x] Production code has no indexing that can panic.
- [x] The cast, indexing and `allow_attributes_without_reason` lints are at `warn`.
- [x] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [Development workflow](/development-workflow--01a1fd4e486d.md)

## Resolution
Done on 2026-11-21. The clippy lints `as_conversions`, the five `cast_*` lints, `indexing_slicing` and `string_slice` are at `warn`, with 0 findings in production code. `zmake check` passes: 614 unit tests, 30 CLI tests, line coverage 95.82%. The 20 snapshot tests pass with no change.

- **`src/num.rs`** holds the checked conversions. A float to an integer maps NaN to 0, clamps, then casts. An integer to a float gives the nearest float. A narrow integer conversion saturates with `try_from`. Tests compare each float function bit for bit with `as`. The 12 `as` casts that remain are all in `num.rs`, and [ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) records them as a deviation.
- **Indexing:** `.get()`, iterators, `split_once` and `Layout::areas` replace the indexing. Where an index fails only when an invariant breaks, a `debug_assert!` with a message and a safe fallback replace it.
- **Behavior for input that is not normal:** a value over 65535 saturates and does not wrap. A negative grid y scrolls to 0. A hex color with a multibyte character no longer panics. `cycle_dashboard` with no dashboards no longer divides by zero.

