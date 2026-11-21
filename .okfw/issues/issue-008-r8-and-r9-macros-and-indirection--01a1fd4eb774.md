---
id: "01a1fd4e-b774-7a3d-b23a-98b1e72eb6a9"
title: "ISSUE-008: R8 and R9: macros and indirection"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "phase-7"
  - "power-of-10"
  - "r8"
  - "r9"
  - "closed"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T05:02:19-06:00"
description: "Replace the theme macro, the function pointer types and the boxed trait objects."
---

# ISSUE-008: R8 and R9: macros and indirection

**Status/Phase:** Closed · Phase 7

## Description
semscope found 1 R8 finding (`theme_spec!`) and 4 R9 findings. `Box<dyn AnnotationProvider>` and `ProviderFuture` use trait objects.

## Acceptance Criteria
- [x] An enum replaces `Box<dyn AnnotationProvider>`. The futures stay `Send`.
- [x] No production code has a function pointer type or a `dyn Fn`.
- [x] A const table replaces `theme_spec!`.
- [x] ADR-0002 records each `dyn` that stays.
- [x] `.semscope.cue` turns on NASA-P10-08 and NASA-P10-09.
- [x] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [The workflow guide](/development-workflow--01a1fd4e486d.md)

## Resolution
Done on 2026-11-21. semscope finds no R8 or R9 finding, and `.semscope.cue` turns on NASA-P10-08 and NASA-P10-09. `zmake check` passes with line coverage 95.82%. The 20 snapshot tests pass with no change.

- **Design:** [ADR-0008](/decisions/adr-0008-enum-dispatch-replaces-trait-objects-and-function-pointers--01a1fdc48e48.md): the `AnnotationProvider` enum, the `TestProvider` enum for the tests, `VariableLookup`, `StepGrowth`, precomputed panel data for conditions and the `ThemeSpec` role table.
- **Deviation:** `&(dyn std::error::Error + 'static)` stays in `src/prom/tls.rs`, because `Error::source()` gives that type. [ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) records it.
- **Learned:** a function pointer came back in Phase 3 (`ticks.rs`) while the R9 gate was off. Each gate that is off can let a finding back in.

