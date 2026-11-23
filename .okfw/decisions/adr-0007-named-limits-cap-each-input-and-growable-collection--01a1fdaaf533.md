---
x-okfw-id: "01a1fdaa-f533-748f-a599-87c9a6c30f81"
title: "ADR-0007: Named limits cap each input and growable collection"
type: "ADR"
tags:
  - "adr"
  - "power-of-10"
  - "r3"
  - "limits"
x-okfw-created: "2026-11-21T04:33:21-06:00"
x-okfw-timestamp: "2026-11-21T04:33:21-06:00"
description: "The table of every resource limit, where the program checks it and what the user sees at the limit."
---

# ADR-0007: Named limits cap each input and growable collection

**Status:** Accepted
**Date:** 2026-11-21
**Source:** Phase 5 of the Power of 10 refactor ([ISSUE-006](/issues/issue-006-r3-resource-limits--01a1fd4eb76a.md)).

## Context
[ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) records the R3 deviation: the program allocates after start-up. A named limit caps each path where external data makes memory grow. The user sees each cut.

## Decision
New limits are in `src/limits.rs`. Older limits stay next to the code that uses them. The program checks each limit where the data comes in, not in the render path.

| Limit | Value | Where the program checks it | At the limit |
|---|---|---|---|
| `MAX_DASHBOARD_BYTES` | 16 MiB | `read_dashboard_file` through `limits::read_capped` | Error with the file and the limit |
| `MAX_CONFIG_BYTES` | 1 MiB | `Config::load` | Error with the file and the limit |
| `MAX_PEM_BYTES` | 1 MiB | `PemFile::read` | Error with the option, the file and the limit |
| `MAX_DASHBOARD_FILES` | 64 | `resolve_dashboard_paths` | Start-up error |
| `MAX_USER_VARS` | 256 | `check_user_vars` (`--var` and the config `vars`) | Start-up error |
| `MAX_DASHBOARD_VARIABLES` | 256 | the importer, for each dashboard with its rows and tabs | Import error |
| `MAX_VARIABLE_VALUES` | 10,000 | `cap_values` after a query variable resolves | Keeps the first values; footer notice |
| `MAX_PANEL_SERIES` | 2,000 | `prom::limit_result` and `PanelResults` | Cut; panel warning |
| `MAX_SERIES_POINTS` | 22,000 | `prom::limit_result` | Keeps the newest points; panel warning |
| `MAX_QUERY_MESSAGES` | 32 | `prom::limit_result` (warnings and infos) | Cut; panel warning |
| `MAX_QUERY_BYTES` | 256 KiB | each `PromClient` query entry point | The query is not sent; error |
| `MAX_SEARCH_CHARS` | 256 | `handle_search_key` | The text stops growing |
| `MAX_RECORD_FRAMES` | 100,000 | `ExportOptions::validate` | Start-up error |
| `MAX_RESPONSE_BYTES` | 64 MiB | the Prometheus client | Error |
| `CACHE_CAPACITY` | 64 windows | the Prometheus query cache | The oldest window goes out |
| `MAX_LAYOUT_DEPTH`, `MAX_LAYOUT_NODES` | 16, 4096 | the importers, the template builder, the CLI | Error, or a footer notice for repeats (ADR-0006) |
| `MAX_TICKS` | 4096 | `ticks.rs` | The grid stops at the limit |
| `MAX_ERROR_CHAIN` | 16 | TLS and error text | The text ends with ": …" |
| `MAX_STDOUT_BYTES`, `MAX_STDERR_BYTES`, `MAX_STDERR_DRAIN_BYTES` | 10 MiB, 64 KiB, 16 MiB | the command annotation provider | Error, or a cut stderr excerpt |
| `MAX_FILE_BYTES` | 16 MiB | the JSONL annotation provider | Error |
| `MAX_EVENTS` | 100,000 | the annotation snapshot | Keeps the newest events |

A test covers each limit. The examples in `examples/` and `tests/fixtures/` are far below each limit.

These paths need no new limit:
- `--query` panels, repeats and the reclaimed panel slots stay within `MAX_LAYOUT_NODES`.
- Section values hold only the sections on screen.
- The maps of variable errors and notices have one key for each panel or variable.
- The program downsamples the points of each panel to 200 for the TUI.
- The byte caps cover the values inside a dashboard or config file.
- `MAX_RESPONSE_BYTES` covers the length of each Prometheus label and message.

## Consequences
- The series, point and message cuts run after the program reads the response. `MAX_RESPONSE_BYTES` caps the peak of one request. The new limits cap what the cache and the panels keep.
- A query over 256 KiB, for example a large "All" regex with many alternatives, gives an error. Prometheus refuses a URL of that size in most set-ups.

## Alternatives considered
- **Limits in the render path.** A check there has no effect on memory. Rejected.
