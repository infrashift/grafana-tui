# SSHadowForge Dashboards

These dashboards cover SSHadowForge, the event-sourced Git forge that runs on the
infrashift hashistack platform, and the NATS JetStream bus its events travel on.
Five are for the operator who runs the forge. One is for the developers who push
to it, and it filters by repository.

They are Grafana 13 V2 resources (`dashboard.grafana.app/v2`). They run unchanged
in Grafana 13 and in grafana-tui, against the platform's mTLS Prometheus. They
are the companion set to [`../hashistack-rdw`](../hashistack-rdw/README.md),
which covers the platform underneath: the hosts, Nomad, Consul, Vault and the
mesh.

Unlike the bundled examples, these need a running datacenter with SSHadowForge
and the NATS exporter deployed. The docker-compose stack exposes none of these
metrics.

## Prerequisites

The same two as the HashiStack set: a route to the datacenter, and a reader
certificate from `make reader-cert`. See [the HashiStack
README](../hashistack-rdw/README.md#prerequisites).

The data comes from three places. All of them were added to the platform for
these dashboards:

- **The forge's own metrics.** The five sfd services (gateway, projector,
  notifier, dispatcher, workflow) each serve `/metrics` behind an mTLS
  "metrics door". Prometheus scrapes these as `job="app"`, and the workflow's
  as `job="nomad-app"`. Every `sfd_*` series carries `service`.
- **The bus.** prometheus-nats-exporter runs beside each NATS server, behind
  that server's own door. Its series carry `service="nats-<i>-metrics"`, and
  every durable consumer appears with `stream_name` and `consumer_name`. Before
  the exporter existed, the only bus signal was the projector's own backlog.
- **Nomad and Envoy.** Allocation CPU and memory carry `exported_job`, `task` and
  `namespace="sshadowforge"`. The mesh sidecars carry `service` and
  `envoy_cluster_name`.

## Quick Start

Build the config from [`grafana-tui.toml.example`](grafana-tui.toml.example) the
same way as for the HashiStack set. It is the same file, and one config serves
both sets. Then open all six dashboards:

```bash
cargo run --release -- --config ~/.config/grafana-tui/gcloud-dc.toml \
  $(printf -- '--grafana-json %s ' examples/demo/sshadowforge/*.json)
```

Tab and Shift+Tab switch dashboards, and `PgUp`/`PgDn` move between rows. Use a
terminal of 160 columns or more for a full row of stat tiles. To open both sets
at once, pass both globs.

## Dashboards

| File | Dashboard | For | Variables | What it answers |
| --- | --- | --- | --- | --- |
| `00-operator.json` | Operator Overview | operator | — | Is the forge healthy right now? Doors up, firing forge and bus alerts, stuck durables, bus backlog and gateway 5xx. Then the last hour of commands, refusals, builds and merges. |
| `10-event-pipeline.json` | Event Pipeline | operator | — | Which stage is slow or refusing? Covers gateway routes, authz latency and commands by kind; each durable's pending, ack-pending and deliveries; projection and notifier outcomes; the dispatcher's per-step timings and merge decisions; and workflow scheduling. |
| `20-nats-jetstream.json` | NATS JetStream | operator | `stream`, `consumer` | Is the bus healthy? Covers server health, connections, slow consumers and JetStream storage against its limit. Shows every durable (stuck, pending, ack-pending, redelivered, delivery rate) and every stream (messages, bytes, publish rate), plus the server's traffic and allocation. |
| `30-builds.json` | Builds and Supply Chain | both | — | What did the builds do? Results, where failures stopped, build time, CVE matches per repository, the re-scan sweep, challenge verdicts, and runner capacity, blocked evaluations and failed dispatches. |
| `40-developer.json` | Developer | developer | `repo` | What happened to my repositories, and is it me or the forge? Covers commands, builds and where they failed, build time, CVE matches, merge decisions with what each means, refusals before and after authorization, and notifications. |
| `50-resources.json` | Resources and Mesh | operator | — | What does the forge cost, and how do its services talk? Covers memory and CPU per job (dispatched runs under their parent), memory against reservation, the Dapr control plane, Envoy upstreams (Postgres, Keto, NATS, Hydra, git), git over SSH, and each sfd process's runtime. |

### The `repo` label

The per-repository series (`sfd_commands_total`, `sfd_builds_finished_total`,
`sfd_build_duration_seconds`, `sfd_scan_matches`, `sfd_merge_decisions_total`,
`sfd_build_requests_total`, `sfd_notifications_total`, and the sweep and
challenge counters) carry a repository NAME: `<owner>/<name>`, or `<name>` for a
shared repository.

A service names only a repository it has verified:

- one the gateway authorized;
- one the dispatcher resolved;
- one the workflow validated;
- one carried by an event whose seal the notifier checked.

Everything else is labelled `other`. So is every repository past
`SFD_METRICS_MAX_REPOS` (default 200) in one process.

A command refused before authorization carries `repo=""`, because its repository
tag is whatever the caller typed. The developer dashboard shows those
forge-wide, in "Refused before authorization": not enrolled, wrong key,
forbidden.

### Conventions

- **Colour means the same thing everywhere.** A tile that counts problems is
  green at 0 and red or amber above it. Thresholds follow the alert rules:
  - `NatsConsumerStuck`: no ack progress for 10m with a message in flight.
  - `NatsJetStreamStorageHigh`: above 80%.
  - `SshadowforgeGatewayErrors`: 5xx above 5%.
- **Row 1 of each dashboard is the summary.**
- **Dispatched jobs are summed under their parent.** A build runner, scan or
  challenge is a dispatched Nomad job (`<parent>/dispatch-<ts>-<id>`). Panels
  aggregate them by parent through `label_replace`, so a burst of builds is one
  series, not hundreds.

### What empty or odd-looking panels mean

- **Empty can be the healthy state.** These show `none (healthy)` or say why
  they are empty:
  - "Firing alerts", "Stuck durables", "Failed at stage".
  - The sweep and challenge panels, which fill only after the nightly (03:00)
    and weekly (Sunday 04:00) runs.
- **Per-repository counters reset when a service restarts**, like every counter.
  - A 24h panel shows what the current process has seen.
  - After a deploy, the build panels stay empty until a build finishes.
- **The stall clocks grow on an idle forge.** `sfd_progress_idle_seconds` is
  "time since work was completed". It is a problem only with a backlog, which is
  what the alert checks.
- **`SshadowforgeEventsRefused` fires after every `make verify`.** Check 45
  publishes forged events on purpose. Match their ids against the verify output.
- **`make verify` also leaves failed builds behind, on purpose.** The probe
  repositories of checks 28, 39 and 40 (`prupd-*`, `dcprobe/*`, `svcprobe/*`)
  build red by design, so they appear in "Dispatched runs failed (Nomad)" until
  Nomad garbage-collects them, and as `build_failed` in the build panels. A
  failure in any other repository is worth a look.

## Editing

The JSON files are generated; do not edit them by hand. Change
[`generator/generate.py`](generator/generate.py), which reuses the HashiStack
set's builder (`../hashistack-rdw/generator/dash.py`), then regenerate and
validate:

```bash
python3 examples/demo/sshadowforge/generator/generate.py
cargo test --test validate_cli sshadowforge
```

## Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| Every bus panel is empty | The NATS exporter is not deployed, or its door is refusing Prometheus. | Check `up{service=~"nats-.+-metrics"}`, and run `make verify` in terraform/live/nats and terraform/live/prometheus. |
| `repo` lists nothing | No per-repository series yet: nothing has been submitted or built since the services started. | Push to a repository, or widen `--range`. |
| A repository shows as `other` | The name was not yet resolved, or the process reached `SFD_METRICS_MAX_REPOS`. | A growing `repo="other"` series means the cap was reached. |
| Everything errors after a day | The reader certificate expired (24h). | Re-run `make reader-cert`. |
