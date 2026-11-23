---
x-okfw-id: "01a1fd4e-b765-722d-8096-39b16e537575"
title: "ISSUE-005: R7: check each result"
type: "Issue"
x-okfw-kind: "issue"
tags:
  - "issue"
  - "phase-4"
  - "power-of-10"
  - "r7"
  - "closed"
x-okfw-created: "2026-11-21T02:52:36-06:00"
x-okfw-timestamp: "2026-11-21T04:13:18-06:00"
description: "Remove unwrap, expect, unreachable and discarded results from production code."
---

# ISSUE-005: R7: check each result

**Status/Phase:** Closed · Phase 4

## Description
semscope found 51 R7 findings. Clippy found 35 `unwrap`, 9 `expect`, 3 `unreachable` and 10 `let _ =` in production code.

## Acceptance Criteria
- [x] Production code has no `unwrap`, `expect` or `unreachable`.
- [x] Each `let _ =` handles or reports the result.
- [x] The SVG writer has no `write!(..).unwrap()`.
- [x] Dead code behind `#[allow(dead_code)]` is gone. Each `allow` that stays has a `reason`.
- [x] `.semscope.cue` turns on NASA-P10-07. The R7 clippy lints are at `warn`.
- [x] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [The workflow guide](/development-workflow--01a1fd4e486d.md)

## Resolution
Done on 2026-11-21. semscope finds no R7 finding (46 before), and `.semscope.cue` turns on NASA-P10-07. The clippy lints `unwrap_used`, `expect_used`, `unreachable`, `unwrap_in_result`, `let_underscore_must_use`, `let_underscore_untyped`, `map_err_ignore` and `allow_attributes_without_reason` are at `warn`. `zmake check` passes with line coverage 95.59%. The 20 snapshot tests pass with no change.

- **SVG writer:** `SvgOut` in `src/export.rs` wraps a `String`. Its methods `put`, `push_str` and `finish` cannot fail, so the 29 `write!(..).unwrap()` calls are gone.
- **Discarded results:** some results have no effect when they fail: `killpg`, a send to a closed channel, and the cleanup of a temporary file. Each such result has a typed name and a comment with the reason. A terminal restore failure goes to stderr. When a pipe is not there, or the stdout limit stops the output, the program now gives an error.
- **Dead code:** the unread fields `Variable.source_path` and `QueryRangeData.result_type` are gone. `visible_panel_rects` is test code now.
- **Allows that stay:** `AppState::new` (`too_many_arguments`: many tests call it) and `LayoutNode` (`large_enum_variant`: `MAX_LAYOUT_NODES` caps the nodes). `tests/themes_cli.rs` and `tests/validate_cli.rs` allow `unwrap_used`, because the rules apply to production code only. Each allow has a `reason`.
- **Behavior:** two error messages change their text. The client no longer reads `resultType` from a range query response.

