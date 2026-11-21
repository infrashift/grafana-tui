---
title: Keyboard and Mouse
description: Keyboard shortcuts and mouse interactions.
---

In grafana-tui, the keyboard controls all of the functions. Use it to examine a dashboard.

## Keyboard Controls

| Key | Action |
|---|---|
| `q` | Quit |
| `r` | Force refresh |
| `+` / `-` | Zoom out / in |
| `Shift+Left` / `Shift+Right` | Pan left / right in time |
| `0` | Reset to live mode in fullscreen; show every series in normal mode |
| `Up` / `Down` or `k` / `j` | Select previous or next visible row or panel |
| `Enter` / `Space` | Toggle the selected row, or enter the selected tab bar's active tab |
| `Left` / `Right` | Collapse / expand the selected row, or switch tabs on the selected tab bar |
| `Tab` / `Shift+Tab` | Show the next / previous dashboard, with [several dashboards](/grafana-tui/grafana-dashboard-import/#several-dashboards) |
| `PgUp` / `PgDn` | Scroll vertically, or select panels in fullscreen |
| `Home` / `End` | Jump to top or bottom |
| `y` | Toggle Y-axis mode |
| `g` | Toggle autogrid guide lines |
| `a` | Toggle external annotation markers |
| `t` | Open the global annotation tag filter |
| `T` | Open the theme picker |
| `1` through `9` | Toggle series visibility |
| `f` | Toggle fullscreen mode for the selected panel |
| `v` | Toggle value inspection mode |
| `Enter` in inspect mode | Open the selected panel's annotation cluster at the cursor |
| `e` | Export current view |
| `Ctrl+E` | Start or stop changed-frame recording |
| `/` | Search visible rows and panels |
| `Left` / `Right` | Move cursor in inspect mode |
| `?` | Toggle debug info |

> **Known issue**: `[` and `]` also pan the time range, but only when the
> terminal reports Shift with them. Most terminals do not. Only the fullscreen
> mode can restore the live mode.

## Mouse Support

| Action | Behavior |
|---|---|
| Click | Select a row or panel; click a row disclosure marker to toggle it; click a tab to show it; move the cursor in fullscreen inspect mode |
| Drag | Move the cursor in fullscreen inspect mode |
| Scroll | Scroll the dashboard vertically |

In normal mode, a click selects a row or a panel. To use the cursor, select a
panel and press `v` or `f`.

## Annotation Modals

The `t` key opens the global tag filter. These keys operate in the tag filter:

| Key | Action |
|---|---|
| `Up` / `Down` or `k` / `j` | Move the highlight |
| `Space` | Toggle the highlighted tag |
| `c` | Clear the draft |
| `Enter` | Apply the draft |
| `Esc` | Discard the draft |

These keys operate in an annotation cluster:

| Key | Action |
|---|---|
| `Up` / `Down` or `k` / `j` | Select an event |
| `PgUp` / `PgDn` | Show the previous / next page |
| `Enter` or `Esc` | Close the cluster |

While an annotation modal is open, the TUI ignores mouse input.

## Theme Picker

`T` opens the theme picker in any mode except search. The picker groups the
themes by family. User-defined themes show under Custom. Each row shows the theme's
background and series colors.

When you move the selection with `Up`/`Down`, `k`/`j`, `PgUp`/`PgDn`, `Home`,
or `End`, the TUI shows that theme immediately. `Enter` keeps it. `Esc`, `T`,
or `q` restores the theme that was active when the picker opened. The picker
marks that theme with `•`. While the picker is open, the TUI ignores mouse
input.

The choice lasts for the current session. To keep it, set `theme` in the
[configuration file](/grafana-tui/configuration/#themes).
