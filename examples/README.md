# grafana-tui Examples

This directory contains example Grafana dashboards and a demo environment. Use them to test grafana-tui.

## Quick Demo

To try grafana-tui fast, use the pre-configured demo environment:

```bash
cd demo
docker-compose up -d && sleep 5 && cd ../.. && cargo run -- --grafana-json examples/dashboards/prometheus_demo.json --prometheus-url http://localhost:19090
```

See [`demo/README.md`](demo/README.md) for details.

## HashiStack RDW Dashboards

[`demo/hashistack-rdw/`](demo/hashistack-rdw/README.md) holds eight operator
dashboards for a Consul/Nomad/Vault platform with a Consul Connect (Envoy) mesh
and remote developer workspaces. They are Grafana 13 V2 resources and run
against the mTLS Prometheus of a real datacenter, not against the bundled demo
stack. This Prometheus requires a client certificate. The README tells how to
issue the reader certificate and how to build the config.

## SSHadowForge Dashboards

[`demo/sshadowforge/`](demo/sshadowforge/README.md) holds six dashboards for
SSHadowForge, the event-sourced Git forge that runs on the same platform, and
its NATS JetStream event bus. Five are for the operator: overview, event
pipeline, NATS JetStream, builds and resources. The sixth is a developer
dashboard, filtered by repository. They need the same mTLS Prometheus as the
HashiStack set.

## External Annotations

[`annotations.jsonl`](annotations.jsonl) is a read-only external JSONL event
source for graph and timeseries panels. Its `Deployed v2.4` event uses
`panel_titles` to target the bundled `HTTP Request Rate by Status Code` panel.
The other events do not have `panel_titles`, so they apply to the full dashboard.
Run it with a dashboard:

```bash
cargo run -- --grafana-json examples/dashboards/prometheus_demo.json --annotations-file examples/annotations.jsonl
```

The [external annotations guide](../docs/src/content/docs/annotations.md) tells
how to target panels, filter tags, examine annotations and reload them. It also
gives the limits.

### Git Command Provider

`git_annotation_provider.rs` turns commits in grafana-tui's requested time window
into external JSONL annotations. Build it from the repository root:

```bash
cargo build --example git_annotation_provider
```

Then configure the compiled example as the one annotation source:

```toml
[annotations_command]
program = "./target/debug/examples/git_annotation_provider"
args = ["."]
timeout = "10s"
```

The optional first argument is the Git repository path and defaults to `.`.

## Dashboards

Every dashboard here is a Grafana 13 V2 resource. Grafana 13.2.3 converted
some of them from Classic JSON: `prometheus_demo.json`,
`all_visualizations.json`, `instant_queries.json`, `thresholds_demo.json`,
`simple_test.json` and the vLLM demo dashboards. The Classic originals are in
`tests/fixtures/grafana/classic_examples`. A test checks that the two
formats import identically.


### `prometheus_demo.json`
**We recommend this dashboard for first-time users.** We made it for the included demo environment.
It shows all 6 panel types. The data is the real metrics that Prometheus records about itself:
- Graph, Gauge, Stat, Bar Gauge, Table, Heatmap
- Works immediately with `demo/docker-compose.yml`

### `all_visualizations.json`
Demonstrates all supported panel types in a single dashboard:
- **Graph**: Line chart of the CPU use over time
- **Timeseries styles**: Bars with a hidden axis, area fill, and a style with only points
- **Gauge**: Progress bar for the memory use
- **Stat**: Big value display with sparkline for uptime
- **Bar Gauge**: Vertical bars that compare request rates
- **Table**: Tabular view of series
- **Heatmap**: Color-coded time-series intensity

### `instant_queries.json`
Demonstrates queries marked `instant` and the default instant query behavior
used by Gauge, Bar Gauge, and Table panels.

### `thresholds_demo.json`
Shows thresholds, the styles of the threshold markers, and explicit field
min/max bounds.

### `grafana_v2_compatibility.json`
An exact Grafana V2 resource with a `GridLayout`, a dynamic Prometheus `job`
variable, refresh settings, and two supported panels. Run it against the bundled
demo stack:

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_compatibility.json --prometheus-url http://localhost:19090
```

### `grafana_v2_rows.json`
An exact Grafana V2 `RowsLayout` resource that shows expanded rows, nested rows,
and rows that start collapsed. It has three Prometheus panels. It also has a
container with a hidden header. The child panel of this container shows directly
below its parent row. Run it against the bundled demo stack:

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_rows.json --prometheus-url http://localhost:19090
```

### `grafana_v2_autogrid.json`
An exact Grafana V2 `AutoGridLayout` resource with five Prometheus panels in up
to three columns. When the terminal size changes, the columns move to fit:
two columns at a width of 120, one column at 80.

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_autogrid.json --prometheus-url http://localhost:19090
```

### `grafana_v2_repeats.yaml`
A Grafana V2 resource in YAML, as Grafana 13 exports it. A request-rate panel
repeats horizontally, two per row, over an `All` selection of HTTP handlers.
The repeats move the panel below them down. A row repeats over every `job` that
the `label_values(up, job)` query variable resolves.

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_repeats.yaml --prometheus-url http://localhost:19090
```

### `grafana_v2_conditional.json`
A Grafana V2 resource that we wrote in Grafana 13 with `conditionalRendering`
rules:

- The `Production` row shows because `env` is `prod`.
- The `Staging only` row does not show.
- The server-error panel shows only when Prometheus returns 5xx requests.
- The `Zoomed in` tab shows only while the time range is an hour or less.

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_conditional.json --prometheus-url http://localhost:19090 --range 1h
```

### `grafana_v2_sections.json`
A Grafana V2 resource that we wrote in Grafana 13 with row variables. The
`Tail latency` row defines its own `quantile` (0.99). In that row only, this
value replaces the 0.9 of the dashboard. The `Busiest handlers` row defines a
`handler` query variable. A repeated panel iterates over its `/api/v1/` values.

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_sections.json --prometheus-url http://localhost:19090
```

### `grafana_v2_tabs.json`
An exact Grafana V2 `TabsLayout` resource with panels, a nested row, and an
empty tab. Focus the tab bar and use Left/Right to switch tabs; Enter/Space
moves into the active content.

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_tabs.json --prometheus-url http://localhost:19090
```

### Several dashboards
Repeat `--grafana-json` to load several dashboards, one per tab of a tab bar
pinned at the top. Tab / Shift+Tab switches dashboards; only the dashboard
shown queries Prometheus.

```bash
cargo run -- --grafana-json examples/dashboards/prometheus_demo.json --grafana-json examples/dashboards/grafana_v2_tabs.json --prometheus-url http://localhost:19090
```

### Usage

```bash
# Test with local Prometheus (default port 9090)
cargo run -- --grafana-json examples/dashboards/all_visualizations.json

# Or with custom Prometheus URL
cargo run -- --grafana-json examples/dashboards/all_visualizations.json --prometheus-url http://prometheus.example.com:9090

# Override variables
cargo run -- --grafana-json examples/dashboards/all_visualizations.json --var instance=prometheus:9090
```

## Creating Your Own

You can export any Grafana dashboard as JSON and use it with grafana-tui:
1. In Grafana, go to Dashboard Settings → JSON Model
2. Copy the JSON
3. Save it to a file
4. Run: `grafana-tui --grafana-json your-dashboard.json`

## Supported Panel Types

- ✅ `graph` / `timeseries` - Line, point, bar, and area-style charts
- ✅ `gauge` - Progress bars
- ✅ `bargauge` - Bar charts
- ✅ `table` - Data tables
- ✅ `stat` - Single value + sparkline
- ✅ `heatmap` - Color intensity grid
