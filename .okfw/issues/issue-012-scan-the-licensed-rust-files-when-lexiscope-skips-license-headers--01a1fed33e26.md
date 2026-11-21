---
id: "01a1fed3-3e26-718f-ae11-4c75738a3239"
title: "ISSUE-012: Scan the licensed Rust files when lexiscope skips license headers"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "open"
  - "ste-100"
  - "tooling"
created: "2026-11-21T09:56:59-06:00"
timestamp: "2026-11-21T09:56:59-06:00"
description: "docs-style excludes the 35 Rust files with the Apache header until lexiscope can skip a license comment block."
---

# ISSUE-012: Scan the licensed Rust files when lexiscope skips license headers

**Status/Phase:** Open · debt

## Description
The Apache license header at the top of 35 Rust files is legal text. It breaks the ASD-STE100 rules: a 37-word sentence, the passive voice, and words such as `compliance`, `governing` and `obtain`. lexiscope cannot skip part of a file. So `make docs-style` excludes each file that holds the header (`STE_LICENSED` in the Makefile and the ZMakefile). The comments below the header follow ASD-STE100, but the gate does not check them.

The user tracks a lexiscope change that skips a license comment block at the top of a file.

## Acceptance Criteria
- [ ] lexiscope skips the license header.
- [ ] `STE_LICENSED` and its `--exclude` flags are gone from the Makefile and the ZMakefile.
- [ ] `make docs-style` passes with the 35 files in the scan.

## Related
[ISSUE-010](/issues/issue-010-asd-ste100-comments-and-documentation--01a1fd4eb77f.md) · [ADR-0004](/decisions/adr-0004-lexiscope-checks-the-asd-ste100-writing-rules--01a1fd4e4882.md)
