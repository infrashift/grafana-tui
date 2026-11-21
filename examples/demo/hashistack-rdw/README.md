# HashiStack RDW Dashboards

These are operator dashboards for the infrashift hashistack
remote-dev-workspace (RDW) platform. They cover Consul, Nomad and Vault, the
Consul Connect (Envoy) mesh, the hosts, the developer workspaces and Prometheus
itself. They are Grafana 13 V2 resources (`dashboard.grafana.app/v2`). They run
unchanged in Grafana 13 and in grafana-tui, against the mTLS Prometheus of the
platform.

Unlike the other examples, these need a hashistack datacenter that is in operation. The
bundled docker-compose stack does not expose any of these metrics.

## Prerequisites

- **A route to the datacenter.** For gcloud-dc that is the WireGuard tunnel
  (`hashistack-gcp`). Prometheus is off-mesh on a node IP, so the sm-jump tunnel
  is not needed.
- **A reader certificate.** Prometheus requires TLS 1.3 and a client certificate
  (`RequireAndVerifyClientCert`). Only the client certificate gives access.
  `make reader-cert` issues one from Vault's `prometheus_pki`. It
  is valid for 24h and is re-issued, never renewed. It needs the operator's Vault
  token:

  ```bash
  cd <collection>/terraform/live/prometheus
  eval "$(make -s -C ../devops/namespaces/prometheus DC=gcloud-dc creds </dev/null)"
  ```

## Quick Start

Build the config from the settings in [`grafana-tui.toml.example`](grafana-tui.toml.example)
followed by the reader-cert output. The settings must come first; the example
file explains why. Keep the result outside this repository.

```bash
{ sed -n '/^# --- settings/,/^# --- end/p' examples/demo/hashistack-rdw/grafana-tui.toml.example
  make -s -C <collection>/terraform/live/prometheus DC=gcloud-dc reader-cert READER=$USER </dev/null
} > ~/.config/grafana-tui/gcloud-dc.toml

cargo run --release -- --config ~/.config/grafana-tui/gcloud-dc.toml \
  $(printf -- '--grafana-json %s ' examples/demo/hashistack-rdw/*.json)
```

This opens all eight dashboards, one per tab of the bar at the top: Tab and
Shift+Tab switch between them. Only the dashboard shown queries Prometheus, so
eight cost no more than one. To open only one, pass a single `--grafana-json`.
A terminal of 160 columns or more
shows a full row of stat tiles clearly. At 100 columns every panel still shows,
but the tiles have little space. Use `PgUp`/`PgDn` to move between rows.

## Dashboards

| File | Dashboard | Variables | What it answers |
| --- | --- | --- | --- |
| `00-overview.json` | Fleet Overview | — | Is the datacenter healthy right now? Covers firing/pending alerts, targets down, Vault seal, raft quorum, failed platform units, certificate runway (agent TLS, CA, mesh leaf), live sidecars, mesh 5xx, host and Nomad capacity headroom, and allocation states. |
| `10-nodes.json` | Nodes | `instance` | Host saturation and errors from node_exporter. Covers CPU, load per core, iowait/steal, memory composition, page faults, root filesystem plus a 24h fill projection, disk latency, NIC errors, TCP retransmits, conntrack, clock sync, SELinux mode and platform systemd units. |
| `20-nomad.json` | Nomad | `node_pool`, `namespace`, `job` | Covers autopilot and failure tolerance, the eval broker, blocked evals, plan queue and latency, raft, and client CPU/memory reservations by node pool. Per-job and per-task CPU, memory, throttling, and tasks running above their memory reservation (on `memory_max` oversubscription). |
| `30-consul.json` | Consul | `instance` | Covers leader count, autopilot, catalog size, raft and RPC (errors and rate limiting), blocking queries, agent-to-server RPC failures, KV/txn/ACL latency, agent TLS and Connect CA runway, and sidecars connected to xDS. |
| `40-vault.json` | Vault | — | Covers seal and HA state, request rate and latency, responses by status code, leases (including irrevocable ones), the expiration queue, PKI issuance per mount, token churn, raft/storage latency and runtime. |
| `50-service-mesh.json` | Service Mesh (Envoy) | `namespace`, `service` | Golden signals at each sidecar's public listener: requests, 4xx/5xx, p50/p95/p99. Also TCP traffic, upstream connections, connect failures, endpoint health, retries and circuit breakers, mTLS handshakes and failures, leaf-certificate runway, sidecar memory and restarts. |
| `60-rdw-workspaces.json` | Remote Dev Workspaces | `developer`, `workspace` | Developer workspaces (`<developer>-devpod-<seed>` jobs) and companion databases. Covers CPU and memory per workspace against the reservation, memory by task role (IDE, git/LLM broker, database, Envoy), placement, SSH (bastion) and web (Traefik) sessions, default-pool headroom, and the health of the onboarding, identity, GitLab and Nexus planes. |
| `90-prometheus.json` | Prometheus | — | Targets by job, scrape duration and cost, TSDB growth against the 8GB / 15d retention, WAL and chunk storage, rule-group duration, the alert list, query latency and process resources. |

### Conventions

- **Colour means the same thing everywhere.** A tile that counts problems is
  green at 0 and red (or amber) at 1 or more. Thresholds follow the platform's
  alert rules (`hashistack.rules.yml`):
  - agent TLS certificate red below 4h
  - CA red below 30d
  - root filesystem red above 90%
  - Connect leaf red below 2h, since leaves rotate with roughly 7–29h left
- **Row 1 of each dashboard is the summary.** Read it first; the rows below are
  for diagnosis.
- **Labels.** `instance` is the Consul node name on every job. Nomad's own job
  label arrives as `exported_job`. Envoy's `service` is the parent service, not
  the registered `-sidecar-proxy` service. `datacenter` is a Prometheus external
  label, so a query cannot use it, and the dashboards have no DC variable. Point the
  config at another datacenter's Prometheus to switch.
- **Grafana.** Every query references the `${datasource}` variable (a Prometheus
  datasource picker). The dashboards set legends, tooltips and table column
  overrides for Grafana. grafana-tui ignores them.

### What empty or odd-looking panels mean

- **Empty can be the healthy state.** These panels are empty when nothing is
  wrong, and show `none (healthy)`:
  - "Active alerts", "Targets down", "Platform units not active".
  - "Upstream connect failures / timeouts", "Upstream HTTP 5xx", "TLS failures".
- **HTTP panels are empty for TCP services.** Many mesh services are TCP in
  Consul (gitlab-http, the postgres services, ssh). They have no HTTP statistics:
  use the TCP rows. Inbound HTTP traffic is sparse in a lab, so latency
  quantiles use a 5m window and still show gaps.
- **Sparse Nomad series come and go.** Each exporter keeps a go-metrics counter
  or timer for a fixed time window. If the counter or timer gets no update in
  that window, it disappears. Later it returns with a reset value. Nomad (1.11)
  has no setting for this window: the window is a fixed 60s. So Nomad plan
  latency and other rare Nomad counters show gaps or NaN between events. Their
  rates also count fewer rare events than occurred. Vault (24h) and Consul (1h)
  keep rare series, for example the PKI certificates that Vault issues, token
  operations and KV/txn latency. The 1h window of Consul has a cost. A gauge for
  an item that is gone, for example a deregistered service, keeps its last value
  for up to an hour.

## Editing

A generator makes the JSON files. Do not edit them by hand. To change the
dashboards, do these steps:

1. Change the panels, queries and thresholds in
   [`generator/generate.py`](generator/generate.py).
2. Change the V2 resource builder and its Grafana display defaults in
   [`generator/dash.py`](generator/dash.py).
3. Run the generator and the test:

```bash
python3 examples/demo/hashistack-rdw/generator/generate.py
cargo test --test validate_cli validate_strict_accepts_every_hashistack_rdw_dashboard
```

The generator uses only the Python 3 standard library. It never writes value
mappings or transformations, because the `--strict` check of grafana-tui rejects
them.

## Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| Every panel errors after a day | The reader certificate expired (24h). | Re-run `make reader-cert` and rebuild the config. |
| `client certificate required` / handshake alert | The config has no client certificate. | Use the reader-cert output, including `client_cert` and `client_key`. |
| Certificate name mismatch | `prometheus_url` uses a name or IP not in the server certificate's SANs. | Use `make prom-url`. The certificate carries the node IP, `prometheus.service.consul` and `localhost`. |
| Connection timed out | No route to the datacenter. | Bring up the WireGuard tunnel. `make prom-url` also needs a Consul token. |
| Variables list nothing | The time range predates the data. | Variables use the labels API over the dashboard range; widen `--range`. |

Check the dashboards offline. This check does not need Prometheus:

```bash
for f in examples/demo/hashistack-rdw/*.json; do cargo run --release -- --validate --strict --grafana-json "$f"; done
```
