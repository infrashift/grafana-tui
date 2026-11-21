---
id: "01a1fdc4-8e48-71b7-bcba-30fb48927319"
title: "ADR-0008: Enum dispatch replaces trait objects and function pointers"
type: "ADR"
tags:
  - "adr"
  - "power-of-10"
  - "r8"
  - "r9"
  - "annotations"
created: "2026-11-21T05:01:19-06:00"
timestamp: "2026-11-21T05:01:19-06:00"
description: "An enum of annotation providers, plain data and small traits replace Box dyn, dyn Fn, function pointers and the theme macro."
---

# ADR-0008: Enum dispatch replaces trait objects and function pointers

**Status:** Accepted
**Date:** 2026-11-21
**Source:** Phase 7 of the Power of 10 refactor ([ISSUE-008](/issues/issue-008-r8-and-r9-macros-and-indirection--01a1fd4eb774.md)).

## Context
Rule R9 forbids function pointer types, and [ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) also forbids `Box<dyn ...>` where an enum works. Rule R8 forbids `macro_rules!` of our own. The annotation providers used `Box<dyn AnnotationProvider>` with a boxed future, `ProviderFuture`. Five functions took a `dyn Fn` or a function pointer. The custom themes used the macro `theme_spec!`.

## Decision
- **Providers:** `AnnotationProvider` is an enum with the variants `File(JsonlFileProvider)`, `Command(CommandProvider)` and `Test(TestProvider)`. Only the test build has `Test`. The inherent `async fn refresh` sends each call to its variant. The compiler checks that the future is `Send`, because `tasks.spawn` needs `Send + 'static`. The test `provider_refresh_futures_are_send` also checks it. A dropped future still kills the command process.
- **Test providers:** `TestProvider` is one enum with the variants `Static`, `Scripted`, `WindowRecorder`, `Recording` and `Handshake`, in a `#[cfg(test)] mod` block of `src/annotations/provider.rs`.
- **Function pointers:**
  - Conditions get a precomputed `&[Option<bool>]` for the data of each panel.
  - The code that replaces the variables uses the trait `VariableLookup`, with one type for each caller. semscope also reports a generic `F: Fn` bound, so a generic is not a solution.
  - The tick generator takes `StepGrowth { TuiTimeSteps, Double }`.
  - The annotation `projection` module takes pairs of an `Option<u32>` and an event.
  - The TLS settings call `config::expand_path` directly.
- **Macro:** `ThemeSpec` lists each field. `role_slots` gives a fixed array of the key, the value and the `Theme` field. The error text and the order of the roles do not change.
- **CLI:** the `KEY=VALUE` parser returns a `String` error. The error text does not change.

## Consequences
- semscope finds no R8 or R9 finding. `.semscope.cue` turns on NASA-P10-08 and NASA-P10-09.
- A new provider kind needs a new enum variant. The compiler shows each `match` that needs it.
- One `dyn` stays: `&(dyn std::error::Error + 'static)` in `src/prom/tls.rs`, which ADR-0002 records.

## Alternatives considered
- **`async-trait` or a boxed future.** Each keeps a trait object. Rejected.
- **Generic providers (`AnnotationState<P>`).** The type parameter spreads to `AppState`. Two production kinds do not need it. Rejected.
