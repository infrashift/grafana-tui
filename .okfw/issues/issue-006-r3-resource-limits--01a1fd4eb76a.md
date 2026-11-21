---
id: "01a1fd4e-b76a-7468-8dda-ac4f61c8fbf3"
title: "ISSUE-006: R3: resource limits"
type: "Issue"
kind: "issue"
tags:
  - "issue"
  - "open"
  - "phase-5"
  - "power-of-10"
  - "r3"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T02:52:36-06:00"
description: "Give each growable collection, buffer, input and queue a named limit."
---

# ISSUE-006: R3: resource limits

**Status/Phase:** Open · Phase 5

## Description
ADR-0002 records the R3 deviation. Named limits cap the memory. Dashboard, config and PEM files have no size limit. Repeat expansion has no limit.

## Acceptance Criteria
- [ ] `src/limits.rs` holds each new limit.
- [ ] The program checks each limit where the data comes in.
- [ ] A test covers each limit.
- [ ] An ADR lists each limit and where the program checks it.
- [ ] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [Development workflow](/development-workflow--01a1fd4e486d.md)
