---
x-okfw-id: "01a1ff3c-1071-77f7-8152-8c42b1aeab0a"
title: "ADR-0009: Remove the upstream history and keep the legal notices"
type: "ADR"
tags:
  - "adr"
  - "history"
  - "license"
x-okfw-created: "2026-11-21T11:51:28-06:00"
x-okfw-timestamp: "2026-11-21T11:51:28-06:00"
description: "A new root commit replaces the upstream git history; NOTICE and the file headers keep the attribution that Apache-2.0 requires."
---

# ADR-0009: Remove the upstream history and keep the legal notices

**Status:** Accepted
**Date:** 2026-11-21
**Source:** User request: remove each tie to the upstream project, also in the git history.

## Context
grafana-tui derives from grafatui. The git history held 278 upstream commits up to `eccfae7` (upstream pull request #85). The changelog held the upstream 0.1.x sections with links to the upstream repository. Apache-2.0 requires the derived code to keep the copyright notices and the attribution.

## Decision
- **History:** a new root commit, "Initial commit", holds the file tree of `eccfae7`. Each commit of this project goes on top of it. The history has 155 commits, and no upstream commit is reachable. The rewrite changed `main`, `fix/tui-legend-rows` and the tags `v0.2.0` and `v0.3.0`. A check showed that each rewritten ref has the same file tree as before. The four merged branches that kept the old history are gone.
- **Text:** one line in `CHANGELOG.md` replaces the upstream 0.1.x sections. The Makefile, the ZMakefile and `RELEASE_PROCESS.md` drop the note on the first release of the fork.
- **Legal notices stay:** `NOTICE`, the copyright line in `README.md` and the Apache header in each source file. Apache-2.0 requires them.
- **Backup:** `grafana-tui-pre-rewrite-2026-11-21.bundle` in the parent folder of the clone holds the old refs.

## Consequences
- Each clone from before the rewrite needs a new clone, or `git fetch` and `git reset --hard origin/main`.
- The commit IDs in older wiki notes and issues refer to the old history.
- GitHub keeps the commits of each pull request (`refs/pull/N/head`). Only GitHub Support can remove them.

## Alternatives considered
- **Remove the legal notices too.** This breaks Apache-2.0 for the derived code. Rejected.
- **Keep the history.** The user asked for no tie to the upstream project. Rejected.
