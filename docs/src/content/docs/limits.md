---
title: Limits
description: The limits on the files, variables, queries, series, recordings and layouts of grafana-tui, and what the TUI shows at each limit.
---

grafana-tui sets a limit on each input that can make its memory grow. Each
limit is much larger than the data of a typical dashboard. The examples in the
repository are far below each limit.

At a limit, grafana-tui does one of these things:

- It stops at start-up or at import, and shows an error. The error gives the
  name of the file or the option, and the value of the limit.
- It keeps a part of the data, and shows a warning or a notice. So you can see
  each cut.

## File Sizes

| File | Limit | At the limit |
|---|---|---|
| Dashboard file (`--grafana-json`, JSON or YAML) | 16 MiB | Error: `reading grafana dashboard: <file>: the file is larger than the limit of 16777216 bytes` |
| Config file | 1 MiB | Error with the config file and the limit |
| PEM file (`ca_cert`, `client_cert`, `client_key`) | 1 MiB | Error with the option, the file and the limit |

A large public dashboard has about 1 MiB. A full public CA bundle has about
220 KiB.

## Dashboards and Variables

| Item | Limit | At the limit |
|---|---|---|
| Dashboard files in one run (`--grafana-json`, or `grafana_json` in the config) | 64 | Start-up error: `<n> dashboard files are more than the limit of 64` |
| `--var` pairs on the command line | 256 | Start-up error: `<n> --var values are more than the limit of 256` |
| Variables in `vars` of the config | 256 | Start-up error: `<n> config vars are more than the limit of 256` |
| Variables in one dashboard, with the variables of its rows and tabs | 256 | Import error: `the dashboard holds more than the limit of 256 variables` |
| Values of one query variable | 10,000 | grafana-tui keeps the first 10,000 values. The footer shows `kept the first 10000 of <n> values (the value limit)` |

Each dashboard file is one tab of the dashboard tab bar. grafana-tui counts the
`--var` pairs and the `vars` of the config separately. Each count has a limit
of 256.

## Queries and Series

| Item | Limit | At the limit |
|---|---|---|
| Series of one query or one panel | 2,000 | The panel keeps the first 2,000 series, and shows a warning |
| Points of one series | 22,000 | The panel keeps the newest 22,000 points, and shows a warning |
| Length of one query | 256 KiB (262,144 bytes) | grafana-tui does not send the query. The panel shows an error: `the query has <n> bytes, more than the limit of 262144 bytes` |

A cut adds the `⚠ warning` marker to the panel title. To read the warning,
select the panel. The footer then shows the text of the warning, for example
`2500 series: the panel keeps the first 2000 (the series limit)`.

Prometheus returns 11,000 points for each series at most. So the point limit
is twice the largest response.

A query of more than 256 KiB can occur after an `All` value of a large
variable expands. Prometheus also refuses a URL of this size in most set-ups.
To make the query shorter, select fewer values of the variable.

## Search and Recording

| Item | Limit | At the limit |
|---|---|---|
| Search text (`/`) | 256 characters | The search text stops at 256 characters. grafana-tui ignores the next characters that you type |
| `--record-max-frames` or `record_max_frames` | 1 to 100,000 | Start-up error: `record_max_frames is <n>, more than the limit of 100000` |

The default value of `--record-max-frames` is 300. When a recording gets to
this number of frames, grafana-tui stops and writes no more frames. The
manifest then records `completed_reason = "capped"`. For more information, see
[Recording Limits](/grafana-tui/exporting-and-recording/#recording-limits).

## Layout

A layout item is a row, a tab group, a tab, an auto grid or a panel.

| Item | Limit | At the limit |
|---|---|---|
| Depth of the nested rows and tabs | 16 levels | Import error: `... nests rows and tabs deeper than the limit of 16 levels` |
| Layout items in one dashboard, or in all of the dashboards of one run | 4,096 | Import error: `... holds more than the limit of 4096 rows, tabs and panels` |
| Layout items after the repeats | 4,096 | grafana-tui stops the repeats. The footer shows `Repeats stopped at the limit of 4096 layout items` |
| `--query` panels | 4,096 | Start-up error: `<n> --query panels are more than the limit of 4096 layout items` |

The top-level list of a dashboard is level 1. When you open more than one
dashboard, the dashboard tab bar adds 1 level. So a dashboard with 16 levels
imports when you open it alone, but not with other dashboards.

grafana-tui counts the layout items of all of the dashboards of one run
together.

When a repeat stops at the limit, grafana-tui always keeps the first copy of
each item.

## Annotations

The external annotation providers have their own limits. For these limits, see
[Current Limits and Roadmap](/grafana-tui/annotations/#current-limits-and-roadmap).
