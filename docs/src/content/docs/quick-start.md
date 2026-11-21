---
title: Quick Start
description: Run grafana-tui against Prometheus and open your first dashboard.
---

## Connect to Prometheus

If Prometheus is on the local host:

```bash
grafana-tui --prometheus-url http://localhost:9090
```

Point grafana-tui at another Prometheus server with the same option:

```bash
grafana-tui --prometheus-url http://prometheus.example.com:9090
```

## Import a Grafana Dashboard

grafana-tui imports either a Classic JSON dashboard or an exact
`dashboard.grafana.app/v2` JSON resource that uses recursive grid, row, or tab
containers:

```bash
grafana-tui --prometheus-url http://localhost:9090 --grafana-json ./dashboard.json
```

Grafana 13 exports V2 resources as JSON or YAML, and both import directly.
A Grafana export refers to each library panel only by its uid. If a dashboard
uses library panels, enable **Share dashboard with another instance** before
the export. grafana-tui does not support V1 Resource files.
For the full format requirements, see
[Grafana Dashboard Import](/grafana-tui/grafana-dashboard-import/).

To override dashboard variables, give one `--var` option for each value. If
you give a name more than one time, the TUI selects each of its values:

```bash
grafana-tui --grafana-json ./dash.json --var job=node --var instance=server-01
```

## Run the Demo

The repository includes a Prometheus demo stack and sample dashboards:

```bash
git clone https://github.com/infrashift/grafana-tui.git
cd grafana-tui
cd examples/demo && docker-compose up -d && sleep 5 && cd ../..
cargo run -- --grafana-json examples/dashboards/prometheus_demo.json --prometheus-url http://localhost:19090
```

When finished:

```bash
cd examples/demo
docker-compose down -v
```

## Useful First Keys

| Key | Action |
|---|---|
| `q` | Quit |
| `r` | Force refresh |
| `+` / `-` | Zoom out / in |
| `Shift+Left` / `Shift+Right` | Pan left / right |
| `f` | Fullscreen selected panel |
| `Enter` / `Space` on a row | Toggle the row |
| `Left` / `Right` on a row | Collapse / expand the row |
| `v` | Inspect values |
| `/` | Search visible rows and panels |
