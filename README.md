# grafana-tui

[![CI](https://github.com/infrashift/grafana-tui/workflows/CI/badge.svg)](https://github.com/infrashift/grafana-tui/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust Version](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)

**grafana-tui** is a terminal user interface (TUI) for Prometheus. Grafana is the model for its design. The TUI shows time-series dashboards in a terminal. The keyboard controls all of its functions. It works over SSH and on hosts with few tools.

[![asciicast](https://asciinema.org/a/vMRNEjG0FEDKGP31.svg)](https://asciinema.org/a/vMRNEjG0FEDKGP31)

## Quick Start

Install the latest prebuilt binary into `~/.local/bin`:

```bash
bash -o pipefail -c 'curl --proto =https --tlsv1.2 -LsSf https://raw.githubusercontent.com/infrashift/grafana-tui/main/install.sh | bash'
```

For other directories, pinned versions, manual downloads and builds from source, see [Install grafana-tui](https://infrashift.github.io/grafana-tui/installation/).

Run against a Prometheus instance:

```bash
grafana-tui --prometheus-url http://localhost:9090
```

Or try the included demo:

```bash
git clone https://github.com/infrashift/grafana-tui.git
cd grafana-tui
cd examples/demo && docker-compose up -d && sleep 5 && cd ../..
cargo run -- --grafana-json examples/dashboards/prometheus_demo.json --prometheus-url http://localhost:19090
```

## Features

- Prometheus range queries and instant queries. The TUI sends the queries asynchronously.
- Import of Grafana Classic JSON and V2 Resource JSON/YAML. The import supports graph, timeseries, stat, gauge, bar gauge, table and heatmap panels. It also supports interactive nested rows and tabs.
- Template variables, Grafana built-in PromQL variables, legend formats, thresholds and the grid layout.
- Grafana timeseries draw styles for lines, points, bars, area fill and hidden axes. Each panel can show or hide its grid.
- Keyboard controls to move through the dashboard. Search for a row, a tab or a panel. Interactive rows and tabs, a fullscreen mode, mouse selection and a value inspector.
- SVG/PNG export, and recording bundles that keep only the changed frames.
- TOML configuration and 19 built-in themes. The themes include Tokyo Night, Catppuccin and Gruvbox flavors.
- Read-only JSONL point annotations from an external file or from a command. Each annotation can target panels. A tag filter selects the annotations. The TUI shows the details of each annotation cluster, and you can move through them.

## Documentation

- [User guide](https://infrashift.github.io/grafana-tui/)
- [Install grafana-tui](https://infrashift.github.io/grafana-tui/installation/)
- [Quick start](https://infrashift.github.io/grafana-tui/quick-start/)
- [Configuration](https://infrashift.github.io/grafana-tui/configuration/)
- [External annotations](https://infrashift.github.io/grafana-tui/annotations/)
- [Grafana dashboard import](https://infrashift.github.io/grafana-tui/grafana-dashboard-import/)
- [Grafana compatibility matrix](https://infrashift.github.io/grafana-tui/grafana-compatibility/)
- [Examples](examples/README.md)

## Common Commands

```bash
# Import a Grafana dashboard
grafana-tui --prometheus-url http://localhost:9090 --grafana-json ./dashboard.json

# Load several dashboards, one per tab (Tab / Shift+Tab to switch)
grafana-tui --grafana-json ./nodes.json --grafana-json ./consul.json

# Override Grafana template variables
grafana-tui --grafana-json ./dash.json --var job=node --var instance=server-01

# Use a theme (run --list-themes for every name)
grafana-tui --theme catppuccin-latte

# Overlay read-only JSONL point events
grafana-tui --grafana-json ./dashboard.json --annotations-file ./events.jsonl

# Query read-only annotations through a command provider
grafana-tui --grafana-json ./dashboard.json --annotations-command ./target/debug/examples/git_annotation_provider --annotations-command-arg=.

# Generate shell completions or a man page
grafana-tui completions zsh
grafana-tui man
```

Grafana 13 users can import an exact `dashboard.grafana.app/v2` JSON or YAML
resource. The resource can have recursive `GridLayout`, `AutoGridLayout`,
`RowsLayout` and `TabsLayout` containers. It can also have repeated panels,
rows and tabs, `conditionalRendering` rules, and row and tab variables.

A Grafana export refers to each library panel only by its uid. To put the
library panels in the export, enable **Share dashboard with another instance**
before the export. For the current format requirements, see the
[dashboard import guide](https://infrashift.github.io/grafana-tui/grafana-dashboard-import/).

## Contributing

We welcome changes from contributors. For the rules, see [CONTRIBUTING.md](CONTRIBUTING.md). For notes about local builds and tests, see [DEVELOPMENT.md](DEVELOPMENT.md).

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

Copyright 2025 Federico D'Ambrosio
