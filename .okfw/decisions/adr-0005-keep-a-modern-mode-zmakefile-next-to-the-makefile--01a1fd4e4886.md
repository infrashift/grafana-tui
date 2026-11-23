---
x-okfw-id: "01a1fd4e-4886-7b5b-b45d-e0fe4eff61dd"
title: "ADR-0005: Keep a modern-mode ZMakefile next to the Makefile"
type: "ADR"
tags:
  - "adr"
  - "build"
  - "tooling"
x-okfw-created: "2026-11-21T02:52:08-06:00"
x-okfw-timestamp: "2026-11-21T02:52:08-06:00"
description: "The ZMakefile has the same tasks as the Makefile in zmake modern mode, and GNU Make keeps the Makefile."
---

# ADR-0005: Keep a modern-mode ZMakefile next to the Makefile

**Status:** Accepted
**Date:** 2026-11-21
**Source:** User request: add a ZMakefile in the `modern` syntax of zmake, and keep the Makefile.

## Context
zmake reads a `ZMakefile` in its `modern` mode. This mode has no `.PHONY` list: `@name:` declares a task. GNU Make cannot read a `ZMakefile` with recipe lines that start with spaces.

## Decision
- The `ZMakefile` has the same variables and targets as the `Makefile`. Each target is a task (`@name:`), with a `## name: description` comment for `zmake help`.
- `docs/node_modules` stays a file target, because its name has a slash and its recipe uses `$@`.
- zmake reads the `ZMakefile` first. GNU Make reads the `Makefile`. CI uses GNU Make.
- When you change one file, change the other. `zmake -n <task>` and `make -n <task>` print the same recipe lines. Only the indent of a line after a `\` is different.

## Consequences
- Two files hold the build tasks, so a change can miss one of them.

## Alternatives considered
- **Only a ZMakefile.** CI and contributors without zmake then cannot build. Rejected.
