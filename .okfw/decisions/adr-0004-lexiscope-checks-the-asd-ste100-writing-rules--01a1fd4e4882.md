---
id: "01a1fd4e-4882-7086-8058-3af1faa3c4ea"
title: "ADR-0004: lexiscope checks the ASD-STE100 writing rules"
type: "ADR"
tags:
  - "adr"
  - "ste-100"
  - "docs"
  - "tooling"
created: "2026-11-21T02:52:08-06:00"
timestamp: "2026-11-21T02:52:08-06:00"
description: "make docs-style runs lexiscope with the ste-100 ruleset and vocabulary.cue on the docs, the wiki and the code comments."
---

# ADR-0004: lexiscope checks the ASD-STE100 writing rules

**Status:** Accepted
**Date:** 2026-11-21
**Source:** User request: write the comments and the documentation in ASD-STE100 Simplified Technical English. lexiscope checks the text.

## Context
The documentation is the `README.md`, the docs site in `docs/`, the example READMEs, the contributor guides, the wiki and the code comments. On `main` at 0a74e24, lexiscope found 143 errors and 941 warnings in these files.

## Decision
- `make docs-style` runs lexiscope with the `ste-100` ruleset and `vocabulary.cue`. Each finding fails, as in the lexiscope self-check (`--fail-on note`).
- The scan covers `README.md`, `DEVELOPMENT.md`, `CONTRIBUTING.md`, `RELEASE_PROCESS.md`, `ROADMAP.md`, `GRAFANA_COMPATIBILITY.md`, `examples/`, `docs/src/content/docs`, `.okfw/`, `src/` and `tests/`. It skips the daily notes and the generated indexes.
- `CHANGELOG.md`, `LICENSE`, `NOTICE` and `CODE_OF_CONDUCT.md` are out of scope. The CLI `--help` text and the UI messages are out of scope.
- `vocabulary.cue` lists the project terms, each with a reason. Add a term only when it names a concept of the project, a tool or a standard. Rewrite other prose.
- `make check` runs `docs-style` after the ASD-STE100 phase (ISSUE-010).

## Consequences
- A contributor needs `lexiscope` on the `PATH` to run `make docs-style`.
- lexiscope does not read Makefile, ZMakefile or YAML comments. Write them to the same rules by hand.
