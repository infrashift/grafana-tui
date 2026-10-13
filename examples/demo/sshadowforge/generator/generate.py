"""Generate the SSHadowForge dashboards.

The JSON files one directory up are this script's output; edit here, not there:

    python3 examples/demo/sshadowforge/generator/generate.py [OUT_DIR]

OUT_DIR defaults to examples/demo/sshadowforge/. Then re-validate:
    cargo test --test validate_cli sshadowforge

The V2 resource builder is hashistack-rdw's (../../hashistack-rdw/generator/dash.py), imported rather
than copied, so both sets render the same way. Every metric and label below was checked against
gcloud-dc's Prometheus on 2026-10-13 (see ../README.md for what each series is and who exports it).
"""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "hashistack-rdw", "generator"))

from dash import (AMBER, GREEN, NEUTRAL, ONE_GOOD, RED, ZERO_GOOD, ZERO_GOOD_AMBER, bargauge,  # noqa: E402
                  dashboard, empty_ok, query_var, stat, steps, table, ts, write)

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(HERE)
os.makedirs(OUT, exist_ok=True)
TAGS = ("sshadowforge", "forge", "gcloud-dc")
RI = "$__rate_interval"
LAT = "5m"  # histogram window: forge traffic is sparse, 1m windows are mostly empty
RATIO_ERR = steps(GREEN, (0.01, AMBER), (0.05, RED))
PASS_RATE = steps(RED, (0.8, AMBER), (0.95, GREEN))
NATS_STORAGE = steps(GREEN, (0.7, AMBER), (0.8, RED))  # NatsJetStreamStorageHigh: > 80%

# The doors (PLAN-SSHadowForge-Review-20261012 E3): gateway, projector, notifier and dispatcher are
# Consul-registered (job="app"); the workflow's is Nomad-registered (job="nomad-app",
# service="sshadowforge-workflow-metrics"). Every sfd_* series carries the door's `service`.
SFD = 'service=~"sshadowforge-.+"'
SFD_NS = 'namespace="sshadowforge"'
GW = 'service="sshadowforge-gateway"'
QUIET = 'route!~"/healthz|/progressz|/stats"'
# The bus (PLAN-SSHadowForge-Dashboards-20261012 Part B): prometheus-nats-exporter behind each
# server's door, service nats-<i>-metrics, job="app". Labels stream_name / consumer_name.
NATS = 'service=~"nats-.+-metrics"'
SFD_DURABLES = 'consumer_name=~"sshadowforge-.+"'
DUR = "stream_name, consumer_name"
# Dispatched jobs arrive as exported_job="<parent>/dispatch-<ts>-<id>": one series per run, garbage
# collected with the run. Every panel over them aggregates by the PARENT, via label_replace.
PARENT = 'label_replace({0}, "parent", "$1", "exported_job", "([^/]+)(/.*)?")'
RUNNERS = 'exported_job=~"sshadowforge-(runner-.+|challenge|scan|advise)/dispatch-.+"'


# YOUNG COUNTERS. A labelled counter is created at its first increment, so Prometheus's first sample of
# it is already 1 and increase() -- last minus first -- never counts that increment. On a forge this
# quiet MOST per-repository series are young: the first live render showed "Commands refused 0" an hour
# after six refusals. Every windowed count below is therefore rewritten (young_counters) so a series
# born inside the window also contributes its first sample:
#
#     increase(x[w]) + (min_over_time(x[w]) unless x offset w)     for a series absent w ago
#     increase(x[w])                                               otherwise (resets handled as usual)
COUNTERS = ("sfd_commands_total", "sfd_builds_finished_total", "sfd_merge_decisions_total",
            "sfd_build_requests_total", "sfd_notifications_total", "sfd_rescans_finished_total",
            "sfd_challenges_finished_total", "sfd_stats_total", "gnatsd_varz_slow_consumers",
            "gnatsd_varz_jetstream_stats_api_errors", "envoy_cluster_upstream_cx_connect_fail")
_INC = re.compile(r"\bincrease\((" + "|".join(COUNTERS) + r")(\{[^}]*\})?\[(\w+)\]\)")


def _young(m):
    sel, w = m.group(1) + (m.group(2) or ""), m.group(3)
    return f"((increase({sel}[{w}]) + (min_over_time({sel}[{w}]) unless {sel} offset {w})) or increase({sel}[{w}]))"


# The same for the per-repository build-time histogram: a repository's buckets are born at its first
# build, so rate() over them is 0 and histogram_quantile() says nothing until its SECOND build. The
# quantile needs only the buckets' ratio, so the young-aware increase stands in for rate().
_HIST = re.compile(r"\brate\((sfd_build_duration_seconds_bucket)(\{[^}]*\})?\[(\w+)\]\)")


def young_counters(dash):
    for el in dash["spec"]["elements"].values():
        for q in el["spec"]["data"]["spec"]["queries"]:
            spec = q["spec"]["query"]["spec"]
            spec["expr"] = _HIST.sub(_young, _INC.sub(_young, spec["expr"]))
    return dash


def stuck(selector):
    """Durables holding an unacked message whose ack floor has not moved for 10m (NatsConsumerStuck)."""
    return (f'(max by ({DUR}) (jetstream_consumer_num_ack_pending{{{selector}}}) > 0) and on ({DUR}) '
            f'(max by ({DUR}) (changes(jetstream_consumer_ack_floor_stream_seq{{{selector}}}[10m])) == 0)')


def gateway_5xx_ratio(window=RI):
    return (f'(sum(rate(sfd_http_request_duration_seconds_count{{{GW},code=~"5.."}}[{window}])) or vector(0)) / '
            f'clamp_min(sum(rate(sfd_http_request_duration_seconds_count{{{GW}}}[{window}])), 1e-9)')


# ---------------------------------------------------------------- 00 operator overview
operator = dashboard(
    "sshadowforge-operator", "SSHadowForge / Operator Overview",
    "Is the forge healthy right now? Doors, alerts, the event bus, the gateway, builds and merges on one "
    "screen. Row 1 is the summary: every tile green. The rows below say where to look next.",
    ["operator"],
    [
        ("Health -- every tile should be green", [
            stat("Forge doors up", f'count(up{{job=~"app|nomad-app",{SFD}}} == 1) or vector(0)',
                 th=steps(RED, (5, GREEN)), graph=False,
                 desc="Metrics doors answering: gateway, projector, notifier, dispatcher and the workflow's. "
                 "Fewer than 5 is a service down, or a door refusing Prometheus."),
            stat("NATS doors up", f'count(up{{{NATS}}} == 1) or vector(0)', th=ONE_GOOD, graph=False,
                 desc="The exporter behind each NATS server's metrics door (one per server)."),
            stat("Firing alerts", 'count(ALERTS{alertstate="firing",alertname=~"Sshadowforge.+|Nats.+"}) or vector(0)',
                 th=ZERO_GOOD, graph=False,
                 desc="Forge and bus alerts. SshadowforgeEventsRefused fires on purpose for a while after every "
                 "`make verify` (check 45 publishes forged events)."),
            stat("Durables stuck", f'count({stuck(SFD_DURABLES)}) or vector(0)', th=ZERO_GOOD, graph=False,
                 desc="SSHadowForge durables holding an unacknowledged message with no progress for 10m. Every "
                 "one runs MaxAckPending 1, so ONE stuck message blocks every later event for every repository."),
            stat("Bus backlog", f'max(jetstream_consumer_num_pending{{{SFD_DURABLES}}}) or vector(0)',
                 th=ZERO_GOOD_AMBER, desc="The largest backlog of any SSHadowForge durable on SF_WORK_LOG."),
            stat("Gateway 5xx", gateway_5xx_ratio(), unit="percentunit", th=RATIO_ERR, decimals=2,
                 desc="Share of gateway requests answered 5xx. A 503 is often the auth seal failing closed: "
                 "the gateway could not reach Vault to sign."),
        ]),
        ("The last hour", [
            stat("Commands accepted", 'sum(increase(sfd_commands_total{result="accepted"}[1h])) or vector(0)',
                 desc="Issues, pull requests, comments, approvals... admitted by the gateway in the last hour."),
            stat("Commands refused", 'sum(increase(sfd_commands_total{result!="accepted"}[1h])) or vector(0)',
                 th=ZERO_GOOD_AMBER, desc="Refused by the gateway: schema, signature, authorization, enrolment, "
                 "or an unavailable dependency. The Event Pipeline dashboard says which."),
            stat("Events refused", f'sum(increase(sfd_stats_total{{name=~"rejected|dropped_unsealed|dropped_unsigned|'
                 f'dropped_invalid",{SFD}}}[1h])) or vector(0)', th=ZERO_GOOD_AMBER,
                 desc="Events the CONSUMERS refused (no seal, bad signature, contract). Nothing the gateway "
                 "admits is refused, so outside a verify run this means something else wrote to the bus."),
            stat("Builds finished", 'sum(increase(sfd_builds_finished_total[1h])) or vector(0)',
                 desc="Build workflows that reached an outcome."),
            stat("Builds not passed", 'sum(increase(sfd_builds_finished_total{result!="passed"}[1h])) or vector(0)',
                 th=ZERO_GOOD_AMBER, desc="build_failed, scan_rejected, scan_error or refused. Builds dashboard."),
            stat("Merged", 'sum(increase(sfd_merge_decisions_total{decision="merged"}[1h])) or vector(0)',
                 desc="Pull requests the dispatcher merged."),
        ]),
        ("Trend", [
            ts("Bus backlog by durable", [(f'max by (consumer_name) (jetstream_consumer_num_pending{{{SFD_DURABLES}}})',
                                           '{{consumer_name}}')], unit="short", w=12,
               desc="Pending messages per SSHadowForge durable. A backlog that does not drain while ack-pending "
               "stays at 1 is the F8 wedge."),
            ts("Commands by result", [('sum by (result) (rate(sfd_commands_total[5m]))', '{{result}}')],
               unit="ops", w=12, desc="Gateway admissions and each refusal reason."),
            ts("Builds by result (per hour)", [('sum by (result) (increase(sfd_builds_finished_total[1h]))',
                                                '{{result}}')], unit="short", w=12, draw="bars", fill=60, stack=True,
               desc="Rolling one-hour count of finished builds, by result."),
            empty_ok(table("Firing alerts", [('ALERTS{alertstate="firing",alertname=~"Sshadowforge.+|Nats.+"}',
                                              '{{alertname}}')], w=12, sort_desc=False,
                           desc="Forge and bus alerts firing now, with their labels.")),
        ]),
    ],
    base_tags=TAGS)

# ---------------------------------------------------------------- 10 event pipeline
pipeline = dashboard(
    "sshadowforge-event-pipeline", "SSHadowForge / Event Pipeline",
    "Each stage of the event pipeline: the gateway admitting commands, the bus, the projector and "
    "notifier consuming, the dispatcher merging and requesting builds, the workflow scheduling them. "
    "Includes the seal refusals and the dispatcher's per-step timings that say where its time goes.",
    ["pipeline"],
    [
        ("Summary", [
            stat("Projector backlog", f'max(sfd_consumer_pending{{{SFD}}}) or vector(0)', th=ZERO_GOOD_AMBER,
                 desc="Events on the bus the projector has not applied yet (its own view of its durable)."),
            stat("Projector idle", 'max(sfd_progress_idle_seconds{service="sshadowforge-projector"})', unit="s",
                 desc="Seconds since the projector last completed work. Only a stall WITH a backlog is a problem "
                 "(SshadowforgeProjectorStalled)."),
            stat("Commands p95", f'histogram_quantile(0.95, sum by (le) (rate(sfd_http_request_duration_seconds_bucket'
                 f'{{{GW},route="/v1/commands"}}[{LAT}])))', unit="s",
                 th=steps(GREEN, (1, AMBER), (2.5, RED)), decimals=2,
                 desc="Time to admit one command: introspection, the Keto check, the Vault seal and the publish. "
                 "Measured baseline p99 ~0.66 s."),
            stat("Dispatcher p95 per event", f'histogram_quantile(0.95, sum by (le) (rate('
                 f'sfd_http_request_duration_seconds_bucket{{service="sshadowforge-dispatcher",'
                 f'route="/events/disposition"}}[{LAT}])))', unit="s", th=steps(GREEN, (5, AMBER), (15, RED)),
                 decimals=1, desc="One approval or disposition, end to end. Everything behind it waits this long."),
            stat("Refused events (1h)", f'sum(increase(sfd_stats_total{{name=~"rejected|dropped_unsealed",{SFD}}}[1h])) '
                 'or vector(0)', th=ZERO_GOOD_AMBER, desc="Events the consumers refused: no valid gateway seal, "
                 "a bad signature or a contract violation."),
            stat("Workflow requests refused (1h)", 'sum(increase(sfd_stats_total{service="sshadowforge-workflow-metrics",'
                 'name=~"build_requests_refused|build_requests_malformed|builds_rejected"}[1h])) or vector(0)',
                 th=ZERO_GOOD_AMBER, desc="Build requests the workflow refused or could not parse."),
        ]),
        ("Gateway", [
            ts("Requests by route", [(f'sum by (route) (rate(sfd_http_request_duration_seconds_count{{{GW},{QUIET}}}[{RI}]))',
                                      '{{route}}')], unit="reqps", w=12,
               desc="Route PATTERNS (bounded cardinality), never raw paths."),
            ts("Latency p95 by route", [(f'histogram_quantile(0.95, sum by (route, le) (rate('
                                         f'sfd_http_request_duration_seconds_bucket{{{GW},{QUIET}}}[{LAT}])))',
                                         '{{route}}')], unit="s", w=12),
            ts("Authz latency p95", [(f'histogram_quantile(0.95, sum by (call, le) (rate('
                                      f'sfd_authz_call_duration_seconds_bucket[{LAT}])))', '{{call}}')], unit="s", w=12,
               desc="Hydra introspection and Keto checks, per gateway request (baseline p99 ~75 ms)."),
            ts("Commands by kind and result", [('sum by (kind, result) (rate(sfd_commands_total[5m]))',
                                                '{{kind}} {{result}}')], unit="ops", w=12,
               desc="What is being submitted and what became of it."),
        ]),
        ("Bus (SF_WORK_LOG)", [
            ts("Pending by durable", [('max by (consumer_name) (jetstream_consumer_num_pending{stream_name="SF_WORK_LOG"})',
                                       '{{consumer_name}}')], unit="short", w=8,
               desc="Messages each durable has yet to receive."),
            ts("Ack pending by durable", [('max by (consumer_name) (jetstream_consumer_num_ack_pending'
                                           '{stream_name="SF_WORK_LOG"})', '{{consumer_name}}')], unit="short", w=8,
               desc="Delivered, not yet acknowledged. With MaxAckPending 1 this is 0 or 1; 1 for long is a wedge."),
            ts("Deliveries by durable", [(f'sum by (consumer_name) (rate(jetstream_consumer_delivered_stream_seq'
                                          f'{{stream_name="SF_WORK_LOG"}}[{LAT}]))', '{{consumer_name}}')],
               unit="ops", w=8, desc="Messages delivered per second (the delivered sequence advancing)."),
        ]),
        ("Projector and notifier", [
            ts("Projection rate by outcome", [(f'sum by (outcome) (rate(sfd_projection_seconds_count[{RI}]))',
                                               '{{outcome}}')], unit="ops", w=12,
               desc="applied / skipped / transient (retried in place) / rejected (poison, terminated)."),
            ts("Projection p95 by kind", [(f'histogram_quantile(0.95, sum by (kind, le) (rate('
                                           f'sfd_projection_seconds_bucket[{LAT}])))', '{{kind}}')], unit="s", w=12),
            ts("Notifier outcomes", [(f'sum by (name) (rate(sfd_stats_total{{service="sshadowforge-notifier"}}[{RI}]))',
                                      '{{name}}')], unit="ops", w=12,
               desc="consumed / notified / ignored / retried / abandoned / rejected (chat posts to chit-server)."),
            ts("Stall clocks", [(f'sfd_progress_idle_seconds{{service=~"sshadowforge-(projector|notifier)"}}',
                                 '{{service}} ({{instance}})')], unit="s", w=12,
               desc="Seconds since each consumer last completed work. It grows on an idle forge; it matters only "
               "with a backlog."),
        ]),
        ("Dispatcher and workflow", [
            ts("Dispatcher step p95", [(f'histogram_quantile(0.95, sum by (step, le) (rate('
                                        f'sfd_dispatcher_step_seconds_bucket[{LAT}])))', '{{step}}')], unit="s", w=12,
               desc="Where one event's time goes. head_resolve (git ls-remote) is the measured tail."),
            ts("Dispatcher step time share", [(f'sum by (step) (rate(sfd_dispatcher_step_seconds_sum[{LAT}]))',
                                               '{{step}}')], unit="s", w=12,
               desc="Seconds per second spent in each step: the budget, not the latency."),
            ts("Merge decisions", [('sum by (decision) (rate(sfd_merge_decisions_total[5m]))', '{{decision}}')],
               unit="ops", w=12, desc="gated (a requirement unmet, usually the build), merged, stale_head, "
               "stale_base, conflict, not_fast_forward, already-merged, transient."),
            ts("Workflow scheduling", [(f'sum by (name) (rate(sfd_stats_total{{service="sshadowforge-workflow-metrics"}}[{RI}]))',
                                        '{{name}}')], unit="ops", w=12,
               desc="builds_from_bus, builds_scheduled, deduped, refused; sweeps_*, challenges_*."),
        ]),
    ],
    base_tags=TAGS)

# ---------------------------------------------------------------- 20 NATS JetStream
NV = f'{NATS}'
STREAM = 'stream_name=~"$stream"'
CONSUMER = 'stream_name=~"$stream",consumer_name=~"$consumer"'
nats = dashboard(
    "sshadowforge-nats-jetstream", "SSHadowForge / NATS JetStream",
    "The event bus: the NATS server, JetStream storage, each stream and every durable consumer on it -- "
    "SSHadowForge's four (projector, notifier, dispatcher, workflow) and artifact-governance's. Series from "
    "prometheus-nats-exporter behind each server's mTLS metrics door.",
    ["nats", "jetstream"],
    [
        ("Server -- every tile should be green", [
            stat("Healthz", f'min(gnatsd_healthz_status_value{{{NV},value="ok"}}) or vector(0)', th=ONE_GOOD,
                 graph=False, desc="NATS /healthz answered ok (1) on every server, as the exporter read it."),
            stat("Uptime", f'time() - max(gnatsd_varz_start{{{NV}}}) / 1000', unit="s", graph=False,
                 desc="Since the server last started. A restart replaces the allocation; JetStream state survives on "
                 "its sticky host volume."),
            stat("Connections", f'sum(gnatsd_varz_connections{{{NV}}})',
                 desc="Client connections: the forge's services, daprd sidecars, governance, the gateway hop."),
            stat("Slow consumers (1h)", f'sum(increase(gnatsd_varz_slow_consumers{{{NV}}}[1h])) or vector(0)',
                 th=ZERO_GOOD_AMBER, desc="Clients NATS disconnected for not reading fast enough."),
            stat("JetStream storage", f'max(gnatsd_varz_jetstream_stats_storage{{{NV}}} / '
                 f'gnatsd_varz_jetstream_config_max_storage{{{NV}}})', unit="percentunit", th=NATS_STORAGE, decimals=1,
                 desc="File storage used against max_file_store (NatsJetStreamStorageHigh at 80%)."),
            stat("API errors (1h)", f'sum(increase(gnatsd_varz_jetstream_stats_api_errors{{{NV}}}[1h])) or vector(0)',
                 th=ZERO_GOOD_AMBER, desc="JetStream API calls that errored (create/lookup consumer, publish acks)."),
        ]),
        ("Consumers", [
            empty_ok(table("Stuck durables", [(stuck(CONSUMER), '{{consumer_name}}')], w=12, th=ZERO_GOOD,
                           desc="Durables with an unacknowledged message and no ack progress for 10m. Empty is "
                           "healthy."), "none (healthy)"),
            table("Pending by durable", [(f'max by ({DUR}) (jetstream_consumer_num_pending{{{CONSUMER}}})',
                                          '{{consumer_name}}')], w=12, th=ZERO_GOOD_AMBER, decimals=0,
                  desc="Messages each durable has yet to receive. 0 is caught up."),
            ts("Pending", [(f'max by (consumer_name) (jetstream_consumer_num_pending{{{CONSUMER}}})',
                            '{{consumer_name}}')], unit="short", w=12),
            ts("Ack pending", [(f'max by (consumer_name) (jetstream_consumer_num_ack_pending{{{CONSUMER}}})',
                                '{{consumer_name}}')], unit="short", w=12,
               desc="Delivered and unacknowledged. The SSHadowForge durables run MaxAckPending 1."),
            ts("Delivery rate", [(f'sum by (consumer_name) (rate(jetstream_consumer_delivered_stream_seq{{{CONSUMER}}}[{LAT}]))',
                                  '{{consumer_name}}')], unit="ops", w=12),
            ts("Redelivered", [(f'max by (consumer_name) (jetstream_consumer_num_redelivered{{{CONSUMER}}})',
                                '{{consumer_name}}')], unit="short", w=12,
               desc="Messages currently redelivered (nak, retry or AckWait expiry). A durable retrying one message "
               "for ever shows here before it shows as stuck."),
        ]),
        ("Streams", [
            ts("Messages by stream", [(f'max by (stream_name) (jetstream_stream_total_messages{{{STREAM}}})',
                                       '{{stream_name}}')], unit="short", w=8,
               desc="SF_WORK_LOG keeps 90 days (limits); SF_INTEL is a work queue; SF_GOVERNANCE has no max age."),
            ts("Bytes by stream", [(f'max by (stream_name) (jetstream_stream_total_bytes{{{STREAM}}})',
                                    '{{stream_name}}')], unit="bytes", w=8),
            ts("Publish rate by stream", [(f'sum by (stream_name) (rate(jetstream_stream_last_seq{{{STREAM}}}[{LAT}]))',
                                           '{{stream_name}}')], unit="ops", w=8,
               desc="The stream's last sequence advancing: messages stored per second."),
        ]),
        ("Server traffic", [
            ts("Messages in / out", [(f'sum(rate(gnatsd_varz_in_msgs{{{NV}}}[{RI}]))', 'in'),
                                     (f'sum(rate(gnatsd_varz_out_msgs{{{NV}}}[{RI}]))', 'out')], unit="ops", w=8),
            ts("Bytes in / out", [(f'sum(rate(gnatsd_varz_in_bytes{{{NV}}}[{RI}]))', 'in'),
                                  (f'sum(rate(gnatsd_varz_out_bytes{{{NV}}}[{RI}]))', 'out')], unit="Bps", w=8),
            ts("JetStream API", [(f'sum(rate(gnatsd_varz_jetstream_stats_api_total{{{NV}}}[{RI}]))', 'calls'),
                                 (f'sum(rate(gnatsd_varz_jetstream_stats_api_errors{{{NV}}}[{RI}]))', 'errors')],
               unit="ops", w=8),
            ts("Server memory", [(f'max(gnatsd_varz_mem{{{NV}}})', 'nats-server RSS')], unit="bytes", w=8),
            ts("Allocation memory by task", [('sum by (task) (nomad_client_allocs_memory_usage{namespace="nats",'
                                              'exported_job="nats"})', '{{task}}')], unit="bytes", w=8,
               desc="nats, the exporter and its metrics door (Nomad allocation metrics)."),
            ts("Allocation CPU by task", [('sum by (task) (nomad_client_allocs_cpu_total_percent{namespace="nats",'
                                           'exported_job="nats"}) / 100', '{{task}}')], unit="short", decimals=2, w=8),
        ]),
    ],
    [query_var("stream", "Stream", f'label_values(jetstream_stream_total_messages{{{NATS}}}, stream_name)'),
     query_var("consumer", "Durable",
               f'label_values(jetstream_consumer_num_pending{{{NATS},stream_name=~"$stream"}}, consumer_name)')],
    base_tags=TAGS)

# ---------------------------------------------------------------- 30 builds
builds = dashboard(
    "sshadowforge-builds", "SSHadowForge / Builds and Supply Chain",
    "Every build the forge ran: results and where failures stop, wall time, what the scans found, the "
    "re-scan sweep and challenge rebuilds, and the runner capacity they compete for.",
    ["builds", "supply-chain"],
    [
        ("Last 24 hours", [
            stat("Builds", 'sum(increase(sfd_builds_finished_total[24h])) or vector(0)',
                 desc="Build workflows that reached an outcome (per-process counters; a restart resets them)."),
            empty_ok(stat("Pass rate", 'sum(increase(sfd_builds_finished_total{result="passed"}[24h])) / '
                 'clamp_min(sum(increase(sfd_builds_finished_total[24h])), 1e-9)', unit="percentunit", th=PASS_RATE,
                 decimals=0, desc="Passed / finished. Empty until a build finishes."), "no build since the workflow started"),
            stat("Build p95", 'histogram_quantile(0.95, sum by (le) (rate(sfd_build_duration_seconds_bucket[24h])))',
                 unit="s", decimals=0, desc="Workflow wall time: build, publish, scan and advisory."),
            stat("Scan rejected", 'sum(increase(sfd_builds_finished_total{result="scan_rejected"}[24h])) or vector(0)',
                 th=ZERO_GOOD_AMBER, desc="Builds the scan gate (SFD_SCAN_FAIL_ON) refused."),
            stat("Runners running", f'count(count by (exported_job) (nomad_client_allocs_memory_usage{{{RUNNERS}}})) '
                 'or vector(0)', desc="Dispatched runner, scan, advise and challenge jobs with a live allocation."),
            stat("Blocked evals", f'count(nomad_nomad_blocked_evals_job_memory{{{SFD_NS}}}) or vector(0)',
                 th=ZERO_GOOD_AMBER, desc="SSHadowForge jobs Nomad cannot place (F7: DimensionExhausted). A build "
                 "waits here before it starts."),
        ]),
        ("Results", [
            ts("Builds by result (per hour)", [('sum by (result) (increase(sfd_builds_finished_total[1h]))', '{{result}}')],
               unit="short", w=12, draw="bars", fill=60, stack=True,
               desc="passed, build_failed, scan_rejected, scan_error, refused."),
            empty_ok(bargauge("Failed at stage (24h)", [('sum by (failed_stage) (increase(sfd_builds_finished_total'
                                                '{result="build_failed"}[24h]))', '{{failed_stage}}')],
                     th=steps(AMBER), w=12, decimals=0, desc="The runner task a failed build stopped at. The sentinel chain makes "
                     "the first failed task the cause; later ones are skipped."), "no failed build (healthy)"),
            ts("Build duration p50 / p95", [('histogram_quantile(0.5, sum by (le) (rate(sfd_build_duration_seconds_bucket[1h])))',
                                             'p50'),
                                            ('histogram_quantile(0.95, sum by (le) (rate(sfd_build_duration_seconds_bucket[1h])))',
                                             'p95')], unit="s", w=12, desc="Over a rolling hour of finished builds."),
            table("Builds by repository (24h)", [('sum by (repo, result) (increase(sfd_builds_finished_total[24h])) > 0',
                                                  '{{repo}} {{result}}')], w=12, decimals=0),
        ]),
        ("Scans", [
            empty_ok(bargauge("CVE matches, latest scan", [('max by (repo) (sfd_scan_matches)', '{{repo}}')],
                     th=steps(GREEN, (1, AMBER), (25, RED)), w=8, decimals=0,
                     desc="Matches in the most recent scan of each repository: a build's scan or a re-scan."), "no scan read since the workflow started"),
            empty_ok(ts("Re-scan sweep (per day)", [('sum by (result) (increase(sfd_rescans_finished_total[24h]))', '{{result}}')],
               unit="short", w=8, desc="Published revisions re-scanned by the nightly sweep: clean, matches, failed."), "no sweep since the workflow started (nightly, 03:00)"),
            empty_ok(ts("Challenge verdicts (per week)", [('sum by (verdict) (increase(sfd_challenges_finished_total[7d]))',
                                                  '{{verdict}}')], unit="short", w=8,
               desc="Weekly rebuilds of published revisions compared with what was published: bit_identical, "
               "content_equivalent, divergent, failed, not_applicable."), "no challenge since the workflow started (Sundays, 04:00)"),
        ]),
        ("Runners", [
            ts("Runner memory by stage", [(f'sum by (task) (nomad_client_allocs_memory_usage{{{RUNNERS}}})', '{{task}}')],
               unit="bytes", w=12, stack=True, desc="Memory in use by each runner stage, every dispatched job summed."),
            ts("Runner CPU by job", [(f'sum by (parent) ({PARENT.format("nomad_client_allocs_cpu_total_percent{" + RUNNERS + "}")}) / 100',
                                      '{{parent}}')], unit="short", decimals=2, w=12, stack=True),
            ts("Dispatched runs failed (Nomad)", [(f'sum by (parent) ({PARENT.format("nomad_nomad_job_summary_failed{" + SFD_NS + "," + RUNNERS + "}")})',
                                                   '{{parent}}')], unit="short", w=12,
               desc="Failed allocations of dispatched runs Nomad still holds (until garbage collection). "
                    "make verify adds some on purpose: the probe repositories of checks 28, 39 and 40 "
                    "(prupd-*, dcprobe/*, svcprobe/*) build red by design. Anything else is worth a look."),
            ts("Build requests", [('sum by (reason) (rate(sfd_build_requests_total[5m]))', '{{reason}}'),
                                  ('sum(rate(sfd_stats_total{service="sshadowforge-workflow-metrics",'
                                   'name="builds_from_bus"}[5m]))', 'scheduled from the bus')], unit="ops", w=12,
               desc="Requested by the dispatcher (kind 30007) and picked up by the workflow."),
        ]),
    ],
    base_tags=TAGS)

# ---------------------------------------------------------------- 40 developer
# The repo matcher stays even under "All" (allValue .*): unauthorized commands carry repo="".
REPO = 'repo=~"$repo",repo!=""'
developer = dashboard(
    "sshadowforge-developer", "SSHadowForge / Developer",
    "For a developer using the forge: what happened to my repositories -- commands, builds, scans, merges, "
    "notifications -- and, when something is slow or failing, whether it is me or the forge.",
    ["developer"],
    [
        ("Is it me or the forge?", [
            stat("Gateway 5xx (forge-wide)", gateway_5xx_ratio("5m"), unit="percentunit", th=RATIO_ERR, decimals=1,
                 desc="Share of gateway requests failing with 5xx across EVERY repository. Green here and a refusal "
                 "in your repository: look at your command's result below."),
            empty_ok(stat("Submit p95", f'histogram_quantile(0.95, sum by (le) (rate(sfd_http_request_duration_seconds_bucket'
                 f'{{{GW},route="/v1/commands"}}[{LAT}])))', unit="s", th=steps(GREEN, (1, AMBER), (2.5, RED)),
                 decimals=2, desc="How long `sfd-submit` waits for the gateway, forge-wide."), "no submissions (5m)"),
            stat("Forge alerts", 'count(ALERTS{alertstate="firing",alertname=~"Sshadowforge.+|Nats.+"}) or vector(0)',
                 th=ZERO_GOOD_AMBER, graph=False, desc="Anything the operators are being paged about."),
            stat("Merge queue", 'max(jetstream_consumer_num_pending{consumer_name="sshadowforge-dispatcher"}) or vector(0)',
                 th=ZERO_GOOD_AMBER, desc="Events waiting for the dispatcher: approvals, pushes and dispositions "
                 "ahead of yours. It handles one at a time."),
            stat("Build queue", f'count(nomad_nomad_blocked_evals_job_memory{{{SFD_NS}}}) or vector(0)',
                 th=ZERO_GOOD_AMBER, desc="SSHadowForge jobs waiting for capacity. Your build may be one."),
            stat("Runners busy", f'count(count by (exported_job) (nomad_client_allocs_memory_usage{{{RUNNERS}}})) '
                 'or vector(0)', desc="Builds, scans and challenges running now, forge-wide."),
        ]),
        ("My repositories (24h)", [
            stat("Commands", f'sum(increase(sfd_commands_total{{{REPO}}}[24h])) or vector(0)',
                 desc="Accepted and refused commands against the selected repositories after authorization."),
            stat("Builds passed", f'sum(increase(sfd_builds_finished_total{{{REPO},result="passed"}}[24h])) or vector(0)',
                 th=steps(GREEN)),
            stat("Builds not passed", f'sum(increase(sfd_builds_finished_total{{{REPO},result!="passed"}}[24h])) '
                 'or vector(0)', th=ZERO_GOOD_AMBER),
            stat("Median build", f'histogram_quantile(0.5, sum by (le) (rate(sfd_build_duration_seconds_bucket{{{REPO}}}[24h])))',
                 unit="s", decimals=0),
            stat("Merged", f'sum(increase(sfd_merge_decisions_total{{{REPO},decision="merged"}}[24h])) or vector(0)',
                 th=steps(GREEN)),
            stat("CVE matches", f'max(sfd_scan_matches{{{REPO}}}) or vector(0)', th=steps(GREEN, (1, AMBER), (25, RED)),
                 desc="The worst repository's matches in its latest scan."),
        ]),
        ("Builds", [
            table("Builds by repository and result", [(f'sum by (repo, result) (increase(sfd_builds_finished_total'
                                                       f'{{{REPO}}}[24h])) > 0', '{{repo}} {{result}}')], w=12, decimals=0,
                  desc="Finished builds in the last 24 hours."),
            ts("Builds over time", [(f'sum by (repo, result) (increase(sfd_builds_finished_total{{{REPO}}}[1h]))',
                                     '{{repo}} {{result}}')], unit="short", w=12, draw="bars", fill=60, stack=True),
            empty_ok(bargauge("Where my builds failed (24h)", [(f'sum by (failed_stage) (increase(sfd_builds_finished_total'
                                                       f'{{{REPO},result="build_failed"}}[24h]))', '{{failed_stage}}')],
                     th=steps(AMBER), w=8, decimals=0, desc="The runner stage a failed build stopped at. `sfd-tui` shows the "
                     "pull request's status; the operator can read that stage's log."), "no failed build (healthy)"),
            ts("Build time p50 by repository", [(f'histogram_quantile(0.5, sum by (repo, le) (rate('
                                                 f'sfd_build_duration_seconds_bucket{{{REPO}}}[6h])))', '{{repo}}')],
               unit="s", w=8),
            empty_ok(bargauge("CVE matches by repository", [(f'max by (repo) (sfd_scan_matches{{{REPO}}})', '{{repo}}')],
                     th=steps(GREEN, (1, AMBER), (25, RED)), w=8, decimals=0), "no scan read for these repositories yet"),
        ]),
        ("Pull requests and activity", [
            ts("Merge decisions", [(f'sum by (repo, decision) (increase(sfd_merge_decisions_total{{{REPO}}}[1h]))',
                                    '{{repo}} {{decision}}')], unit="short", w=12,
               desc="gated: a requirement is unmet (usually the build has not passed yet). stale_head: pushed "
               "after approval, re-approve. stale_base / conflict / not_fast_forward: rebase and re-approve."),
            ts("Commands by kind", [(f'sum by (kind) (increase(sfd_commands_total{{{REPO}}}[1h]))', '{{kind}}')],
               unit="short", w=12, desc="Issues, pull requests, comments, approvals, retries..."),
            empty_ok(ts("Refused commands", [(f'sum by (result) (increase(sfd_commands_total{{{REPO},result!="accepted"}}[1h]))',
                                     '{{result}}')], unit="short", w=12,
               desc="Refused AFTER authorization (enrolment, wrong key, parent missing, an outage). A refusal before "
               "authorization carries no repository: see 'Refused before authorization'."), "none refused"),
            ts("Refused before authorization (forge-wide)", [('sum by (result) (increase(sfd_commands_total{repo="",'
                                                              'result!="accepted"}[1h]))', '{{result}}')],
               unit="short", w=12, desc="Not enrolled, wrong key, forbidden, unauthenticated: usually a credential "
               "on the laptop (`make login-keys`, `sfd-submit enrol`)."),
            ts("Notifications", [(f'sum by (repo, outcome) (increase(sfd_notifications_total{{{REPO}}}[1h]))',
                                  '{{repo}} {{outcome}}')], unit="short", w=12,
               desc="Chat notifications to the repository's chit channel."),
            ts("Build requests", [(f'sum by (repo, reason) (increase(sfd_build_requests_total{{{REPO}}}[1h]))',
                                   '{{repo}} {{reason}}')], unit="short", w=12,
               desc="Builds the dispatcher requested for a pull request head; retried = a `retry` from the TUI."),
        ]),
    ],
    [query_var("repo", "Repository",
               'label_values({__name__=~"sfd_commands_total|sfd_builds_finished_total|sfd_merge_decisions_total",'
               'repo!=""}, repo)')],
    base_tags=TAGS)

# ---------------------------------------------------------------- 50 resources
SVC_UP = 'service=~"sshadowforge-.+",envoy_cluster_name!~"local_app|local_agent|self_admin"'
resources = dashboard(
    "sshadowforge-resources", "SSHadowForge / Resources and Mesh",
    "What the forge costs and how its services talk: memory and CPU per job, the Envoy sidecars' "
    "upstreams (Postgres, Keto, git, NATS, Hydra), and each sfd process's runtime.",
    ["resources", "mesh"],
    [
        ("Now", [
            stat("Memory", f'sum(nomad_client_allocs_memory_usage{{{SFD_NS}}})', unit="bytes",
                 desc="Every SSHadowForge allocation, runners included."),
            stat("Memory reserved", f'sum(nomad_client_allocs_memory_allocated{{{SFD_NS}}})', unit="bytes"),
            stat("CPU", f'sum(nomad_client_allocs_cpu_total_percent{{{SFD_NS}}}) / 100', unit="short", decimals=2,
                 desc="Cores in use."),
            stat("Upstream connect failures (1h)", f'sum(increase(envoy_cluster_upstream_cx_connect_fail{{{SVC_UP}}}[1h])) '
                 'or vector(0)', th=ZERO_GOOD_AMBER,
                 desc="Sidecars failing to reach an upstream: a dependency down, or an intention missing."),
            stat("Goroutines", f'sum(go_goroutines{{{SFD}}})', desc="Across the five sfd processes."),
            stat("sfd RSS", f'sum(process_resident_memory_bytes{{{SFD}}})', unit="bytes"),
        ]),
        ("Jobs", [
            ts("Memory by job", [(f'sum by (parent) ({PARENT.format("nomad_client_allocs_memory_usage{" + SFD_NS + "}")})',
                                  '{{parent}}')], unit="bytes", w=12, stack=True,
               desc="Dispatched runs are summed under their parent job."),
            ts("CPU by job", [(f'sum by (parent) ({PARENT.format("nomad_client_allocs_cpu_total_percent{" + SFD_NS + "}")}) / 100',
                               '{{parent}}')], unit="short", decimals=2, w=12, stack=True),
            table("Memory against reservation", [(f'sum by (exported_job, task) (nomad_client_allocs_memory_usage{{{SFD_NS},'
                                                  f'exported_job!~".+/dispatch-.+"}}) / sum by (exported_job, task) '
                                                  f'(nomad_client_allocs_memory_allocated{{{SFD_NS},'
                                                  f'exported_job!~".+/dispatch-.+"}})', '{{exported_job}} {{task}}')],
                  unit="percentunit", th=steps(GREEN, (0.8, AMBER), (1, RED)), w=12, decimals=0,
                  desc="Long-running tasks' memory as a share of MemoryMB. Above 100% runs on memory_max "
                  "oversubscription."),
            ts("Workflow (off-mesh) memory by task", [('sum by (task) (nomad_client_allocs_memory_usage{namespace="sshadowforge",'
                                                       'exported_job="sshadowforge-workflow"})', '{{task}}')],
               unit="bytes", w=12, desc="The Dapr control plane: workflow, daprd, scheduler, placement, state-db."),
        ]),
        ("Mesh", [
            ts("Active upstream connections", [(f'sum by (service, envoy_cluster_name) (envoy_cluster_upstream_cx_active{{{SVC_UP}}})',
                                                '{{service}} -> {{envoy_cluster_name}}')], unit="short", w=12),
            ts("Upstream requests", [(f'sum by (service, envoy_cluster_name) (rate(envoy_cluster_upstream_rq_total{{{SVC_UP}}}[{RI}]))',
                                      '{{service}} -> {{envoy_cluster_name}}')], unit="reqps", w=12,
               desc="HTTP upstreams only (Keto, Hydra, the gateway); TCP upstreams (Postgres, NATS, git) show as "
               "connections."),
            ts("Connect failures and timeouts", [(f'sum by (service, envoy_cluster_name) (rate(envoy_cluster_upstream_cx_connect_fail{{{SVC_UP}}}[5m]))',
                                                  '{{service}} -> {{envoy_cluster_name}} fail'),
                                                 (f'sum by (service, envoy_cluster_name) (rate(envoy_cluster_upstream_cx_connect_timeout{{{SVC_UP}}}[5m]))',
                                                  '{{service}} -> {{envoy_cluster_name}} timeout')], unit="ops", w=12),
            ts("Git over SSH (inbound)", [('sum(rate(envoy_tcp_downstream_cx_total{service="sshadowforge-git"}[5m]))',
                                           'connections/s'),
                                          ('sum(rate(envoy_tcp_downstream_cx_rx_bytes_total{service="sshadowforge-git"}[5m]))',
                                           'bytes in/s')], unit="short", w=12,
               desc="Clones, fetches and pushes reaching soft-serve through its sidecar."),
        ]),
        ("sfd processes", [
            ts("Resident memory", [(f'process_resident_memory_bytes{{{SFD}}}', '{{service}}')], unit="bytes", w=8),
            ts("Goroutines", [(f'go_goroutines{{{SFD}}}', '{{service}}')], unit="short", w=8),
            ts("CPU seconds", [(f'rate(process_cpu_seconds_total{{{SFD}}}[{RI}])', '{{service}}')], unit="short",
               decimals=3, w=8),
        ]),
    ],
    base_tags=TAGS)

for fname, d in [("00-operator.json", operator), ("10-event-pipeline.json", pipeline),
                 ("20-nats-jetstream.json", nats), ("30-builds.json", builds),
                 ("40-developer.json", developer), ("50-resources.json", resources)]:
    write(os.path.join(OUT, fname), young_counters(d))
    print(fname, len(d["spec"]["elements"]), "panels")
