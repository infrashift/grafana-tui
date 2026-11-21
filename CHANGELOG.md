# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-11-21

### ⚠️ Behavior changes

The code now follows the NASA Power of 10 rules for safety-critical code (see the wiki ADR-0002). Named limits protect each input, and a limit gives an error or a notice. See the [Limits](https://infrashift.github.io/grafana-tui/limits/) page.

- Start-up fails with an error for more than 64 dashboard files, 256 `--var` pairs, 4096 `--query` panels or 100000 `--record-max-frames`.
- A dashboard file over 16 MiB, a config file over 1 MiB, or a PEM file over 1 MiB gives an error.
- A dashboard layout deeper than 16 levels, or with more than 4096 rows, tabs and panels, gives an import error. A repeat that reaches the limit stops, and the footer shows a notice.
- A panel keeps 2000 series and the newest 22000 points of each series, with a warning on the panel. A query variable keeps its first 10000 values, with a notice.
- An empty variable query fails before grafana-tui sends it. A config with more than 64 dashboard files or 256 variables fails when it loads.
- A command annotation provider that writes more than 16 MiB to stderr fails.
- An SVG export over a range of about 11 years or more uses a wider time step.
- Dependency updates fix 18 RustSec advisories in quinn, rustls, rustls-webpki, webpki-roots and zstd.

### ⛰️  Features

- **demo:** Hashistack-rdw SSHadowForge dashboard (70-sshadowforge)
- **demo:** SSHadowForge dashboard set -- operator, NATS JetStream, builds, developer (examples/demo/sshadowforge)
- **sshadowforge:** A build-pins row on the builds dashboard
- **p10:** Byte caps on dashboard, config and PEM files (ISSUE-006)
- **p10:** Limit the series, points, messages and query length of Prometheus queries (ISSUE-006)
- **p10:** Limit the variables of a dashboard and the values of a query variable (ISSUE-006)
- **p10:** Limit the search text and the recording frame option (ISSUE-006)

### 🐛 Bug Fixes

- **demo/sshadowforge:** Young histograms, counts without decimals, honest empty states
- **p10:** Report the layout limit cuts and turn on the R1 gate (ISSUE-003)
- **export:** No NaN coordinates for NaN or huge values (ISSUE-011)
- **ci:** Install the pinned toolchain with each release target
- **deps:** Update quinn, rustls, rustls-webpki, webpki-roots and zstd for 18 advisories

### 📚 Documentation

- **sshadowforge:** Name the failed builds make verify leaves on purpose
- **wiki:** Name the shared graph view in the architecture overview
- **p10:** Write the search limit comment in Simplified Technical English (ISSUE-006)
- **p10:** ADR-0007 lists each resource limit; close ISSUE-006
- **ste:** ASD-STE100 README and examples (ISSUE-010)
- **ste:** ASD-STE100 contributor guides and the gates (ISSUE-010)
- **ste:** ASD-STE100 comments in the ui, theme and test modules (ISSUE-010)
- **ste:** ASD-STE100 comments in the grafana and export modules (ISSUE-010)
- **ste:** ASD-STE100 wiki ADRs, issues and guides (ISSUE-010)
- **ste:** ASD-STE100 user guide (ISSUE-010)
- **ste:** Add the Limits page (ISSUE-010)
- **ste:** ASD-STE100 comments in the app module (ISSUE-010)
- **ste:** Vocabulary terms and ASD-STE100 example script comments (ISSUE-010)
- **ste:** ASD-STE100 comments in the annotations and prom modules (ISSUE-010)
- **ste:** ASD-STE100 comments in main, config, num, conditions and dashboard (ISSUE-010)
- **ste:** Docs-style joins make check; Makefile and CI comments (ISSUE-010)
- **wiki:** ASD-STE100 wording in ISSUE-010 and ISSUE-012

### 🚜 Refactor

- **p10:** Split the ui renderers and layout to 60 lines or fewer (ISSUE-002)
- **p10:** Split main and the importers to 60 lines or fewer (ISSUE-002)
- **p10:** Split the app, prom and annotation functions to 60 lines or fewer (ISSUE-002)
- **p10:** Split the graph and export renderers to 60 lines or fewer (ISSUE-002)
- **p10:** Share the graph helpers with the export and turn on the R4 gate (ISSUE-002)
- **p10:** Layout limits and iterative dashboard import (ISSUE-003)
- **p10:** Bounded layout walker replaces recursive reads (ISSUE-003)
- **p10:** Iterative layout builders and state changes (ISSUE-003)
- **p10:** Remove the last recursion findings (ISSUE-003)
- **p10:** Bounded tick generators (ISSUE-004)
- **p10:** Bound the loops that semscope reports (ISSUE-004)
- **p10:** Bounded while loops (ISSUE-004)
- **p10:** An SVG writer that cannot fail replaces the write unwraps (ISSUE-005)
- **p10:** Check each result in annotations, themes and the terminal restore (ISSUE-005)
- **p10:** Delete dead code and give each allow a reason (ISSUE-005)
- **p10:** Checked conversions and no panicking indexing (ISSUE-007)
- **p10:** An explicit role table replaces the theme_spec macro (ISSUE-008)
- **p10:** Data, enums and traits replace function pointer types (ISSUE-008)
- **p10:** Enum dispatch for annotation providers (ISSUE-008)
- **p10:** The KEY=VALUE parser returns its error as text (ISSUE-008)
- **p10:** Assertions in the data, variable, event loop and dashboard code (ISSUE-009)
- **p10:** Assertions in the input handlers (ISSUE-009)
- **p10:** Assertions in the dashboard template (ISSUE-009)
- **p10:** Assertions in the background refresh (ISSUE-009)
- **p10:** Assertions in the app state (ISSUE-009)
- **p10:** Assertions in the graph, theme, config and conversion code (ISSUE-009)
- **p10:** Assertions in the SVG export (ISSUE-009)
- **p10:** Assertions in the layout model, walker and conditions (ISSUE-009)
- **p10:** Assertions in the dashboard loader and the Classic adapter (ISSUE-009)
- **p10:** Assertions in the importer (ISSUE-009)
- **p10:** Assertions in the V2 importer (ISSUE-009)
- **p10:** Assertions in the entry point (ISSUE-009)
- **p10:** Assertions in the Prometheus client (ISSUE-009)
- **p10:** Assertions in the annotation providers and state (ISSUE-009)
- **p10:** Assertions in the layout, tabs, formats, modals and panels (ISSUE-009)
- **p10:** Assertions in the screen drawing (ISSUE-009)

### ⚙️ Miscellaneous Tasks

- Move repository and docs site to the infrashift organization
- Ignore the old grafatui-exports recording directory
- **p10:** Gates, ZMakefile, OKF wiki and snapshot tests for the Power of 10 refactor
- **p10:** Turn on the R2 gate (ISSUE-004)
- **p10:** Turn on the R7 gate (ISSUE-005)
- **p10:** Turn on the cast and indexing lints (ISSUE-007)
- **p10:** Turn on the R8 and R9 gates (ISSUE-008)
- **p10:** Assertion density check for non-trivial functions (ISSUE-009)
- **p10:** The density check also skips pub test modules; file ISSUE-011
- **p10:** Gate the assertion density and the assert messages (ISSUE-009)
- **ste:** Docs-style skips the files with the Apache header (ISSUE-012)
- **release:** Start a release run by hand for a tag (ISSUE-013)

## [0.2.0] - 2026-10-08

### ⛰️  Features

- Import Grafana v2 rows layouts
- Preserve Classic dashboard rows
- Make dashboard state row aware
- Add interactive dashboard rows
- Export visible dashboard rows
- Add Grafana v2 RowsLayout support
- Add Grafana v2 tabs layout support
- Add Grafana v2 TabsLayout support
- Accept Grafana V2 resource YAML
- Add Grafana V2 AutoGridLayout support
- Support multi-value variables and dashboard repeats
- Support Grafana V2 conditional rendering
- Support Grafana V2 row and tab variables
- **theme:** Catppuccin, Tokyo Night and Gruvbox flavors
- **theme:** User-defined themes in config
- **ui:** Live theme picker
- **prom:** Trust a custom CA bundle and present a client certificate
- **examples:** HashiStack RDW operator dashboards for an mTLS Prometheus
- Load several dashboards, one per tab, querying only the shown one
- [**breaking**] Rebrand to grafana-tui and drop package-manager distribution

### 🐛 Bug Fixes

- Harden Grafana V2 import against real exports
- Read Grafana's converted legacy variable queries
- Harden the Prometheus client against leaks and cancellation
- Restore the terminal and survive export failures
- Reclaim repeat copies and keep refreshes from clobbering state
- Contain annotation command processes and bound file sources
- Shut down cleanly on SIGTERM and SIGHUP
- **query:** Scale the range query step with the time range
- **query:** Key in-flight range queries by millisecond step
- **app:** Refresh in the background so input never waits on Prometheus
- **query:** Keep last good data on errors and surface Prometheus warnings
- **variables:** Honor Grafana's refresh setting and report errors per variable
- **ui:** Format units, legends and stat/bar gauge values as Grafana does
- **ui:** Show noValue on empty panels, keep infos off the title, fit export text
- **hashistack-rdw:** Average Nomad client host CPU by instance
- **export:** Keep panel text inside its panel and off other text
- **deps:** Update rustls to 0.23.45 for RUSTSEC-2026-0285

### 📚 Documentation

- Demonstrate interactive Grafana rows
- Note V2 YAML support in the quick start
- Describe Grafana V2 schema support
- Convert example dashboards to Grafana V2
- Add production-readiness track and correct compatibility matrix
- **hashistack-rdw:** Sparse series are Nomad's only now -- Vault keeps rare series 24h, Consul 1h
- **hashistack-rdw:** Empty panels show noValue and infos have no marker since #25
- Replace mdBook with an Astro Starlight site

### 🚜 Refactor

- Carry dashboard layout through Grafana imports
- **theme:** Semantic color roles and full theme coverage

### 🎨 Styling

- Apply rustfmt and fix clippy lints

### ⚙️ Miscellaneous Tasks

- Update ratatui to 0.30.2
- Model the workflows on mrman

### Build

- [**breaking**] Drop Homebrew, crates.io and deb/rpm distribution

### Earlier versions

- The versions before 0.2.0 came from the upstream project. NOTICE gives the attribution.

<!-- generated by git-cliff -->
