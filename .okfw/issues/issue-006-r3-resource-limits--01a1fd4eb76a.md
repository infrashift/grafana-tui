---
x-okfw-id: "01a1fd4e-b76a-7468-8dda-ac4f61c8fbf3"
title: "ISSUE-006: R3: resource limits"
type: "Issue"
x-okfw-kind: "issue"
tags:
  - "issue"
  - "phase-5"
  - "power-of-10"
  - "r3"
  - "closed"
x-okfw-created: "2026-11-21T02:52:36-06:00"
x-okfw-timestamp: "2026-11-21T04:34:35-06:00"
description: "Give each growable collection, buffer, input and queue a named limit."
---

# ISSUE-006: R3: resource limits

**Status/Phase:** Closed · Phase 5

## Description
ADR-0002 records the R3 deviation. Named limits cap the memory. Dashboard, config and PEM files have no size limit. The repeats of rows, tabs and panels have no limit.

## Acceptance Criteria
- [x] `src/limits.rs` holds each new limit.
- [x] The program checks each limit where the data comes in.
- [x] A test covers each limit.
- [x] An ADR lists each limit and where the program checks it.
- [x] `make check` passes.

## Related
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) · [ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) · [The workflow guide](/development-workflow--01a1fd4e486d.md)

## Resolution
Done on 2026-11-21. `src/limits.rs` holds 13 new limits. [ADR-0007](/decisions/adr-0007-named-limits-cap-each-input-and-growable-collection--01a1fdaaf533.md) lists each limit, where the program checks it and what the user sees. `zmake check` passes with line coverage 95.74%. The 20 snapshot tests pass with no change, and `validate` accepts each example dashboard.

- **Files:** `limits::read_capped` reads the file metadata first, then reads `limit + 1` bytes at most. A file that grows during the read also gives the error.
- **Prometheus:** `prom::limit_result` cuts the series, the points and the messages of each result. The panel shows a warning for each cut.
- **Variables:** a query variable keeps its first 10,000 values, and the footer shows a notice.
- **Help text:** `--record-max-frames` names its range, 1 to 100000.
- **Docs:** ISSUE-010 adds a page on the docs site with the limits that a user can reach.

