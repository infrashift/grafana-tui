# Development Notes

This document tells contributors how to build, test and check grafana-tui.
[CONTRIBUTING.md](CONTRIBUTING.md) tells you how to send a change.

## Build Files

grafana-tui has two build files with the same targets:

- `Makefile` for GNU Make. CI uses it.
- `ZMakefile` for zmake. Each `@name:` line in it declares a task. zmake reads the `ZMakefile` before the `Makefile`.

Keep the two files in step (wiki ADR-0005). When you change a target in one
file, make the same change in the other file. Then compare the two recipes:

```bash
zmake -n <task>
make -n <task>
```

The two commands print the same recipe lines. Only the indent of a line
after a `\` is different. `make help` and `zmake help` list each target.

## Tools

1. Install the Rust tools with `make tools`. It adds clippy, rustfmt, llvm-tools, cargo-audit and cargo-llvm-cov.
2. Put `semscope`, `lexiscope` and `okfw` on the `PATH`.
3. Put `python3` on the `PATH` for `scripts/p10-assert-density.py`.

semscope, lexiscope and okfw have no public releases yet, so the CI runners
cannot install them. CI runs `make lint test test-install cover-check` only
(wiki ADR-0003). The clippy lints carry the Power of 10 rules in CI. Run
`make check` on the local computer before each push.

## The Gates

`make check` and `zmake check` run all of the gates. Each gate must pass
before you commit.

| Target | What it does |
|---|---|
| `fmt` | Runs `cargo fmt`. |
| `lint` | Runs clippy on each target with `-D warnings`. |
| `test` | Runs the unit tests, the CLI tests in `tests/` and the snapshot tests. |
| `test-install` | Runs the `install.sh` tests in `tests/install.sh`. |
| `cover-check` | Fails when the line coverage is less than 95% (`COVER_MIN`). |
| `p10` | Runs semscope with the `nasa-p10` ruleset and `.semscope.cue` on `src`. Then it runs `scripts/p10-assert-density.py`. |
| `wiki` | Runs `okfw validate --all` on the wiki in `.okfw/`. |
| `docs-style` | Runs lexiscope with the `ste-100` ruleset and `vocabulary.cue` on the documentation and the code comments. |

To run one gate, give its name, for example `make p10` or `zmake docs-style`.

## The Power of 10 Rules

The production code follows the NASA Power of 10 rules, as the wiki ADR-0002
adapts them to Rust. Read ADR-0002 for the full table and the recorded
deviations. In short:

- A function has 60 lines or fewer.
- The code has no recursion. A tree walk uses an explicit stack with a fixed capacity.
- Each loop has a bound that a reader can see.
- A named limit caps each input and each growable collection.
- The functions of more than 5 lines have an average of 2 or more assertions.
- The code has no `unwrap` and no `expect`.
- The code has no `as` cast outside `src/num.rs`.
- The code has no `dyn` and no function pointers.

Test code in `#[cfg(test)]` modules and in `tests/` is exempt from the rules.
It must compile with no warnings.

## The Working Loop

Use this loop for each change. Do not commit while a gate fails.

1. Write the code for one small slice of the change.
2. Run `make fmt lint p10` to lint the change.
3. Run `make test test-install cover-check` to verify the change.
4. Fix each failure, then run all of the gates again.
5. Repeat steps 2 to 4 until all of the gates pass.
6. Update the wiki in `.okfw/`.
7. Run `make check`.
8. Commit the code, the wiki and the docs together.

Do not weaken or delete a test to get a pass. Do not loosen a gate. The wiki
guide `.okfw/development-workflow--*.md` gives the full loop, with the okfw
commands for the issues, the ADRs and the daily notes.

## Snapshot Tests

`src/snapshot_tests.rs` loads each example dashboard on the same path as
`main`. It fills each panel with fixed synthetic series. Then it draws the
dashboard to a TUI buffer and to SVG. The tests compare the output with the
files in `tests/snapshots/`. A refactor must not change the output.

When you change the output on purpose, record new snapshots:

1. Run `UPDATE_SNAPSHOTS=1 cargo test snapshot_tests`.
2. Examine the diff of `tests/snapshots/` with `git diff`.
3. Commit the new snapshots with the change.

## Other Test Commands

```bash
# Run all unit tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_calculate_y_bounds

# Build optimized release
cargo build --release
```

## Test Dashboards

The file `examples/dashboards/all_visualizations.json` has each panel type
that grafana-tui supports. It has these panels:

1. **Graph:** a time series line chart.
2. **Gauge:** a single value as a progress bar.
3. **Stat:** a big number and a sparkline.
4. **Bar Gauge:** bars that you can compare.
5. **Table:** the data in rows and columns.
6. **Heatmap:** a grid of colors that show the intensity.

To open the dashboard with a local Prometheus:

```bash
# If you have Prometheus running locally
cargo run -- --grafana-json examples/dashboards/all_visualizations.json

# Or connect to a remote Prometheus
cargo run -- --grafana-json examples/dashboards/all_visualizations.json --prometheus-url http://your-prometheus:9090
```

To start Prometheus in Docker first:

```bash
# Download and run Prometheus locally for testing
docker run -p 9090:9090 prom/prometheus

# Then in another terminal:
cargo run -- --grafana-json examples/dashboards/all_visualizations.json
```

To add a test dashboard for a new feature:

1. Put the dashboard in `examples/dashboards/`.
2. Give the file a name that tells its purpose, for example `feature_name_test.json`.
3. Add the dashboard to `examples/README.md`.
4. Use Prometheus queries on the `prometheus_http_*` metrics, because each Prometheus has them.

Each new UI or UX feature must update an example or add a new example. Each
example must work with the mock and demo environment in `examples/demo/`.

## Project Structure

```
grafana-tui/
├── src/                         # Rust source code
│   ├── main.rs                  # Program entry point and app wiring
│   ├── cli.rs                   # clap CLI argument and subcommand definitions
│   ├── config.rs                # TOML configuration loading and path expansion
│   ├── limits.rs                # Named limits for inputs and collections (ADR-0007)
│   ├── num.rs                   # The only numeric `as` casts (ADR-0002)
│   ├── conditions.rs            # Grafana v2 conditional rendering
│   ├── dashboard.rs             # Dashboard model and layout tree
│   ├── dashboard/walk.rs        # Bounded layout walker (ADR-0006)
│   ├── export.rs                # SVG/PNG export and changed-frame recordings
│   ├── snapshot_tests.rs        # Characterization snapshots (tests/snapshots/)
│   ├── grafana.rs, grafana/     # Grafana dashboard JSON and YAML import (classic, v2)
│   ├── prom.rs, prom/           # Prometheus HTTP client and TLS
│   ├── theme.rs, theme/         # Color themes and Grafana color parsing
│   ├── annotations/             # External annotation providers
│   ├── app/                     # Runtime state, input, refresh loop, and variables
│   └── ui/                      # Ratatui rendering and formatting
│       └── panels/              # Panel renderers (graph, gauge, stat, table, ...)
├── tests/                       # CLI tests, install.sh tests, and snapshots
├── scripts/p10-assert-density.py # Power of 10 rule R5 gate
├── docs/                        # User guide: Astro Starlight site, built with bun
├── examples/                    # Example dashboards and local demo stack
├── .okfw/                       # OKF wiki: architecture, ADRs, issues, daily notes
├── .semscope.cue                # semscope overlay for `make p10`
├── vocabulary.cue               # Project terms for lexiscope (`make docs-style`)
├── Makefile, ZMakefile          # Build targets for GNU Make and zmake
└── Cargo.toml                   # Crate metadata, dependencies, and clippy lints
```

## Documentation Site

The user guide in `docs/` is an [Astro](https://astro.build/) site with the
[Starlight](https://starlight.astro.build/) theme. The pages are Markdown files
in `docs/src/content/docs/`. `docs/astro.config.mjs` sets the sidebar. Put the
site base in each link between pages, for example
`[Configuration](/grafana-tui/configuration/)`.

The site needs [bun](https://bun.sh/) 1.3.9 or newer. Run these commands from
the root of the repository:

```bash
# Serve with live reload at http://localhost:4321/grafana-tui/
make docs-dev

# Build the static site into docs/dist, as CI does
make docs-build

# Serve the built site to check a production build
make docs-preview
```

CI builds the site for each pull request. `docs-release.yml` publishes it to
<https://infrashift.github.io/grafana-tui/> from `main`.

## Code Style

- Run `make fmt` to format the code with rustfmt.
- Run `make lint` to run clippy on each target, with each warning as an error.
- Write a doc comment (`///`) for each public API.
- Write the comments and the docs in ASD-STE100 Simplified Technical English (wiki ADR-0004).
- Run `make docs-style` to check the comments and the docs.
