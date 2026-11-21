---
id: "01a1fd4e-487d-746f-8c08-4609bc8863b2"
title: "ADR-0003: semscope and clippy enforce the Power of 10 rules"
type: "ADR"
tags:
  - "adr"
  - "power-of-10"
  - "lint"
  - "tooling"
created: "2026-11-21T02:52:08-06:00"
timestamp: "2026-11-21T02:52:08-06:00"
description: "make p10 runs semscope with a ratchet overlay, and the clippy lints table turns on each rule when its phase closes."
---

# ADR-0003: semscope and clippy enforce the Power of 10 rules

**Status:** Accepted
**Date:** 2026-11-21
**Source:** The Power of 10 refactor plan.

## Context
Some Power of 10 rules have no clippy lint: recursion, loop bounds and assertion density. semscope is a static checker from the infrashift tools. Its ruleset `nasa-p10` checks all ten rules on Rust code and skips the test code. On `main` at 0a74e24, semscope found 24 R1, 6 R2, 41 R4, 51 R7, 1 R8 and 4 R9 errors. It also found 162 R3 warnings, and 1 assertion in 683 functions.

## Decision
- `make p10` runs `semscope scan --ruleset nasa-p10 --ruleset .semscope.cue --fail-level error src`. `make check` runs it.
- `.semscope.cue` sets R3 to a warning (ADR-0002). It adds `ensure!` and `bail!` to the R5 assertion list.
- `scripts/p10-assert-density.py` checks R5: an average of 2 or more assertions in the functions of more than 5 lines. `make p10` runs it after semscope. The semscope rule NASA-P10-05 stays off, because it also counts the one-line helpers (ADR-0002).
- `.semscope.cue` turns off each rule that a phase has not cleared. The phase that clears a rule removes its line. This is the ratchet.
- Clippy adds lints for the rules in the `[lints.clippy]` table of `Cargo.toml`, with the limits in `clippy.toml`. A lint at `allow` waits for its phase. The phase sets it to `warn`, and `-D warnings` makes it a gate.
- `src/main.rs` allows the production-only lints in the test build.
- `rust-toolchain.toml` pins Rust 1.99.0, so a new stable release cannot add new lint failures.
- Suppress one semscope finding with `// semscope: allow(NASA-P10-0N) <reason>`. Each clippy `allow` has a `reason`.

## Consequences
- A contributor needs `semscope` on the `PATH` to run `make check`.
- semscope, lexiscope and okfw have no public repository yet, so the CI runners cannot install them. CI runs `make lint test test-install cover-check`, and the clippy lints carry the rules there. Run `make check` before each push. Add the three gates to `.github/workflows/ci.yml` when the tools have releases.
- The gates get stricter in each phase, and no phase makes them weaker.

## Alternatives considered
- **A custom checker crate with `syn`.** It duplicates semscope. Rejected.
