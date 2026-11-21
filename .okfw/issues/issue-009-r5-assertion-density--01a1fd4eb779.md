---
id: "01a1fd4e-b779-7e30-9661-c442995c8318"
title: "ISSUE-009: R5: assertion density"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "open"
  - "phase-8"
  - "power-of-10"
  - "r5"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T02:52:36-06:00"
description: "Add checks on external data and invariant assertions until the average is 2 or more for each function."
---

# ISSUE-009: R5: assertion density

**Status/Phase:** Open · Phase 8

## Description
semscope found 1 assertion in 683 functions. This phase comes last in the code work, because the splits add functions.

## Acceptance Criteria
- [ ] The semscope R5 average is 2.0 or more.
- [ ] Each assertion has a message.
- [ ] `.semscope.cue` turns on NASA-P10-05. `missing_assert_message` is at `warn`.
- [ ] CI runs `make p10`.
- [ ] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [Development workflow](/development-workflow--01a1fd4e486d.md)
