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

use crate::app::{AppState, PanelState, SeriesView};
use crate::num;
use crate::theme::Theme;
use crate::ui::format::value_to_heatmap_color;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

pub(super) fn render_heatmap(frame: &mut Frame, area: Rect, p: &PanelState, app: &AppState) {
    let theme = &app.theme;

    if p.series.is_empty() {
        render_message(frame, area, p.display.no_data_text(), theme);
        return;
    }

    // For heatmap, we'll display each series as a row, with time buckets as columns
    // Each cell is colored based on value intensity

    let rows_available = usize::from(area.height.saturating_sub(2)); // Reserve space for labels
    let cols_available = usize::from(area.width);

    if rows_available == 0 || cols_available == 0 {
        return;
    }

    // Limit to visible series
    let visible_series: Vec<_> = p.series.iter().filter(|s| s.visible).collect();
    if visible_series.is_empty() {
        render_message(frame, area, "No visible series", theme);
        return;
    }

    // Calculate global min/max for color scaling
    let (global_min, global_max) = value_range(&visible_series);

    if !global_min.is_finite() || !global_max.is_finite() || global_min == global_max {
        render_message(frame, area, "Invalid data range", theme);
        return;
    }

    // Prepare text lines for the heatmap
    let scale = HeatmapScale {
        cols: cols_available,
        min: global_min,
        max: global_max,
    };
    let lines: Vec<Line> = visible_series
        .iter()
        .take(rows_available)
        .filter_map(|series| heatmap_line(series, &scale, theme))
        .collect();

    if lines.is_empty() {
        render_message(frame, area, p.display.no_data_text(), theme);
        return;
    }

    let heatmap_widget = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));

    frame.render_widget(heatmap_widget, area);
}

/// Draws a one-line message in the panel area.
fn render_message(frame: &mut Frame, area: Rect, text: &str, theme: &Theme) {
    let para = Paragraph::new(text).style(Style::default().fg(theme.text));
    frame.render_widget(para, area);
}

/// Returns the smallest and the largest finite value of the series.
fn value_range(series: &[&SeriesView]) -> (f64, f64) {
    let (mut global_min, mut global_max) = (f64::MAX, f64::MIN);
    for s in series {
        for (_, v) in &s.points {
            if v.is_finite() {
                global_min = global_min.min(*v);
                global_max = global_max.max(*v);
            }
        }
    }
    (global_min, global_max)
}

/// The column count and the value range of a heatmap.
struct HeatmapScale {
    cols: usize,
    min: f64,
    max: f64,
}

/// Builds the heatmap row of one series, or `None` when it has no points.
fn heatmap_line(series: &SeriesView, scale: &HeatmapScale, theme: &Theme) -> Option<Line<'static>> {
    let cols_available = scale.cols;
    let (global_min, global_max) = (scale.min, scale.max);
    // Downsample points to fit available columns
    let total_points = series.points.len();
    if total_points == 0 {
        return None;
    }

    let step = (num::usize_to_f64(total_points) / num::usize_to_f64(cols_available)).max(1.0);

    let mut spans = Vec::new();
    for col_idx in 0..cols_available {
        let point_idx = num::f64_to_usize(num::usize_to_f64(col_idx) * step).min(total_points - 1);
        // The `min` keeps the index in the series. NaN draws the empty color.
        let value = series
            .points
            .get(point_idx)
            .map_or(f64::NAN, |&(_, value)| value);

        // Map value to color intensity (from blue/cold to red/hot)
        let color = if value.is_finite() {
            let normalized = ((value - global_min) / (global_max - global_min)).clamp(0.0, 1.0);
            value_to_heatmap_color(normalized, theme.heatmap)
        } else {
            theme.heatmap_empty
        };

        spans.push(Span::styled("█", Style::default().fg(color)));
    }

    Some(Line::from(spans))
}
