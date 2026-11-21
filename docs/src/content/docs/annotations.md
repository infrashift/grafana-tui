---
title: External Annotations
description: Overlay read-only point events from a JSONL file or command.
---

grafana-tui can show read-only, external point events from one source only:
a JSONL file or a command provider. It never edits or writes to the source.
External annotations are not Grafana dashboard annotations. grafana-tui does
not support Grafana annotation queries, the Grafana annotation APIs,
`annotations`, or `annotations.list`.

## Enable Annotations

Select one source only. For a file source, give the path on the command line:

```bash
grafana-tui \
  --grafana-json ./dashboard.json \
  --annotations-file ./events.jsonl
```

Or configure the file source in TOML:

```toml
annotations_file = "./events.jsonl"
```

For a command source, configure an executable that accepts the request protocol
below. grafana-tui does not start a shell for the command, so no shell expands
the arguments:

```toml
[annotations_command]
program = "./target/debug/examples/git_annotation_provider"
args = ["."]
timeout = "10s"
```

Or select it from the command line:

```bash
grafana-tui \
  --grafana-json ./dashboard.json \
  --annotations-command ./target/debug/examples/git_annotation_provider \
  --annotations-command-arg=.
```

grafana-tui does not read an annotation file larger than 16 MiB, and it shows
a warning. From each kind of source, grafana-tui keeps the newest 100,000
events at most.

Use a file source or a command source, not the two. grafana-tui stops with an
error when a TOML configuration sets the two sources, also when the CLI
selects a source. A CLI file or command replaces all of the TOML annotation
source. grafana-tui never mixes a CLI program, CLI arguments, or a CLI timeout
with TOML values. The sources are optional and read-only. grafana-tui does not
create, edit, or write to them.

## Command Provider Protocol

grafana-tui writes one version-1 request line to the standard input of the
command, then closes stdin. The request gives the full refresh window:

```json
{"version":1,"range":{"from":"2026-08-12T10:00:00Z","to":"2026-08-12T10:05:00Z"}}
```

`range.from` and `range.to` are inclusive UTC RFC 3339 bounds. grafana-tui
also applies its own visible-range filter to the events that the provider
returns.

The provider writes zero or more JSONL events to stdout, and it writes
diagnostics to stderr. An exit status of `0` with valid, bounded JSONL replaces
all of the annotation snapshot. An empty stdout with exit status `0` clears the
snapshot. grafana-tui keeps the last valid snapshot and shows a warning when:

- The command does not start.
- The command does not stop before the timeout.
- The exit status is not zero.
- The stdout has invalid UTF-8 or invalid JSONL.
- The stdout is too large.

The default timeout is 10 seconds. grafana-tui accepts 10 MiB of stdout at
most, and it keeps 64 KiB of stderr at most. A provider must complete its work
before it exits. On Linux and macOS, the provider runs in its own process group.
When the refresh ends, grafana-tui stops all of the processes that the provider
started. The refresh can end with success, with an error, with a timeout, or
because grafana-tui quits. On Windows, grafana-tui stops only the provider
process. A provider gets the current directory and the environment of
grafana-tui. Put credentials in that environment, or use a standard credential
tool. Never put secrets in dashboard JSON or in command arguments.

The Git provider in the repository is a good first example:

```bash
cargo build --example git_annotation_provider
printf '%s\n' '{"version":1,"range":{"from":"2026-08-12T10:00:00Z","to":"2026-08-12T10:05:00Z"}}' \
  | ./target/debug/examples/git_annotation_provider .
```

## JSONL Event Format and Targeting

The file has one JSON object on each line. grafana-tui ignores blank lines.
Each event must have these fields:

- `time`: an RFC3339 string with an explicit timezone or offset. grafana-tui
  does not accept a numeric timestamp.
- `text`: a string that is not empty.

`tags` is an optional array of strings that are not empty.

```json
{"time":"2026-07-23T14:30:00Z","text":"Maintenance window","tags":["maintenance"]}
{"time":"2026-07-23T14:30:00Z","text":"Deployed v2.4","tags":["deploy","production"],"panel_titles":["HTTP Request Rate by Status Code"]}
```

Omit `panel_titles` to show the event on all of the applicable graph and
timeseries panels, as the first event does. When the event has
`panel_titles`, the array must have one or more titles that are not blank.
grafana-tui compares each title with the displayed title of each applicable
graph or timeseries panel. The comparison is exact and case-sensitive.
grafana-tui compares the titles after it replaces the dashboard variables, so
each copy of a repeated panel has its own title. `null`, an empty array, and a
blank title are errors.

If more than one applicable panel has the same title, the event goes to all of
those panels. grafana-tui shows one warning for that duplicate title. A title
can match no panel, or it can match only a panel that is not a graph. Then
grafana-tui shows one warning and draws no marker for that title. These titles
are grafana-tui labels that send events to panels. They are not Grafana panel
IDs.

grafana-tui sorts the events by timestamp. It ignores unknown JSON fields. It
accepts times with fractional seconds. It uses the full fractional timestamp to
put an event on the graph. The inline timestamp display has little space, so it
can show fewer digits.

## Target, Filter, Inspect, and Reload

This example uses current UTC timestamps, so the two events are in the visible
15-minute range. It uses only POSIX shell tools. It does not use `jq`.

1. Start the Prometheus demo stack from the repository root:

   ```bash
   cd examples/demo && docker-compose up -d && sleep 5 && cd ../..
   ```

2. Run these commands:

   ```bash
   annotation_demo_file=/tmp/grafana-tui-annotations-demo.jsonl
   annotation_demo_time="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

   printf '{"time":"%s","text":"Maintenance window","tags":["maintenance"]}\n' \
     "$annotation_demo_time" > "$annotation_demo_file"
   printf '{"time":"%s","text":"API deployed","tags":["deploy","production"],"panel_titles":["HTTP Request Rate by Status Code"]}\n' \
     "$annotation_demo_time" >> "$annotation_demo_file"

   cargo run -- \
     --grafana-json examples/dashboards/prometheus_demo.json \
     --prometheus-url http://localhost:19090 \
     --range 15m \
     --annotations-file "$annotation_demo_file"
   ```

   The commands write the annotation file and start grafana-tui.
   `Maintenance window` shows on each graph or timeseries panel.
   `API deployed` shows only on `HTTP Request Rate by Status Code`.

3. Press `t`.
4. Select `deploy` with `Space`.
5. Press `Enter`. Only the event with the `deploy` tag stays visible.
6. Press `v`.
7. Move the cursor to the marker.
8. Press `Enter`. The cluster list and the event detail pane of the selected
   panel open.
9. Keep grafana-tui open. In a second terminal, add an event:

   ```bash
   annotation_demo_file=/tmp/grafana-tui-annotations-demo.jsonl
   annotation_demo_time="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
   printf '{"time":"%s","text":"Rollback started","tags":["rollback","production"],"panel_titles":["HTTP Request Rate by Status Code"]}\n' \
     "$annotation_demo_time" >> "$annotation_demo_file"
   ```

   grafana-tui reads the new event at the next normal refresh. You do not
   restart grafana-tui. The filter selects only `deploy`, so
   `Rollback started` stays hidden.

10. Press `t`.
11. Press `c` to clear the filter.
12. Press `Enter` to apply the clear filter. The rollback marker shows.

As an alternative to steps 11 and 12, select `rollback` in the filter and
apply the filter.

## Tag Filter and Cluster Controls

Press `t` to open the global annotation tag filter. The filter has effect
only while grafana-tui runs. grafana-tui does not write it to the JSONL source
or to the configuration. The filter applies to all of the applicable panels.

The filter compares the selected tags with the event tags. The comparison is
exact, case-sensitive, and uses OR. An event stays visible when it has one or
more of the selected tags. When the filter has no selected tags, all events stay
visible, with tags or without tags. The tag list keeps a selected tag with a zero event
count, so that you can clear that tag after a reload.

In the tag filter, use these keys:

| Key | Action |
| --- | --- |
| `Up`/`Down` or `j`/`k` | Move |
| `Space` | Select or clear the highlighted tag |
| `c` | Clear the draft |
| `Enter` | Apply the draft |
| `Esc` | Cancel and keep the applied filter |

In inspect mode, `Enter` opens only the cluster that the selected panel draws
at the cursor. In the cluster, use these keys:

| Key | Action |
| --- | --- |
| `Up`/`Down` or `j`/`k` | Select an event |
| `PgUp`/`PgDn` | Go up or down one page |
| `Enter` or `Esc` | Close the cluster |

grafana-tui ignores the mouse while an annotation modal is open.

The cluster keeps the same contents while it is open. A reload can replace the
live annotation snapshot. The rows of the cluster do not move, and the event
details of the cluster do not change.

## Automatic Reload, Rendering, and Exports

grafana-tui refreshes the two source types at each normal refresh, also when
the TUI does not show the markers. For a file source, grafana-tui reads the file metadata
first. When the metadata changes, grafana-tui reads, parses, and checks the
full new file. Then it replaces the snapshot in one step. A zero-byte file is a
valid update that clears all events. The command refresh and the Prometheus
refresh use the same time range. They start together and the screen updates
for the two together. Annotations do not depend on Prometheus. An annotation
failure never stops startup or a Prometheus refresh.

Only `graph` and `timeseries` panels show annotation markers. Press `a` to
show or hide the markers. grafana-tui draws one marker with a count for all of
the events in the same terminal column:

| Events in the column | Marker |
| --- | --- |
| 1 | `•` |
| 2 to 9 | The decimal digit `2`–`9` |
| 10 or more | `+` |

In inspect mode, move the cursor onto that column to see the timestamp, text,
and tags of the cluster inline. For a cluster of more than one event, the
details give the exact number of events.

The applied panel targets and the applied tag filter also apply to the
markers in SVG/PNG exports and in changed-frame recordings. The exports include
the active inline annotation details. They do not include the tag-filter modal
or the cluster modal.

## Errors and Last Valid Snapshot

A file can be absent or unreadable, or it can have a malformed event. Then
grafana-tui keeps the last valid snapshot on the screen and shows an
annotation warning. The same rule applies to command provider failures. A bad
update does not replace the events that grafana-tui read before. It does not
stop startup, and it does not stop the Prometheus refresh.

## CI/CD and Provider Integrations

```text
CI workflow → durable deployment/release record → command provider query
            → normalized JSONL point events → grafana-tui overlay
```

GitHub Actions gives a good example of this pattern. A workflow writes a record
of each deploy, release, or workflow result. It writes the record to an API, an
object store, a database, or a shared event log. A local provider gets the
range from grafana-tui and queries that system of record. Then the provider
writes normalized JSONL point events. Useful tags are repository, workflow,
environment, status, commit, and deploy.

Give credentials to the provider through its environment or a standard
credential tool. Never put them in dashboard JSON or in command arguments. Use
a shared JSONL file only when the workflow and grafana-tui share the same
storage. Do not commit an event log that always gets larger to the application
repository. Put vendor-specific providers in user or community plugins.
grafana-tui adds built-in integrations only when users ask for them.

## Current Limits and Roadmap

This feature supports one external file or command source and point events.
Panel titles send events to panels. One global tag filter has effect only at
runtime. It has no stable event IDs, no tag filter for each panel, no
multiple sources, and no edit function. It does not add support for Grafana
annotation queries or Grafana annotation APIs. Range events, stable event IDs,
and a tag filter for each panel are for a later release. `--validate` checks
only Grafana dashboard imports. It does not check annotations.
