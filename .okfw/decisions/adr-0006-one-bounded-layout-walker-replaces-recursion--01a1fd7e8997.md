---
id: "01a1fd7e-8997-7c2d-af6e-0734c816ea05"
title: "ADR-0006: One bounded layout walker replaces recursion"
type: "ADR"
tags:
  - "adr"
  - "power-of-10"
  - "r1"
  - "layout"
created: "2026-11-21T03:44:50-06:00"
timestamp: "2026-11-21T03:44:50-06:00"
description: "Explicit stacks with depth, node and step limits replace the 24 recursive walks of the layout tree and other chains."
---

# ADR-0006: One bounded layout walker replaces recursion

**Status:** Accepted
**Date:** 2026-11-21
**Source:** Phase 2 of the Power of 10 refactor ([ISSUE-003](/issues/issue-003-r1-no-recursion--01a1fd4eb75b.md)).

## Context
Rule R1 forbids recursion ([ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md)). The layout tree (`DashboardLayoutItem`: `Row`, `Tabs`, `Panel`, `AutoGrid`) has rows and tabs that hold child lists. On `main`, 24 functions walked the tree or a chain with recursion. A deep or large dashboard file had no limit.

## Decision
- **Limits** in `src/dashboard.rs`:
  - `MAX_LAYOUT_DEPTH = 16` levels of child lists. The top-level list is level 1. The deepest example uses 3 levels, and a dashboard set adds 1.
  - `MAX_LAYOUT_NODES = 4096` rows, tab groups, tabs, auto grids and panels. The largest example has 46.
  - `MAX_LAYOUT_STEPS = 2 * MAX_LAYOUT_NODES + 2` is the visible bound of each builder loop.
- **Import checks.** The V2 parser, the Classic normalizer and the importer reject a deeper or larger layout with an error that names the limit. The importer counts across a dashboard set, with the tab group of the set.
- **Read-only walks** use `LayoutWalk` in `src/dashboard/walk.rs`. It is an `Iterator` of `WalkEvent { item, depth, level }` in pre-order. `WalkScope::All` visits each item. `WalkScope::Visible` skips collapsed rows and inactive tabs. Each frame holds a `slice::Iter` of items, the tabs that the walk has not visited, and a depth. The stack holds `MAX_LAYOUT_DEPTH` frames or fewer. `truncated()` tells when the walk skipped items. A row with a hidden header keeps its children at its own depth, as before.
- **Mutable walks** use `sections_mut`: an explicit stack of `&mut Vec<DashboardLayoutItem>`. It gives the collapsed flag of each row and the active tab of each tab group. No `unsafe` code is necessary.
- **Builders** (the importer, the V2 parser, the Classic normalizer, the template builder and the TUI layout in `project_dashboard`) use explicit frame stacks. A frame holds the parent that is not complete, and the builder completes it when the child frame closes. Panel, row and tab ids keep their pre-order.
- **Repeats.** The template builder makes a copy after the first only while the layout stays at `MAX_LAYOUT_NODES` or fewer. The first copy of each item always stays. When the limit stops a repeat, the footer shows "Repeats stopped at the limit of 4096 layout items".
- **CLI.** More `--query` panels than `MAX_LAYOUT_NODES` give an error at start-up.
- **Other chains.** `find_rustls_error` walks the `source()` chain with `MAX_ERROR_CHAIN = 16`. The refresh pipeline renames the free function `query_panel_jobs`, because semscope matched it to the method `AppState::fetch_panels`. The `Drop` impl in `src/prom.rs` binds the waiters to a local, because semscope matched the `drop` call to the `Drop::drop` method.

## Consequences
- semscope finds no recursion. `.semscope.cue` turns on NASA-P10-01.
- `value_is_all` no longer looks for `$__all` inside a nested array of an array. Grafana does not write nested arrays there.
- A dashboard at exactly 16 levels imports alone, but not in a dashboard set.

## Alternatives considered
- **Recursion with a depth counter.** The depth is then bounded, but R1 still forbids the call cycle, and the stack size depends on the compiler. Rejected.
- **One walker with a closure for each visit.** R9 forbids function pointers, and an enum scope covers the two cases. Rejected.
