---
id: "01a1fd4e-b750-7076-889f-7d400e0eac73"
title: "ISSUE-001: Tooling for the Power of 10 refactor"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "phase-0"
  - "tooling"
  - "closed"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T02:55:52-06:00"
description: "Add the ZMakefile, the gates, the wiki and the characterization snapshots."
---

# ISSUE-001: Tooling for the Power of 10 refactor

**Status/Phase:** Closed · Phase 0

## Description
Set up the branch `nasa-p10-refactor`, the gates and the wiki. Add snapshots that guard the refactor against visual and layout regressions.

## Acceptance Criteria
- [x] The `ZMakefile` has each task of the `Makefile`. `zmake -n` and `make -n` agree.
- [x] `make p10`, `make docs-style` and `make wiki` exist in both files. `make check` runs `p10` and `wiki`.
- [x] `.semscope.cue`, `vocabulary.cue`, `clippy.toml`, `rust-toolchain.toml` and the `[lints]` table exist.
- [x] The wiki holds the conventions, the workflow, the architecture, ADR-0001 to ADR-0005 and the phase issues.
- [x] Snapshot tests record the SVG export and the TUI buffer for the example dashboards, with nested rows and tabs.
- [x] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [Development workflow](/development-workflow--01a1fd4e486d.md)

## Resolution
Done on 2026-11-21. `zmake check` passes: 547 unit tests, 22 CLI tests, line coverage 95.41%, 0 semscope errors, 18 wiki concepts.

- **Snapshots:** `src/snapshot_tests.rs` draws 20 example dashboards to a TUI buffer and to SVG, with fixed synthetic series. The files are in `tests/snapshots/`. The test code is in a `#[cfg(test)] mod tests` block, because semscope does not recognize a file that `main.rs` declares under `#[cfg(test)]`.
- **Tools:** semscope, lexiscope and okfw have no public repository. CI keeps `make lint test test-install cover-check`. Run `make check` before each push ([ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md)).
- **Baseline:** the coverage margin is small (95.41% for a gate of 95%). Each new error branch needs a test.

