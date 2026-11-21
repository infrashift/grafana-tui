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

//! Graph data that the terminal renderer and the SVG export share.

use crate::annotations::{AnnotationEvent, AnnotationPanelContext};
use crate::app::{AppState, GraphOptions, PanelState, PanelType, ThresholdMode};
use crate::theme::Theme;
use crate::ui::{calculate_y_bounds, get_hash_color};
use ratatui::style::Color;
use std::collections::HashMap;

/// Selects the series that a legend lists and the name of an unnamed series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegendMode {
    /// Lists all series and names an unnamed series "Series {i}". The terminal uses this mode.
    AllSeries,
    /// Lists only the visible series. The SVG export uses this mode.
    VisibleOnly,
}

/// The data of one graph panel that both renderers draw.
pub(crate) struct GraphView<'a> {
    pub(crate) panel: &'a PanelState,
    pub(crate) x_bounds: [f64; 2],
    pub(crate) y_bounds: [f64; 2],
    pub(crate) options: GraphOptions,
    /// The annotation events in the time range. Only graph panels have events.
    pub(crate) events: Vec<&'a AnnotationEvent>,
    /// The value of each series at the cursor, by series name.
    pub(crate) cursor_values: HashMap<String, f64>,
    /// The color of each series, by series index.
    pub(crate) colors: Vec<Color>,
    /// The value and the theme color of each threshold step.
    pub(crate) thresholds: Vec<(f64, Color)>,
    /// True when the threshold lines use dashes.
    pub(crate) thresholds_dashed: bool,
}

impl<'a> GraphView<'a> {
    /// Collects the shared graph data of a panel at the cursor position `cursor_x`.
    pub(crate) fn build(
        app: &'a AppState,
        panel_index: usize,
        panel: &'a PanelState,
        cursor_x: Option<f64>,
    ) -> Self {
        let (start, now) = app.time_bounds();
        debug_assert!(start <= now, "the time range starts before it ends");
        let x_bounds = [start, now];
        let view = Self {
            panel,
            x_bounds,
            y_bounds: calculate_y_bounds(panel),
            options: panel.graph_options(),
            events: annotation_events(app, panel_index, panel, x_bounds),
            cursor_values: cursor_values(app, panel, cursor_x),
            colors: series_colors(panel, &app.theme),
            thresholds: threshold_steps(panel)
                .into_iter()
                .map(|(value, color)| (value, app.theme.threshold_color(color)))
                .collect(),
            thresholds_dashed: thresholds_dashed(app, panel),
        };
        let [lower, upper] = view.y_bounds;
        debug_assert!(lower <= upper, "the y bounds are in order");
        debug_assert!(
            view.colors.len() == panel.series.len(),
            "each series has a color"
        );
        debug_assert!(
            view.cursor_values.len() <= panel.series.len(),
            "no more than one cursor value for each series"
        );
        debug_assert!(
            view.events
                .iter()
                .all(|event| (start..=now).contains(&event.timestamp_secs())),
            "each annotation event is in the time range"
        );
        view
    }

    /// Returns true when the panel has annotation events or series points to draw.
    pub(crate) fn has_content(&self) -> bool {
        !self.events.is_empty() || self.panel.series.iter().any(|s| !s.points.is_empty())
    }

    /// Returns true when the panel reserves room for a legend or annotation details.
    pub(crate) fn has_legend(&self) -> bool {
        !self.panel.series.is_empty() || !self.events.is_empty()
    }

    /// Returns the baseline that area fills and bars start from.
    pub(crate) fn area_baseline(&self) -> f64 {
        area_fill_baseline(self.y_bounds)
    }

    /// Returns the export legend labels: the visible series only, as series index and text.
    pub(crate) fn visible_legend_labels(&self) -> Vec<(usize, String)> {
        self.legend_labels(LegendMode::VisibleOnly)
    }

    /// Returns the legend labels as series index and text.
    pub(crate) fn legend_labels(&self, mode: LegendMode) -> Vec<(usize, String)> {
        let labels: Vec<(usize, String)> = self
            .panel
            .series
            .iter()
            .enumerate()
            .filter(|(_, series)| mode == LegendMode::AllSeries || series.visible)
            .map(|(index, series)| {
                let mut label = series.name.clone();
                let value = self.cursor_values.get(&series.name).copied();
                if let Some(value) = value.or(series.value) {
                    label.push_str(&format!(" ({})", self.panel.display.format_number(value)));
                }
                if label.is_empty() && mode == LegendMode::AllSeries {
                    label = format!("Series {}", index);
                }
                (index, label)
            })
            .collect();
        let count = self.panel.series.len();
        debug_assert!(
            labels
                .iter()
                .zip(labels.iter().skip(1))
                .all(|(a, b)| a.0 < b.0)
                && labels.last().is_none_or(|(index, _)| *index < count),
            "the label indexes are series indexes in increasing order"
        );
        debug_assert!(
            mode == LegendMode::VisibleOnly || labels.len() == count,
            "the full legend has a label for each series"
        );
        debug_assert!(
            mode == LegendMode::VisibleOnly || labels.iter().all(|(_, label)| !label.is_empty()),
            "each label of the full legend has text"
        );
        labels
    }
}

impl AppState {
    /// Collects the shared graph data of a panel at the app cursor, for callers outside the `ui` module.
    pub(crate) fn graph_view<'a>(
        &'a self,
        panel_index: usize,
        panel: &'a PanelState,
    ) -> GraphView<'a> {
        debug_assert!(
            self.cursor_x.is_none_or(f64::is_finite),
            "the cursor is at a finite time"
        );
        GraphView::build(self, panel_index, panel, self.cursor_x)
    }
}

/// Returns zero when the y range contains zero, else the lower y bound.
pub(super) fn area_fill_baseline(y_bounds: [f64; 2]) -> f64 {
    let [lower, upper] = y_bounds;
    debug_assert!(lower <= upper, "the y bounds are in order");
    let baseline = if lower <= 0.0 && upper >= 0.0 {
        0.0
    } else {
        lower
    };
    debug_assert!(
        (lower..=upper).contains(&baseline),
        "the baseline is inside the y bounds"
    );
    baseline
}

/// Returns the annotation events of a graph panel in the time range `x_bounds`.
fn annotation_events<'a>(
    app: &'a AppState,
    panel_index: usize,
    panel: &PanelState,
    x_bounds: [f64; 2],
) -> Vec<&'a AnnotationEvent> {
    if panel.panel_type != PanelType::Graph {
        return Vec::new();
    }
    app.annotations.events_for_panel(
        AnnotationPanelContext {
            index: panel_index,
            title: &panel.title,
        },
        x_bounds,
    )
}

/// Returns the value of each series at the point nearest the cursor, within two steps.
fn cursor_values(
    app: &AppState,
    panel: &PanelState,
    cursor_x: Option<f64>,
) -> HashMap<String, f64> {
    let mut values = HashMap::new();
    let Some(cursor_x) = cursor_x else {
        return values;
    };
    let max_distance = app.default_intervals().step.as_secs_f64() * 2.0;
    debug_assert!(
        max_distance.is_finite() && max_distance >= 0.0,
        "the cursor distance is finite and not negative"
    );
    for series in &panel.series {
        let closest = series.points.iter().min_by(|a, b| {
            let da = (a.0 - cursor_x).abs();
            let db = (b.0 - cursor_x).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        if let Some((ts, value)) = closest
            && (ts - cursor_x).abs() <= max_distance
        {
            values.insert(series.name.clone(), *value);
        }
    }
    debug_assert!(
        values.len() <= panel.series.len(),
        "no more than one cursor value for each series"
    );
    values
}

/// Returns the color of each series: the palette, or a name hash when the palette is too short.
fn series_colors(panel: &PanelState, theme: &Theme) -> Vec<Color> {
    debug_assert!(
        !theme.palette.is_empty(),
        "a theme palette has at least one color"
    );
    let use_hash_colors = panel.series.len() > theme.palette.len();
    let colors: Vec<Color> = panel
        .series
        .iter()
        .enumerate()
        .map(|(index, series)| {
            if use_hash_colors {
                get_hash_color(&series.name)
            } else {
                theme.palette_color(index)
            }
        })
        .collect();
    debug_assert!(
        colors.len() == panel.series.len(),
        "each series has a color"
    );
    colors
}

/// Returns the value and the configured color of each threshold step that has a value.
pub(super) fn threshold_steps(panel: &PanelState) -> Vec<(f64, Color)> {
    let Some(thresholds) = &panel.thresholds else {
        return Vec::new();
    };
    let steps: Vec<(f64, Color)> = thresholds
        .steps
        .iter()
        .filter_map(|step| {
            let value = step.value?;
            let value = match thresholds.mode {
                ThresholdMode::Absolute => value,
                ThresholdMode::Percentage => {
                    let min = panel.min.unwrap_or(0.0);
                    let max = panel.max.unwrap_or(100.0);
                    min + (value / 100.0) * (max - min)
                }
            };
            Some((value, step.color))
        })
        .collect();
    debug_assert!(
        steps.len()
            == thresholds
                .steps
                .iter()
                .filter(|step| step.value.is_some())
                .count(),
        "each threshold step with a value gives one entry"
    );
    steps
}

/// Returns true when the threshold marker or the panel threshold style uses dashes.
fn thresholds_dashed(app: &AppState, panel: &PanelState) -> bool {
    let style = panel
        .thresholds
        .as_ref()
        .and_then(|thresholds| thresholds.style.as_deref());
    app.threshold_marker.starts_with("dashed") || style == Some("dashed")
}
