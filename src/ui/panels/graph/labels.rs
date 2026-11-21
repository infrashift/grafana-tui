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

use super::overlay::is_blank_cell;
use crate::num;
use crate::ui::format::{DisplayFormat, format_axis_time};
use ratatui::prelude::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct PlotBounds {
    pub(super) left: u16,
    pub(super) right: u16,
    pub(super) top: u16,
    pub(super) bottom: u16,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct YLabelArea {
    pub(super) left: u16,
    pub(super) width: u16,
}

pub(super) struct YLabelContext<'a> {
    pub(super) y_bounds: [f64; 2],
    pub(super) autogrid_ticks: &'a [f64],
    pub(super) threshold_labels: &'a [(f64, Color)],
    pub(super) display: &'a DisplayFormat,
    pub(super) color: Color,
}

pub(super) fn y_label_width(
    axis_labels: &[Span<'_>],
    autogrid_ticks: &[f64],
    threshold_labels: &[(f64, Color)],
    display: &DisplayFormat,
) -> u16 {
    debug_assert!(
        axis_labels.len() != 1,
        "the y axis has no labels or a label at each end"
    );
    debug_assert!(
        autogrid_ticks
            .iter()
            .zip(autogrid_ticks.iter().skip(1))
            .all(|(a, b)| a < b),
        "the grid ticks are in increasing order"
    );
    // Reserve space using the same unit formatter that will draw the labels;
    // otherwise bytes/percent suffixes can clip or push into the plot area.
    let axis_width = axis_labels
        .iter()
        .map(|label| num::usize_to_u16(label.width()))
        .max()
        .unwrap_or(0);
    let grid_width = autogrid_ticks
        .iter()
        .map(|tick| num::usize_to_u16(display.format_number(*tick).len()))
        .max()
        .unwrap_or(0);
    let threshold_width = threshold_labels
        .iter()
        .map(|(tick, _)| num::usize_to_u16(display.format_number(*tick).len()))
        .max()
        .unwrap_or(0);

    axis_width.max(grid_width).max(threshold_width)
}

pub(super) fn render_intermediate_y_labels(
    frame: &mut Frame,
    label_area: YLabelArea,
    plot: PlotBounds,
    labels: YLabelContext<'_>,
) {
    if label_area.width == 0 {
        return;
    }
    let [lower, upper] = labels.y_bounds;
    debug_assert!(lower <= upper, "the y bounds are in order");
    debug_assert!(
        labels
            .autogrid_ticks
            .iter()
            .all(|tick| *tick > lower && *tick < upper),
        "each grid tick is inside the y bounds"
    );

    for tick in labels.autogrid_ticks {
        if let Some(y) = value_to_y_label_row(*tick, labels.y_bounds, plot) {
            write_right_aligned_label(
                frame,
                label_area.left,
                y,
                label_area.width,
                &labels.display.format_number(*tick),
                labels.color,
            );
        }
    }

    for (tick, color) in labels.threshold_labels {
        if let Some(y) = value_to_y_label_row(*tick, labels.y_bounds, plot) {
            write_right_aligned_label(
                frame,
                label_area.left,
                y,
                label_area.width,
                &labels.display.format_number(*tick),
                *color,
            );
        }
    }
}

pub(super) fn render_autogrid_time_labels(
    frame: &mut Frame,
    plot: PlotBounds,
    x_bounds: [f64; 2],
    ticks: &[f64],
    range_secs: f64,
    color: Color,
) {
    let y = plot.bottom.saturating_add(1);
    for tick in ticks {
        if let Some(x) = value_to_plot_x(*tick, x_bounds, plot) {
            write_centered_label(
                frame,
                x,
                y,
                plot.left,
                plot.right,
                &format_axis_time(*tick, range_secs),
                color,
            );
        }
    }
}

fn value_to_plot_y(value: f64, y_bounds: [f64; 2], plot: PlotBounds) -> Option<u16> {
    if value <= y_bounds[0] || value >= y_bounds[1] || y_bounds[1] <= y_bounds[0] {
        return None;
    }

    let height = f64::from(plot.bottom.saturating_sub(plot.top));
    let ratio = (value - y_bounds[0]) / (y_bounds[1] - y_bounds[0]);
    debug_assert!(
        !(ratio < 0.0 || ratio > 1.0),
        "the ratio is not outside the unit range"
    );
    let y_offset = num::f64_to_u16((ratio * height).round());
    debug_assert!(
        f64::from(y_offset) <= height,
        "the row offset stays in the plot height"
    );
    let y = plot.bottom.saturating_sub(y_offset);
    debug_assert!(
        plot.bottom < plot.top || y >= plot.top,
        "the row is not above the plot"
    );
    Some(y)
}

fn value_to_y_label_row(value: f64, y_bounds: [f64; 2], plot: PlotBounds) -> Option<u16> {
    value_to_plot_y(value, y_bounds, plot)
}

pub(super) fn value_to_plot_x(value: f64, x_bounds: [f64; 2], plot: PlotBounds) -> Option<u16> {
    if value < x_bounds[0] || value > x_bounds[1] || x_bounds[1] <= x_bounds[0] {
        return None;
    }

    let width = f64::from(plot.right.saturating_sub(plot.left).saturating_sub(1));
    let ratio = (value - x_bounds[0]) / (x_bounds[1] - x_bounds[0]);
    debug_assert!(
        !(ratio < 0.0 || ratio > 1.0),
        "the ratio is not outside the unit range"
    );
    let x_offset = num::f64_to_u16((ratio * width).round());
    debug_assert!(
        f64::from(x_offset) <= width,
        "the column offset stays in the plot width"
    );
    let x = plot.left.saturating_add(x_offset);
    debug_assert!(
        plot.right <= plot.left || x < plot.right,
        "the column is left of the plot right edge"
    );
    Some(x)
}

fn write_right_aligned_label(
    frame: &mut Frame,
    left: u16,
    y: u16,
    width: u16,
    label: &str,
    color: Color,
) {
    debug_assert!(width > 0, "the label area has a width");
    let label_width = num::usize_to_u16(label.chars().count());
    let x = left.saturating_add(width.saturating_sub(label_width));
    debug_assert!(
        label_width > width || x.checked_add(label_width) == left.checked_add(width),
        "a label that fits ends at the right edge of the area"
    );
    write_label(frame, x, y, label, color, false);
}

fn write_centered_label(
    frame: &mut Frame,
    center: u16,
    y: u16,
    min_x: u16,
    max_x: u16,
    label: &str,
    color: Color,
) {
    debug_assert!(center >= min_x, "the label center is not left of the plot");
    let label_width = num::usize_to_u16(label.chars().count());
    let Some(start_x) = centered_label_start(center, label_width, min_x, max_x) else {
        return;
    };
    debug_assert!(label_width > 0, "a placed label has characters");

    let buf = frame.buffer_mut();
    for offset in 0..label_width {
        let Some(cell) = buf.cell((start_x.saturating_add(offset), y)) else {
            return;
        };
        if !is_blank_cell(cell) {
            return;
        }
    }

    let style = Style::default().fg(color);
    for (offset, ch) in label.chars().enumerate() {
        if let Some(cell) = buf.cell_mut((start_x.saturating_add(num::usize_to_u16(offset)), y)) {
            cell.set_char(ch).set_style(style);
        }
    }
}

fn centered_label_start(center: u16, label_width: u16, min_x: u16, max_x: u16) -> Option<u16> {
    if label_width == 0 || max_x <= min_x {
        return None;
    }

    let half_width = label_width / 2;
    // For even-length labels, bias one cell right so the visual midpoint better matches
    // the target chart column instead of consistently leaning left.
    let mut start_x = center.checked_sub(half_width)?;
    if label_width.is_multiple_of(2) {
        start_x = start_x.saturating_add(1);
    }
    let end_x_exclusive = start_x.saturating_add(label_width);
    if start_x < min_x || end_x_exclusive > max_x {
        return None;
    }
    debug_assert!(
        start_x <= center,
        "the label starts at or left of its center"
    );
    debug_assert!(
        end_x_exclusive > center,
        "the label covers its center column"
    );

    Some(start_x)
}

fn write_label(frame: &mut Frame, x: u16, y: u16, label: &str, color: Color, blank_only: bool) {
    let style = Style::default().fg(color);
    let buf = frame.buffer_mut();
    for (offset, ch) in label.chars().enumerate() {
        let Some(cell) = buf.cell_mut((x.saturating_add(num::usize_to_u16(offset)), y)) else {
            continue;
        };
        if !blank_only || is_blank_cell(cell) {
            cell.set_char(ch).set_style(style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_to_plot_y_matches_grid_row() {
        let plot = PlotBounds {
            left: 0,
            right: 20,
            top: 10,
            bottom: 20,
        };

        assert_eq!(value_to_plot_y(10.0, [0.0, 20.0], plot), Some(15));
        assert_eq!(value_to_plot_y(5.0, [0.0, 20.0], plot), Some(17));
        assert_eq!(value_to_plot_y(15.0, [0.0, 20.0], plot), Some(12));
        assert_eq!(value_to_plot_y(0.0, [0.0, 20.0], plot), None);
        assert_eq!(value_to_plot_y(20.0, [0.0, 20.0], plot), None);
    }

    #[test]
    fn test_value_to_y_label_row_matches_grid_row() {
        let plot = PlotBounds {
            left: 0,
            right: 20,
            top: 10,
            bottom: 20,
        };

        assert_eq!(value_to_y_label_row(10.0, [0.0, 20.0], plot), Some(15));
        assert_eq!(value_to_y_label_row(5.0, [0.0, 20.0], plot), Some(17));
        assert_eq!(value_to_y_label_row(15.0, [0.0, 20.0], plot), Some(12));
        assert_eq!(value_to_y_label_row(0.0, [0.0, 20.0], plot), None);
        assert_eq!(value_to_y_label_row(20.0, [0.0, 20.0], plot), None);
    }

    #[test]
    fn test_value_to_plot_x_matches_grid_column() {
        let plot = PlotBounds {
            left: 10,
            right: 21,
            top: 0,
            bottom: 10,
        };

        assert_eq!(value_to_plot_x(5.0, [0.0, 10.0], plot), Some(15));
        assert_eq!(value_to_plot_x(0.0, [0.0, 10.0], plot), Some(10));
        assert_eq!(value_to_plot_x(10.0, [0.0, 10.0], plot), Some(20));
    }

    #[test]
    fn test_value_to_plot_x_includes_window_boundaries() {
        let plot = PlotBounds {
            left: 10,
            right: 21,
            top: 0,
            bottom: 5,
        };

        assert_eq!(value_to_plot_x(0.0, [0.0, 100.0], plot), Some(10));
        assert_eq!(value_to_plot_x(100.0, [0.0, 100.0], plot), Some(20));
        assert_eq!(value_to_plot_x(-1.0, [0.0, 100.0], plot), None);
        assert_eq!(value_to_plot_x(101.0, [0.0, 100.0], plot), None);
    }

    #[test]
    fn test_centered_label_start_returns_centered_position_when_it_fits() {
        assert_eq!(centered_label_start(50, 8, 10, 90), Some(47));
        assert_eq!(centered_label_start(50, 7, 10, 90), Some(47));
    }

    #[test]
    fn test_centered_label_start_skips_labels_that_would_be_clamped() {
        assert_eq!(centered_label_start(12, 8, 10, 90), None);
        assert_eq!(centered_label_start(88, 8, 10, 90), None);
    }
}
