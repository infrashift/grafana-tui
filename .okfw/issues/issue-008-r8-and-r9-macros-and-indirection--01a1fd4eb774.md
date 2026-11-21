---
id: "01a1fd4e-b774-7a3d-b23a-98b1e72eb6a9"
title: "ISSUE-008: R8 and R9: macros and indirection"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "open"
  - "phase-7"
  - "power-of-10"
  - "r8"
  - "r9"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T02:52:36-06:00"
description: "Replace the theme macro, the function pointer types and the boxed trait objects."
---

# ISSUE-008: R8 and R9: macros and indirection

**Status/Phase:** Open · Phase 7

## Description
semscope found 1 R8 finding (`theme_spec!`) and 4 R9 findings. `Box<dyn AnnotationProvider>` and `ProviderFuture` use trait objects.

## Acceptance Criteria
- [ ] An enum replaces `Box<dyn AnnotationProvider>`. The futures stay `Send`.
- [ ] No production code has a function pointer type or a `dyn Fn`.
- [ ] A const table replaces `theme_spec!`.
- [ ] ADR-0002 records each remaining `dyn`.
- [ ] `.semscope.cue` turns on NASA-P10-08 and NASA-P10-09.
- [ ] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [Development workflow](/development-workflow--01a1fd4e486d.md)
