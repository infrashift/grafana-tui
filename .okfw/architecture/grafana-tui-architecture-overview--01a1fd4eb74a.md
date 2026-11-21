---
id: "01a1fd4e-b74a-7d9a-a481-5509c2e3acf8"
title: "grafana-tui architecture overview"
type: "Architecture"
tags:
  - "architecture"
  - "overview"
created: "2026-11-21T02:52:36-06:00"
timestamp: "2026-11-21T02:52:36-06:00"
description: "The modules of grafana-tui, the layout tree and the data flow from the dashboard file to the terminal."
---

# grafana-tui architecture overview

grafana-tui is one Rust binary. It shows Grafana dashboards in a terminal and gets the data from Prometheus.

## Modules
| Module | Function |
|---|---|
| `src/main.rs`, `src/cli.rs` | Parse the command line with clap. Run a subcommand, a validation or the TUI. Restore the terminal on exit. |
| `src/config.rs` | Read the TOML config file. |
| `src/grafana.rs`, `src/grafana/` | Read Grafana Classic JSON and V2 Resource JSON or YAML. Convert each panel and variable into the internal model (`import.rs`, `classic.rs`, `v2.rs`, `model.rs`). |
| `src/dashboard.rs` | The layout tree: `DashboardLayoutItem` is `Row`, `Tabs`, `Panel` or `AutoGrid`. Rows and tabs hold child lists. |
| `src/app/` | `AppState`, the event loop (`event_loop.rs`), keyboard and mouse input (`input.rs`), variables (`variables.rs`), templates and repeats (`template.rs`). |
| `src/app/state/refresh.rs` | The refresh pipeline. Spawned tasks run the Prometheus queries and the annotation providers. The event loop applies each result. |
| `src/prom.rs`, `src/prom/tls.rs` | The Prometheus HTTP client, a response cache and the TLS settings. |
| `src/annotations/` | Annotation providers (a JSONL file or a command), the event model, filters and the details modal. |
| `src/conditions.rs` | Grafana V2 conditional rendering: rules that show or hide rows, tabs and auto grid items. |
| `src/ui/` | ratatui rendering: the frame (`draw.rs`), the grid layout (`layout.rs`), tabs, the theme picker and one renderer for each panel type (`panels/`). |
| `src/export.rs` | SVG and PNG snapshots and recordings of the dashboard. |
| `src/theme.rs`, `src/theme/` | The built-in themes and the custom themes from the config file. |

## Data flow
1. `main` reads the CLI, the config and the dashboard files.
2. The importer makes a dashboard template: the layout tree, the panels and the variables.
3. The refresh pipeline resolves the variables. Then it expands the repeats and queries Prometheus for the visible panels.
4. The event loop handles the input, applies the refresh results and draws a frame.
5. The export module draws the same model to SVG or PNG.

## Safety rules
The production code follows the NASA Power of 10 rules, adapted to Rust in [ADR-0002](/decisions/adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md). semscope and clippy enforce them ([ADR-0003](/decisions/adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md)).
