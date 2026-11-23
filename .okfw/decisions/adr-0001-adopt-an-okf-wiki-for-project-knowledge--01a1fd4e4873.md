---
x-okfw-id: "01a1fd4e-4873-74cc-b209-845b7afb46de"
title: "ADR-0001: Adopt an OKF wiki for project knowledge"
type: "ADR"
tags:
  - "adr"
  - "wiki"
  - "tooling"
x-okfw-created: "2026-11-21T02:52:08-06:00"
x-okfw-timestamp: "2026-11-21T02:52:08-06:00"
description: "The project knowledge is in an OKF wiki in .okfw/ that okfw manages and make wiki validates."
---

# ADR-0001: Adopt an OKF wiki for project knowledge

**Status:** Accepted
**Date:** 2026-11-21
**Source:** User request: keep the OKF wiki current during the Power of 10 refactor.

## Context
The project knowledge was in commit messages, `ROADMAP.md` and `DEVELOPMENT.md`. The Power of 10 refactor makes many decisions and has ten phases. The sibling projects zmake and lexiscope keep this kind of knowledge in an OKF wiki that `okfw` manages.

## Decision
- The wiki is in `.okfw/` at the repository root. okfw makes the `index.md` listings.
- The wiki holds the architecture overview, the ADRs, one issue for each phase, and daily notes.
- `make wiki` and `zmake wiki` run `okfw validate --all`. `make check` runs it.

## Consequences
- Each phase commit holds the code, the wiki changes and the docs together.
- A contributor needs `okfw` on the `PATH` to run `make check`.

## Alternatives considered
- GitHub issues and wiki: these are not in the repository, so a commit cannot change the code and the record together.
