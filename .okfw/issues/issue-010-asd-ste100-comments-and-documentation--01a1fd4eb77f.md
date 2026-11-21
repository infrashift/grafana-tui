---
id: "01a1fd4e-b77f-72f4-867f-1de33b5aa521"
title: "ISSUE-010: ASD-STE100 comments and documentation"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "open"
  - "phase-9"
  - "ste-100"
  - "docs"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T02:52:36-06:00"
description: "Rewrite the comments, the docs and the wiki until lexiscope reports no findings."
---

# ISSUE-010: ASD-STE100 comments and documentation

**Status/Phase:** Open · Phase 9

## Description
lexiscope found 143 errors and 941 warnings. See [ADR-0004](/decisions/adr-0004-lexiscope-checks-the-asd-ste100-writing-rules--01a1fd4e4882.md).

## Acceptance Criteria
- [ ] `make docs-style` passes with `--fail-on note`.
- [ ] `make check` runs `docs-style`.
- [ ] `DEVELOPMENT.md` and `CONTRIBUTING.md` describe the gates, the ZMakefile, the wiki and the workflow.
- [ ] The Makefile, ZMakefile and CI comments follow the same rules.
- [ ] `make check` passes.

## Notes
- Add a page or a section on the docs site that lists the limits of [ADR-0007](/decisions/adr-0007-named-limits-cap-each-input-and-growable-collection--01a1fdaaf533.md) that a user can reach: the file sizes, the series and points, the variables and the layout.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [Development workflow](/development-workflow--01a1fd4e486d.md)
