---
x-okfw-id: "01a1fd4e-b77f-72f4-867f-1de33b5aa521"
title: "ISSUE-010: ASD-STE100 comments and documentation"
type: "Issue"
x-okfw-kind: "issue"
tags:
  - "issue"
  - "phase-9"
  - "ste-100"
  - "docs"
  - "closed"
x-okfw-created: "2026-11-21T02:52:36-06:00"
x-okfw-timestamp: "2026-11-21T10:09:08-06:00"
description: "Rewrite the comments, the docs and the wiki until lexiscope reports no findings."
---

# ISSUE-010: ASD-STE100 comments and documentation

**Status/Phase:** Closed · Phase 9

## Description
lexiscope found 143 errors and 941 warnings. See [ADR-0004](/decisions/adr-0004-lexiscope-checks-the-asd-ste100-writing-rules--01a1fd4e4882.md).

## Acceptance Criteria
- [x] `make docs-style` passes with `--fail-on note`.
- [x] `make check` runs `docs-style`.
- [x] `DEVELOPMENT.md` and `CONTRIBUTING.md` describe the gates, the ZMakefile, the wiki and the workflow.
- [x] The Makefile, ZMakefile and CI comments follow the same rules.
- [x] `make check` passes.

## Notes
- Add a page or a section on the docs site for the limits of [ADR-0007](/decisions/adr-0007-named-limits-cap-each-input-and-growable-collection--01a1fdaaf533.md) that a user can reach. These are the limits for the file sizes, the series and points, the variables and the layout.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [The workflow guide](/development-workflow--01a1fd4e486d.md)

## Resolution
Done on 2026-11-21. `make docs-style` reports 0 findings in 80 files (1178 at the start of this phase). `make check` and `zmake check` run `docs-style`, and both pass.

- **Work split:** four agents rewrote the code comments (two groups), the user docs and the contributor docs with the wiki. The coordinator rewrote the comments of the demo scripts, the Makefile and the CI files by hand.
- **New page:** `docs/src/content/docs/limits.md` lists each limit that a user can reach, from [ADR-0007](/decisions/adr-0007-named-limits-cap-each-input-and-growable-collection--01a1fdaaf533.md).
- **Workflow docs:** `DEVELOPMENT.md` and `CONTRIBUTING.md` describe the gates, the tools and the two build files. They also describe the Power of 10 rules, the loop for each change and the snapshot tests.
- **Vocabulary:** two new terms: "production" (production code) and "recording" (the Ctrl+E feature).
- **License headers:** the user chose to keep the Apache header. `docs-style` excludes the 35 files that hold it, until lexiscope can skip the header ([ISSUE-012](/issues/issue-012-scan-the-licensed-rust-files-when-lexiscope-skips-license-headers--01a1fed33e26.md)). The comments below each header follow ASD-STE100.
- **Out of scope:** the clap doc comments in `src/cli.rs` are the `--help` text, so they keep their words.

