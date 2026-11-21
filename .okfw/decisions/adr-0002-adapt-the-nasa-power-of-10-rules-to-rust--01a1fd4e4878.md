---
id: "01a1fd4e-4878-795e-a03e-6420e079a308"
title: "ADR-0002: Adapt the NASA Power of 10 rules to Rust"
type: "ADR"
tags:
  - "adr"
  - "power-of-10"
  - "safety"
created: "2026-11-21T02:52:08-06:00"
timestamp: "2026-11-21T02:52:08-06:00"
description: "The production code follows the ten rules with Rust adaptations for rules 3, 5 and 9, and this ADR records each deviation."
---

# ADR-0002: Adapt the NASA Power of 10 rules to Rust

**Status:** Accepted
**Date:** 2026-11-21
**Source:** User request: refactor grafana-tui to follow the NASA Power of 10 rules for safety-critical software. The user selected the Rust adaptation for rules 3 and 9, and the assertion split for rule 5.

## Context
The Power of 10 rules (Holzmann, NASA/JPL, 2006) are for C. grafana-tui is Rust. It uses tokio, reqwest, serde and ratatui. These crates allocate memory after start-up and use trait objects. A literal application of rules 3 and 9 removes these crates.

## Decision
The rules apply to production code. Test code (`#[cfg(test)]` modules and `tests/`) is exempt, but it must compile with no warnings.

| Rule | Adaptation for Rust |
|---|---|
| R1 Simple control flow | No direct or indirect recursion. Tree walks use explicit stacks with a fixed capacity (ADR-0006). |
| R2 Loop bounds | Each loop has a bound that a reader can see: a `for` over a range or a bounded collection, or a counter with a limit. The event loop in `src/app/event_loop.rs` is the one loop that runs until the user quits. |
| R3 No allocation after start-up | **Deviation.** The program allocates. A named limit caps each growable collection, buffer, input and queue. Input over a limit is rejected or cut (ADR-0007). |
| R4 Short functions | 60 lines or fewer for each function. |
| R5 Assertions | An average of 2 or more for each function of more than 5 lines. A check on external data (files, Prometheus responses, CLI input) returns an error with `ensure!` or `bail!`. A check on an internal invariant uses `debug_assert!`. |
| R6 Smallest scope | No mutable global state. Declare each value near its first use. |
| R7 Check return values | No `unwrap`, `expect` or `let _ =` on a result in production code. Each error goes to the caller or to the user. |
| R8 Limited preprocessor | No `macro_rules!` of our own. `#[cfg]` is limited to `test`, `unix` and `windows`. |
| R9 Restricted pointers | `unsafe` is forbidden, so the code has no raw pointers. No function pointer types and no `Box<dyn ...>` where an enum works. Closures in iterator adapters are allowed. Each remaining `dyn` is a deviation in this table. |
| R10 No warnings | `make lint` denies each clippy warning. `make p10` runs semscope. The toolchain is pinned. |

### Recorded deviations
| Location | Rule | Reason |
|---|---|---|
| Whole program | R3 | The dependencies allocate. Limits cap the memory (ADR-0007). |
| `src/app/event_loop.rs` `run_app` | R2 | The TUI event loop runs until the user quits. |
| semscope NASA-P10-05 | R5 | semscope averages over every function, also the one-line helpers. The user chose to count the functions of more than 5 lines. `scripts/p10-assert-density.py` checks that average, and `make p10` runs it. The semscope rule stays off. |
| `src/num.rs` | R10 | The one place with `as` casts. Each function maps NaN, clamps, then casts, and tests compare it bit for bit with `as`. Each has an `#[allow]` with a reason. |
| `LayoutNode` in `src/grafana/model.rs` | R3 | `large_enum_variant` is allowed: `MAX_LAYOUT_NODES` caps the nodes, and a `Box` adds one allocation for each panel. |
| `AppState::new` | R4 | `too_many_arguments` is allowed: the constructor takes each start-up setting, and many tests call it. |
| `src/prom/tls.rs` (`RustlsFailure::find`, `find_rustls_error`, `error_chain`) | R9 | These take `&(dyn std::error::Error + 'static)`, the type that `Error::source()` gives. A walk of an error chain needs it. |

The phases add rows to this table when they find a deviation.

## Consequences
- The refactor runs in phases. Each phase clears one rule and turns its gate on (ADR-0003).
- semscope reports the R3 findings as warnings, not errors.

## Alternatives considered
- **Literal rules 3 and 9.** This is a rewrite with no async runtime, no HTTP client and no serde. Rejected.
- **Pre-allocated buffers in the render path.** More work and more risk of regressions for a terminal tool. Rejected for now.
