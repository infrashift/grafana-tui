---
id: "01a1fd4e-b779-7e30-9661-c442995c8318"
title: "ISSUE-009: R5: assertion density"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "phase-8"
  - "power-of-10"
  - "r5"
  - "closed"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T09:46:29-06:00"
description: "Add checks on external data and invariant assertions until the average is 2 or more for each function."
---

# ISSUE-009: R5: assertion density

**Status/Phase:** Closed · Phase 8

## Description
semscope found 1 assertion in 683 functions. This phase comes last in the code work, because the splits add functions.

## Acceptance Criteria
- [x] The semscope R5 average is 2.0 or more.
- [x] Each assertion has a message.
- [x] `.semscope.cue` turns on NASA-P10-05. `missing_assert_message` is at `warn`.
- [x] CI runs `make p10`.
- [x] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [The workflow guide](/development-workflow--01a1fd4e486d.md)

## Resolution
Done on 2026-11-21. `scripts/p10-assert-density.py` counts 1456 assertions in 722 functions of more than 5 lines (average 2.02; 110 before). `make p10` runs it after semscope. Clippy `missing_assert_message` is at `warn`. `zmake check` passes: 617 unit tests, 30 CLI tests, line coverage 96.01%. The 20 snapshot tests pass with no change.

- **Gate:** the user chose to count the functions of more than 5 lines. semscope can average only over every function, so its rule NASA-P10-05 stays off. [ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) records the deviation.
- **Form:** `debug_assert!` with a message checks internal invariants. `ensure!` and `bail!` check external data. Typical checks: an index in range, bounds in order, a positive step and a limit held. Others: a stack empty at the end of a build, and a refresh of the current `generation`.
- **Work split:** four agents did four groups of files. Two worked at a time and shared one build directory.
- **Findings:** the assertions found the NaN coordinates of the SVG export ([ISSUE-011](/issues/issue-011-the-svg-export-writes-nan-coordinates-for-nan-or-huge-values--01a1feaaf30f.md), closed). Six assumptions were wrong for valid test input, and the agents removed or changed those assertions.
- **Error paths:** an empty Prometheus variable query fails at once with "the variable query is empty". A config with more than 64 dashboard files or 256 variables fails on load.
- **Margin:** the average is 2.02. A new function of more than 5 lines needs its own assertions.

