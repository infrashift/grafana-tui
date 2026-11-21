---
title: Exporting and Recording
description: Export the dashboard as SVG or PNG and record changed frames.
---

grafana-tui can export the current dashboard view as SVG, PNG, or both. It can also record changed dashboard states into a timestamped frame bundle.

## Export a Snapshot

Press `e` to export the current visible dashboard.

grafana-tui writes the output files in `--export-dir`:

```bash
grafana-tui --export-dir ./grafana-tui-exports --export-format both
```

Supported formats:

- `svg`
- `png`
- `both`

## Record Changed Frames

1. Press `Ctrl+E` to start a recording.
2. Press `Ctrl+E` again, or quit with `q`, to complete the bundle.

grafana-tui records only changed rendered states:

```text
grafana-tui-recording-<timestamp>/
  frame-000001.svg
  frame-000002.svg
  manifest.json
```

If you set `--export-format png` or `both`, grafana-tui also writes a PNG file for each SVG file.

External annotations can be visible. Then their panel targets and the applied
tag filter change the markers in SVG/PNG exports and in recordings. The active
inline annotation details also go into the export. Exports and recordings do
not show the annotation modals. A change to the draft of the tag
filter does not make a new frame. A frame can change after you apply or clear
the filter.

## Recording Limits

Limit the number of changed frames in one recording:

```bash
grafana-tui --record-max-frames 300
```

At the frame cap, grafana-tui stops and writes no more frames. When the recording completes, the manifest records `completed_reason = "capped"`.

## Manifest

Each recording writes a `manifest.json` file. Other tools can read its metadata:

```json
{
  "version": 1,
  "format": "svg",
  "changed_only": true,
  "frame_count": 2,
  "max_frames": 300,
  "completed_reason": "stopped",
  "viewport": { "width": 100, "height": 40 },
  "frames": [
    {
      "index": 1,
      "elapsed_ms": 0,
      "files": ["frame-000001.svg"]
    }
  ]
}
```
