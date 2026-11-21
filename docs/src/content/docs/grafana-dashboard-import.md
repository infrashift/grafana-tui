---
title: Grafana Dashboard Import
description: Import Grafana Classic and V2 dashboards from JSON or YAML.
---

grafana-tui imports Grafana dashboards exported as JSON or YAML and renders their
supported panels in the terminal.

| Format | Status | Requirements |
|---|---|---|
| Classic JSON | ✅ Supported | Non-resource object with fields such as `title`, `panels`, and `templating` |
| V2 Resource JSON | ✅ Supported | Exact `apiVersion: dashboard.grafana.app/v2`, Grafana 13's default export format |
| V2 Resource YAML | ✅ Supported | The same resource, read from a `.yaml` or `.yml` file |
| V2 alpha/beta resources | ❌ Unsupported | `dashboard.grafana.app/v2alpha1` and `v2beta1`, exported by Grafana 12, are not accepted |
| V1 Resource JSON | ❌ Unsupported | The `dashboard.grafana.app/v1` resource envelope is not accepted |

`--grafana-json` (alias `--grafana-dashboard`) reads `.json` files as JSON and
`.yaml`/`.yml` files as YAML. For a file with a different extension, it tries
JSON first and then YAML.

V2 resources and Classic dashboards share one importer, so panels, Prometheus
queries, variables, field configuration, and diagnostics behave the same in
both. On top of that, V2 dashboards support:

- `GridLayout`, `AutoGridLayout`, `RowsLayout`, and `TabsLayout`, nested in any
  mix ([auto grids](#auto-grid-layouts));
- repeated grid items, auto grid items, rows, and tabs ([repeats](#repeats));
- conditions that show or hide rows, tabs, and auto grid items
  ([conditions](#conditional-rendering));
- variables defined by rows and tabs ([row and tab variables](#row-and-tab-variables));
- `timeSettings.autoRefresh` as the default refresh interval.

grafana-tui accepts and ignores the settings that only apply in a browser, such as
rows and auto grids that fill the viewport. Layout kinds grafana-tui does not know, and
malformed fields, are import errors that name the field's path, such as
`spec.layout.spec.rows[0].spec.repeat.direction`.

Grafana's resource API writes empty lists and objects as `null` (for example
`links`, `transformations`, `options`, and `variables`), and its exporter may
omit them entirely. grafana-tui treats both the same as an empty value, so
dashboards exported from Grafana 13 import unchanged. Exports made with
**Share dashboard with another instance** enabled also work: their queries carry
no datasource and run against the Prometheus server given by `--prometheus-url`.

A Grafana export gives a library panel only as a reference to its uid, without
the panel itself. So grafana-tui skips the library panel with an import
diagnostic. To put the library panels into the export, enable **Share dashboard
with another instance** when you export the dashboard.

## Auto Grid Layouts

An `AutoGridLayout` places its panels left to right, top to bottom, in columns of
equal width. As in Grafana, the column count changes with the available width.
It is the number of minimum-width columns that fit, up to `maxColumnCount`
(default 3). It is never more than the number of panels. So a grid with fewer
panels than columns stretches them across the full width. When you resize the
terminal, the grid reflows.

Grafana sizes auto grids in CSS pixels. grafana-tui converts them as follows:

| Setting | Grafana size | grafana-tui size |
|---|---|---|
| `columnWidthMode: narrow` | 192px minimum column width | 24 terminal columns |
| `columnWidthMode: standard` (default) | 448px | 56 terminal columns |
| `columnWidthMode: wide` | 768px | 96 terminal columns |
| `columnWidthMode: custom`, `columnWidth: N` | N px | N / 8 terminal columns |
| `rowHeightMode: short` | 168px row height | 5 grid rows |
| `rowHeightMode: standard` (default) | 320px | 9 grid rows |
| `rowHeightMode: tall` | 512px | 14 grid rows |
| `rowHeightMode: custom`, `rowHeight: N` | N px | (N + 8) / 38 grid rows, rounded |

A terminal column is 8px, the Grafana grid unit. So the column counts match what
Grafana shows at a similar pixel width. A 120-column terminal shows two
`standard` columns, and an 80-column terminal shows one. Grid rows are the same
unit as a fixed-grid panel's `height` (30px plus an 8px margin in Grafana).
grafana-tui ignores `fillScreen`, `fitContent`, and the minimum/maximum height
settings.

## Row and Tab Variables

A V2 row or tab can define `variables` of its own. As in Grafana, they apply to
the row or tab itself (its title, repeat, and conditions) and to everything
inside it. There, they shadow a dashboard variable with the same name. The rest
of the dashboard keeps the dashboard's value. Each variable kind that grafana-tui
supports at the dashboard level also works here, with the same selection,
multi-value, and `All` rules.

Query variables in a row or tab resolve against Prometheus with the variables
around that row or tab. Inside a repeated row, each copy resolves its own, so a
`label_values(up{dc="$dc"}, host)` variable in a row repeated over `dc` lists
each data center's hosts.

`--var` and the config file's `vars` override only dashboard variables. A row or
tab variable with the same name still wins inside its row or tab.

## Conditional Rendering

V2 rows, tabs, and auto grid items can carry a `conditionalRendering` group that
shows or hides them. grafana-tui evaluates these groups as Grafana 13 does:

| Condition | Holds when |
|---|---|
| Variable `equals` / `notEquals` | Any selected value equals the value (or not). `All` also matches while `All` is selected |
| Variable `matches` / `notMatches` | Any selected value matches the regular expression (or not); an invalid expression shows the item |
| Data (auto grid items only) | The panel returned data (`value: true`) or no data (`value: false`) |
| Time range size | The dashboard time range is at most the value, such as `1h` or `7d` |

A group's `condition` combines its conditions with `and` or `or`. Its
`visibility` shows (`show`) or hides (`hide`) the item when they hold.
grafana-tui leaves out a condition that it cannot decide yet:

- a variable that the dashboard does not define;
- a time range size that grafana-tui cannot parse;
- a data condition before the first query of its panel.

A group with no conditions left shows its item.

Items re-evaluate as query variables resolve, data refreshes, and the time range
changes with zoom and pan. A hidden row, tab, or auto grid item leaves the
layout, and the auto grid reflows. If a condition hides the active tab of a tab
group, the group shows its first visible tab. A collapsed row and a selected tab
keep their state while a condition hides them. grafana-tui continues to query a
panel that a data condition hides, so that the panel can come back. Inside a
repeated row or tab, a variable condition sees the value of that copy.

## Export From Grafana

Grafana 13 exports dashboards in the V2 Resource model by default:

1. Open the dashboard in Grafana.
2. In the toolbar, open **Export** and select **Export as code**.
3. Choose JSON or YAML.
4. If the dashboard uses library panels, enable **Share dashboard with another
   instance**. This option puts the library panels in the file.
5. Download the file, or copy it into a local `.json` or `.yaml` file.
6. Run grafana-tui with `--grafana-json`.

```bash
grafana-tui --prometheus-url http://localhost:9090 --grafana-json ./node-exporter.yaml
```

Grafana 12 exports dynamic dashboards as `v2alpha1` or `v2beta1` resources,
which grafana-tui does not accept. For those dashboards, and for dashboards from
an older Grafana, export with the Classic model:

1. Under **Export as code**, expand **Advanced options**.
2. Set **Model** to **Classic**.
3. Save the JSON.

Grafana
documents the available models and export controls in
[Export a dashboard as code](https://grafana.com/docs/grafana/latest/visualizations/dashboards/share-dashboards-panels/#export-a-dashboard-as-code).

## Supported Panel Types

grafana-tui currently supports:

- `graph`
- `timeseries`
- `stat`
- `gauge`
- `bargauge`
- `table`
- `heatmap`

grafana-tui draws Classic row headers and their collapsed state, and you can
expand and collapse them. The panel focus keys, search, refresh, and export
skip all items inside a collapsed row. A row with a hidden header is
transparent: it takes no header line, and its children stay visible.

## Variables

grafana-tui reads dashboard variables from `templating.list` (Classic) or
`spec.variables` (V2) and expands `$var` and `${var}` in PromQL expressions and
in panel, row, and tab titles. grafana-tui matches whole names, so `$job_name`
never expands a variable called `job`.

Defaults come from the dashboard's saved selection; a variable without one
selects its first option, as Grafana does. Override them from the CLI:

```bash
grafana-tui --grafana-json ./dash.json --var job=node --var instance=server-01
```

grafana-tui resolves Prometheus query variables such as
`label_values(up, instance)` and `query_result(...)` before panel queries run.
If Prometheus still offers the saved selection, the variable keeps it. If not,
the variable selects the first value.
`label_values` only lists values from series in the current time range.

A query variable's **Refresh** setting decides when its query runs again:

| Grafana setting | grafana-tui |
|---|---|
| On dashboard load | Once, when the dashboard opens |
| On time range change | When the dashboard opens, and again after zooming, panning, returning to live, or pressing `r` |
| Never | Not queried; the saved selection is used. A variable saved without a selection is resolved once |

Periodic refreshes do not re-query variables. When the value of a variable
changes, grafana-tui queries again each variable whose query refers to it. When
a variable query fails, the variable keeps its value, the other variables still
resolve, and the footer shows the error.

### Multi-Value and All Selections

Multi-value and include-all variables follow Grafana's Prometheus datasource:

- Each value is regex-escaped, so `web.1` becomes `web\\.1` in the query.
- Several values become one regex group with `|`, such as `(api|web\\.1)`, for
  `=~` matchers.
- `All` selects every option: a custom variable's options, or every value a
  query variable resolves. It interpolates as the variable's `allValue` when
  set, and otherwise as all values joined.

In titles, grafana-tui joins several values with ` + `, like Grafana's text
format.

Repeat `--var` for one name to select several values. grafana-tui uses a single
`--var` value verbatim, so it can still be a regex such as `--var job='api|web'`:

```bash
grafana-tui --grafana-json ./dash.json --var instance=server-01 --var instance=server-02
```

## Repeats

grafana-tui copies a panel, row, or tab that repeats over a variable once for
each selected value. So `All` or a multi-value selection shows one copy for each
value. Each copy uses its own value. `$var` in its title and queries is that
single value (regex-escaped for multi-value variables). The copies of a repeated
row or tab carry that value into every panel inside them. Nested repeats
combine.

| Repeat | Classic | V2 | Layout |
|---|---|---|---|
| Panel, horizontal | `repeat`, `repeatDirection: h` | `GridLayoutItem` `repeat.direction: h` | Spans the full grid width, in rows of up to `maxPerRow` equal columns (default 4) |
| Panel, vertical | `repeatDirection: v` | `repeat.direction: v` | Copies stack at the panel's width |
| Auto grid item | — | `AutoGridLayoutItem` `repeat` | Copies flow into the auto grid after the item |
| Row | row `repeat` | `RowsLayoutRow` `repeat` | One row per value |
| Tab | — | `TabsLayoutTab` `repeat` | One tab per value |

When a repeated grid panel grows, panels that start below it move down by the
height it adds, as in Grafana. If the variable has no selected values,
grafana-tui shows the item once. If the dashboard does not define the variable,
grafana-tui also shows the item once and prints an `unknown_repeat_variable`
diagnostic.

Copies follow query variables as they resolve. A copy keeps its place, data,
and collapsed state while its value stays selected.

## Import Diagnostics

Before the TUI starts, grafana-tui prints import warnings when a dashboard uses
important Grafana features that it skips or ignores. Diagnostics include
unsupported panel types, value mappings, reduce options, unresolved variables,
unsupported V2 datasources, and unsupported variable modifiers such as
`${var:regex}`. V2 diagnostics keep their `spec.*` source paths.

Run a non-interactive check with:

```bash
grafana-tui --validate --grafana-json ./dash.json
```

Warnings do not make the check fail. If grafana-tui can parse and import a
dashboard, the check exits successfully, even if it prints diagnostics.

Use `--strict` to make warnings fail the check, or `--format json` to emit a
machine-readable summary. Fatal V2 layout errors fail the check in all modes.
`--strict` also fails when import diagnostics are present:

```bash
grafana-tui --validate --strict --grafana-json ./dash.json
grafana-tui --validate --format json --grafana-json ./dash.json
```

## Several Dashboards

Repeat `--grafana-json` to load several dashboards in one session, or list them
in the configuration file as `grafana_json = ["a.json", "b.json"]`:

```bash
grafana-tui --grafana-json ./nodes.json --grafana-json ./consul.json --grafana-json ./vault.yaml
```

Each dashboard is a tab of a tab bar pinned above the dashboards, labelled with
its title. When every title starts with the same `Prefix / `, the labels leave
it out. Press `Tab` / `Shift+Tab` to show the next or previous dashboard, or
click a label. The header shows the dashboard's full title.

Only the dashboard shown queries Prometheus: its panels and its variables, so
several dashboards cost Prometheus no more than the one on screen. When you
switch to a dashboard, grafana-tui refreshes it at once and reloads its
variables, as Grafana does when it opens a dashboard. It refreshes at its own auto-refresh interval, unless
`--refresh-rate` or `refresh_rate` sets one for every dashboard.

Each dashboard keeps its own variables, even when two dashboards define a
variable of the same name with different queries. `--var` and `[vars]` set a
variable in every dashboard that defines it.

`--validate` checks every file. With `--format json`, it prints one object with
an entry per file; warnings name their file:

```json
{
  "dashboards": [
    {"path": "./nodes.json", "title": "Nodes", "panel_count": 39, "diagnostics": []}
  ]
}
```

## Hidden Targets

grafana-tui skips hidden queries during import: `targets[].hide` in Classic JSON
and `PanelQuery` `hidden` in V2 resources. Panels with a mix of hidden and
visible queries render only the visible ones.

## Query Modes

grafana-tui honors a query's `instant` setting: `targets[].instant` in Classic JSON,
and `instant` in the Prometheus query `spec` of a V2 `PanelQuery`. Instant
queries use the Prometheus instant `query` endpoint, while range queries use
`query_range`.

If a target does not specify `instant`, Gauge, Bar Gauge, and Table panels
default to instant queries. Graph, Timeseries, Stat, and Heatmap panels default
to range queries.

## Field Configuration

grafana-tui applies selected `fieldConfig.defaults` values where they map cleanly
to a terminal display:

- `min` and `max` set explicit Graph y-axis bounds and Gauge limits.
- `thresholds` render graph threshold lines and set the dynamic color of Stat,
  Gauge, and Bar Gauge panels.
- `unit`, `decimals`, and `noValue` affect supported panel values, axes,
  legends, and exports.
- `custom.axisGridShow` controls per-panel graph guide lines.

## Built-In PromQL Variables

grafana-tui expands these Grafana-style variables:

- `$__interval`
- `$__interval_ms`
- `$__range`
- `$__range_s`
- `$__range_ms`
- `$__rate_interval`
- `$__rate_interval_ms`

`$__interval` is the query's step, which scales with the time range and honors
panel `maxDataPoints` and `interval` and target `interval`. See
[query resolution](/grafana-tui/configuration/#query-resolution).

## Compatibility Details

See the [Grafana compatibility matrix](/grafana-tui/grafana-compatibility/) for field-by-field support details.
