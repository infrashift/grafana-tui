/*
 * Copyright 2026 Federico D'Ambrosio
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use crate::annotations::{
    AnnotationCluster, cluster_projected_events, format_cluster_detail_lines,
};
use crate::app::{AppMode, AppState, PanelState, PanelType, SeriesView};
use crate::limits::MAX_RECORD_FRAMES;
use crate::num;
#[cfg(test)]
use crate::theme::Theme;
use crate::ui;
use anyhow::{Context, Result, anyhow};
use clap::ValueEnum;
use ratatui::layout::Rect;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

const CELL_WIDTH: f64 = 10.0;
const CELL_HEIGHT: f64 = 18.0;
const FONT_SIZE: f64 = 13.0;
const SMALL_FONT_SIZE: f64 = 11.0;
const PANEL_PADDING: f64 = 12.0;
const TITLE_HEIGHT: f64 = 28.0;
const X_LABEL_HEIGHT: f64 = 24.0;
const LEGEND_HEIGHT: f64 = 28.0;
/// Advance of a monospace character, in ems, with slack for glyphs such as
/// `⚠` that a fallback font may draw wider.
const CHAR_ADVANCE: f64 = 0.62;
/// Distance between the baselines of wrapped error lines.
const ERROR_LINE_HEIGHT: f64 = 16.0;
/// Font size of a stat value with room to spare.
const STAT_FONT_SIZE: f64 = 28.0;
/// Least space between two labels on one axis.
const AXIS_LABEL_GAP: f64 = 6.0;
/// Space above the first legend row's baseline and below its last.
const LEGEND_PADDING: f64 = 13.0;
/// Distance between the baselines of legend rows.
const LEGEND_ROW_HEIGHT: f64 = 15.0;
/// Width of a legend swatch and the space after it.
const LEGEND_SWATCH: f64 = 13.0;
/// Space between legend entries in a row.
const LEGEND_GAP: f64 = 11.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ExportFormat {
    #[default]
    Svg,
    Png,
    Both,
}

#[derive(Debug, Clone)]
pub(crate) struct ExportOptions {
    pub(crate) dir: PathBuf,
    pub(crate) format: ExportFormat,
    pub(crate) record_max_frames: usize,
}

impl Default for ExportOptions {
    fn default() -> Self {
        let options = Self {
            dir: PathBuf::from("./grafana-tui-exports"),
            format: ExportFormat::Svg,
            record_max_frames: 300,
        };
        debug_assert!(
            (1..=MAX_RECORD_FRAMES).contains(&options.record_max_frames),
            "the default frame limit is in the valid range"
        );
        debug_assert!(
            !options.dir.as_os_str().is_empty(),
            "the default export directory has a name"
        );
        options
    }
}

impl ExportOptions {
    pub(crate) fn validate(self) -> Result<Self> {
        anyhow::ensure!(
            self.record_max_frames > 0,
            "record_max_frames must be greater than 0"
        );
        anyhow::ensure!(
            self.record_max_frames <= MAX_RECORD_FRAMES,
            "record_max_frames is {}, more than the limit of {MAX_RECORD_FRAMES}",
            self.record_max_frames
        );
        Ok(self)
    }
}

#[derive(Debug)]
pub(crate) struct RecordingState {
    pub(crate) dir: PathBuf,
    pub(crate) frame_count: usize,
    pub(crate) max_frames: usize,
    pub(crate) last_svg: Option<String>,
    pub(crate) frames: Vec<RecordingFrame>,
    pub(crate) started_at: String,
    pub(crate) started_instant: Instant,
    pub(crate) viewport: RecordingViewport,
}

#[derive(Debug, Serialize)]
pub(crate) struct RecordingFrame {
    pub(crate) index: usize,
    pub(crate) captured_at: String,
    pub(crate) elapsed_ms: u128,
    pub(crate) files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RecordingViewport {
    width: u16,
    height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RecordingCompletionReason {
    Stopped,
    Quit,
    Capped,
}

#[derive(Debug, Serialize)]
struct RecordingManifest {
    version: u8,
    started_at: String,
    completed_at: String,
    format: ExportFormat,
    viewport: RecordingViewport,
    changed_only: bool,
    frame_count: usize,
    max_frames: usize,
    completed_reason: RecordingCompletionReason,
    frames: Vec<RecordingFrame>,
}

#[derive(Clone, Copy)]
struct PlotRect {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

struct LineStyle<'a> {
    color: &'a str,
    dash: Option<&'a str>,
    width: f64,
}

/// An SVG text buffer. Its writes cannot fail, so no caller checks a result (R7).
#[derive(Debug, Default)]
struct SvgOut(String);

impl SvgOut {
    /// Adds formatted text to the end of the buffer.
    fn put(&mut self, text: std::fmt::Arguments<'_>) {
        self.0.push_str(&std::fmt::format(text));
    }

    /// Adds literal text to the end of the buffer.
    fn push_str(&mut self, text: &str) {
        self.0.push_str(text);
    }

    /// Gives the SVG text to the caller.
    fn finish(self) -> String {
        self.0
    }

    /// Tells if the text is empty or ends with the end of an element.
    fn is_complete(&self) -> bool {
        self.0.is_empty() || self.0.ends_with('>')
    }
}

impl PlotRect {
    fn right(self) -> f64 {
        self.left + self.width
    }

    fn bottom(self) -> f64 {
        self.top + self.height
    }

    /// Tells if the rectangle has finite edges and no negative size.
    fn is_valid(self) -> bool {
        let edges = [self.left, self.top, self.width, self.height];
        edges.iter().all(|edge| edge.is_finite()) && self.width >= 0.0 && self.height >= 0.0
    }
}

/// Tells if `color` is an SVG color of the form `#rrggbb`.
fn is_hex_color(color: &str) -> bool {
    let digits = color.strip_prefix('#').unwrap_or_default();
    color.len() == 7 && digits.chars().all(|digit| digit.is_ascii_hexdigit())
}

/// Tells if `anchor` is an SVG `text-anchor` value.
fn is_text_anchor(anchor: &str) -> bool {
    matches!(anchor, "start" | "middle" | "end")
}

pub(crate) fn export_current(app: &mut AppState, viewport: Rect) -> Result<Vec<PathBuf>> {
    let svg = render_svg(app, viewport);
    let stem = format!("grafana-tui-{}", timestamp_id());
    let paths = write_outputs(&svg, &app.export.dir, &stem, app.export.format)?;
    debug_assert!(!paths.is_empty(), "the export writes a file");
    debug_assert!(
        paths.iter().all(|path| path.starts_with(&app.export.dir)),
        "each export file is in the export directory"
    );
    app.export_status = Some(format!("Exported {}", display_paths(&paths)));
    Ok(paths)
}

pub(crate) fn toggle_recording(app: &mut AppState, viewport: Rect) -> Result<()> {
    let was_recording = app.recording.is_some();
    let result = if was_recording {
        stop_recording(app, RecordingCompletionReason::Stopped)
    } else {
        start_recording(app, viewport)
    };
    debug_assert!(
        !was_recording || app.recording.is_none(),
        "a stop ends the recording, also on an error"
    );
    debug_assert!(
        result.is_err() || app.recording.is_some() != was_recording,
        "a toggle that succeeds changes the recording state"
    );
    result
}

pub(crate) fn capture_recording_frame(app: &mut AppState, viewport: Rect) -> Result<()> {
    let Some(recording) = app.recording.as_ref() else {
        return Ok(());
    };
    debug_assert!(
        recording.frame_count <= recording.max_frames,
        "the frame count obeys the frame limit"
    );
    if recording.frame_count >= recording.max_frames {
        app.export_status = Some(format!(
            "Recording capped at {}/{} frames in {}; press Ctrl+E or q to save",
            recording.frame_count,
            recording.max_frames,
            recording.dir.display()
        ));
        return Ok(());
    }

    let svg = render_svg(app, viewport);
    if recording.last_svg.as_deref() == Some(svg.as_str()) {
        return Ok(());
    }

    let frame_index = recording.frame_count + 1;
    let stem = format!("frame-{frame_index:06}");
    let paths = write_outputs(&svg, &recording.dir, &stem, app.export.format)?;
    let captured_at = timestamp_rfc3339();
    let elapsed_ms = recording.started_instant.elapsed().as_millis();
    let files = paths
        .iter()
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();
    debug_assert!(files.len() == paths.len(), "each frame file has a name");

    if let Some(recording) = app.recording.as_mut() {
        recording.frame_count = frame_index;
        recording.last_svg = Some(svg);
        recording.frames.push(RecordingFrame {
            index: frame_index,
            captured_at,
            elapsed_ms,
            files,
        });
        debug_assert!(
            recording.frames.len() == recording.frame_count,
            "the recording keeps one entry for each frame"
        );
    }
    app.export_status = Some(format!("Recording frame {frame_index}"));
    Ok(())
}

fn start_recording(app: &mut AppState, viewport: Rect) -> Result<()> {
    debug_assert!(app.recording.is_none(), "no recording runs");
    debug_assert!(
        app.export.record_max_frames > 0,
        "the frame limit is positive"
    );
    let started_at = timestamp_rfc3339();
    let dir = app
        .export
        .dir
        .join(format!("grafana-tui-recording-{}", timestamp_id()));
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create recording directory {}", dir.display()))?;
    app.recording = Some(RecordingState {
        dir,
        frame_count: 0,
        max_frames: app.export.record_max_frames,
        last_svg: None,
        frames: Vec::new(),
        started_at,
        started_instant: Instant::now(),
        viewport: RecordingViewport {
            width: viewport.width,
            height: viewport.height,
        },
    });
    app.export_status = Some("Recording started".to_string());
    capture_recording_frame(app, viewport)
}

pub(crate) fn stop_recording(app: &mut AppState, reason: RecordingCompletionReason) -> Result<()> {
    let Some(recording) = app.recording.take() else {
        return Ok(());
    };

    let completed_reason = if recording.frame_count >= recording.max_frames {
        RecordingCompletionReason::Capped
    } else {
        reason
    };
    let manifest = RecordingManifest {
        version: 1,
        started_at: recording.started_at,
        completed_at: timestamp_rfc3339(),
        format: app.export.format,
        viewport: recording.viewport,
        changed_only: true,
        frame_count: recording.frame_count,
        max_frames: recording.max_frames,
        completed_reason,
        frames: recording.frames,
    };
    debug_assert!(
        manifest.frames.len() == manifest.frame_count,
        "the manifest lists each frame"
    );
    debug_assert!(
        manifest.frame_count <= manifest.max_frames,
        "the frame count obeys the frame limit"
    );
    let manifest_path = recording.dir.join("manifest.json");
    let json = serde_json::to_string_pretty(&manifest)?;
    write_atomic(&manifest_path, json.as_bytes())?;
    let capped = if completed_reason == RecordingCompletionReason::Capped {
        ", capped"
    } else {
        ""
    };
    app.export_status = Some(format!(
        "Recording saved {} ({}/{} frames{capped})",
        recording.dir.display(),
        recording.frame_count,
        recording.max_frames
    ));
    Ok(())
}

pub(crate) fn render_svg(app: &AppState, viewport: Rect) -> String {
    let width = f64::from(viewport.width).max(1.0) * CELL_WIDTH;
    let height = f64::from(viewport.height).max(1.0) * CELL_HEIGHT;
    debug_assert!(
        width >= CELL_WIDTH && height >= CELL_HEIGHT,
        "the SVG is at least one cell in size"
    );
    let bg = color_hex(app.theme.background, "#111111");
    let text = color_hex(app.theme.text, "#e6e6e6");
    let border = color_hex(app.theme.border, "#555555");

    let mut out = SvgOut::default();
    out.put(format_args!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.0} {height:.0}">"#
    ));
    out.put(format_args!(
        r#"<rect width="100%" height="100%" fill="{bg}"/>"#
    ));
    out.put(format_args!(
        r#"<g font-family="ui-monospace, SFMono-Regular, Menlo, Consolas, monospace" font-size="{FONT_SIZE}" fill="{text}">"#
    ));
    render_header(app, &mut out, width, &text, &border);

    for item in ui::visible_dashboard_rects(viewport, app) {
        render_dashboard_item(app, &item, &text, &mut out);
    }

    render_footer(app, &mut out, width, height, &text, &border);
    out.push_str("</g></svg>");
    let svg = out.finish();
    debug_assert!(
        svg.starts_with("<svg ") && svg.ends_with("</g></svg>"),
        "the SVG text has one root element"
    );
    svg
}

/// Draws one panel, row header, tab bar or empty tab message of the dashboard.
fn render_dashboard_item(app: &AppState, item: &ui::DashboardRect, text: &str, out: &mut SvgOut) {
    debug_assert!(is_hex_color(text), "the text color is a hex color");
    debug_assert!(out.is_complete(), "the SVG text ends with an element");
    let selected = app.selected_item == Some(item.id);
    match item.kind {
        ui::DashboardRectKind::Panel { index } => {
            let Some(panel) = app.panels.get(index) else {
                return;
            };
            let panel_rect = scaled_rect(item.rect);
            render_panel(app, index, panel, panel_rect, selected, out);
        }
        ui::DashboardRectKind::Row {
            row_id,
            depth,
            collapsed,
        } => render_row_header(
            app,
            row_id,
            depth,
            collapsed,
            scaled_rect(item.rect),
            selected,
            out,
        ),
        ui::DashboardRectKind::Tabs { group_id, depth } => {
            render_tab_group(app, item, group_id, depth, text, out);
        }
        ui::DashboardRectKind::TabEmpty { group_id } => {
            let rect = scaled_rect(item.rect);
            write_text(
                out,
                rect.left + 16.0,
                rect.top + FONT_SIZE,
                app.empty_tab_message(group_id),
                text,
                "start",
                FONT_SIZE,
            );
        }
    }
}

/// Draws the tab titles of a tab group. The active tab is bold, and a selected bar is underlined.
fn render_tab_group(
    app: &AppState,
    item: &ui::DashboardRect,
    group_id: crate::dashboard::TabGroupId,
    depth: usize,
    text: &str,
    out: &mut SvgOut,
) {
    debug_assert!(is_hex_color(text), "the text color is a hex color");
    let Some(group) = app.layout.tabs(group_id) else {
        return;
    };
    let selected = app.selected_item == Some(item.id);
    let titles = group
        .tabs
        .iter()
        .map(|tab| tab.title.clone())
        .collect::<Vec<_>>();
    let geometry = ui::tab_bar_geometry(item.rect, &titles, group.active, depth);
    let decoration = if selected {
        r#" text-decoration="underline""#
    } else {
        ""
    };
    for segment in geometry.segments {
        let rect = scaled_rect(segment.rect);
        let (color, weight) = if segment.active {
            (app.theme.title, r#" font-weight="bold""#)
        } else {
            (app.theme.text, "")
        };
        out.put(format_args!(
            r#"<text x="{:.2}" y="{:.2}" fill="{}" font-size="{:.1}" text-anchor="start"{}{}>{}</text>"#,
            rect.left,
            rect.top + FONT_SIZE,
            color_hex(color, text),
            FONT_SIZE,
            weight,
            decoration,
            escape_xml(&segment.text)
        ));
    }
    debug_assert!(out.is_complete(), "the tab bar ends with an element");
}

fn render_header(app: &AppState, out: &mut SvgOut, width: f64, text: &str, border: &str) {
    debug_assert!(width > 8.0, "the header frame has a positive width");
    debug_assert!(
        is_hex_color(text) && is_hex_color(border),
        "the header colors are hex colors"
    );
    out.put(format_args!(
        r#"<rect x="4" y="4" width="{:.0}" height="44" fill="none" stroke="{border}"/>"#,
        width - 8.0
    ));
    let title = format!(
        "{} - range={} step={} panels={}",
        app.title,
        humantime::format_duration(app.range),
        humantime::format_duration(app.default_intervals().step),
        displayed_panel_count(app)
    );
    write_text(out, width / 2.0, 31.0, &title, text, "middle", FONT_SIZE);
}

fn displayed_panel_count(app: &AppState) -> String {
    app.panel_count_label()
}

fn render_row_header(
    app: &AppState,
    row_id: crate::dashboard::RowId,
    depth: usize,
    collapsed: bool,
    rect: PlotRect,
    selected: bool,
    out: &mut SvgOut,
) {
    debug_assert!(rect.is_valid(), "the row rectangle is valid");
    let Some(row) = app.layout.row(row_id) else {
        return;
    };
    let color = color_hex(
        if selected {
            app.theme.border_focused
        } else {
            app.theme.border
        },
        "#555555",
    );
    let marker = if collapsed { '▶' } else { '▼' };
    let title = format!("{marker} {}{}", "  ".repeat(depth), row.title);
    write_rect(out, rect, "none", &color, if selected { 2.0 } else { 1.0 });
    write_styled_text(
        out,
        rect.left + 4.0,
        rect.top + (rect.height * 0.75),
        &title,
        TextStyle {
            color: &color,
            anchor: "start",
            size: FONT_SIZE,
            bold: selected,
        },
    );
    debug_assert!(out.is_complete(), "the row header ends with an element");
}

fn render_footer(
    app: &AppState,
    out: &mut SvgOut,
    width: f64,
    height: f64,
    text: &str,
    border: &str,
) {
    debug_assert!(width > 0.0 && height > 0.0, "the SVG has a positive size");
    debug_assert!(
        is_hex_color(text) && is_hex_color(border),
        "the footer colors are hex colors"
    );
    out.put(format_args!(
        r#"<line x1="4" y1="{:.0}" x2="{:.0}" y2="{:.0}" stroke="{border}"/>"#,
        height - 30.0,
        width - 4.0,
        height - 30.0
    ));
    let mode = match app.mode {
        AppMode::Normal => "NORMAL",
        AppMode::Search => "SEARCH",
        AppMode::Fullscreen => "FULLSCREEN",
        AppMode::Inspect => "INSPECT",
        AppMode::FullscreenInspect => "FULLSCREEN INSPECT",
    };
    let recording = if app.recording.is_some() {
        " | REC"
    } else {
        ""
    };
    write_text(
        out,
        10.0,
        height - 11.0,
        &format!("Mode: {mode}{recording}"),
        text,
        "start",
        SMALL_FONT_SIZE,
    );
}

fn render_panel(
    app: &AppState,
    panel_index: usize,
    panel: &PanelState,
    rect: PlotRect,
    selected: bool,
    out: &mut SvgOut,
) {
    debug_assert!(rect.is_valid(), "the panel rectangle is valid");
    render_panel_frame(app, panel, rect, selected, out);
    let inner = PlotRect {
        left: rect.left + PANEL_PADDING,
        top: rect.top + TITLE_HEIGHT,
        width: (rect.width - PANEL_PADDING * 2.0).max(0.0),
        height: (rect.height - TITLE_HEIGHT - PANEL_PADDING).max(0.0),
    };
    debug_assert!(inner.is_valid(), "the inner rectangle is valid");

    if let Some(err) = &panel.last_error
        && panel.series.is_empty()
    {
        render_panel_error(app, err, inner, out);
        return;
    }

    match panel.panel_type {
        PanelType::Graph | PanelType::Unknown => {
            render_graph_panel(app, panel_index, panel, inner, out)
        }
        PanelType::Stat => render_stat_panel(app, panel, inner, out),
        PanelType::Gauge => render_gauge_panel(app, panel, inner, out),
        PanelType::BarGauge => render_bar_gauge_panel(app, panel, inner, out),
        PanelType::Table => render_table_panel(app, panel, inner, out),
        PanelType::Heatmap => render_heatmap_panel(app, panel, inner, out),
    }
    debug_assert!(out.is_complete(), "the panel ends with an element");
}

/// Draws the panel border and the panel title, with the data notice after the title.
fn render_panel_frame(
    app: &AppState,
    panel: &PanelState,
    rect: PlotRect,
    selected: bool,
    out: &mut SvgOut,
) {
    debug_assert!(rect.is_valid(), "the panel rectangle is valid");
    let theme = &app.theme;
    let border = if selected {
        color_hex(theme.border_focused, "#f0d000")
    } else {
        color_hex(theme.border, "#555555")
    };
    let title = color_hex(theme.title, "#00c8ff");
    let bg = color_hex(theme.background, "#111111");

    out.put(format_args!(
        r#"<rect x="{:.0}" y="{:.0}" width="{:.0}" height="{:.0}" fill="{bg}" stroke="{border}"/>"#,
        rect.left, rect.top, rect.width, rect.height
    ));
    // The notice follows the title in the same text, as in the TUI, so the
    // two cannot overlap.
    let notice = ui::data_notice(panel);
    let (title_text, badge) = ui::fit_panel_title(
        &panel.title,
        notice.map(|(text, _)| text),
        text_columns(rect.width - 16.0, FONT_SIZE),
    );
    out.put(format_args!(
        r#"<text x="{:.2}" y="{:.2}" fill="{title}" font-size="{FONT_SIZE:.1}" text-anchor="start">{}"#,
        rect.left + 8.0,
        rect.top + 18.0,
        escape_xml(&title_text)
    ));
    if let (Some(badge), Some((_, level))) = (badge, notice) {
        let color = match level {
            ui::NoticeLevel::Error => color_hex(theme.error, "#ff5555"),
            ui::NoticeLevel::Warning => color_hex(theme.warning, "#f0d000"),
        };
        out.put(format_args!(
            r#" <tspan fill="{color}" data-role="panel-notice">{}</tspan>"#,
            escape_xml(badge)
        ));
    }
    out.push_str("</text>");
    debug_assert!(out.is_complete(), "the panel title ends with an element");
}

/// Draws the query error of a panel without series, wrapped to the inner rectangle `inner`.
fn render_panel_error(app: &AppState, err: &str, inner: PlotRect, out: &mut SvgOut) {
    let color = color_hex(app.theme.error, "#ff5555");
    debug_assert!(inner.is_valid(), "the inner rectangle is valid");
    let max_lines = num::f64_to_usize((inner.height - 18.0).max(0.0) / ERROR_LINE_HEIGHT) + 1;
    let lines = wrap_text(err, text_columns(inner.width, FONT_SIZE), max_lines);
    debug_assert!(lines.len() <= max_lines, "the error fits in its lines");
    for (row, line) in lines.iter().enumerate() {
        write_text(
            out,
            inner.left,
            inner.top + 18.0 + num::usize_to_f64(row) * ERROR_LINE_HEIGHT,
            line,
            &color,
            "start",
            FONT_SIZE,
        );
    }
}

/// The geometry, colors and data of one SVG graph.
struct SvgGraph<'a> {
    app: &'a AppState,
    panel: &'a PanelState,
    plot: PlotRect,
    x_bounds: [f64; 2],
    y_bounds: [f64; 2],
    /// The width that a y label can take, in pixels.
    y_label_room: f64,
    /// The value that area fills and bars start from.
    baseline: f64,
    options: crate::app::GraphOptions,
    /// The color of each series, by series index.
    colors: Vec<Color>,
    /// The value and the theme color of each threshold step.
    thresholds: Vec<(f64, Color)>,
    thresholds_dashed: bool,
    show_grid: bool,
    text: String,
    grid: String,
}

impl SvgGraph<'_> {
    /// Returns the label of the y value `value`, shortened to the label room.
    fn y_label(&self, value: f64) -> String {
        debug_assert!(self.y_label_room.is_finite(), "the label room is finite");
        debug_assert!(self.y_label_room >= 0.0, "the label room is not negative");
        fit_text(
            &self.panel.display.format_number(value),
            self.y_label_room,
            SMALL_FONT_SIZE,
        )
        .0
    }

    /// Returns the line style of the dashed grid lines.
    fn grid_style(&self) -> LineStyle<'_> {
        debug_assert!(is_hex_color(&self.grid), "the grid color is a hex color");
        LineStyle {
            color: &self.grid,
            dash: Some("3 5"),
            width: 0.7,
        }
    }

    /// Returns the SVG color of the series at `index`.
    fn series_color(&self, index: usize) -> String {
        let color = self.colors.get(index).copied().unwrap_or(Color::Reset);
        color_hex(color, "#00ff88")
    }
}

fn render_graph_panel(
    app: &AppState,
    panel_index: usize,
    panel: &PanelState,
    rect: PlotRect,
    out: &mut SvgOut,
) {
    debug_assert!(rect.is_valid(), "the graph rectangle is valid");
    let view = app.graph_view(panel_index, panel);
    if !view.has_content() {
        render_no_data(app, panel, rect, out);
        return;
    }
    if rect.width < 120.0 || rect.height < 80.0 {
        return;
    }
    let y_label_room = y_label_room(panel, view.y_bounds, &view.thresholds, rect.width);
    let legend_items = view.visible_legend_labels();
    let (plot, legend) = graph_geometry(rect, y_label_room, legend_items, view.has_legend());
    debug_assert!(
        plot.left >= rect.left && plot.right() <= rect.right() + 1.0,
        "the plot is in the panel columns"
    );
    let graph = SvgGraph {
        app,
        panel,
        plot,
        x_bounds: view.x_bounds,
        y_bounds: view.y_bounds,
        y_label_room,
        baseline: view.area_baseline(),
        options: view.options,
        colors: view.colors,
        thresholds: view.thresholds,
        thresholds_dashed: view.thresholds_dashed,
        show_grid: app.autogrid_enabled && panel.autogrid.unwrap_or(true),
        text: color_hex(app.theme.text, "#e6e6e6"),
        grid: color_hex(app.grid_color(), "#6d6d6d"),
    };

    render_axes(&graph, out);
    let (show_y_max, thresholds) = render_value_grid(&graph, out);
    let show_x_max = render_time_grid(&graph, out);
    render_axis_end_labels(&graph, show_y_max, show_x_max, out);
    render_thresholds(&graph, &thresholds, out);
    // Area fills remain behind annotations so marker lines stay legible.
    render_area_fills(&graph, out);
    let clusters = render_graph_annotations(app, view.events, plot, graph.x_bounds, out);
    // Primary data follows annotations to preserve graph-data precedence.
    render_series(&graph, out);
    render_cursor(&graph, out);
    render_legend_or_detail(&graph, &clusters, &legend, out);
    debug_assert!(out.is_complete(), "the graph ends with an element");
}

/// Returns the width of the widest y label, up to 40% of the panel width.
fn y_label_room(
    panel: &PanelState,
    y_bounds: [f64; 2],
    thresholds: &[(f64, Color)],
    width: f64,
) -> f64 {
    std::iter::once(y_bounds[0])
        .chain(std::iter::once(y_bounds[1]))
        .chain(ui::svg_value_ticks(y_bounds[0], y_bounds[1]))
        .chain(thresholds.iter().map(|(value, _)| *value))
        .map(|value| text_width(&panel.display.format_number(value), SMALL_FONT_SIZE))
        .fold(0.0, f64::max)
        .min(width * 0.4 - 12.0)
}

/// Places the plot and the legend in the panel rectangle `rect`.
fn graph_geometry(
    rect: PlotRect,
    y_label_room: f64,
    legend_items: Vec<(usize, String)>,
    has_legend: bool,
) -> (PlotRect, LegendLayout) {
    let y_label_width = (y_label_room + 12.0).max(64.0);
    let plot_width = (rect.width - y_label_width - 8.0).max(1.0);
    // The legend takes the rows it needs, up to a third of the panel.
    let max_legend_rows = num::f64_to_usize(
        ((rect.height / 3.0 - LEGEND_PADDING) / LEGEND_ROW_HEIGHT)
            .floor()
            .max(1.0),
    );
    let legend = layout_legend(legend_items, plot_width, max_legend_rows);
    let legend_height = if has_legend {
        (LEGEND_PADDING + num::usize_to_f64(legend.rows) * LEGEND_ROW_HEIGHT).max(LEGEND_HEIGHT)
    } else {
        0.0
    };
    let plot = PlotRect {
        left: rect.left + y_label_width,
        top: rect.top + 6.0,
        width: plot_width,
        height: (rect.height - X_LABEL_HEIGHT - legend_height - 10.0).max(1.0),
    };
    debug_assert!(
        plot.is_valid() && plot.width >= 1.0 && plot.height >= 1.0,
        "the plot is valid and at least one pixel in size"
    );
    debug_assert!(
        max_legend_rows >= 1 && legend.rows <= max_legend_rows,
        "the legend obeys its row limit"
    );
    (plot, legend)
}

/// Draws the y axis and the x axis lines.
fn render_axes(graph: &SvgGraph<'_>, out: &mut SvgOut) {
    let plot = graph.plot;
    debug_assert!(plot.is_valid(), "the plot is valid");
    let axis = color_hex(graph.app.theme.axis, "#777777");
    debug_assert!(is_hex_color(&axis), "the axis color is a hex color");
    out.put(format_args!(
        r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{axis}"/>"#,
        plot.left,
        plot.top,
        plot.left,
        plot.bottom()
    ));
    out.put(format_args!(
        r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{axis}"/>"#,
        plot.left,
        plot.bottom(),
        plot.right(),
        plot.bottom()
    ));
}

/// A threshold value and color, with whether its label fits on the y axis.
type LabelledThreshold = ((f64, Color), bool);

/// Draws the value grid and its labels. Returns whether the y maximum label fits, and
/// each threshold in range with whether its label fits.
fn render_value_grid(graph: &SvgGraph<'_>, out: &mut SvgOut) -> (bool, Vec<LabelledThreshold>) {
    let (plot, y_bounds) = (graph.plot, graph.y_bounds);
    debug_assert!(y_bounds[0] <= y_bounds[1], "the y bounds are in order");
    // Labels give way rather than overprint: the axis ends first, then
    // thresholds, then ticks. A label that gives way keeps its line.
    let y_extent = |y: f64| [y + 4.0 - SMALL_FONT_SIZE, y + 4.0];
    let mut y_labels = AxisLabels::default();
    y_labels.claim(y_extent(plot.bottom()));
    let show_y_max = y_labels.claim(y_extent(plot.top));
    let thresholds = graph
        .thresholds
        .iter()
        .filter(|(value, _)| *value > y_bounds[0] && *value < y_bounds[1])
        .map(|&line| {
            let labelled = y_labels.claim(y_extent(map_y(line.0, y_bounds, plot)));
            (line, labelled)
        })
        .collect::<Vec<_>>();
    for tick in ui::svg_value_ticks(y_bounds[0], y_bounds[1]) {
        let y = map_y(tick, y_bounds, plot);
        let labelled = y_labels.claim(y_extent(y));
        if graph.show_grid {
            draw_line(out, (plot.left, y), (plot.right(), y), graph.grid_style());
        }
        if labelled {
            let label = graph.y_label(tick);
            write_text(
                out,
                plot.left - 8.0,
                y + 4.0,
                &label,
                &graph.grid,
                "end",
                SMALL_FONT_SIZE,
            );
        }
    }
    debug_assert!(
        thresholds.len() <= graph.thresholds.len(),
        "the grid keeps no more thresholds than the graph has"
    );
    debug_assert!(out.is_complete(), "the value grid ends with an element");
    (show_y_max, thresholds)
}

/// Draws the time grid and its labels. Returns whether the end time label fits.
fn render_time_grid(graph: &SvgGraph<'_>, out: &mut SvgOut) -> bool {
    let plot = graph.plot;
    let [x_min, x_max] = graph.x_bounds;
    debug_assert!(x_min <= x_max, "the x bounds are in order");
    let start_label = ui::format_time(x_min);
    let end_label = ui::format_time(x_max);
    let mut x_labels = AxisLabels::default();
    x_labels.claim([
        plot.left,
        plot.left + text_width(&start_label, SMALL_FONT_SIZE),
    ]);
    let show_x_max = x_labels.claim([
        plot.right() - text_width(&end_label, SMALL_FONT_SIZE),
        plot.right(),
    ]);
    for tick in ui::svg_time_ticks(x_min, x_max) {
        let x = map_x(tick, graph.x_bounds, plot);
        let label = ui::format_time(tick);
        let half = text_width(&label, SMALL_FONT_SIZE) / 2.0;
        let labelled = x_labels.claim([x - half, x + half]);
        if graph.show_grid {
            draw_line(out, (x, plot.top), (x, plot.bottom()), graph.grid_style());
        }
        if labelled {
            write_text(
                out,
                x,
                plot.bottom() + 17.0,
                &label,
                &graph.grid,
                "middle",
                SMALL_FONT_SIZE,
            );
        }
    }
    debug_assert!(out.is_complete(), "the time grid ends with an element");
    show_x_max
}

/// Draws the labels at the ends of the y axis and the x axis.
fn render_axis_end_labels(
    graph: &SvgGraph<'_>,
    show_y_max: bool,
    show_x_max: bool,
    out: &mut SvgOut,
) {
    let (plot, text) = (graph.plot, &graph.text);
    debug_assert!(plot.is_valid(), "the plot is valid");
    let label_x = plot.left - 8.0;
    let y_min = graph.y_label(graph.y_bounds[0]);
    write_text(
        out,
        label_x,
        plot.bottom() + 4.0,
        &y_min,
        text,
        "end",
        SMALL_FONT_SIZE,
    );
    if show_y_max {
        let y_max = graph.y_label(graph.y_bounds[1]);
        write_text(
            out,
            label_x,
            plot.top + 4.0,
            &y_max,
            text,
            "end",
            SMALL_FONT_SIZE,
        );
    }
    let label_y = plot.bottom() + 17.0;
    let start_label = ui::format_time(graph.x_bounds[0]);
    write_text(
        out,
        plot.left,
        label_y,
        &start_label,
        text,
        "start",
        SMALL_FONT_SIZE,
    );
    if show_x_max {
        let end_label = ui::format_time(graph.x_bounds[1]);
        write_text(
            out,
            plot.right(),
            label_y,
            &end_label,
            text,
            "end",
            SMALL_FONT_SIZE,
        );
    }
    debug_assert!(out.is_complete(), "the labels end with an element");
}

/// Draws each threshold line in range, with its label where the label fits.
fn render_thresholds(graph: &SvgGraph<'_>, thresholds: &[LabelledThreshold], out: &mut SvgOut) {
    let plot = graph.plot;
    let [lower, upper] = graph.y_bounds;
    debug_assert!(
        thresholds
            .iter()
            .all(|((value, _), _)| *value > lower && *value < upper),
        "each threshold is inside the y bounds"
    );
    for &((value, color), labelled) in thresholds {
        let y = map_y(value, graph.y_bounds, plot);
        let color = color_hex(color, "#ffaa00");
        draw_line(
            out,
            (plot.left, y),
            (plot.right(), y),
            LineStyle {
                color: &color,
                dash: graph.thresholds_dashed.then_some("6 5"),
                width: 1.2,
            },
        );
        if labelled {
            let label = graph.y_label(value);
            write_text(
                out,
                plot.left - 8.0,
                y + 4.0,
                &label,
                &color,
                "end",
                SMALL_FONT_SIZE,
            );
        }
    }
    debug_assert!(out.is_complete(), "the thresholds end with an element");
}

/// Draws the area fill of each visible series of a line graph with fill opacity.
fn render_area_fills(graph: &SvgGraph<'_>, out: &mut SvgOut) {
    if graph.options.draw_style != crate::app::GraphDrawStyle::Line {
        return;
    }
    let Some(opacity) = graph_area_opacity(&graph.options) else {
        return;
    };
    debug_assert!(
        graph.colors.len() == graph.panel.series.len(),
        "each series has a color"
    );
    debug_assert!(
        opacity > 0.0 && opacity <= 1.0,
        "the fill opacity is a fraction"
    );
    for (series, color) in graph.panel.series.iter().zip(&graph.colors) {
        if !series.visible {
            continue;
        }
        let color = color_hex(*color, "#00ff88");
        render_graph_area(graph, series, &color, opacity, out);
    }
}

/// Draws each visible series in the draw style of the graph.
fn render_series(graph: &SvgGraph<'_>, out: &mut SvgOut) {
    debug_assert!(
        graph.colors.len() == graph.panel.series.len(),
        "each series has a color"
    );
    debug_assert!(out.is_complete(), "the SVG text ends with an element");
    for (series, color) in graph.panel.series.iter().zip(&graph.colors) {
        if !series.visible {
            continue;
        }
        let color = color_hex(*color, "#00ff88");
        match graph.options.draw_style {
            crate::app::GraphDrawStyle::Points => render_graph_points(graph, series, &color, out),
            crate::app::GraphDrawStyle::Bars => render_graph_bars(graph, series, &color, out),
            crate::app::GraphDrawStyle::Line => render_graph_line(graph, series, &color, out),
        }
    }
}

/// Draws one series as a line, with its points when the graph always shows points.
fn render_graph_line(graph: &SvgGraph<'_>, series: &SeriesView, color: &str, out: &mut SvgOut) {
    debug_assert!(is_hex_color(color), "the series color is a hex color");
    debug_assert!(graph.plot.is_valid(), "the plot is valid");
    if let Some(path) = series_path(series, graph.x_bounds, graph.y_bounds, graph.plot) {
        out.put(format_args!(
            r#"<path data-role="graph-line" d="{path}" fill="none" stroke="{color}" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round"/>"#
        ));
    }
    if graph.options.show_points == crate::app::GraphPointMode::Always {
        render_graph_points(graph, series, color, out);
    }
}

/// Draws the cursor line when the cursor is in the time range.
fn render_cursor(graph: &SvgGraph<'_>, out: &mut SvgOut) {
    let [x_min, x_max] = graph.x_bounds;
    let Some(cursor_x) = graph.app.cursor_x else {
        return;
    };
    if cursor_x < x_min || cursor_x > x_max {
        return;
    }
    let x = map_x(cursor_x, graph.x_bounds, graph.plot);
    debug_assert!(
        x >= graph.plot.left && x <= graph.plot.right(),
        "the cursor line is in the plot"
    );
    debug_assert!(
        graph.plot.height >= 0.0,
        "the cursor line has no negative length"
    );
    draw_line(
        out,
        (x, graph.plot.top),
        (x, graph.plot.bottom()),
        LineStyle {
            color: &color_hex(graph.app.theme.cursor, "#ffffff"),
            dash: Some("4 4"),
            width: 1.0,
        },
    );
}

/// Draws the details of the annotation cluster at the cursor, or else the legend.
fn render_legend_or_detail(
    graph: &SvgGraph<'_>,
    clusters: &[AnnotationCluster<'_>],
    legend: &LegendLayout,
    out: &mut SvgOut,
) {
    let plot = graph.plot;
    debug_assert!(
        legend
            .entries
            .iter()
            .all(|entry| entry.index < graph.colors.len()),
        "each legend entry is a series of the graph"
    );
    debug_assert!(
        clusters.iter().all(|cluster| !cluster.events.is_empty()),
        "each annotation cluster has an event"
    );
    let active_annotation =
        active_export_annotation(clusters, graph.app.cursor_x, graph.x_bounds, plot);
    let legend_top = plot.bottom() + X_LABEL_HEIGHT;
    if let Some(cluster) = active_annotation {
        render_annotation_details(
            cluster,
            plot.left,
            legend_top,
            plot.width,
            &color_hex(graph.app.theme.annotation, "#f0d000"),
            out,
        );
    } else {
        render_legend(graph, legend, plot.left, legend_top, out);
    }
}

fn render_graph_annotations<'a>(
    app: &AppState,
    events: Vec<&'a crate::annotations::AnnotationEvent>,
    plot: PlotRect,
    x_bounds: [f64; 2],
    out: &mut SvgOut,
) -> Vec<AnnotationCluster<'a>> {
    let clusters = cluster_projected_events(events.into_iter().map(|event| {
        let x = map_x(event.timestamp_secs(), x_bounds, plot).round();
        (Some(num::f64_to_u32(x)), event)
    }));
    let color = color_hex(app.theme.annotation, "#f0d000");

    for cluster in &clusters {
        let x = f64::from(cluster.coordinate);
        out.put(format_args!(
            r#"<line data-role="annotation-marker" x1="{x:.2}" y1="{:.2}" x2="{x:.2}" y2="{:.2}" stroke="{color}" stroke-width="1.00" stroke-dasharray="3 4"/>"#,
            plot.top,
            plot.bottom()
        ));
        out.put(format_args!(
            r#"<text x="{x:.2}" y="{:.2}" fill="{color}" font-size="{SMALL_FONT_SIZE:.1}" text-anchor="middle" data-role="annotation-count">{}</text>"#,
            plot.top + SMALL_FONT_SIZE,
            crate::ui::cluster_badge(cluster.events.len())
        ));
    }

    clusters
}

fn active_export_annotation<'a>(
    clusters: &'a [AnnotationCluster<'a>],
    cursor_x: Option<f64>,
    x_bounds: [f64; 2],
    plot: PlotRect,
) -> Option<&'a AnnotationCluster<'a>> {
    let cursor_x = cursor_x?;
    if cursor_x < x_bounds[0] || cursor_x > x_bounds[1] {
        return None;
    }
    let coordinate = num::f64_to_u32(map_x(cursor_x, x_bounds, plot).round());
    clusters
        .iter()
        .find(|cluster| cluster.coordinate == coordinate)
}

fn render_annotation_details(
    cluster: &AnnotationCluster<'_>,
    left: f64,
    top: f64,
    width: f64,
    color: &str,
    out: &mut SvgOut,
) {
    debug_assert!(is_hex_color(color), "the annotation color is a hex color");
    debug_assert!(
        left.is_finite() && top.is_finite(),
        "the details have a finite position"
    );
    let character_budget = if width.is_finite() && width > 0.0 {
        num::f64_to_usize((width / SMALL_FONT_SIZE).floor())
    } else {
        0
    };
    let [heading, details] = format_cluster_detail_lines(cluster, character_budget);
    for (row, line) in [heading, details].iter().enumerate() {
        out.put(format_args!(
            r#"<text x="{left:.2}" y="{:.2}" fill="{color}" font-size="{SMALL_FONT_SIZE:.1}" text-anchor="start" data-role="annotation-detail">{}</text>"#,
            top + 12.0 + num::usize_to_f64(row) * 13.0,
            escape_xml(line)
        ));
    }
}

fn render_stat_panel(app: &AppState, panel: &PanelState, rect: PlotRect, out: &mut SvgOut) {
    debug_assert!(rect.is_valid(), "the stat rectangle is valid");
    let Some(series) = panel.series.iter().find(|series| series.visible) else {
        render_no_data(app, panel, rect, out);
        return;
    };

    // As in the TUI, Grafana's noValue stands in for a null value here, and
    // render_no_data shows it for no series at all.
    let color = series
        .value
        .map(|value| value_color(app, panel, value))
        .unwrap_or_else(|| color_hex(app.theme.text, "#e6e6e6"));
    let text = color_hex(app.theme.text, "#e6e6e6");
    // As Grafana does, the value shrinks to fit a narrow panel.
    let width = rect.width - 8.0;
    let (value, size) = fit_text(
        &panel.display.format_value(series.value),
        width,
        STAT_FONT_SIZE,
    );
    debug_assert!(
        (SMALL_FONT_SIZE..=STAT_FONT_SIZE).contains(&size),
        "the stat value size is in the font size range"
    );
    write_text(
        out,
        rect.left + rect.width / 2.0,
        rect.top + 34.0,
        &value,
        &color,
        "middle",
        size,
    );
    let (name, _) = fit_text(&series.name, width, SMALL_FONT_SIZE);
    write_text(
        out,
        rect.left + rect.width / 2.0,
        rect.top + 56.0,
        &name,
        &text,
        "middle",
        SMALL_FONT_SIZE,
    );

    let sparkline = PlotRect {
        left: rect.left + 8.0,
        top: rect.top + 72.0,
        width: (rect.width - 16.0).max(1.0),
        height: (rect.height - 82.0).max(1.0),
    };
    debug_assert!(sparkline.is_valid(), "the sparkline rectangle is valid");
    render_sparkline(series, sparkline, &color, out);
}

fn render_gauge_panel(app: &AppState, panel: &PanelState, rect: PlotRect, out: &mut SvgOut) {
    let Some((series, value)) = first_visible_value(panel) else {
        render_no_data(app, panel, rect, out);
        return;
    };

    let min = panel.min.unwrap_or(0.0);
    let max = panel
        .max
        .unwrap_or(if value > 100.0 { value * 1.2 } else { 100.0 });
    let ratio = value_ratio(value, min, max);
    debug_assert!(
        !(ratio < 0.0 || ratio > 1.0),
        "the ratio is not out of range"
    );
    let color = value_color(app, panel, value);
    let text = color_hex(app.theme.text, "#e6e6e6");
    let track = color_hex(app.theme.gauge_track, "#444444");
    let gauge = PlotRect {
        left: rect.left + 10.0,
        top: rect.top + rect.height / 2.0 - 13.0,
        width: (rect.width - 20.0).max(1.0),
        height: 26.0,
    };

    write_rect(out, gauge, &track, "none", 0.0);
    let fill = PlotRect {
        width: gauge.width * ratio,
        ..gauge
    };
    write_rect(out, fill, &color, "none", 0.0);
    let center = rect.left + rect.width / 2.0;
    let (name, _) = fit_text(&series.name, gauge.width, SMALL_FONT_SIZE);
    write_text(
        out,
        center,
        gauge.top - 12.0,
        &name,
        &text,
        "middle",
        SMALL_FONT_SIZE,
    );
    let label = format!(
        "{} ({:.0}%)",
        panel.display.format_number(value),
        ratio * 100.0
    );
    let (label, size) = fit_text(&label, gauge.width, FONT_SIZE);
    let label_color = color_hex(app.theme.text, "#ffffff");
    write_text(
        out,
        center,
        gauge.top + 18.0,
        &label,
        &label_color,
        "middle",
        size,
    );
    render_gauge_range_labels(panel, gauge, [min, max], &text, out);
}

/// Draws the minimum and the maximum labels below a gauge. The maximum label gives way to the minimum label.
fn render_gauge_range_labels(
    panel: &PanelState,
    gauge: PlotRect,
    [min, max]: [f64; 2],
    text: &str,
    out: &mut SvgOut,
) {
    let (min_label, _) = fit_text(
        &panel.display.format_number(min),
        gauge.width,
        SMALL_FONT_SIZE,
    );
    let (max_label, _) = fit_text(
        &panel.display.format_number(max),
        gauge.width,
        SMALL_FONT_SIZE,
    );
    write_text(
        out,
        gauge.left,
        gauge.bottom() + 17.0,
        &min_label,
        text,
        "start",
        SMALL_FONT_SIZE,
    );
    let mut labels = AxisLabels::default();
    labels.claim([
        gauge.left,
        gauge.left + text_width(&min_label, SMALL_FONT_SIZE),
    ]);
    if labels.claim([
        gauge.right() - text_width(&max_label, SMALL_FONT_SIZE),
        gauge.right(),
    ]) {
        write_text(
            out,
            gauge.right(),
            gauge.bottom() + 17.0,
            &max_label,
            text,
            "end",
            SMALL_FONT_SIZE,
        );
    }
}

/// The column widths, the scale and the colors of a bar gauge panel.
struct BarGaugeLayout {
    rect: PlotRect,
    label_width: f64,
    value_width: f64,
    bar_width: f64,
    max_value: f64,
    text: String,
    track: String,
}

fn render_bar_gauge_panel(app: &AppState, panel: &PanelState, rect: PlotRect, out: &mut SvgOut) {
    let mut values = panel
        .series
        .iter()
        .filter(|series| series.visible)
        .filter_map(|series| series.value.map(|value| (series, value)))
        .collect::<Vec<_>>();
    values.sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    if values.is_empty() {
        render_no_data(app, panel, rect, out);
        return;
    }

    let max_value = values
        .iter()
        .filter_map(|(_, value)| value.is_finite().then_some(*value))
        .fold(0.0_f64, f64::max)
        .max(1.0);
    debug_assert!(
        max_value.is_finite() && max_value >= 1.0,
        "the bar scale is finite and at least one"
    );
    let row_height = 22.0;
    let max_rows = num::f64_to_usize((rect.height / row_height).floor().max(1.0));
    let shown = values.into_iter().take(max_rows).collect::<Vec<_>>();
    debug_assert!(
        !shown.is_empty() && shown.len() <= max_rows,
        "the panel shows one bar or more, up to the row limit"
    );
    // The name column is as wide as the widest name, up to half the panel,
    // and the value column as wide as the widest value, up to 30%.
    let label_width = shown
        .iter()
        .map(|(series, _)| text_width(&series.name, SMALL_FONT_SIZE) + 8.0)
        .fold(0.0, f64::max)
        .min(rect.width * 0.5);
    let value_width = shown
        .iter()
        .map(|(_, value)| text_width(&panel.display.format_number(*value), SMALL_FONT_SIZE))
        .fold(0.0, f64::max)
        .min(rect.width * 0.3);
    let layout = BarGaugeLayout {
        rect,
        label_width,
        value_width,
        bar_width: (rect.width - label_width - value_width - 8.0).max(1.0),
        max_value,
        text: color_hex(app.theme.text, "#e6e6e6"),
        track: color_hex(app.theme.gauge_track, "#444444"),
    };

    for (row, (series, value)) in shown.into_iter().enumerate() {
        let y = rect.top + num::usize_to_f64(row) * row_height + 15.0;
        render_bar_gauge_row(app, panel, &layout, y, (series, value), out);
    }
}

/// Draws the name, the bar and the value of one series of a bar gauge at the baseline `y`.
fn render_bar_gauge_row(
    app: &AppState,
    panel: &PanelState,
    layout: &BarGaugeLayout,
    y: f64,
    (series, value): (&SeriesView, f64),
    out: &mut SvgOut,
) {
    let rect = layout.rect;
    debug_assert!(layout.bar_width >= 1.0, "the bar has a width");
    debug_assert!(layout.max_value >= 1.0, "the scale is at least one");
    // A NaN value draws an empty bar, as in the TUI.
    let ratio = (value / layout.max_value).clamp(0.0, 1.0);
    let ratio = if ratio.is_nan() { 0.0 } else { ratio };
    let color = value_color(app, panel, value);
    let (name, _) = fit_text(&series.name, layout.label_width - 8.0, SMALL_FONT_SIZE);
    write_text(
        out,
        rect.left + 4.0,
        y,
        &name,
        &layout.text,
        "start",
        SMALL_FONT_SIZE,
    );
    let track_rect = PlotRect {
        left: rect.left + layout.label_width,
        top: y - 12.0,
        width: layout.bar_width,
        height: 14.0,
    };
    write_rect(out, track_rect, &layout.track, "none", 0.0);
    write_rect(
        out,
        PlotRect {
            width: track_rect.width * ratio,
            ..track_rect
        },
        &color,
        "none",
        0.0,
    );
    let (value, _) = fit_text(
        &panel.display.format_number(value),
        layout.value_width,
        SMALL_FONT_SIZE,
    );
    write_text(
        out,
        track_rect.right() + 8.0,
        y,
        &value,
        &color,
        "start",
        SMALL_FONT_SIZE,
    );
}

/// The columns and the text color of a table panel.
struct TableLayout {
    rect: PlotRect,
    row_height: f64,
    value_x: f64,
    name_width: f64,
    value_width: f64,
    text: String,
}

fn render_table_panel(app: &AppState, panel: &PanelState, rect: PlotRect, out: &mut SvgOut) {
    let values = panel
        .series
        .iter()
        .filter(|series| series.visible)
        .collect::<Vec<_>>();
    if values.is_empty() {
        render_no_data(app, panel, rect, out);
        return;
    }

    debug_assert!(rect.is_valid(), "the table rectangle is valid");
    let row_height = 20.0;
    let value_x = rect.left + rect.width * 0.7;
    debug_assert!(
        value_x >= rect.left && value_x <= rect.right(),
        "the value column is in the table"
    );
    let layout = TableLayout {
        rect,
        row_height,
        value_x,
        name_width: value_x - rect.left - 6.0 - AXIS_LABEL_GAP,
        value_width: rect.right() - value_x - 4.0,
        text: color_hex(app.theme.text, "#e6e6e6"),
    };
    let max_rows = num::f64_to_usize(((rect.height - row_height) / row_height).floor().max(1.0));
    debug_assert!(max_rows >= 1, "the table shows one row or more");

    render_table_header(app, &layout, out);
    for (row, series) in values.into_iter().take(max_rows).enumerate() {
        let y = rect.top + row_height * (num::usize_to_f64(row) + 2.0) - 5.0;
        render_table_row(app, panel, &layout, y, series, out);
    }
}

/// Draws the column titles of a table and the line below them.
fn render_table_header(app: &AppState, layout: &TableLayout, out: &mut SvgOut) {
    let rect = layout.rect;
    debug_assert!(rect.is_valid(), "the table rectangle is valid");
    let title = color_hex(app.theme.title, "#00c8ff");
    let border = color_hex(app.theme.border, "#555555");
    write_text(
        out,
        rect.left + 6.0,
        rect.top + 15.0,
        &fit_text("Series", layout.name_width, SMALL_FONT_SIZE).0,
        &title,
        "start",
        SMALL_FONT_SIZE,
    );
    write_text(
        out,
        layout.value_x,
        rect.top + 15.0,
        &fit_text("Value", layout.value_width, SMALL_FONT_SIZE).0,
        &title,
        "start",
        SMALL_FONT_SIZE,
    );
    draw_line(
        out,
        (rect.left, rect.top + layout.row_height),
        (rect.right(), rect.top + layout.row_height),
        LineStyle {
            color: &border,
            dash: None,
            width: 0.8,
        },
    );
    debug_assert!(out.is_complete(), "the table header ends with an element");
}

/// Draws the name and the value of one series of a table at the baseline `y`.
fn render_table_row(
    app: &AppState,
    panel: &PanelState,
    layout: &TableLayout,
    y: f64,
    series: &SeriesView,
    out: &mut SvgOut,
) {
    debug_assert!(is_hex_color(&layout.text), "the text color is a hex color");
    debug_assert!(y.is_finite(), "the row baseline is finite");
    let value = series
        .value
        .map(|value| panel.display.format_number(value))
        .unwrap_or_else(|| panel.display.format_value(None));
    let value_color = series
        .value
        .map(|value| value_color(app, panel, value))
        .unwrap_or_else(|| layout.text.clone());
    let (name, _) = fit_text(&series.name, layout.name_width, SMALL_FONT_SIZE);
    let (value, _) = fit_text(&value, layout.value_width, SMALL_FONT_SIZE);
    write_text(
        out,
        layout.rect.left + 6.0,
        y,
        &name,
        &layout.text,
        "start",
        SMALL_FONT_SIZE,
    );
    write_text(
        out,
        layout.value_x,
        y,
        &value,
        &value_color,
        "start",
        SMALL_FONT_SIZE,
    );
}

fn render_heatmap_panel(app: &AppState, panel: &PanelState, rect: PlotRect, out: &mut SvgOut) {
    let visible = panel
        .series
        .iter()
        .filter(|series| series.visible)
        .collect::<Vec<_>>();
    if visible.is_empty() {
        render_no_data(app, panel, rect, out);
        return;
    }
    let Some(range) = heatmap_range(&visible) else {
        render_no_data(app, panel, rect, out);
        return;
    };

    debug_assert!(range[0] < range[1], "the heatmap range is not empty");
    let label_width = (rect.width * 0.24).clamp(60.0, 150.0);
    let plot_left = rect.left + label_width;
    let plot_width = (rect.width - label_width - 4.0).max(1.0);
    let rows = visible
        .len()
        .min(num::f64_to_usize((rect.height / 18.0).floor().max(1.0)));
    let row_height = (rect.height / num::usize_to_f64(rows)).max(1.0);
    let cols = num::f64_to_usize((plot_width / 10.0).floor().max(1.0));
    debug_assert!(
        rows >= 1 && rows <= visible.len(),
        "the heatmap shows one series or more, up to the row limit"
    );
    debug_assert!(cols >= 1, "the heatmap has a column");
    let cell_width = plot_width / num::usize_to_f64(cols);
    let text = color_hex(app.theme.text, "#e6e6e6");

    for (row, series) in visible.into_iter().take(rows).enumerate() {
        let top = rect.top + num::usize_to_f64(row) * row_height;
        write_text(
            out,
            rect.left + 4.0,
            top + row_height.min(16.0),
            &series.name,
            &text,
            "start",
            SMALL_FONT_SIZE,
        );
        let cells = PlotRect {
            left: plot_left,
            top,
            width: cell_width,
            height: row_height,
        };
        render_heatmap_cells(app, series, cells, cols, range, out);
    }
}

/// Returns the least and the greatest finite value of the series, or `None` for no range.
fn heatmap_range(series: &[&SeriesView]) -> Option<[f64; 2]> {
    debug_assert!(!series.is_empty(), "the heatmap has a series");
    let (mut min, mut max) = (f64::MAX, f64::MIN);
    for (_, value) in series.iter().flat_map(|series| &series.points) {
        if value.is_finite() {
            min = min.min(*value);
            max = max.max(*value);
        }
    }
    if !min.is_finite() || !max.is_finite() || min == max {
        return None;
    }
    debug_assert!(min < max, "the heatmap range is in order and not empty");
    Some([min, max])
}

/// Draws `cols` heatmap cells of one series. `cells` holds the row top, the row height, the
/// left edge and the cell width.
fn render_heatmap_cells(
    app: &AppState,
    series: &SeriesView,
    cells: PlotRect,
    cols: usize,
    [min, max]: [f64; 2],
    out: &mut SvgOut,
) {
    if series.points.is_empty() {
        return;
    }
    let step = (num::usize_to_f64(series.points.len()) / num::usize_to_f64(cols)).max(1.0);
    for col in 0..cols {
        let point = num::f64_to_usize(num::usize_to_f64(col) * step).min(series.points.len() - 1);
        // The `min` keeps the index in the series. NaN draws the empty color.
        let value = series
            .points
            .get(point)
            .map_or(f64::NAN, |&(_, value)| value);
        let color = if value.is_finite() {
            let normalized = ((value - min) / (max - min)).clamp(0.0, 1.0);
            color_hex(
                ui::value_to_heatmap_color(normalized, app.theme.heatmap),
                "#666666",
            )
        } else {
            color_hex(app.theme.heatmap_empty, "#444444")
        };
        write_rect(
            out,
            PlotRect {
                left: cells.left + num::usize_to_f64(col) * cells.width,
                top: cells.top,
                width: cells.width.max(1.0),
                height: (cells.height - 2.0).max(1.0),
            },
            &color,
            "none",
            0.0,
        );
    }
}

fn first_visible_value(panel: &PanelState) -> Option<(&SeriesView, f64)> {
    let first = panel
        .series
        .iter()
        .filter(|series| series.visible)
        .find_map(|series| series.value.map(|value| (series, value)));
    debug_assert!(
        first.is_none_or(|(series, _)| series.visible),
        "the first value is of a visible series"
    );
    debug_assert!(
        first.is_none_or(|(series, value)| series
            .value
            .is_some_and(|own| own.to_bits() == value.to_bits())),
        "the value is the value of its series"
    );
    first
}

fn value_color(app: &AppState, panel: &PanelState, value: f64) -> String {
    debug_assert!(
        !app.theme.palette.is_empty(),
        "the theme palette has a color"
    );
    let color = color_hex(
        panel
            .get_color_for_value(value)
            .unwrap_or(app.theme.palette_color(0)),
        "#00ff88",
    );
    debug_assert!(is_hex_color(&color), "the value color is a hex color");
    color
}

fn value_ratio(value: f64, min: f64, max: f64) -> f64 {
    let ratio = if !value.is_finite() || max <= min {
        0.0
    } else {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    };
    // A limit that is not finite can give NaN, which the SVG writes as is.
    debug_assert!(
        !(ratio < 0.0 || ratio > 1.0),
        "the ratio is not outside the unit range"
    );
    debug_assert!(
        value.is_finite() || ratio == 0.0,
        "a value that is not finite gives a ratio of zero"
    );
    ratio
}

fn render_no_data(app: &AppState, panel: &PanelState, rect: PlotRect, out: &mut SvgOut) {
    debug_assert!(rect.is_valid(), "the panel rectangle is valid");
    let columns = text_columns(rect.width - 8.0, FONT_SIZE);
    let text = ui::truncate_title(
        panel.display.no_data_text(),
        u16::try_from(columns).unwrap_or(u16::MAX),
    );
    write_text(
        out,
        rect.left + 8.0,
        rect.top + 24.0,
        &text,
        &color_hex(app.theme.text, "#e6e6e6"),
        "start",
        FONT_SIZE,
    );
    debug_assert!(out.is_complete(), "the notice ends with an element");
}

fn render_sparkline(series: &SeriesView, rect: PlotRect, color: &str, out: &mut SvgOut) {
    if rect.width <= 0.0 || rect.height <= 0.0 || series.points.len() < 2 {
        return;
    }

    let values = series
        .points
        .iter()
        .filter_map(|(_, value)| value.is_finite().then_some(*value))
        .collect::<Vec<_>>();
    if values.len() < 2 {
        return;
    }

    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    debug_assert!(min <= max, "the sparkline range is in order");
    let span = (max - min).max(f64::EPSILON);
    let step = rect.width / num::usize_to_f64(values.len() - 1);
    debug_assert!(
        step > 0.0 && step.is_finite(),
        "the sparkline step is finite and positive"
    );
    let mut path = SvgOut::default();
    for (index, value) in values.iter().enumerate() {
        let x = rect.left + num::usize_to_f64(index) * step;
        let y = rect.bottom() - ((*value - min) / span).clamp(0.0, 1.0) * rect.height;
        if index == 0 {
            path.put(format_args!("M {x:.2} {y:.2}"));
        } else {
            path.put(format_args!(" L {x:.2} {y:.2}"));
        }
    }
    let path = path.finish();

    out.put(format_args!(
        r#"<path d="{path}" fill="none" stroke="{color}" stroke-width="1.4" stroke-linejoin="round" stroke-linecap="round"/>"#
    ));
}

/// A graph legend laid out in rows.
struct LegendLayout {
    entries: Vec<LegendEntry>,
    /// Where "+N more" starts on the last row, and the text, when some
    /// entries did not fit.
    more: Option<(f64, String)>,
    rows: usize,
}

/// One legend label, with its series index and its place in the legend.
struct LegendEntry {
    index: usize,
    label: String,
    x: f64,
    row: usize,
}

/// Lays legend entries out in rows of `width` pixels, shortening any label
/// wider than a row. Past `max_rows`, the last row ends with "+N more".
fn layout_legend(items: Vec<(usize, String)>, width: f64, max_rows: usize) -> LegendLayout {
    let entry_width = |label: &str| LEGEND_SWATCH + text_width(label, SMALL_FONT_SIZE);
    let mut entries: Vec<LegendEntry> = Vec::new();
    let (mut x, mut row) = (0.0, 0);
    for (index, label) in items {
        let label = fit_text(&label, width - LEGEND_SWATCH, SMALL_FONT_SIZE).0;
        let label_width = entry_width(&label);
        if x > 0.0 && x + label_width > width {
            x = 0.0;
            row += 1;
        }
        entries.push(LegendEntry {
            index,
            label,
            x,
            row,
        });
        x += label_width + LEGEND_GAP;
    }
    let rows = entries.last().map_or(0, |entry| entry.row + 1);
    debug_assert!(rows <= entries.len(), "each row has an entry");
    if rows <= max_rows {
        return LegendLayout {
            entries,
            more: None,
            rows,
        };
    }

    let total = entries.len();
    let last_row = max_rows.max(1) - 1;
    entries.retain(|entry| entry.row <= last_row);
    let mut more = (0.0, String::new());
    // Each pass drops one entry. "+N more" always fits after the last entry
    // goes, so one pass more than the entries is enough.
    for _ in 0..=entries.len() {
        let label = format!("+{} more", total - entries.len());
        let x = match entries.last() {
            Some(entry) if entry.row == last_row => {
                entry.x + entry_width(&entry.label) + LEGEND_GAP
            }
            _ => 0.0,
        };
        let fits = x == 0.0 || x + text_width(&label, SMALL_FONT_SIZE) <= width;
        more = (x, label);
        if fits {
            break;
        }
        entries.pop();
    }
    debug_assert!(entries.len() < total, "some entries are cut");
    LegendLayout {
        entries,
        more: Some(more),
        rows: last_row + 1,
    }
}

/// Draws the legend entries and the "+N more" text of a graph.
fn render_legend(
    graph: &SvgGraph<'_>,
    legend: &LegendLayout,
    left: f64,
    top: f64,
    out: &mut SvgOut,
) {
    let app = graph.app;
    debug_assert!(
        legend.more.is_none() || legend.rows >= 1,
        "a legend with more text has a row"
    );
    debug_assert!(
        legend.entries.iter().all(|entry| entry.row < legend.rows),
        "each legend entry is in a legend row"
    );
    let text = color_hex(app.theme.text, "#e6e6e6");
    let baseline =
        |row: usize| top + LEGEND_PADDING + 2.0 + num::usize_to_f64(row) * LEGEND_ROW_HEIGHT;
    for entry in &legend.entries {
        let color = graph.series_color(entry.index);
        let (x, y) = (left + entry.x, baseline(entry.row));
        out.put(format_args!(
            r#"<rect x="{:.2}" y="{:.2}" width="8" height="8" fill="{color}"/>"#,
            x,
            y - 8.0
        ));
        write_text(
            out,
            x + LEGEND_SWATCH,
            y,
            &entry.label,
            &text,
            "start",
            SMALL_FONT_SIZE,
        );
    }
    if let Some((x, more)) = &legend.more {
        let muted = color_hex(app.grid_color(), "#6d6d6d");
        write_text(
            out,
            left + x,
            baseline(legend.rows - 1),
            more,
            &muted,
            "start",
            SMALL_FONT_SIZE,
        );
    }
}

fn graph_area_opacity(options: &crate::app::GraphOptions) -> Option<f64> {
    let opacity = options
        .fill_opacity
        .filter(|value| *value > 0)
        .map(|value| f64::from(value.min(100)) / 100.0);
    debug_assert!(
        opacity.is_none_or(|opacity| opacity > 0.0 && opacity <= 1.0),
        "the fill opacity is a fraction"
    );
    opacity
}

fn graph_point_in_range(x: f64, y: f64, x_bounds: [f64; 2]) -> bool {
    x.is_finite() && y.is_finite() && x >= x_bounds[0] && x <= x_bounds[1]
}

fn render_graph_points(graph: &SvgGraph<'_>, series: &SeriesView, color: &str, out: &mut SvgOut) {
    let (rect, y_bounds, x_bounds) = (graph.plot, graph.y_bounds, graph.x_bounds);
    debug_assert!(rect.is_valid(), "the plot is valid");
    debug_assert!(is_hex_color(color), "the series color is a hex color");
    for (x, y) in &series.points {
        if !graph_point_in_range(*x, *y, x_bounds) {
            continue;
        }
        let px = map_x(*x, x_bounds, rect);
        let py = map_y(*y, y_bounds, rect);
        out.put(format_args!(
            r#"<circle data-role="graph-point" cx="{:.2}" cy="{:.2}" r="2.2" fill="{}" />"#,
            px, py, color
        ));
    }
}

fn render_graph_bars(graph: &SvgGraph<'_>, series: &SeriesView, color: &str, out: &mut SvgOut) {
    let (rect, y_bounds, x_bounds) = (graph.plot, graph.y_bounds, graph.x_bounds);
    let visible_points: Vec<_> = series
        .points
        .iter()
        .filter(|(x, y)| graph_point_in_range(*x, *y, x_bounds))
        .collect();
    if visible_points.is_empty() {
        return;
    }
    let bar_width = (rect.width / num::usize_to_f64(visible_points.len())).max(1.0) * 0.7;
    debug_assert!(
        bar_width > 0.0 && bar_width.is_finite(),
        "a bar has a finite positive width"
    );
    let baseline = graph.baseline;
    let baseline_y = map_y(baseline, y_bounds, rect);
    debug_assert!(
        !(baseline_y < rect.top || baseline_y > rect.bottom()),
        "the bar baseline is not outside the plot"
    );

    for (x, y) in visible_points {
        let px = map_x(*x, x_bounds, rect) - bar_width / 2.0;
        let py = map_y(*y, y_bounds, rect).min(baseline_y);
        let height = (baseline_y - map_y(*y, y_bounds, rect)).abs().max(1.0);
        out.put(format_args!(
            r#"<rect data-role="graph-bar" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}" />"#,
            px, py, bar_width, height, color
        ));
    }
}

fn render_graph_area(
    graph: &SvgGraph<'_>,
    series: &SeriesView,
    color: &str,
    opacity: f64,
    out: &mut SvgOut,
) {
    let (rect, y_bounds, x_bounds) = (graph.plot, graph.y_bounds, graph.x_bounds);
    let points: Vec<_> = series
        .points
        .iter()
        .filter(|(x, y)| graph_point_in_range(*x, *y, x_bounds))
        .collect();
    let [first, .., last] = points.as_slice() else {
        return;
    };

    debug_assert!(
        opacity > 0.0 && opacity <= 1.0,
        "the fill opacity is a fraction"
    );
    let baseline = graph.baseline;
    let baseline_y = map_y(baseline, y_bounds, rect);
    debug_assert!(
        !(baseline_y < rect.top || baseline_y > rect.bottom()),
        "the area baseline is not outside the plot"
    );
    let first_x = map_x(first.0, x_bounds, rect);
    let last_x = map_x(last.0, x_bounds, rect);

    let mut path = format!("M {:.2} {:.2}", first_x, baseline_y);
    for (x, y) in points {
        path.push_str(&format!(
            " L {:.2} {:.2}",
            map_x(*x, x_bounds, rect),
            map_y(*y, y_bounds, rect)
        ));
    }
    path.push_str(&format!(" L {:.2} {:.2} Z", last_x, baseline_y));

    out.put(format_args!(
        r#"<path data-role="graph-area" d="{}" fill="{}" fill-opacity="{:.2}" stroke="none" />"#,
        path, color, opacity
    ));
}

fn series_path(
    series: &SeriesView,
    x_bounds: [f64; 2],
    y_bounds: [f64; 2],
    plot: PlotRect,
) -> Option<String> {
    let mut path = SvgOut::default();
    let mut started = false;

    for &(x_value, y_value) in &series.points {
        if !x_value.is_finite()
            || !y_value.is_finite()
            || x_value < x_bounds[0]
            || x_value > x_bounds[1]
        {
            continue;
        }
        let x = map_x(x_value, x_bounds, plot);
        let y = map_y(y_value, y_bounds, plot);
        if started {
            path.put(format_args!(" L {x:.2} {y:.2}"));
        } else {
            path.put(format_args!("M {x:.2} {y:.2}"));
            started = true;
        }
    }

    started.then(|| path.finish())
}

fn write_outputs(svg: &str, dir: &Path, stem: &str, format: ExportFormat) -> Result<Vec<PathBuf>> {
    debug_assert!(!stem.is_empty(), "the file stem has a name");
    fs::create_dir_all(dir)
        .with_context(|| format!("failed to create export directory {}", dir.display()))?;
    let mut paths = Vec::new();

    if matches!(format, ExportFormat::Svg | ExportFormat::Both) {
        let path = dir.join(format!("{stem}.svg"));
        write_atomic(&path, svg.as_bytes())?;
        paths.push(path);
    }

    if matches!(format, ExportFormat::Png | ExportFormat::Both) {
        let path = dir.join(format!("{stem}.png"));
        write_png(svg, &path)?;
        paths.push(path);
    }

    debug_assert!(
        (1..=2).contains(&paths.len()),
        "each format writes one or two files"
    );
    Ok(paths)
}

/// Writes `bytes` to `path` through a temporary file in the same directory, so
/// a failed or interrupted write never leaves a truncated file at `path`.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    debug_assert!(!name.is_empty(), "the target path has a file name");
    let temporary = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    debug_assert!(
        temporary.parent() == path.parent(),
        "the temporary file is in the directory of the target"
    );
    let result = fs::write(&temporary, bytes).and_then(|()| fs::rename(&temporary, path));
    if result.is_err() {
        // The write error goes to the caller. A failed cleanup leaves only a hidden
        // temporary file, so the cleanup result does not change the report.
        let _cleanup: std::io::Result<()> = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("failed to write {}", path.display()))
}

/// System fonts for PNG rendering, loaded once; scanning them is slow and
/// recordings render a PNG for every changed frame.
fn system_fonts() -> Arc<resvg::usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    let fonts = Arc::clone(FONTS.get_or_init(|| {
        let mut fonts = resvg::usvg::fontdb::Database::new();
        fonts.load_system_fonts();
        Arc::new(fonts)
    }));
    debug_assert!(
        Arc::strong_count(&fonts) >= 2,
        "the cache keeps the font database"
    );
    fonts
}

fn write_png(svg: &str, path: &Path) -> Result<()> {
    debug_assert!(svg.starts_with("<svg"), "the input is SVG text");
    let options = resvg::usvg::Options {
        fontdb: system_fonts(),
        ..resvg::usvg::Options::default()
    };
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &options)
        .map_err(|err| anyhow!("failed to parse generated SVG: {err}"))?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .context("failed to allocate PNG pixmap")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    let png = pixmap
        .encode_png()
        .with_context(|| format!("failed to encode {}", path.display()))?;
    debug_assert!(
        png.starts_with(b"\x89PNG"),
        "the encoded data has the PNG signature"
    );
    write_atomic(path, &png)
}

fn draw_line(out: &mut SvgOut, start: (f64, f64), end: (f64, f64), style: LineStyle<'_>) {
    let (x1, y1) = start;
    let (x2, y2) = end;
    // Infinite y bounds give NaN rows, so the check lets NaN through.
    debug_assert!(
        [x1, y1, x2, y2].iter().all(|value| !value.is_infinite()),
        "the line ends are not infinite"
    );
    debug_assert!(
        style.width > 0.0 && is_hex_color(style.color),
        "the line has a width and a hex color"
    );
    out.put(format_args!(
        r#"<line x1="{x1:.2}" y1="{y1:.2}" x2="{x2:.2}" y2="{y2:.2}" stroke="{}" stroke-width="{:.2}""#,
        style.color,
        style.width
    ));
    if let Some(dash) = style.dash {
        out.put(format_args!(r#" stroke-dasharray="{dash}""#));
    }
    out.push_str("/>");
}

/// How many characters of `size` fit in `width` pixels.
fn text_columns(width: f64, size: f64) -> usize {
    debug_assert!(size > 0.0, "the font size is positive");
    let columns = if width.is_finite() && width > 0.0 {
        num::f64_to_usize((width / (size * CHAR_ADVANCE)).floor())
    } else {
        0
    };
    debug_assert!(
        columns == 0 || num::usize_to_f64(columns) * size * CHAR_ADVANCE <= width * (1.0 + 1e-9),
        "the columns fit in the width"
    );
    columns
}

/// Extents of the labels placed along one axis, so that a later label can
/// give way instead of overprinting one already there.
#[derive(Default)]
struct AxisLabels(Vec<[f64; 2]>);

impl AxisLabels {
    /// Places a label over `[start, end]`, unless it would come within
    /// `AXIS_LABEL_GAP` of a placed one. Returns whether it was placed.
    fn claim(&mut self, [start, end]: [f64; 2]) -> bool {
        debug_assert!(
            !start.is_infinite() && !end.is_infinite(),
            "the label extent is not infinite"
        );
        debug_assert!(start <= end, "the label extent is numbers in order");
        let clear = self.0.iter().all(|&[placed_start, placed_end]| {
            end + AXIS_LABEL_GAP <= placed_start || placed_end + AXIS_LABEL_GAP <= start
        });
        if clear {
            self.0.push([start, end]);
        }
        clear
    }
}

/// Estimated width of `text` at `size`, in pixels.
fn text_width(text: &str, size: f64) -> f64 {
    num::usize_to_f64(text.chars().count()) * size * CHAR_ADVANCE
}

/// Fits `text` in `width` pixels: at `size`, or as small as
/// `SMALL_FONT_SIZE`, then cut short with `…`. Returns the text and its size.
fn fit_text(text: &str, width: f64, size: f64) -> (String, f64) {
    debug_assert!(
        size > 0.0 && size.is_finite(),
        "the font size is finite and positive"
    );
    let fitted = (size * width / text_width(text, size).max(size * CHAR_ADVANCE)).min(size);
    if fitted >= SMALL_FONT_SIZE.min(size) {
        debug_assert!(fitted <= size, "a fitted size is not larger than the size");
        return (text.to_string(), fitted);
    }
    let size = SMALL_FONT_SIZE.min(size);
    let columns = u16::try_from(text_columns(width, size)).unwrap_or(u16::MAX);
    (ui::truncate_title(text, columns), size)
}

/// Wraps `text` at spaces into at most `max_lines` lines of `columns`
/// characters, keeping its line breaks and breaking words longer than a line.
/// Text that does not fit ends the last line with `…`.
fn wrap_text(text: &str, columns: usize, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    if columns == 0 || max_lines == 0 {
        return lines;
    }
    for paragraph in text.lines() {
        let mut line = String::new();
        let mut width = 0;
        for word in paragraph.split_whitespace() {
            if width > 0 && width + 1 + word.chars().count() > columns {
                lines.push(std::mem::take(&mut line));
                width = 0;
            }
            if width > 0 {
                line.push(' ');
                width += 1;
            }
            for ch in word.chars() {
                if width == columns {
                    lines.push(std::mem::take(&mut line));
                    width = 0;
                }
                line.push(ch);
                width += 1;
            }
        }
        lines.push(line);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            // Keep `columns - 1` characters, so the `…` ends the line.
            if let Some((cut, _)) = last.char_indices().nth(columns.saturating_sub(1)) {
                last.truncate(cut);
            }
            last.push('…');
        }
    }
    debug_assert!(lines.len() <= max_lines, "the text obeys the line limit");
    debug_assert!(
        lines.iter().all(|line| line.chars().count() <= columns),
        "each line obeys the column limit"
    );
    lines
}

fn write_text(out: &mut SvgOut, x: f64, y: f64, text: &str, color: &str, anchor: &str, size: f64) {
    debug_assert!(
        !x.is_infinite() && !y.is_infinite() && size > 0.0,
        "the text has a position that is not infinite and a positive size"
    );
    debug_assert!(
        is_hex_color(color) && is_text_anchor(anchor),
        "the text has a hex color and a text anchor"
    );
    out.put(format_args!(
        r#"<text x="{x:.2}" y="{y:.2}" fill="{color}" font-size="{size:.1}" text-anchor="{anchor}">{}</text>"#,
        escape_xml(text)
    ));
}

/// The color, anchor, size and weight of one SVG text element.
struct TextStyle<'a> {
    color: &'a str,
    anchor: &'a str,
    size: f64,
    bold: bool,
}

/// Writes one text element at `(x, y)` in the style `style`.
fn write_styled_text(out: &mut SvgOut, x: f64, y: f64, text: &str, style: TextStyle<'_>) {
    let TextStyle {
        color,
        anchor,
        size,
        bold,
    } = style;
    debug_assert!(
        !x.is_infinite() && !y.is_infinite() && size > 0.0,
        "the text has a position that is not infinite and a positive size"
    );
    debug_assert!(
        is_hex_color(color) && is_text_anchor(anchor),
        "the text has a hex color and a text anchor"
    );
    let weight = if bold { r#" font-weight="bold""# } else { "" };
    out.put(format_args!(
        r#"<text x="{x:.2}" y="{y:.2}" fill="{color}" font-size="{size:.1}" text-anchor="{anchor}"{weight}>{}</text>"#,
        escape_xml(text)
    ));
}

fn write_rect(out: &mut SvgOut, rect: PlotRect, fill: &str, stroke: &str, stroke_width: f64) {
    debug_assert!(
        rect.left.is_finite() && rect.top.is_finite(),
        "the rectangle has a finite corner"
    );
    debug_assert!(
        rect.width >= 0.0 && rect.height >= 0.0,
        "the rectangle has a size that is a number and not negative"
    );
    debug_assert!(
        [fill, stroke]
            .iter()
            .all(|color| *color == "none" || is_hex_color(color)),
        "the fill and the stroke are hex colors or none"
    );
    out.put(format_args!(
        r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width:.2}"/>"#,
        rect.left, rect.top, rect.width, rect.height
    ));
}

fn scaled_rect(rect: Rect) -> PlotRect {
    let scaled = PlotRect {
        left: f64::from(rect.x) * CELL_WIDTH,
        top: f64::from(rect.y) * CELL_HEIGHT,
        width: f64::from(rect.width) * CELL_WIDTH,
        height: f64::from(rect.height) * CELL_HEIGHT,
    };
    debug_assert!(scaled.is_valid(), "the scaled rectangle is valid");
    debug_assert!(
        scaled.left >= 0.0 && scaled.top >= 0.0,
        "the scaled rectangle is not left of or above the origin"
    );
    scaled
}

/// The place of `value` between `bounds`, from 0 to 1. The function subtracts
/// halves, so values near `f64::MAX` do not overflow. NaN gives 0.
fn bounds_fraction(value: f64, [low, high]: [f64; 2]) -> f64 {
    let span = (high / 2.0 - low / 2.0).max(f64::EPSILON / 2.0);
    let fraction = ((value / 2.0 - low / 2.0) / span).clamp(0.0, 1.0);
    let fraction = if fraction.is_nan() { 0.0 } else { fraction };
    debug_assert!(
        (0.0..=1.0).contains(&fraction),
        "the fraction is a number from 0 to 1"
    );
    fraction
}

fn map_x(value: f64, bounds: [f64; 2], plot: PlotRect) -> f64 {
    plot.left + bounds_fraction(value, bounds) * plot.width
}

fn map_y(value: f64, bounds: [f64; 2], plot: PlotRect) -> f64 {
    plot.bottom() - bounds_fraction(value, bounds) * plot.height
}

fn color_hex(color: Color, reset: &str) -> String {
    let hex = match color {
        Color::Reset => reset.to_string(),
        Color::Black => "#000000".to_string(),
        Color::Red => "#cc3333".to_string(),
        Color::Green => "#33cc66".to_string(),
        Color::Yellow => "#d6c343".to_string(),
        Color::Blue => "#4f83ff".to_string(),
        Color::Magenta => "#cc66cc".to_string(),
        Color::Cyan => "#33c8cc".to_string(),
        Color::Gray => "#a0a0a0".to_string(),
        Color::DarkGray => "#666666".to_string(),
        Color::LightRed => "#ff6666".to_string(),
        Color::LightGreen => "#66ff99".to_string(),
        Color::LightYellow => "#fff06a".to_string(),
        Color::LightBlue => "#7aa2ff".to_string(),
        Color::LightMagenta => "#ff8cff".to_string(),
        Color::LightCyan => "#66ffff".to_string(),
        Color::White => "#f5f5f5".to_string(),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Indexed(value) => indexed_color_hex(value).to_string(),
    };
    debug_assert!(
        color == Color::Reset || is_hex_color(&hex),
        "a terminal color gives a hex color"
    );
    hex
}

fn indexed_color_hex(value: u8) -> &'static str {
    const ANSI: [&str; 16] = [
        "#000000", "#cc3333", "#33cc66", "#d6c343", "#4f83ff", "#cc66cc", "#33c8cc", "#d0d0d0",
        "#666666", "#ff6666", "#66ff99", "#fff06a", "#7aa2ff", "#ff8cff", "#66ffff", "#f5f5f5",
    ];
    let hex = ANSI.get(usize::from(value)).copied().unwrap_or("#a0a0a0");
    debug_assert!(is_hex_color(hex), "the indexed color is a hex color");
    debug_assert!(
        usize::from(value) >= ANSI.len() || hex != "#a0a0a0",
        "each basic color index has its own color"
    );
    hex
}

fn escape_xml(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            character if is_valid_xml_1_0_character(character) => escaped.push(character),
            _ => escaped.push(char::REPLACEMENT_CHARACTER),
        }
    }
    debug_assert!(
        !escaped.contains(['<', '>', '"', '\'']),
        "the escaped text has no markup characters"
    );
    debug_assert!(
        escaped.len() >= input.len(),
        "the escaped text is not shorter than the input"
    );
    escaped
}

fn is_valid_xml_1_0_character(character: char) -> bool {
    matches!(
        character,
        '\u{9}'
            | '\u{a}'
            | '\u{d}'
            | '\u{20}'..='\u{d7ff}'
            | '\u{e000}'..='\u{fffd}'
            | '\u{10000}'..='\u{10ffff}'
    )
}

fn timestamp_id() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S-%9f").to_string()
}

fn timestamp_rfc3339() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn display_paths(paths: &[PathBuf]) -> String {
    let text = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    debug_assert!(
        paths.is_empty() || !text.is_empty(),
        "a list of paths gives text"
    );
    debug_assert!(
        text.matches(", ").count() >= paths.len().saturating_sub(1),
        "the paths have a separator between each pair"
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        GraphAxisPlacement, GraphDrawStyle, GraphOptions, GraphPointMode, GraphStackingMode,
        PanelOptions, PanelState, SeriesView, YAxisMode,
    };
    use crate::dashboard::{
        DashboardItemId, DashboardLayout, DashboardLayoutItem, DashboardRow, DashboardTab,
        DashboardTabs, RowId, TabGroupId,
    };

    fn test_panel(start: f64) -> PanelState {
        PanelState {
            title: "CPU <main>".to_string(),
            exprs: vec![],
            legends: vec![],
            query_modes: vec![],
            series: vec![SeriesView {
                name: "usage & total".to_string(),
                value: Some(10.0),
                points: vec![(start, 0.0), (start + 50.0, 50.0), (start + 100.0, 100.0)],
                visible: true,
            }],
            last_error: None,
            last_url: None,
            last_samples: 3,
            grid: None,
            y_axis_mode: YAxisMode::Auto,
            panel_type: PanelType::Graph,
            thresholds: None,
            min: None,
            max: None,
            autogrid: None,
            display: crate::ui::DisplayFormat::default(),
            options: PanelOptions::Graph(GraphOptions::default()),
            resolution: Default::default(),
            notices: Default::default(),
        }
    }

    fn test_export_dir(name: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "grafana-tui-{name}-{}-{suffix}",
            std::process::id()
        ))
    }

    fn test_app(export: ExportOptions) -> AppState {
        let prom = crate::prom::PromClient::new("http://localhost:9090".to_string());
        let now = chrono::Utc::now().timestamp() as f64;
        let range = std::time::Duration::from_secs(100);
        AppState::new(
            prom,
            range,
            std::time::Duration::from_secs(10),
            std::time::Duration::from_secs(1),
            "Dash & Main".to_string(),
            vec![test_panel(now - range.as_secs_f64())],
            0,
            Theme::default(),
            "dashed-line".to_string(),
            export,
        )
    }

    #[test]
    fn atomic_writes_replace_files_and_leave_nothing_behind_on_failure() {
        let dir = std::env::temp_dir().join(format!(
            "grafana-tui-atomic-{}-{}",
            std::process::id(),
            timestamp_id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("frame.svg");

        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");

        // Renaming onto a directory fails; the temporary file must not linger.
        let occupied = dir.join("occupied");
        fs::create_dir_all(occupied.join("inner")).unwrap();
        assert!(write_atomic(&occupied, b"data").is_err());

        let mut names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["frame.svg", "occupied"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn system_fonts_are_loaded_once() {
        assert!(Arc::ptr_eq(&system_fonts(), &system_fonts()));
    }

    #[tokio::test]
    async fn svg_exports_show_the_pinned_dashboards_bar_and_active_title() {
        use crate::app::dashboard_test_support::{dashboards_app, v2_dashboard};
        let nodes = v2_dashboard("Ops / Nodes", "30s", "up", "label_values(up, instance)");
        let consul = v2_dashboard("Ops / Consul", "30s", "up", "label_values(up, instance)");
        let mut app = dashboards_app("http://127.0.0.1:9", &[&nodes, &consul]);
        app.cycle_dashboard(1).unwrap();
        app.vertical_scroll = 3;

        let svg = render_svg(&app, Rect::new(0, 0, 120, 40));

        assert!(svg.contains("Ops / Consul (imported)"), "{svg}");
        assert!(
            svg.contains(">Nodes<") && svg.contains(">* Consul<"),
            "{svg}"
        );
    }

    #[test]
    fn svg_tab_switch_changes_frame_and_escapes_active_title() {
        let recording_dir = test_export_dir("tabs-recording");
        let mut app = test_app(ExportOptions {
            dir: recording_dir.clone(),
            format: ExportFormat::Svg,
            record_max_frames: 10,
        });
        app.view_end_ts = 1_783_080_000;
        let id = TabGroupId::new(0);
        app.apply_layout(DashboardLayout::new(vec![DashboardLayoutItem::Tabs(
            DashboardTabs::new(
                id,
                vec![
                    DashboardTab {
                        title: "CPU & load".into(),
                        children: vec![],
                    },
                    DashboardTab {
                        title: "Memory <rss>".into(),
                        children: vec![],
                    },
                ],
            ),
        )]));
        let viewport = Rect::new(0, 0, 100, 40);

        let before = render_svg(&app, viewport);
        start_recording(&mut app, viewport).unwrap();
        assert_eq!(app.recording.as_ref().unwrap().frame_count, 1);
        app.layout.set_active_tab(id, 1).unwrap();
        let after = render_svg(&app, viewport);
        capture_recording_frame(&mut app, viewport).unwrap();
        assert_eq!(app.recording.as_ref().unwrap().frame_count, 2);

        assert_ne!(before, after);
        assert!(before.contains("CPU &amp; load"));
        assert!(after.contains("Memory &lt;rss&gt;"));
        assert!(after.contains("No supported panels in this tab"));
        app.layout.set_active_tab(id, 1).unwrap();
        assert_eq!(render_svg(&app, viewport), after);
        capture_recording_frame(&mut app, viewport).unwrap();
        assert_eq!(app.recording.as_ref().unwrap().frame_count, 2);
        if let Ok(root) = std::env::var("GRAFANA_TUI_TABS_CAPTURE_DIR") {
            let directory = std::path::PathBuf::from(root).join("tabs-export");
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("tabs-export-100x40.svg"), &after).unwrap();
            write_png(&after, &directory.join("tabs-export-100x40.png")).unwrap();
        }
        std::fs::remove_dir_all(recording_dir).unwrap();
    }

    fn test_app_with_panel_type(panel_type: PanelType) -> AppState {
        let mut app = test_app(ExportOptions::default());
        app.panels[0].panel_type = panel_type;
        app
    }

    fn nested_row_export_app() -> AppState {
        let mut app = test_app(ExportOptions {
            dir: test_export_dir("nested-row"),
            format: ExportFormat::Svg,
            record_max_frames: 10,
        });
        app.panels.push(test_panel(app.view_end_ts as f64 - 100.0));
        app.panels[0].title = "Visible child".to_string();
        app.panels[1].title = "Collapsed child".to_string();
        app.layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(0),
            "Expanded",
            false,
            false,
            vec![
                DashboardLayoutItem::Panel(0),
                DashboardLayoutItem::Row(DashboardRow::new(
                    RowId::new(1),
                    "Nested",
                    true,
                    false,
                    vec![DashboardLayoutItem::Panel(1)],
                )),
            ],
        ))]);
        app.selected_item = Some(DashboardItemId::Row(RowId::new(0)));
        app
    }

    fn targeted(
        timestamp: f64,
        text: &str,
        tags: &[&str],
        titles: &[&str],
    ) -> crate::annotations::AnnotationEvent {
        crate::annotations::AnnotationEvent {
            time: chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
                (timestamp * 1_000.0).round() as i64,
            )
            .unwrap(),
            text: text.to_string(),
            tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
            target: crate::annotations::AnnotationTarget::PanelTitles(
                titles.iter().map(|title| (*title).to_string()).collect(),
            ),
        }
    }

    fn global(timestamp: f64, text: &str, tags: &[&str]) -> crate::annotations::AnnotationEvent {
        let mut event = crate::annotations::test_event_at(timestamp, text);
        event.tags = tags.iter().map(|tag| (*tag).to_string()).collect();
        event
    }

    fn panel_svg_section<'a>(svg: &'a str, title: &str, next_title: Option<&str>) -> &'a str {
        let start = svg
            .find(&format!(">{title}</text>"))
            .unwrap_or_else(|| panic!("missing panel title {title}"));
        let section = &svg[start..];
        match next_title {
            Some(next_title) => {
                &section[..section
                    .find(&format!(">{next_title}</text>"))
                    .unwrap_or_else(|| panic!("missing next panel title {next_title}"))]
            }
            None => section,
        }
    }

    fn assert_panel_title_offset(app: &AppState, viewport: Rect, index: usize) {
        let (rect, _) = ui::visible_panel_rects(viewport, app)
            .into_iter()
            .find(|(_, panel_index)| *panel_index == index)
            .unwrap_or_else(|| panic!("panel {index} is not visible"));
        let rect = scaled_rect(rect);
        let title = &app.panels[index].title;
        let svg = render_svg(app, viewport);
        let title_element = format!(
            r#"<text x="{:.2}" y="{:.2}" fill="{}" font-size="{FONT_SIZE:.1}" text-anchor="start">{}</text>"#,
            rect.left + 8.0,
            rect.top + 18.0,
            color_hex(app.theme.title, "#00c8ff"),
            escape_xml(title),
        );
        assert!(svg.contains(&title_element));
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("<a&b\"c'>"), "&lt;a&amp;b&quot;c&apos;&gt;");
    }

    #[test]
    fn test_escape_xml_replaces_invalid_xml_1_0_characters() {
        assert_eq!(
            escape_xml(
                "\u{0}\u{1}\t\n\r \u{d7ff}\u{e000}\u{fffd}\u{fffe}\u{ffff}\u{10000}\u{10ffff}&"
            ),
            "��\t\n\r \u{d7ff}\u{e000}\u{fffd}��\u{10000}\u{10ffff}&amp;"
        );
    }

    #[test]
    fn test_map_coordinates_respect_bounds() {
        let plot = PlotRect {
            left: 10.0,
            top: 20.0,
            width: 100.0,
            height: 50.0,
        };
        assert_eq!(map_x(50.0, [0.0, 100.0], plot), 60.0);
        assert_eq!(map_y(50.0, [0.0, 100.0], plot), 45.0);
    }

    #[test]
    fn test_value_ticks_are_interior() {
        let ticks = ui::svg_value_ticks(329.0, 1287.0);
        assert!(ticks.contains(&500.0));
        assert!(ticks.contains(&1000.0));
        assert!(!ticks.contains(&329.0));
        assert!(!ticks.contains(&1287.0));
    }

    #[test]
    fn test_time_ticks_choose_expected_boundaries() {
        let two_hours =
            ui::svg_time_ticks(11.0 * 3600.0 + 22.0 * 60.0, 13.0 * 3600.0 + 22.0 * 60.0);
        assert_eq!(two_hours, vec![12.0 * 3600.0, 13.0 * 3600.0]);

        let one_hour = ui::svg_time_ticks(12.0 * 3600.0 + 22.0 * 60.0, 13.0 * 3600.0 + 22.0 * 60.0);
        assert_eq!(one_hour, vec![12.5 * 3600.0, 13.0 * 3600.0]);

        let five_minutes =
            ui::svg_time_ticks(12.0 * 3600.0 + 22.0 * 60.0, 12.0 * 3600.0 + 27.0 * 60.0);
        assert_eq!(
            five_minutes,
            vec![
                12.0 * 3600.0 + 23.0 * 60.0,
                12.0 * 3600.0 + 24.0 * 60.0,
                12.0 * 3600.0 + 25.0 * 60.0,
                12.0 * 3600.0 + 26.0 * 60.0
            ]
        );
    }

    #[test]
    fn test_svg_contains_escaped_text_and_axes() {
        let app = test_app(ExportOptions::default());

        let svg = render_svg(&app, Rect::new(0, 0, 100, 40));
        assert!(svg.starts_with("<svg "));
        assert!(svg.contains("Dash &amp; Main"));
        assert!(svg.contains("CPU &lt;main&gt;"));
        assert!(svg.contains("<line "));
        assert!(svg.contains("<path "));
    }

    #[test]
    fn svg_renders_rows_and_omits_collapsed_descendants() {
        let app = nested_row_export_app();

        let svg = render_svg(&app, Rect::new(0, 0, 100, 40));

        assert!(svg.contains("▼ Expanded"));
        assert!(svg.contains("▶   Nested"));
        assert!(svg.contains("Visible child"));
        assert!(!svg.contains("Collapsed child"));
    }

    #[test]
    fn svg_selected_row_uses_theme_style_and_escapes_title() {
        let mut app = nested_row_export_app();
        app.layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(0),
            "Expanded <&>",
            false,
            false,
            vec![DashboardLayoutItem::Panel(0)],
        ))]);
        app.theme = crate::theme::builtin("terminal").unwrap();

        let svg = render_svg(&app, Rect::new(0, 0, 100, 40));

        assert!(svg.contains("▼ Expanded &lt;&amp;&gt;"));
        assert!(svg.contains(r##"stroke="#d6c343" stroke-width="2.00""##));
        assert!(svg.contains(r#"font-weight="bold""#));
        assert!(svg.contains("panels=1/2"));
    }

    #[test]
    fn svg_keeps_hidden_header_rows_transparent() {
        let mut app = nested_row_export_app();
        app.layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(0),
            "Hidden row",
            true,
            true,
            vec![DashboardLayoutItem::Panel(0)],
        ))]);

        let svg = render_svg(&app, Rect::new(0, 0, 100, 40));

        assert!(svg.contains("Visible child"));
        assert!(!svg.contains("Hidden row"));
        assert!(svg.contains("panels=1/2"));
    }

    #[test]
    fn svg_renders_auto_grid_panels() {
        let mut app = nested_row_export_app();
        app.layout = DashboardLayout::new(vec![DashboardLayoutItem::AutoGrid(
            crate::dashboard::DashboardAutoGrid {
                panels: vec![0, 1],
                max_columns: 3,
                min_column_width: 10,
                row_height: 2,
            },
        )]);

        let svg = render_svg(&app, Rect::new(0, 0, 100, 40));

        assert!(svg.contains("Visible child"));
        assert!(svg.contains("Collapsed child"));
        assert!(svg.contains("panels=2"));
    }

    #[test]
    fn toggling_a_header_only_row_changes_the_recorded_frame() {
        let mut app = test_app(ExportOptions {
            dir: test_export_dir("header-only-row-recording"),
            format: ExportFormat::Svg,
            record_max_frames: 10,
        });
        app.layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(0),
            "Header only",
            false,
            false,
            vec![],
        ))]);
        app.selected_item = Some(DashboardItemId::Row(RowId::new(0)));
        start_recording(&mut app, Rect::new(0, 0, 100, 40)).unwrap();
        let expanded_svg = app.recording.as_ref().unwrap().last_svg.clone().unwrap();

        app.layout.set_row_collapsed(RowId::new(0), true).unwrap();
        capture_recording_frame(&mut app, Rect::new(0, 0, 100, 40)).unwrap();

        let recording = app.recording.as_ref().unwrap();
        assert!(expanded_svg.contains("▼ Header only"));
        assert!(
            recording
                .last_svg
                .as_deref()
                .unwrap()
                .contains("▶ Header only")
        );
        assert_ne!(recording.last_svg.as_deref().unwrap(), expanded_svg);
        assert_eq!(recording.frame_count, 2);
    }

    #[test]
    fn test_graph_export_uses_refreshed_time_bounds() {
        let mut app = test_app(ExportOptions::default());
        app.view_end_ts = 1_700_000_000;
        app.range = std::time::Duration::from_secs(100);
        app.panels[0].series[0].points = vec![
            (1_699_999_900.0, 0.0),
            (1_699_999_950.0, 50.0),
            (1_700_000_000.0, 100.0),
        ];

        let svg = render_svg(&app, Rect::new(0, 0, 100, 40));

        assert!(svg.contains(&ui::format_time(1_699_999_900.0)));
        assert!(svg.contains(&ui::format_time(1_700_000_000.0)));
    }

    #[test]
    fn annotation_target_and_filter_are_shared_by_svg() {
        let viewport = Rect::new(0, 0, 160, 50);
        let mut app = test_app(ExportOptions::default());
        app.panels.push(test_panel(app.view_end_ts as f64 - 100.0));
        app.panels[0].title = "CPU".to_string();
        app.panels[1].title = "Memory".to_string();
        app.view_end_ts = 100;
        app.range = std::time::Duration::from_secs(100);
        app.cursor_x = Some(50.0);
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            targeted(50.0, "cpu deploy", &["deploy"], &["CPU"]),
            targeted(50.0, "memory incident", &["incident"], &["Memory"]),
            global(50.0, "global deploy", &["deploy"]),
        ]);
        app.annotations
            .set_filter(crate::annotations::TagFilter::from_selected([
                "deploy".to_string()
            ]));

        assert_panel_title_offset(&app, viewport, 0);
        assert_panel_title_offset(&app, viewport, 1);
        let svg = render_svg(&app, viewport);
        let cpu = panel_svg_section(&svg, "CPU", Some("Memory"));
        let memory = panel_svg_section(&svg, "Memory", None);

        assert_eq!(cpu.matches(r#"data-role="annotation-marker""#).count(), 1);
        assert!(cpu.contains(r#"data-role="annotation-count">2</text>"#));
        assert!(cpu.contains("cpu deploy"));
        assert!(cpu.contains("global deploy"));
        assert!(!cpu.contains("memory incident"));

        assert_eq!(
            memory.matches(r#"data-role="annotation-marker""#).count(),
            1
        );
        assert!(memory.contains(r#"data-role="annotation-count">•</text>"#));
        assert!(memory.contains("global deploy"));
        assert!(!memory.contains("cpu deploy"));
        assert!(!memory.contains("memory incident"));
    }

    #[test]
    fn annotation_modals_are_omitted_and_latest_snapshot_is_exported() {
        let viewport = Rect::new(0, 0, 120, 50);
        let mut app = test_app(ExportOptions::default());
        app.view_end_ts = 100;
        app.range = std::time::Duration::from_secs(100);
        app.cursor_x = Some(50.0);
        app.rendered_annotation_cluster = Some(vec![global(50.0, "old modal event", &[])]);
        app.open_rendered_annotation_cluster();
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![global(
            50.0,
            "latest snapshot event",
            &["draft-only-tag"],
        )]);

        let svg = render_svg(&app, viewport);
        assert!(!svg.contains("old modal event"));
        assert!(svg.contains("latest snapshot event"));
        assert!(!svg.contains("annotations modal"));

        app.open_tag_filter_modal();
        let Some(crate::annotations::AnnotationModal::TagFilter(modal)) =
            app.annotation_modal.as_mut()
        else {
            panic!("tag filter modal should open");
        };
        modal.toggle_selected();

        app.cursor_x = None;
        let svg = render_svg(&app, viewport);
        assert!(!svg.contains("Filter annotations by tag"));
        assert!(!svg.contains("draft-only-tag"));
    }

    #[test]
    fn recording_changes_only_after_filter_apply_or_clear() {
        let dir = test_export_dir("annotation-filter-recording");
        let viewport = Rect::new(0, 0, 120, 50);
        let export = ExportOptions {
            dir: dir.clone(),
            format: ExportFormat::Svg,
            record_max_frames: 10,
        };
        let mut app = test_app(export);
        app.view_end_ts = 100;
        app.range = std::time::Duration::from_secs(100);
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            global(50.0, "deploy event", &["deploy"]),
            global(75.0, "incident event", &["incident"]),
        ]);

        toggle_recording(&mut app, viewport).unwrap();
        assert_eq!(app.recording.as_ref().unwrap().frame_count, 1);

        app.open_tag_filter_modal();
        let draft = match app.annotation_modal.as_mut() {
            Some(crate::annotations::AnnotationModal::TagFilter(modal)) => {
                modal.toggle_selected();
                modal.draft().clone()
            }
            _ => panic!("tag filter modal should open"),
        };
        capture_recording_frame(&mut app, viewport).unwrap();
        assert_eq!(app.recording.as_ref().unwrap().frame_count, 1);

        app.annotations.set_filter(draft);
        capture_recording_frame(&mut app, viewport).unwrap();
        assert_eq!(app.recording.as_ref().unwrap().frame_count, 2);

        app.annotations
            .set_filter(crate::annotations::TagFilter::default());
        capture_recording_frame(&mut app, viewport).unwrap();
        assert_eq!(app.recording.as_ref().unwrap().frame_count, 3);

        toggle_recording(&mut app, viewport).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn graph_export_renders_visible_annotations_and_exact_cluster_count() {
        let mut app = test_app_with_panel_type(PanelType::Graph);
        app.view_end_ts = 100;
        app.range = std::time::Duration::from_secs(100);
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(50.0, "deploy"),
            crate::annotations::test_event_at(50.01, "rollback"),
            crate::annotations::test_event_at(50.02, "resolved"),
        ]);
        app.cursor_x = Some(50.0);

        let svg = render_svg(&app, Rect::new(0, 0, 120, 50));

        assert_eq!(svg.matches(r#"data-role="annotation-marker""#).count(), 1);
        assert!(svg.contains(r#"data-role="annotation-count">3</text>"#));
        assert_eq!(svg.matches(r#"data-role="annotation-detail""#).count(), 2);
        assert!(svg.contains("3 events near 1970-01-01 00:00:50 UTC"));
        assert!(svg.contains("deploy · rollback · resolved"));
    }

    #[test]
    fn graph_export_bounds_large_annotation_details_with_ellipsis() {
        let mut app = test_app_with_panel_type(PanelType::Graph);
        app.view_end_ts = 100;
        app.range = std::time::Duration::from_secs(100);
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(
            (0..100)
                .map(|index| {
                    crate::annotations::test_event_at(
                        50.0,
                        &format!("event-{index:03}-{}", "x".repeat(200)),
                    )
                })
                .collect(),
        );
        app.cursor_x = Some(50.0);

        let svg = render_svg(&app, Rect::new(0, 0, 120, 50));
        let detail_lines = svg
            .split(r#"data-role="annotation-detail""#)
            .skip(1)
            .map(|suffix| {
                suffix
                    .split_once('>')
                    .unwrap()
                    .1
                    .split_once("</text>")
                    .unwrap()
                    .0
            })
            .collect::<Vec<_>>();

        assert_eq!(detail_lines.len(), 2);
        assert!(detail_lines[0].starts_with("100 events near"));
        assert!(detail_lines[1].starts_with("event-000-"));
        assert!(detail_lines[1].ends_with('…'));
        assert!(!detail_lines[1].contains("event-001-"));
        assert!(detail_lines.iter().all(|line| line.chars().count() <= 120));
    }

    #[test]
    fn export_uses_theme_roles_and_the_grid_override() {
        let theme = crate::theme::builtin("solarized-light").unwrap();
        let hex = |color| color_hex(color, "");

        let mut graph = test_app_with_panel_type(PanelType::Graph);
        graph.theme = theme.clone();
        graph.cursor_x = Some(graph.view_end_ts as f64 - 50.0);
        let svg = render_svg(&graph, Rect::new(0, 0, 120, 40));
        assert!(svg.contains(&format!(r#"fill="{}""#, hex(theme.background))));
        assert!(svg.contains(&format!(r#"stroke="{}""#, hex(theme.axis))));
        assert!(svg.contains(&format!(r#"stroke="{}""#, hex(theme.grid))));
        assert!(svg.contains(&format!(r#"stroke="{}""#, hex(theme.cursor))));
        assert!(!svg.contains("#6d6d6d"));

        graph.autogrid_color = Some(Color::Rgb(1, 2, 3));
        let svg = render_svg(&graph, Rect::new(0, 0, 120, 40));
        assert!(svg.contains(r##"stroke="#010203""##));

        graph.autogrid_enabled = false;
        let svg = render_svg(&graph, Rect::new(0, 0, 120, 40));
        assert!(!svg.contains(r##"stroke="#010203""##));

        let mut gauge = test_app_with_panel_type(PanelType::Gauge);
        gauge.theme = theme.clone();
        let svg = render_svg(&gauge, Rect::new(0, 0, 120, 40));
        assert!(svg.contains(&format!(r#"fill="{}""#, hex(theme.gauge_track))));
    }

    #[test]
    fn graph_export_routes_inclusive_fractional_annotations_and_escapes_active_details() {
        let mut app = test_app_with_panel_type(PanelType::Graph);
        app.view_end_ts = 100;
        app.range = std::time::Duration::from_secs(100);
        app.theme.annotation = Color::Rgb(1, 2, 3);
        let active_time = chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:50.123456789Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let active_timestamp = active_time.timestamp() as f64
            + f64::from(active_time.timestamp_subsec_nanos()) / 1_000_000_000.0;
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(0.0, "start"),
            crate::annotations::AnnotationEvent {
                time: active_time,
                text: "deploy <prod> & \"main\"".to_string(),
                tags: vec!["api&edge".to_string(), "<blue>".to_string()],
                target: crate::annotations::AnnotationTarget::All,
            },
            crate::annotations::test_event_at(100.0, "end"),
        ]);
        app.cursor_x = Some(active_timestamp);

        let svg = render_svg(&app, Rect::new(0, 0, 120, 50));

        assert_eq!(svg.matches(r#"data-role="annotation-marker""#).count(), 3);
        let active_marker = svg
            .split(r#"data-role="annotation-marker""#)
            .nth(2)
            .unwrap()
            .split("/>")
            .next()
            .unwrap();
        assert!(active_marker.contains(r##"stroke="#010203""##));
        assert!(svg.contains("1970-01-01 00:00:50.123 UTC"));
        assert!(!svg.contains("1970-01-01 00:00:50.123456789 UTC"));
        assert!(
            svg.contains("deploy &lt;prod&gt; &amp; &quot;main&quot; [api&amp;edge, &lt;blue&gt;]")
        );
        assert!(!svg.contains("usage &amp; total ("));
    }

    #[test]
    fn graph_export_omits_hidden_annotations_and_non_graph_markers() {
        let mut hidden = test_app_with_panel_type(PanelType::Graph);
        hidden.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(hidden.view_end_ts as f64, "deploy"),
        ]);
        hidden.annotations.toggle_visibility();
        assert!(
            !render_svg(&hidden, Rect::new(0, 0, 120, 50))
                .contains(r#"data-role="annotation-marker""#)
        );

        let mut stat = test_app_with_panel_type(PanelType::Stat);
        stat.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(stat.view_end_ts as f64, "deploy"),
        ]);
        assert!(
            !render_svg(&stat, Rect::new(0, 0, 120, 50))
                .contains(r#"data-role="annotation-marker""#)
        );

        let mut unknown = test_app_with_panel_type(PanelType::Unknown);
        unknown.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(unknown.view_end_ts as f64, "deploy"),
        ]);
        assert!(
            !render_svg(&unknown, Rect::new(0, 0, 120, 50))
                .contains(r#"data-role="annotation-marker""#)
        );
    }

    #[test]
    fn graph_export_orders_annotations_after_area_and_before_primary_data() {
        let mut app = test_app_with_panel_type(PanelType::Graph);
        app.panels[0].options = PanelOptions::Graph(GraphOptions {
            draw_style: GraphDrawStyle::Line,
            show_points: GraphPointMode::Always,
            fill_opacity: Some(30),
            axis_placement: GraphAxisPlacement::Visible,
            line_interpolation: None,
            stacking: GraphStackingMode::Off,
        });
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(app.view_end_ts as f64, "deploy"),
        ]);

        let svg = render_svg(&app, Rect::new(0, 0, 120, 50));
        let area = svg.find(r#"data-role="graph-area""#).unwrap();
        let annotation = svg.find(r#"data-role="annotation-marker""#).unwrap();
        let line = svg.find(r#"data-role="graph-line""#).unwrap();
        let point = svg.find(r#"data-role="graph-point""#).unwrap();

        assert!(area < annotation);
        assert!(annotation < line);
        assert!(annotation < point);
    }

    #[test]
    fn test_graph_export_renders_points_area_and_bars() {
        let mut points_app = test_app_with_panel_type(PanelType::Graph);
        points_app.panels[0].options = PanelOptions::Graph(GraphOptions {
            draw_style: GraphDrawStyle::Points,
            show_points: GraphPointMode::Auto,
            fill_opacity: None,
            axis_placement: GraphAxisPlacement::Visible,
            line_interpolation: None,
            stacking: GraphStackingMode::Off,
        });
        let points_svg = render_svg(&points_app, Rect::new(0, 0, 120, 50));
        assert!(points_svg.contains(r#"data-role="graph-point""#));

        let mut area_app = test_app_with_panel_type(PanelType::Graph);
        area_app.panels[0].options = PanelOptions::Graph(GraphOptions {
            draw_style: GraphDrawStyle::Line,
            show_points: GraphPointMode::Never,
            fill_opacity: Some(30),
            axis_placement: GraphAxisPlacement::Visible,
            line_interpolation: None,
            stacking: GraphStackingMode::Off,
        });
        let area_svg = render_svg(&area_app, Rect::new(0, 0, 120, 50));
        assert!(area_svg.contains(r#"data-role="graph-area""#));
        assert!(area_svg.contains("fill-opacity=\"0.30\""));

        let mut bars_app = test_app_with_panel_type(PanelType::Graph);
        bars_app.panels[0].options = PanelOptions::Graph(GraphOptions {
            draw_style: GraphDrawStyle::Bars,
            show_points: GraphPointMode::Auto,
            fill_opacity: None,
            axis_placement: GraphAxisPlacement::Visible,
            line_interpolation: None,
            stacking: GraphStackingMode::Off,
        });
        let bars_svg = render_svg(&bars_app, Rect::new(0, 0, 120, 50));
        assert!(bars_svg.contains(r#"data-role="graph-bar""#));
    }

    #[test]
    fn test_graph_export_skips_non_finite_style_points() {
        let mut points_app = test_app_with_panel_type(PanelType::Graph);
        points_app.panels[0].options = PanelOptions::Graph(GraphOptions {
            draw_style: GraphDrawStyle::Points,
            show_points: GraphPointMode::Auto,
            fill_opacity: None,
            axis_placement: GraphAxisPlacement::Visible,
            line_interpolation: None,
            stacking: GraphStackingMode::Off,
        });
        let points_start = points_app.panels[0].series[0].points[0].0;
        points_app.panels[0].series[0].points = vec![
            (points_start, 0.0),
            (f64::NAN, 50.0),
            (points_start + 50.0, f64::NAN),
            (points_start + 100.0, 100.0),
        ];
        let points_svg = render_svg(&points_app, Rect::new(0, 0, 120, 50));
        assert!(!points_svg.contains("NaN"));

        let mut area_app = test_app_with_panel_type(PanelType::Graph);
        area_app.panels[0].options = PanelOptions::Graph(GraphOptions {
            draw_style: GraphDrawStyle::Line,
            show_points: GraphPointMode::Never,
            fill_opacity: Some(30),
            axis_placement: GraphAxisPlacement::Visible,
            line_interpolation: None,
            stacking: GraphStackingMode::Off,
        });
        let area_start = area_app.panels[0].series[0].points[0].0;
        area_app.panels[0].series[0].points = vec![
            (area_start, 0.0),
            (f64::NAN, 50.0),
            (area_start + 50.0, f64::NAN),
            (area_start + 100.0, 100.0),
        ];
        let area_svg = render_svg(&area_app, Rect::new(0, 0, 120, 50));
        assert!(!area_svg.contains("NaN"));

        let mut bars_app = test_app_with_panel_type(PanelType::Graph);
        bars_app.panels[0].options = PanelOptions::Graph(GraphOptions {
            draw_style: GraphDrawStyle::Bars,
            show_points: GraphPointMode::Auto,
            fill_opacity: None,
            axis_placement: GraphAxisPlacement::Visible,
            line_interpolation: None,
            stacking: GraphStackingMode::Off,
        });
        let bars_start = bars_app.panels[0].series[0].points[0].0;
        bars_app.panels[0].series[0].points = vec![
            (bars_start, 0.0),
            (f64::NAN, 50.0),
            (bars_start + 50.0, f64::NAN),
            (bars_start + 100.0, 100.0),
        ];
        let bars_svg = render_svg(&bars_app, Rect::new(0, 0, 120, 50));
        assert!(!bars_svg.contains("NaN"));
    }

    #[test]
    fn nan_bar_gauge_values_draw_an_empty_bar() {
        let mut app = test_app_with_panel_type(PanelType::BarGauge);
        app.panels[0].series[0].value = Some(f64::NAN);
        if let Some(point) = app.panels[0].series[0].points.last_mut() {
            point.1 = f64::NAN;
        }

        let svg = render_svg(&app, Rect::new(0, 0, 120, 40));

        // The value label shows "NaN"; no coordinate does.
        assert!(!svg.contains("=\"NaN\""), "{svg}");
    }

    #[test]
    fn data_near_the_float_limits_keeps_finite_coordinates() {
        let mut app = test_app_with_panel_type(PanelType::Graph);
        let start = app.panels[0].series[0].points[0].0;
        app.panels[0].series[0].points = vec![
            (start, -1e308),
            (start + 50.0, 1e308),
            (start + 100.0, f64::MAX),
        ];

        let svg = render_svg(&app, Rect::new(0, 0, 120, 40));

        for bad in ["=\"NaN\"", "=\"inf\"", "=\"-inf\""] {
            assert!(!svg.contains(bad), "{bad} in {svg}");
        }
    }

    #[test]
    fn test_non_graph_panel_exports_render_representative_svg() {
        let cases = [
            (PanelType::Stat, "usage &amp; total", "<path "),
            (PanelType::Gauge, "10%", "100.00"),
            (PanelType::BarGauge, "usage &amp; total", "10.00"),
            (PanelType::Table, "Series", "Value"),
            (PanelType::Heatmap, "#cc3333", "#33c8cc"),
        ];

        for (panel_type, first, second) in cases {
            let mut app = test_app_with_panel_type(panel_type);
            app.theme = crate::theme::builtin("terminal").unwrap();
            let svg = render_svg(&app, Rect::new(0, 0, 100, 40));

            assert!(svg.contains("CPU &lt;main&gt;"));
            assert!(svg.contains(first), "{panel_type:?} missing {first}");
            assert!(svg.contains(second), "{panel_type:?} missing {second}");
            assert!(!svg.contains("No data"));
        }
    }

    #[test]
    fn test_export_uses_panel_display_format_for_stat_and_table_values() {
        let mut stat_app = test_app_with_panel_type(PanelType::Stat);
        stat_app.panels[0].display = ui::DisplayFormat {
            unit: Some("bytes".to_string()),
            decimals: Some(1),
            no_value: None,
        };
        stat_app.panels[0].series[0].value = Some(1536.0);

        let stat_svg = render_svg(&stat_app, Rect::new(0, 0, 100, 40));
        assert!(stat_svg.contains("1.5KiB"));

        stat_app.panels[0].display.no_value = Some("n/a".to_string());
        stat_app.panels[0].series[0].value = None;
        let stat_no_value_svg = render_svg(&stat_app, Rect::new(0, 0, 100, 40));
        assert!(stat_no_value_svg.contains("n/a"));

        let mut table_app = test_app_with_panel_type(PanelType::Table);
        table_app.panels[0].display = ui::DisplayFormat {
            unit: Some("percentunit".to_string()),
            decimals: Some(0),
            no_value: Some("n/a".to_string()),
        };
        table_app.panels[0].series[0].value = Some(0.42);
        table_app.panels[0].series.push(SeriesView {
            name: "missing".to_string(),
            value: None,
            points: vec![],
            visible: true,
        });

        let table_svg = render_svg(&table_app, Rect::new(0, 0, 100, 40));
        assert!(table_svg.contains("42%"));
        assert!(table_svg.contains("n/a"));
    }

    #[test]
    fn test_empty_panels_export_the_dashboard_no_value_text() {
        for panel_type in [PanelType::Graph, PanelType::Table, PanelType::Stat] {
            let mut app = test_app_with_panel_type(panel_type);
            app.panels[0].series.clear();
            let svg = render_svg(&app, Rect::new(0, 0, 100, 40));
            assert!(svg.contains("No data"), "{panel_type:?}");

            app.panels[0].display.no_value = Some("none (healthy)".to_string());
            let svg = render_svg(&app, Rect::new(0, 0, 100, 40));
            assert!(svg.contains("none (healthy)"), "{panel_type:?}");
            assert!(!svg.contains("No data"), "{panel_type:?}");
        }
    }

    #[test]
    fn test_export_keeps_panel_notices_beside_shortened_titles() {
        let mut app = test_app_with_panel_type(PanelType::Stat);
        app.panels[0].title = "Upstream connect failures / timeouts".to_string();
        app.panels[0].notices.warnings = vec!["partial response".to_string()];

        let svg = render_svg(&app, Rect::new(0, 0, 50, 20));
        assert!(svg.contains(">Upstream"), "{svg}");
        assert!(
            svg.contains(r#"… <tspan fill=""#)
                && svg.contains(r#"data-role="panel-notice">⚠ warning</tspan></text>"#),
            "{svg}"
        );

        // Infos alone get no marker.
        app.panels[0].notices.warnings.clear();
        app.panels[0].notices.infos = vec!["metric might not be a counter".to_string()];
        let svg = render_svg(&app, Rect::new(0, 0, 50, 20));
        assert!(!svg.contains("panel-notice"), "{svg}");
    }

    #[test]
    fn test_export_wraps_errors_inside_the_panel() {
        let mut app = test_app_with_panel_type(PanelType::Graph);
        app.panels[0].series.clear();
        let error = format!(
            "query_range failed for `{}`: request failed",
            "rate(envoy_cluster_upstream_cx_connect_fail[5m])".repeat(2)
        );
        app.panels[0].last_error = Some(error.clone());
        let error_color = color_hex(app.theme.error, "#ff5555");
        let error_lines = |svg: &str| {
            svg.split(r#"<text x="22.00""#)
                .skip(1)
                .filter(|text| text.contains(&format!(r#"fill="{error_color}""#)))
                .filter_map(|text| text.split_once('>'))
                .filter_map(|(_, rest)| rest.split_once("</text>"))
                .map(|(line, _)| line.to_string())
                .collect::<Vec<_>>()
        };

        // The panel is 190px wide in a 400px export.
        let columns = text_columns(190.0 - PANEL_PADDING * 2.0, FONT_SIZE);
        let svg = render_svg(&app, Rect::new(0, 0, 40, 30));
        let lines = error_lines(&svg);
        assert!(lines.len() > 1, "{svg}");
        assert!(
            lines.iter().all(|line| line.chars().count() <= columns),
            "{lines:?}"
        );
        assert_eq!(
            lines.concat().replace(' ', ""),
            error.replace(' ', ""),
            "{svg}"
        );

        // A short panel cuts the error off.
        let svg = render_svg(&app, Rect::new(0, 0, 40, 10));
        let lines = error_lines(&svg);
        assert_eq!(lines.len(), 1, "{svg}");
        assert!(lines[0].ends_with('…'), "{lines:?}");
    }

    #[test]
    fn test_fit_text_shrinks_then_shortens() {
        // 10 characters at 28px need 173.6px.
        assert_eq!(
            fit_text("0.00 ops/s", 200.0, 28.0),
            ("0.00 ops/s".to_string(), 28.0)
        );
        let (text, size) = fit_text("0.00 ops/s", 124.0, 28.0);
        assert_eq!(text, "0.00 ops/s");
        assert!((size - 20.0).abs() < 1e-9, "{size}");
        // Below the small size, the text is cut short instead.
        let (text, size) = fit_text("0.00 ops/s", 50.0, 28.0);
        assert_eq!((text.as_str(), size), ("0.00 o…", SMALL_FONT_SIZE));
        // Text already at the small size is never enlarged.
        assert_eq!(
            fit_text("api", 500.0, SMALL_FONT_SIZE),
            ("api".to_string(), SMALL_FONT_SIZE)
        );
    }

    #[test]
    fn test_stat_values_fit_narrow_export_panels() {
        let mut app = test_app_with_panel_type(PanelType::Stat);
        app.panels[0].display.unit = Some("ops".to_string());
        app.panels[0].series[0].name = "Upstream connect failures".to_string();
        // The value and the name sit on these baselines inside the panel.
        let text_at = |svg: &str, y: &str| {
            let (_, rest) = svg.split_once(&format!(r#"y="{y}""#)).unwrap();
            let (_, rest) = rest.split_once(r#"font-size=""#).unwrap();
            let (size, rest) = rest.split_once('"').unwrap();
            let (_, rest) = rest.split_once('>').unwrap();
            let (text, _) = rest.split_once("</text>").unwrap();
            (text.to_string(), size.parse::<f64>().unwrap())
        };
        let stat_value = |svg: &str| {
            let (value, size) = text_at(svg, "134.00");
            (value, size, text_at(svg, "156.00").0)
        };
        let panel_width = |svg: &str| {
            let (_, rest) = svg.split_once(r#"<rect x="10" y="72" width=""#).unwrap();
            rest.split_once('"').unwrap().0.parse::<f64>().unwrap()
        };

        // Wide: the full value at full size.
        let svg = render_svg(&app, Rect::new(0, 0, 100, 40));
        let (value, size, name) = stat_value(&svg);
        assert_eq!(
            (value.as_str(), size),
            ("10.00 ops/s", STAT_FONT_SIZE),
            "{svg}"
        );
        assert_eq!(name, "Upstream connect failures");

        // Narrow: smaller, still whole, and inside the panel.
        let svg = render_svg(&app, Rect::new(0, 0, 32, 40));
        let (value, size, name) = stat_value(&svg);
        let width = panel_width(&svg) - PANEL_PADDING * 2.0;
        assert_eq!(value, "10.00 ops/s", "{svg}");
        assert!((SMALL_FONT_SIZE..STAT_FONT_SIZE).contains(&size), "{size}");
        assert!(value.chars().count() as f64 * size * CHAR_ADVANCE <= width);
        assert!(name.ends_with('…'), "{name}");
        assert!(name.chars().count() as f64 * SMALL_FONT_SIZE * CHAR_ADVANCE <= width);

        // Very narrow: cut short at the small size.
        let svg = render_svg(&app, Rect::new(0, 0, 18, 40));
        let (value, size, _) = stat_value(&svg);
        assert_eq!(size, SMALL_FONT_SIZE, "{svg}");
        assert!(value.ends_with('…'), "{value}");
    }

    #[test]
    fn test_axis_labels_give_way_to_placed_ones() {
        let mut labels = AxisLabels::default();
        assert!(labels.claim([0.0, 50.0]));
        assert!(!labels.claim([40.0, 90.0]), "overlaps");
        assert!(!labels.claim([53.0, 90.0]), "closer than the gap");
        assert!(labels.claim([56.0, 90.0]));
        assert!(labels.claim([-40.0, -6.0]));
    }

    #[test]
    fn test_layout_legend_wraps_shortens_and_counts_the_rest() {
        let items = |names: &[&str]| {
            names
                .iter()
                .enumerate()
                .map(|(index, name)| (index, name.to_string()))
                .collect::<Vec<_>>()
        };
        // Each "aaaaaaaaaa" entry is 13 + 68.2 wide, plus an 11 gap.
        let legend = layout_legend(items(&["aaaaaaaaaa"; 3]), 200.0, 3);
        assert_eq!(legend.rows, 2);
        assert_eq!(
            legend
                .entries
                .iter()
                .map(|entry| entry.row)
                .collect::<Vec<_>>(),
            [0, 0, 1]
        );
        assert!(legend.more.is_none());

        // A label wider than the legend is shortened.
        let legend = layout_legend(items(&[&"x".repeat(80)]), 200.0, 3);
        assert!(legend.entries[0].label.ends_with('…'));
        assert!(LEGEND_SWATCH + text_width(&legend.entries[0].label, SMALL_FONT_SIZE) <= 200.0);

        // Past the last row, "+N more" takes the place of what does not fit.
        let legend = layout_legend(items(&["aaaaaaaaaa"; 9]), 200.0, 2);
        assert_eq!(legend.rows, 2);
        assert_eq!(legend.entries.len(), 3);
        let (x, more) = legend.more.unwrap();
        assert_eq!(more, "+6 more");
        assert!(x + text_width(&more, SMALL_FONT_SIZE) <= 200.0);
    }

    /// The `<text>` elements of an SVG: each one's box, estimated as the
    /// layout estimates it, as `[left, top, right, baseline]`, and its text.
    fn svg_text_boxes(svg: &str) -> Vec<([f64; 4], String)> {
        let attribute = |attributes: &str, name: &str| {
            attributes
                .split_once(&format!(r#" {name}=""#))
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(value, _)| value.to_string())
        };
        svg.split("<text")
            .skip(1)
            .map(|element| {
                let (attributes, rest) = element.split_once('>').unwrap();
                let (content, _) = rest.split_once("</text>").unwrap();
                let mut text = String::new();
                let mut in_tag = false;
                for ch in content.chars() {
                    match ch {
                        '<' => in_tag = true,
                        '>' => in_tag = false,
                        _ if !in_tag => text.push(ch),
                        _ => {}
                    }
                }
                let text = text
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&quot;", "\"")
                    .replace("&amp;", "&");
                let number = |name| attribute(attributes, name).unwrap().parse::<f64>().unwrap();
                let (x, y, size) = (number("x"), number("y"), number("font-size"));
                let width = text_width(&text, size);
                let left = match attribute(attributes, "text-anchor").as_deref() {
                    Some("middle") => x - width / 2.0,
                    Some("end") => x - width,
                    _ => x,
                };
                ([left, y - size, left + width, y], text)
            })
            .collect()
    }

    #[test]
    fn test_export_text_stays_in_its_panel_without_overprinting() {
        for panel_type in [
            PanelType::Graph,
            PanelType::Stat,
            PanelType::Gauge,
            PanelType::BarGauge,
            PanelType::Table,
        ] {
            for cells in [18, 25, 32, 50, 100] {
                let mut app = test_app_with_panel_type(panel_type);
                // Three hours ending 9m12s past an hour, so the last hour
                // tick falls near the end label.
                app.range = std::time::Duration::from_secs(3 * 3600);
                app.view_end_ts = 1_700_002_752;
                let (start, end) = app.time_bounds();
                app.panels[0].title = "Upstream connect failures / timeouts".to_string();
                app.panels[0].display.unit = Some("reqps".to_string());
                app.panels[0].series = (0..12)
                    .map(|index| SeriesView {
                        name: format!("nexus-uar-docker-token-svc-{index}"),
                        value: Some(1234.5 * index as f64),
                        points: vec![(start, 0.0), (end, 0.54 * index as f64)],
                        visible: true,
                    })
                    .collect();
                let svg = render_svg(&app, Rect::new(0, 0, cells, 40));

                let boxes = svg_text_boxes(&svg);
                for (index, (a, a_text)) in boxes.iter().enumerate() {
                    for (b, b_text) in &boxes[index + 1..] {
                        let overlap = a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
                        assert!(
                            !overlap,
                            "{panel_type:?} at {cells} cells: {a_text:?} {a:?} overprints {b_text:?} {b:?}"
                        );
                    }
                }

                let (_, rest) = svg.split_once(r#"<rect x="10" y="72" width=""#).unwrap();
                let (width, rest) = rest.split_once('"').unwrap();
                let (_, rest) = rest.split_once(r#"height=""#).unwrap();
                let (height, _) = rest.split_once('"').unwrap();
                let (right, bottom) = (
                    10.0 + width.parse::<f64>().unwrap(),
                    72.0 + height.parse::<f64>().unwrap(),
                );
                for (bounds, text) in boxes
                    .iter()
                    .filter(|(bounds, _)| bounds[3] > 72.0 && bounds[1] < bottom)
                {
                    assert!(
                        bounds[0] >= 10.0 && bounds[2] <= right && bounds[3] <= bottom,
                        "{panel_type:?} at {cells} cells: {text:?} {bounds:?} leaves the panel (10, 72)-({right}, {bottom})"
                    );
                }
            }
        }
    }

    #[test]
    fn test_wrap_text_breaks_at_spaces_lines_and_long_words() {
        assert_eq!(wrap_text("one two three", 8, 5), ["one two", "three"]);
        assert_eq!(wrap_text("a\nb", 8, 5), ["a", "b"]);
        assert_eq!(wrap_text("abcdefghij", 4, 5), ["abcd", "efgh", "ij"]);
        assert_eq!(wrap_text("one two three four", 7, 2), ["one two", "three…"]);
        assert_eq!(wrap_text("abcdefghij", 4, 1), ["abc…"]);
        assert!(wrap_text("text", 0, 3).is_empty());
    }

    #[test]
    fn test_png_rasterization_writes_non_empty_file() {
        let mut app = test_app(ExportOptions::default());
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(app.view_end_ts as f64, "deploy"),
        ]);
        let svg = render_svg(&app, Rect::new(0, 0, 100, 40));
        assert!(svg.contains(r#"data-role="annotation-marker""#));
        let dir = test_export_dir("png");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("snapshot.png");

        write_png(&svg, &path).unwrap();

        assert!(fs::metadata(&path).unwrap().len() > 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn annotation_xml_controls_are_safe_for_svg_png_and_recording() {
        let dir = test_export_dir("annotation-xml");
        let export = ExportOptions {
            dir: dir.clone(),
            format: ExportFormat::Both,
            record_max_frames: 10,
        };
        let mut app = test_app(export);
        app.view_end_ts = 100;
        app.range = std::time::Duration::from_secs(100);
        let mut event = crate::annotations::test_event_at(
            50.0,
            "deploy\t\n\r Ω😀\u{0}\u{1}\u{fffe}\u{ffff} complete",
        );
        event.tags = vec!["valid\u{10000}".to_string(), "bad\u{8}".to_string()];
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![event]);
        app.cursor_x = Some(50.0);
        let viewport = Rect::new(0, 0, 120, 50);

        let svg = render_svg(&app, viewport);
        assert!(!svg.chars().any(|character| {
            matches!(
                character,
                '\u{0}' | '\u{1}' | '\u{8}' | '\u{fffe}' | '\u{ffff}'
            )
        }));
        assert!(svg.contains("\t\n\r Ω😀"));
        assert!(svg.contains("valid\u{10000}"));
        assert!(svg.contains('�'));
        let options = resvg::usvg::Options::default();
        resvg::usvg::Tree::from_data(svg.as_bytes(), &options).unwrap();

        toggle_recording(&mut app, viewport).unwrap();
        let recording = app.recording.as_ref().unwrap();
        assert_eq!(recording.frame_count, 1);
        assert_eq!(recording.frames[0].files.len(), 2);
        assert!(
            fs::metadata(recording.dir.join("frame-000001.svg"))
                .unwrap()
                .len()
                > 0
        );
        assert!(
            fs::metadata(recording.dir.join("frame-000001.png"))
                .unwrap()
                .len()
                > 0
        );
        toggle_recording(&mut app, viewport).unwrap();

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_recording_writes_manifest_and_skips_duplicates() {
        let dir = test_export_dir("recording");
        let export = ExportOptions {
            dir: dir.clone(),
            format: ExportFormat::Svg,
            record_max_frames: 10,
        };
        let mut app = test_app(export);
        let viewport = Rect::new(0, 0, 100, 40);

        toggle_recording(&mut app, viewport).unwrap();
        capture_recording_frame(&mut app, viewport).unwrap();
        assert_eq!(app.recording.as_ref().unwrap().frame_count, 1);

        app.panels[0].series[0].value = Some(42.0);
        app.panels[0].series[0]
            .points
            .push((chrono::Utc::now().timestamp() as f64, 42.0));
        capture_recording_frame(&mut app, viewport).unwrap();
        assert_eq!(app.recording.as_ref().unwrap().frame_count, 2);

        toggle_recording(&mut app, viewport).unwrap();

        let recording_dir = fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path();
        let manifest = recording_dir.join("manifest.json");
        assert!(manifest.exists());
        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(manifest).unwrap()).unwrap();
        assert_eq!(json["version"], 1);
        assert_eq!(json["format"], "svg");
        assert_eq!(json["changed_only"], true);
        assert_eq!(json["frame_count"], 2);
        assert_eq!(json["max_frames"], 10);
        assert_eq!(json["completed_reason"], "stopped");
        assert_eq!(json["viewport"]["width"], viewport.width);
        assert_eq!(json["viewport"]["height"], viewport.height);
        assert!(json["started_at"].as_str().unwrap().contains('T'));
        assert!(json["completed_at"].as_str().unwrap().contains('T'));
        assert_eq!(json["frames"].as_array().unwrap().len(), 2);
        assert_eq!(json["frames"][1]["index"], 2);
        assert!(
            json["frames"][1]["captured_at"]
                .as_str()
                .unwrap()
                .contains('T')
        );
        assert!(json["frames"][1]["elapsed_ms"].as_u64().is_some());
        assert_eq!(json["frames"][1]["files"][0], "frame-000002.svg");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_export_options_reject_zero_recording_frames() {
        let export = ExportOptions {
            record_max_frames: 0,
            ..ExportOptions::default()
        };

        let err = export.validate().unwrap_err();

        assert!(
            err.to_string().contains("record_max_frames"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn export_options_reject_recording_frames_over_the_limit() {
        let options = |record_max_frames| ExportOptions {
            record_max_frames,
            ..ExportOptions::default()
        };
        assert!(options(MAX_RECORD_FRAMES).validate().is_ok());
        let err = options(MAX_RECORD_FRAMES + 1).validate().unwrap_err();
        assert_eq!(
            err.to_string(),
            "record_max_frames is 100001, more than the limit of 100000"
        );
    }

    #[test]
    fn test_recording_frame_cap_prevents_extra_frames() {
        let dir = test_export_dir("recording-cap");
        let export = ExportOptions {
            dir: dir.clone(),
            format: ExportFormat::Svg,
            record_max_frames: 1,
        };
        let mut app = test_app(export);
        let viewport = Rect::new(0, 0, 100, 40);

        toggle_recording(&mut app, viewport).unwrap();
        app.panels[0].series[0].value = Some(42.0);
        app.panels[0].series[0]
            .points
            .push((chrono::Utc::now().timestamp() as f64, 42.0));
        capture_recording_frame(&mut app, viewport).unwrap();

        let recording = app.recording.as_ref().unwrap();
        assert_eq!(recording.frame_count, 1);
        assert_eq!(recording.frames.len(), 1);
        let status = app.export_status.as_deref().unwrap();
        assert!(status.contains("Recording capped at 1/1 frames"));
        assert!(status.contains(&dir.display().to_string()));
        assert!(status.contains("press Ctrl+E or q to save"));

        toggle_recording(&mut app, viewport).unwrap();

        let recording_dir = fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path();
        let manifest = recording_dir.join("manifest.json");
        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(manifest).unwrap()).unwrap();
        assert_eq!(json["frame_count"], 1);
        assert_eq!(json["max_frames"], 1);
        assert_eq!(json["completed_reason"], "capped");
        assert_eq!(json["frames"].as_array().unwrap().len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
