# issues

<!-- okfw:generated — maintained by okfw; edits here are overwritten -->

## Concepts

* [ISSUE-001: Tooling for the Power of 10 refactor](issue-001-tooling-for-the-power-of-10-refactor--01a1fd4eb750.md) - Add the ZMakefile, the gates, the wiki and the characterization snapshots.
* [ISSUE-002: R4: functions of 60 lines or fewer](issue-002-r4-functions-of-60-lines-or-fewer--01a1fd4eb756.md) - Split each production function longer than 60 lines, with no change to behavior.
* [ISSUE-003: R1: no recursion](issue-003-r1-no-recursion--01a1fd4eb75b.md) - Replace each recursive walk with a bounded explicit stack, with depth and node limits on import.
* [ISSUE-004: R2: bounded loops](issue-004-r2-bounded-loops--01a1fd4eb760.md) - Give each production loop a bound that a reader can see.
* [ISSUE-005: R7: check each result](issue-005-r7-check-each-result--01a1fd4eb765.md) - Remove unwrap, expect, unreachable and discarded results from production code.
* [ISSUE-006: R3: resource limits](issue-006-r3-resource-limits--01a1fd4eb76a.md) - Give each growable collection, buffer, input and queue a named limit.
* [ISSUE-007: R10: casts and indexing](issue-007-r10-casts-and-indexing--01a1fd4eb76f.md) - Replace the as casts and the indexing in production code with checked forms.
* [ISSUE-008: R8 and R9: macros and indirection](issue-008-r8-and-r9-macros-and-indirection--01a1fd4eb774.md) - Replace the theme macro, the function pointer types and the boxed trait objects.
* [ISSUE-009: R5: assertion density](issue-009-r5-assertion-density--01a1fd4eb779.md) - Add checks on external data and invariant assertions until the average is 2 or more for each function.
* [ISSUE-010: ASD-STE100 comments and documentation](issue-010-asd-ste100-comments-and-documentation--01a1fd4eb77f.md) - Rewrite the comments, the docs and the wiki until lexiscope reports no findings.
* [ISSUE-011: The SVG export writes NaN coordinates for NaN or huge values](issue-011-the-svg-export-writes-nan-coordinates-for-nan-or-huge-values--01a1feaaf30f.md) - A NaN bar gauge value or data near f64::MAX gives NaN coordinates in the SVG export.
* [ISSUE-012: Scan the licensed Rust files when lexiscope skips license headers](issue-012-scan-the-licensed-rust-files-when-lexiscope-skips-license-headers--01a1fed33e26.md) - docs-style excludes the 35 Rust files with the Apache header until lexiscope can skip a license comment block.
* [ISSUE-013: Push and pull request events start no workflow run](issue-013-push-and-pull-request-events-start-no-workflow-run--01a1ff09635a.md) - Since the move to the infrashift account, only a manual workflow_dispatch starts a GitHub Actions run.

