---
id: "01a1fd4e-b765-722d-8096-39b16e537575"
title: "ISSUE-005: R7: check each result"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "open"
  - "phase-4"
  - "power-of-10"
  - "r7"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T02:52:36-06:00"
description: "Remove unwrap, expect, unreachable and discarded results from production code."
---

# ISSUE-005: R7: check each result

**Status/Phase:** Open · Phase 4

## Description
semscope found 51 R7 findings. Clippy found 35 `unwrap`, 9 `expect`, 3 `unreachable` and 10 `let _ =` in production code.

## Acceptance Criteria
- [ ] Production code has no `unwrap`, `expect` or `unreachable`.
- [ ] Each `let _ =` handles or reports the result.
- [ ] The SVG writer has no `write!(..).unwrap()`.
- [ ] Dead code behind `#[allow(dead_code)]` is gone. Each remaining `allow` has a `reason`.
- [ ] `.semscope.cue` turns on NASA-P10-07. The R7 clippy lints are at `warn`.
- [ ] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [Development workflow](/development-workflow--01a1fd4e486d.md)
