---
title: Troubleshooting
description: Diagnose connection, TLS, variable, and rendering problems.
---

## Prometheus Connection Refused

grafana-tui continues to respond to keys while Prometheus is slow or down. The
title bar shows the connection state:

| Title bar | Meaning |
|---|---|
| `◌ connecting` | The first refresh has not finished yet; panels show `Loading…` |
| `⟳ refreshing` | A refresh has been running for more than a second |
| `✗ Prometheus unreachable` | Every query in the last refresh failed to connect |
| `✗ Prometheus unreachable (TLS: …)` | The connection failed during TLS; see [TLS Errors](#tls-errors) |

While Prometheus is unreachable, grafana-tui tries again. The delay between
tries increases, up to a maximum of 30 seconds. When Prometheus answers again,
grafana-tui recovers automatically.
Press `r` to retry immediately. Queries that Prometheus rejects, such as invalid
PromQL, show as panel errors rather than as unreachable.

## TLS Errors

When an `https://` connection fails during TLS, the title bar names the cause.
The panel error repeats it. The panel error also tells what to check, and gives
the error from the TLS library. grafana-tui tries again with the same delays
as for an unreachable server. So it recovers when you correct the
certificates. For the settings, see
[the mTLS settings for Prometheus](/grafana-tui/configuration/#connecting-to-an-mtls-prometheus).

### `TLS: unknown issuer`

A CA that grafana-tui trusts did not sign the server's certificate. If `ca_cert`
is not set, grafana-tui trusts only the built-in web PKI roots. So a server with
a private CA always fails this way. Set `ca_cert` to the CA bundle. If `ca_cert`
is already set, make sure that the bundle has the CA that signed the server's
certificate. When you change from one CA to a new CA, include the two CAs:

```bash
openssl s_client -connect 10.60.1.21:9090 -showcerts </dev/null  # the server's chain
openssl x509 -in ca.pem -noout -subject                            # the first CA in the bundle
```

### `TLS: client certificate required`

The server requires a client certificate, and grafana-tui did not send one. Set
`client_cert` and `client_key`.

### `TLS: client certificate rejected (…)`

The server received the client certificate but did not accept it. The alert in
parentheses comes from the server:

- `UnknownCA` or `BadCertificate` usually means that a CA that the server does
  not trust issued the certificate.
- `CertificateExpired` means that the certificate has expired.

Check the certificate against the server's client CA:

```bash
openssl verify -CAfile client-ca.pem client.pem
openssl x509 -in client.pem -noout -subject -issuer -enddate
```

### `TLS: certificate not valid for this host`

The server's certificate doesn't list the host or IP address in
`prometheus_url` among its subject alternative names. Use the name in the
certificate, or issue a new certificate with an IP SAN.

## Panel Markers

When a later fetch has problems, a panel continues to show its data. It also marks its title:

| Panel title | Meaning |
|---|---|
| `⚠ stale: queries failed` | Every query of the latest fetch failed; the chart shows the last data that loaded |
| `⚠ query failed` | Some of the panel's queries failed; the chart shows the ones that succeeded |
| `⚠ warning` | Prometheus returned the data with warnings, such as partial results |

Select the panel to see the error or warning in the footer. A panel that has
never loaded data shows the full error instead. grafana-tui makes a long title
shorter, so that the marker stays in view.

Prometheus 3 also returns informational notices, such as `metric might not be a
counter` for `rate()` over a counter whose name does not end in `_total`. The
data is fine, so these get no marker: select the panel to see them in the
footer as `info:`. Prometheus rejects a query with
a 4xx status the same way every time. So grafana-tui tries again only after a
connection failure or a 5xx or 429 response.

Make sure that Prometheus operates and that you can reach it:

```bash
curl http://localhost:9090/-/healthy
```

For the demo stack, the Prometheus port is `19090`:

```bash
curl http://localhost:19090/-/healthy
```

## No Data Appears

Prometheus can need a few scrape intervals before data is available.

1. Wait 10 to 15 seconds.
2. Press `r` to force a refresh.

Also make sure that the dashboard queries match the labels in the Prometheus server:

```bash
grafana-tui --prometheus-url http://localhost:9090 --grafana-json ./dashboard.json --var job=prometheus
```

## Dashboard Variables Do Not Match

Override variables explicitly with `--var`:

```bash
grafana-tui --grafana-json ./dashboard.json --var instance=localhost:9090
```

A Grafana dashboard can use multi-select format modifiers, such as `${var:csv}` or `${var:regex}`. grafana-tui does not support every Grafana format modifier. For the supported modifiers, see the [compatibility matrix](/grafana-tui/grafana-compatibility/).

## Demo Port Conflict

The demo Prometheus service uses host port `19090`. If that port is already in use, edit `examples/demo/docker-compose.yml` and run grafana-tui with the updated URL.

## Export Directory Problems

Set an explicit export directory:

```bash
grafana-tui --export-dir ./grafana-tui-exports
```

Make sure that the current user can write to the directory.
