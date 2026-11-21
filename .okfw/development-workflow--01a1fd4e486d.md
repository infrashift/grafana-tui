---
id: "01a1fd4e-486d-72b3-b6a4-c1f67fb7dfe0"
title: "Development workflow"
type: "Guide"
tags:
  - "conventions"
  - "workflow"
  - "meta"
created: "2026-11-21T02:52:08-06:00"
timestamp: "2026-11-21T02:52:08-06:00"
description: "The loop for each refactor phase: implement, lint, verify until green, update the wiki, then commit."
---

# Development workflow

Each change follows this loop. Do not report or commit work while a gate fails.

1. **Pick the phase.** Find the first open phase issue with `okfw search --path issues --tag open`.
2. **Tag the issue.** Run `okfw remove-tags <id> open && okfw add-tags <id> in-progress`.
3. **Write one slice of the phase.** Keep each commit small enough to review.
4. **Apply the Power of 10 rules.** Read ADR-0002. Use bounded loops and explicit stacks, not recursion.
5. **Lint.** Run `zmake fmt lint p10` or `make fmt lint p10`.
6. **Verify.** Run `zmake test test-install cover-check docs-style`.
   - The snapshot tests in `src/snapshot_tests.rs` compare the output with `tests/snapshots/`. A refactor must not change the output.
   - For an intended change in the output, run `UPDATE_SNAPSHOTS=1 cargo test snapshot_tests`. Then examine the diff.
7. **Repeat until green.** Fix each failure. Then run all gates again. Do not weaken or delete tests. Do not loosen a gate to get a pass.
8. **Update the wiki during the work.** Write an ADR for each design choice when you make it. File each bug or design debt as an issue.
9. **Close the phase.** Tick each box in the `Acceptance Criteria` section. Add a `Resolution` section. Change the tag `in-progress` to `closed`.
10. **Ratchet the gates.** Remove the rule of the phase from the `enabled: false` list in `.semscope.cue`. Set its clippy lints to `warn` in `Cargo.toml`.
11. **Check the wiki.** Run `zmake wiki`.
12. **Journal.** Add an entry to the daily note of today (`okfw daily`): what changed, what you learned, what is next.
13. **Commit.** Commit the code, the wiki and the docs together. Push the branch.

`zmake check` and `make check` run every gate in one command.
