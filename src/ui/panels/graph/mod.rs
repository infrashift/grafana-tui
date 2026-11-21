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

mod annotations;
mod autogrid;
mod bounds;
mod labels;
mod overlay;
mod thresholds;
mod ticks;
mod view;

use annotations::{active_cluster, render_annotation_clusters, terminal_clusters};
use autogrid::build_autogrid_datasets;
use labels::{
    PlotBounds, YLabelArea, YLabelContext, render_autogrid_time_labels,
    render_intermediate_y_labels, y_label_width,
};
use overlay::{merge_overlay_buffer, merge_overlay_buffer_preserving_data};
use thresholds::{render_raw_threshold_lines, threshold_datasets, threshold_marker};
use ticks::{calculate_time_grid_ticks, calculate_value_grid_ticks};
use view::{GraphView, LegendMode, area_fill_baseline};

use crate::annotations::{AnnotationCluster, format_cluster_detail_lines};
use crate::app::{AppState, PanelState};
use crate::num;
use crate::theme::Theme;
use crate::ui::format::format_axis_time;
use ratatui::{
    buffer::Buffer,
    prelude::*,
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph, Wrap},
};

pub(crate) use annotations::cluster_badge;
pub(crate) use bounds::calculate_y_bounds;
pub(crate) use ticks::{svg_time_ticks, svg_value_ticks};

fn graph_type_for_draw_style(draw_style: crate::app::GraphDrawStyle) -> GraphType {
    match draw_style {
        crate::app::GraphDrawStyle::Line => GraphType::Line,
        crate::app::GraphDrawStyle::Points => GraphType::Scatter,
        crate::app::GraphDrawStyle::Bars => GraphType::Bar,
    }
}

fn should_overlay_points(options: &crate::app::GraphOptions) -> bool {
    options.show_points == crate::app::GraphPointMode::Always
        && options.draw_style != crate::app::GraphDrawStyle::Points
}

fn is_y_axis_hidden(options: &crate::app::GraphOptions) -> bool {
    options.axis_placement == crate::app::GraphAxisPlacement::Hidden
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StrongDataMaskMode {
    AreaFillOnly,
    AllDrawStyles,
}

fn strong_data_mask_mode(
    annotation_events: &[&crate::annotations::AnnotationEvent],
) -> StrongDataMaskMode {
    if annotation_events.is_empty() {
        StrongDataMaskMode::AreaFillOnly
    } else {
        StrongDataMaskMode::AllDrawStyles
    }
}

fn chart_plot_left(
    chart_area: Rect,
    y_label_width: u16,
    x_labels: &[Span<'_>],
    has_y_axis_labels: bool,
) -> u16 {
    let first_x_label_width = x_labels
        .first()
        .map(|label| num::usize_to_u16(label.width()))
        .unwrap_or_default();
    let y_axis_offset = u16::from(has_y_axis_labels);
    let x_label_gutter = first_x_label_width.saturating_sub(y_axis_offset);
    let labels_left_of_y_axis = if has_y_axis_labels {
        y_label_width.max(x_label_gutter)
    } else {
        x_label_gutter
    }
    .min(chart_area.width / 3);
    let gutter = labels_left_of_y_axis + y_axis_offset;

    chart_area.left() + gutter
}

fn chart_y_label_width(labels: &[Span<'_>]) -> u16 {
    labels
        .iter()
        .map(|label| num::usize_to_u16(label.width()))
        .max()
        .unwrap_or_default()
}

fn point_to_braille_cell(
    x: f64,
    y: f64,
    x_bounds: [f64; 2],
    y_bounds: [f64; 2],
    plot: PlotBounds,
) -> Option<(u16, u16)> {
    if !x.is_finite()
        || !y.is_finite()
        || !x_bounds[0].is_finite()
        || !x_bounds[1].is_finite()
        || !y_bounds[0].is_finite()
        || !y_bounds[1].is_finite()
        || x < x_bounds[0]
        || x > x_bounds[1]
        || y < y_bounds[0]
        || y > y_bounds[1]
        || x_bounds[1] <= x_bounds[0]
        || y_bounds[1] <= y_bounds[0]
        || plot.right <= plot.left
        || plot.bottom < plot.top
    {
        return None;
    }

    let plot_width = plot.right.saturating_sub(plot.left);
    let plot_height = plot.bottom.saturating_sub(plot.top).saturating_add(1);
    let x_resolution = f64::from(plot_width) * 2.0;
    let y_resolution = f64::from(plot_height) * 4.0;

    let braille_x = num::f64_to_u16(
        ((x - x_bounds[0]) * (x_resolution - 1.0) / (x_bounds[1] - x_bounds[0])).round(),
    );
    let braille_y = num::f64_to_u16(
        ((y_bounds[1] - y) * (y_resolution - 1.0) / (y_bounds[1] - y_bounds[0])).round(),
    );

    let cell_x = plot.left.saturating_add(braille_x / 2);
    let cell_y = plot.top.saturating_add(braille_y / 4);
    (cell_x < plot.right && cell_y <= plot.bottom).then_some((cell_x, cell_y))
}

fn render_forced_point_markers(
    frame: &mut Frame,
    markers: &[(f64, f64, Color)],
    x_bounds: [f64; 2],
    y_bounds: [f64; 2],
    plot: PlotBounds,
) {
    let buf = frame.buffer_mut();
    for (x, y, color) in markers {
        let Some((cell_x, cell_y)) = point_to_braille_cell(*x, *y, x_bounds, y_bounds, plot) else {
            continue;
        };
        if let Some(cell) = buf.cell_mut((cell_x, cell_y)) {
            cell.set_symbol(ratatui::symbols::DOT)
                .set_style(Style::default().fg(*color));
        }
    }
}

/// The datasets and point markers that the terminal graph draws for its series.
struct SeriesLayers<'d> {
    chart: Vec<Dataset<'d>>,
    strong: Vec<Dataset<'d>>,
    markers: Vec<(f64, f64, Color)>,
}

/// The chart area, the axes and the plot bounds of one terminal graph.
struct ChartFrame<'a> {
    app: &'a AppState,
    view: &'a GraphView<'a>,
    area: Rect,
    plot: PlotBounds,
    plot_height: u16,
    x_labels: Vec<Span<'static>>,
    y_labels: Vec<Span<'static>>,
    value_ticks: Vec<f64>,
    show_autogrid: bool,
}

impl ChartFrame<'_> {
    /// Returns a chart with the graph axes and the datasets `datasets`.
    fn overlay_chart<'d>(&self, datasets: Vec<Dataset<'d>>) -> Chart<'d> {
        let theme = &self.app.theme;
        Chart::new(datasets)
            .x_axis(
                Axis::default()
                    .bounds(self.view.x_bounds)
                    .labels(self.x_labels.clone())
                    .style(Style::default().fg(theme.text)),
            )
            .y_axis(
                Axis::default()
                    .style(Style::default().fg(theme.axis))
                    .bounds(self.view.y_bounds)
                    .labels(self.y_labels.clone()),
            )
    }

    /// Renders the datasets `datasets` into an empty buffer of the chart area.
    fn render_to_buffer(&self, datasets: Vec<Dataset<'_>>) -> Buffer {
        let mut buf = Buffer::empty(self.area);
        self.overlay_chart(datasets).render(self.area, &mut buf);
        buf
    }
}

pub(super) fn render_graph_panel(
    frame: &mut Frame,
    area: Rect,
    panel_index: usize,
    p: &PanelState,
    app: &AppState,
    cursor_x: Option<f64>,
    is_selected: bool,
) -> Option<Vec<crate::annotations::AnnotationEvent>> {
    let view = GraphView::build(app, panel_index, p, cursor_x);
    // Hidden series still have points, so their legend stays to show them.
    if !view.has_content() {
        let para =
            Paragraph::new(p.display.no_data_text()).style(Style::default().fg(app.theme.text));
        frame.render_widget(para, area);
        return None;
    }
    let (chart_area, legend_area, show_legend) = layout_areas(area, view.has_legend());
    let chart = chart_frame(app, &view, chart_area);
    let cursor_line = cursor_x.map_or_else(Vec::new, |cx| {
        vec![(cx, view.y_bounds[0]), (cx, view.y_bounds[1])]
    });
    let layers = build_datasets(&view, &cursor_line, app.theme.cursor);
    let strong_data_buf = render_main_chart(frame, &chart, layers.chart, layers.strong);

    let clusters = terminal_clusters(view.events.clone(), view.x_bounds, chart.plot);
    let active_annotation = active_cluster(&clusters, cursor_x, view.x_bounds, chart.plot);
    let rendered_cluster = active_annotation.filter(|_| is_selected).map(|cluster| {
        cluster
            .events
            .iter()
            .map(|event| (*event).clone())
            .collect()
    });

    render_overlay(frame, &chart, strong_data_buf.as_ref());
    render_annotation_clusters(
        frame,
        &clusters,
        chart.plot,
        strong_data_buf.as_ref(),
        app.theme.annotation,
    );
    render_forced_point_markers(
        frame,
        &layers.markers,
        view.x_bounds,
        view.y_bounds,
        chart.plot,
    );
    if show_legend {
        render_legend_or_detail(frame, legend_area, active_annotation, &view, &app.theme);
    }
    rendered_cluster
}

/// Splits the panel area into the chart area and the legend area.
fn layout_areas(area: Rect, has_legend: bool) -> (Rect, Rect, bool) {
    // Reserve two rows for the legend or the annotation details.
    let legend_height = if has_legend && area.height > 5 { 2 } else { 0 };
    let [chart, legend] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(legend_height)]).areas(area);
    (chart, legend, legend_height > 0)
}

/// Builds the axes, the value ticks and the plot bounds of the chart area `area`.
fn chart_frame<'a>(app: &'a AppState, view: &'a GraphView<'a>, area: Rect) -> ChartFrame<'a> {
    let [start, now] = view.x_bounds;
    let text_style = Style::default().fg(app.theme.text);
    let x_labels = vec![
        Span::styled(format_axis_time(start, now - start), text_style),
        Span::styled(format_axis_time(now, now - start), text_style),
    ];
    // The x-axis occupies the last rows.
    let chart_bottom = area.bottom().saturating_sub(2);
    let chart_top = area.top();
    let plot_height = chart_bottom.saturating_sub(chart_top).saturating_add(1);
    let show_autogrid = app.autogrid_enabled && view.panel.autogrid.unwrap_or(true);
    let value_ticks = if show_autogrid {
        calculate_value_grid_ticks(view.y_bounds, plot_height)
    } else {
        Vec::new()
    };
    let y_labels = axis_y_labels(view, plot_height, app.theme.text);
    let left = chart_plot_left(
        area,
        chart_y_label_width(&y_labels),
        &x_labels,
        !y_labels.is_empty(),
    );
    ChartFrame {
        app,
        view,
        area,
        plot: PlotBounds {
            left,
            right: area.right(),
            top: chart_top,
            bottom: chart_bottom,
        },
        plot_height,
        x_labels,
        y_labels,
        value_ticks,
        show_autogrid,
    }
}

/// Returns the y-axis labels with the bounds at both ends, or no labels for a hidden axis.
fn axis_y_labels(view: &GraphView<'_>, plot_height: u16, color: Color) -> Vec<Span<'static>> {
    if is_y_axis_hidden(&view.options) {
        return Vec::new();
    }
    let y_axis_height = usize::from(plot_height).max(2);
    let style = Style::default().fg(color);
    let display = &view.panel.display;
    let mut labels = vec![Span::raw(""); y_axis_height];
    if let Some(first) = labels.first_mut() {
        *first = Span::styled(display.format_number(view.y_bounds[0]), style);
    }
    if let Some(last) = labels.last_mut() {
        *last = Span::styled(display.format_number(view.y_bounds[1]), style);
    }
    labels
}

/// Returns a nameless Braille dataset, so that the chart draws no legend for it.
fn braille_dataset(data: &[(f64, f64)], graph_type: GraphType, color: Color) -> Dataset<'_> {
    Dataset::default()
        .name("")
        .marker(ratatui::symbols::Marker::Braille)
        .graph_type(graph_type)
        .style(Style::default().fg(color))
        .data(data)
}

/// Builds the series datasets, the strong data mask datasets and the forced point markers.
fn build_datasets<'d>(
    view: &GraphView<'d>,
    cursor_line: &'d [(f64, f64)],
    cursor_color: Color,
) -> SeriesLayers<'d> {
    let options = &view.options;
    let graph_type = graph_type_for_draw_style(options.draw_style);
    let mask_mode = strong_data_mask_mode(&view.events);
    let is_area_filled = options.fill_opacity.unwrap_or(0) > 0
        && options.draw_style == crate::app::GraphDrawStyle::Line;
    let mut layers = SeriesLayers {
        chart: Vec::new(),
        strong: Vec::new(),
        markers: Vec::new(),
    };
    for (s, color) in view.panel.series.iter().zip(&view.colors) {
        let data = if s.visible { s.points.as_slice() } else { &[] };
        let mut dataset = braille_dataset(data, graph_type, *color);
        if is_area_filled || mask_mode == StrongDataMaskMode::AllDrawStyles {
            layers
                .strong
                .push(braille_dataset(data, graph_type, *color));
        }
        if is_area_filled {
            dataset = dataset
                .graph_type(GraphType::Area)
                .fill_to_y(area_fill_baseline(view.y_bounds));
        }
        layers.chart.push(dataset);
        if should_overlay_points(options) {
            layers
                .markers
                .extend(data.iter().map(|(x, y)| (*x, *y, *color)));
        }
    }
    // Add the cursor line when inspecting.
    if !cursor_line.is_empty() {
        layers
            .chart
            .push(braille_dataset(cursor_line, GraphType::Line, cursor_color));
        if !layers.strong.is_empty() || mask_mode == StrongDataMaskMode::AllDrawStyles {
            layers
                .strong
                .push(braille_dataset(cursor_line, GraphType::Line, cursor_color));
        }
    }
    layers
}

/// Renders the series chart and returns the strong data mask, if the graph needs one.
fn render_main_chart(
    frame: &mut Frame,
    chart: &ChartFrame<'_>,
    datasets: Vec<Dataset<'_>>,
    strong_datasets: Vec<Dataset<'_>>,
) -> Option<Buffer> {
    // The chart fills its plot with the style's background, or reset without one.
    let main_chart = chart
        .overlay_chart(datasets)
        .style(Style::default().bg(chart.app.background()));
    frame.render_widget(main_chart, chart.area);
    if strong_datasets.is_empty() {
        None
    } else {
        Some(chart.render_to_buffer(strong_datasets))
    }
}

/// Renders the threshold markers, the autogrid and the intermediate y labels over the chart.
fn render_overlay(frame: &mut Frame, chart: &ChartFrame<'_>, strong_data_buf: Option<&Buffer>) {
    let view = chart.view;
    render_threshold_overlay(frame, chart, strong_data_buf);
    render_raw_threshold_lines(
        frame,
        &chart.app.threshold_marker,
        &view.thresholds,
        view.y_bounds,
        chart.plot,
        strong_data_buf,
    );
    if chart.show_autogrid && chart.plot.top <= chart.plot.bottom {
        render_autogrid(frame, chart);
    }
    if !is_y_axis_hidden(&view.options) {
        render_y_labels(frame, chart);
    }
}

/// Merges the threshold marker datasets onto the blank cells of the chart.
fn render_threshold_overlay(
    frame: &mut Frame,
    chart: &ChartFrame<'_>,
    strong_data_buf: Option<&Buffer>,
) {
    let (view, marker_name) = (chart.view, &chart.app.threshold_marker);
    if marker_name.ends_with("line") {
        return;
    }
    let points = threshold_datasets(&view.thresholds, view.thresholds_dashed, view.x_bounds);
    let (marker, graph_type) = threshold_marker(marker_name);
    let datasets: Vec<_> = view
        .thresholds
        .iter()
        .zip(&points)
        .map(|((_, color), data)| {
            Dataset::default()
                .name("")
                .marker(marker)
                .graph_type(graph_type)
                .style(Style::default().fg(*color))
                .data(data)
        })
        .collect();
    // Data curves keep precedence wherever both map to the same terminal cell.
    if datasets.is_empty() || chart.plot.top > chart.plot.bottom {
        return;
    }
    let threshold_buf = chart.render_to_buffer(datasets);
    if let Some(strong_data_buf) = strong_data_buf {
        merge_overlay_buffer_preserving_data(frame, &threshold_buf, strong_data_buf, chart.plot);
    } else {
        merge_overlay_buffer(frame, &threshold_buf, chart.plot);
    }
}

/// Renders the autogrid lines on the blank plot cells and the time labels below them.
fn render_autogrid(frame: &mut Frame, chart: &ChartFrame<'_>) {
    let (view, plot) = (chart.view, chart.plot);
    let [start, now] = view.x_bounds;
    let plot_width = plot.right.saturating_sub(plot.left);
    let time_ticks = calculate_time_grid_ticks(start, now, plot_width);
    let grid_lines = build_autogrid_datasets(
        view.x_bounds,
        view.y_bounds,
        &time_ticks,
        &chart.value_ticks,
        plot_width,
        chart.plot_height,
    );
    let grid_color = chart.app.grid_color();
    let datasets = grid_lines
        .iter()
        .map(|data| braille_dataset(data, GraphType::Line, grid_color))
        .collect();
    let autogrid_buf = chart.render_to_buffer(datasets);
    // The autogrid is a background layer: it fills only empty plot cells.
    merge_overlay_buffer(frame, &autogrid_buf, plot);
    render_autogrid_time_labels(
        frame,
        plot,
        view.x_bounds,
        &time_ticks,
        now - start,
        grid_color,
    );
}

/// Renders the autogrid and threshold value labels on the y axis.
fn render_y_labels(frame: &mut Frame, chart: &ChartFrame<'_>) {
    let view = chart.view;
    let width = y_label_width(
        &chart.y_labels,
        &chart.value_ticks,
        &view.thresholds,
        &view.panel.display,
    );
    render_intermediate_y_labels(
        frame,
        YLabelArea {
            left: chart.area.left(),
            width,
        },
        chart.plot,
        YLabelContext {
            y_bounds: view.y_bounds,
            autogrid_ticks: &chart.value_ticks,
            threshold_labels: &view.thresholds,
            display: &view.panel.display,
            color: chart.app.grid_color(),
        },
    );
}

/// Renders the details of the active annotation cluster, or else the series legend.
fn render_legend_or_detail(
    frame: &mut Frame,
    area: Rect,
    active_annotation: Option<&AnnotationCluster<'_>>,
    view: &GraphView<'_>,
    theme: &Theme,
) {
    if let Some(cluster) = active_annotation {
        let [heading, details] = format_cluster_detail_lines(cluster, usize::from(area.width));
        let detail_style = Style::default().fg(theme.annotation);
        let annotation_detail = Paragraph::new(vec![
            Line::styled(heading, detail_style),
            Line::styled(details, detail_style),
        ]);
        frame.render_widget(annotation_detail, area);
        return;
    }
    let legend_items: Vec<Span<'_>> = view
        .legend_labels(LegendMode::AllSeries)
        .into_iter()
        .zip(&view.colors)
        .flat_map(|((_, label), color)| {
            [
                Span::styled("■ ".to_string(), Style::default().fg(*color)),
                Span::styled(format!("{}  ", label), Style::default().fg(theme.text)),
            ]
        })
        .collect();
    let legend = Paragraph::new(Line::from(legend_items)).wrap(Wrap { trim: true });
    frame.render_widget(legend, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        GraphAxisPlacement, GraphDrawStyle, GraphOptions, GraphPointMode, GraphStackingMode,
        PanelOptions, PanelType, QueryMode, SeriesView, YAxisMode,
    };
    use crate::export::ExportOptions;
    use crate::theme::Theme;
    use ratatui::{Terminal, backend::TestBackend};
    use std::time::Duration;

    #[test]
    fn test_graph_type_for_draw_style() {
        assert_eq!(
            graph_type_for_draw_style(GraphDrawStyle::Line),
            GraphType::Line
        );
        assert_eq!(
            graph_type_for_draw_style(GraphDrawStyle::Points),
            GraphType::Scatter
        );
        assert_eq!(
            graph_type_for_draw_style(GraphDrawStyle::Bars),
            GraphType::Bar
        );
    }

    #[test]
    fn test_should_overlay_points() {
        let mut options = GraphOptions {
            draw_style: GraphDrawStyle::Line,
            show_points: GraphPointMode::Auto,
            ..GraphOptions::default()
        };
        assert!(!should_overlay_points(&options));

        options.show_points = GraphPointMode::Always;
        assert!(should_overlay_points(&options));

        options.draw_style = GraphDrawStyle::Points;
        assert!(!should_overlay_points(&options));

        options.draw_style = GraphDrawStyle::Bars;
        options.show_points = GraphPointMode::Always;
        assert!(should_overlay_points(&options));
    }

    #[test]
    fn test_area_fill_baseline_prefers_zero_when_visible() {
        assert_eq!(area_fill_baseline([-10.0, 20.0]), 0.0);
        assert_eq!(area_fill_baseline([5.0, 20.0]), 5.0);
        assert_eq!(area_fill_baseline([-20.0, -5.0]), -20.0);
    }

    #[test]
    fn test_hidden_axis_flag() {
        let visible = GraphOptions::default();
        assert!(!is_y_axis_hidden(&visible));

        let hidden = GraphOptions {
            axis_placement: GraphAxisPlacement::Hidden,
            draw_style: GraphDrawStyle::Line,
            show_points: GraphPointMode::Auto,
            fill_opacity: None,
            line_interpolation: Some("smooth".to_string()),
            stacking: GraphStackingMode::Normal,
        };
        assert!(is_y_axis_hidden(&hidden));
    }

    #[test]
    fn comprehensive_strong_data_mask_requires_visible_routed_annotations() {
        let mut disabled = area_fill_app(area_fill_panel());
        disabled.annotations = crate::annotations::AnnotationState::from_path(None);
        let disabled_events = routed_annotation_events(&disabled);
        assert!(disabled_events.is_empty());
        assert_eq!(
            strong_data_mask_mode(&disabled_events),
            StrongDataMaskMode::AreaFillOnly
        );

        let mut hidden = area_fill_app(area_fill_panel());
        hidden.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(50.0, "hidden"),
        ]);
        hidden.annotations.toggle_visibility();
        let hidden_events = routed_annotation_events(&hidden);
        assert!(hidden_events.is_empty());
        assert_eq!(
            strong_data_mask_mode(&hidden_events),
            StrongDataMaskMode::AreaFillOnly
        );

        let mut outside = area_fill_app(area_fill_panel());
        outside.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(101.0, "outside"),
        ]);
        let outside_events = routed_annotation_events(&outside);
        assert!(outside_events.is_empty());
        assert_eq!(
            strong_data_mask_mode(&outside_events),
            StrongDataMaskMode::AreaFillOnly
        );

        let mut visible = area_fill_app(area_fill_panel());
        visible.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(50.0, "visible"),
        ]);
        let visible_events = routed_annotation_events(&visible);
        assert_eq!(
            strong_data_mask_mode(&visible_events),
            StrongDataMaskMode::AllDrawStyles
        );
    }

    #[test]
    fn test_chart_plot_left_visible_axis_uses_y_label_width_when_dominant() {
        let chart_area = Rect::new(10, 0, 90, 20);
        let x_labels = vec![Span::raw("abc")];

        assert_eq!(chart_plot_left(chart_area, 6, &x_labels, true), 17);
    }

    #[test]
    fn test_chart_plot_left_visible_axis_uses_first_x_label_when_dominant() {
        let chart_area = Rect::new(10, 0, 90, 20);
        let x_labels = vec![Span::raw("long-start-label")];

        assert_eq!(chart_plot_left(chart_area, 6, &x_labels, true), 26);
    }

    #[test]
    fn test_chart_plot_left_visible_axis_clamps_gutter() {
        let chart_area = Rect::new(10, 0, 30, 20);
        let x_labels = vec![Span::raw("very-long-start-label")];

        assert_eq!(chart_plot_left(chart_area, 2, &x_labels, true), 21);
    }

    #[test]
    fn test_chart_plot_left_hidden_axis_uses_first_x_label_gutter() {
        let chart_area = Rect::new(10, 0, 90, 20);
        let x_labels = vec![Span::raw("long-start-label")];

        assert_eq!(chart_plot_left(chart_area, 0, &x_labels, false), 26);
    }

    #[test]
    fn test_chart_plot_left_hidden_axis_clamps_first_x_label_gutter() {
        let chart_area = Rect::new(10, 0, 30, 20);
        let x_labels = vec![Span::raw("very-long-start-label")];

        assert_eq!(chart_plot_left(chart_area, 0, &x_labels, false), 20);
    }

    #[test]
    fn test_chart_plot_left_ignores_custom_label_width() {
        let chart_area = Rect::new(10, 0, 90, 20);
        let x_labels = vec![Span::raw("abc")];
        let chart_y_labels = vec![Span::raw("0"), Span::raw("9")];
        let custom_label_width = 30;
        let chart_y_label_width = chart_y_label_width(&chart_y_labels);

        assert_eq!(
            chart_plot_left(
                chart_area,
                chart_y_label_width,
                &x_labels,
                !chart_y_labels.is_empty(),
            ),
            13
        );
        assert_ne!(
            chart_plot_left(
                chart_area,
                custom_label_width,
                &x_labels,
                !chart_y_labels.is_empty(),
            ),
            13
        );
    }

    fn area_fill_panel() -> PanelState {
        PanelState {
            title: "area".to_string(),
            exprs: vec![],
            legends: vec![],
            query_modes: vec![QueryMode::Range],
            series: vec![SeriesView {
                name: "filled".to_string(),
                value: Some(8.0),
                points: vec![(0.0, 8.0), (50.0, 8.0), (100.0, 8.0)],
                visible: true,
            }],
            last_error: None,
            last_url: None,
            last_samples: 3,
            grid: None,
            y_axis_mode: YAxisMode::Auto,
            panel_type: PanelType::Graph,
            thresholds: None,
            min: Some(0.0),
            max: Some(10.0),
            autogrid: Some(true),
            display: crate::ui::DisplayFormat::default(),
            options: PanelOptions::Graph(GraphOptions {
                draw_style: GraphDrawStyle::Line,
                show_points: GraphPointMode::Never,
                fill_opacity: Some(30),
                axis_placement: GraphAxisPlacement::Visible,
                line_interpolation: None,
                stacking: GraphStackingMode::Off,
            }),
            resolution: Default::default(),
            notices: Default::default(),
        }
    }

    fn area_fill_app(panel: PanelState) -> AppState {
        let mut app = AppState::new(
            crate::prom::PromClient::new("http://localhost:9090".to_string()),
            Duration::from_secs(100),
            Duration::from_secs(5),
            Duration::from_secs(1),
            "test".to_string(),
            vec![panel],
            0,
            Theme::default(),
            "dashed-line".to_string(),
            ExportOptions::default(),
        );
        app.view_end_ts = 100;
        app.autogrid_color = Some(Color::Red);
        app
    }

    fn routed_annotation_events(app: &AppState) -> Vec<&crate::annotations::AnnotationEvent> {
        let (start, end) = app.time_bounds();
        app.annotations.events_for_panel(
            crate::annotations::AnnotationPanelContext {
                index: 0,
                title: &app.panels[0].title,
            },
            [start, end],
        )
    }

    #[test]
    fn test_area_fill_keeps_precedence_over_autogrid() {
        let app = area_fill_app(area_fill_panel());
        let panel = &app.panels[0];
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();

        terminal
            .draw(|frame| {
                render_graph_panel(frame, Rect::new(0, 0, 80, 20), 0, panel, &app, None, false);
            })
            .unwrap();

        let y_bounds = calculate_y_bounds(panel);
        let chart_area = Rect::new(0, 0, 80, 18);
        let x_labels = vec![Span::raw("00:00:00"), Span::raw("00:01:40")];
        let chart_y_labels = vec![Span::raw("0"), Span::raw("10")];
        let plot = PlotBounds {
            left: chart_plot_left(
                chart_area,
                chart_y_label_width(&chart_y_labels),
                &x_labels,
                true,
            ),
            right: chart_area.right(),
            top: chart_area.top(),
            bottom: chart_area.bottom().saturating_sub(2),
        };
        let plot_height = plot.bottom.saturating_sub(plot.top) as f64;
        let grid_ratio = (5.0 - y_bounds[0]) / (y_bounds[1] - y_bounds[0]);
        let grid_y = plot
            .bottom
            .saturating_sub((grid_ratio * plot_height).round() as u16);

        let grid_colored_cells_inside_fill = (plot.left..plot.right)
            .filter(|x| {
                terminal
                    .backend()
                    .buffer()
                    .cell((*x, grid_y))
                    .is_some_and(|cell| cell.style().fg == Some(Color::Red))
            })
            .count();

        assert_eq!(grid_colored_cells_inside_fill, 0);
    }

    #[test]
    fn overlays_keep_the_painted_background_and_use_theme_roles() {
        for marker in ["dashed-line", "dot"] {
            let mut panel = area_fill_panel();
            panel.thresholds = Some(crate::app::Thresholds {
                mode: crate::app::ThresholdMode::Absolute,
                steps: vec![crate::app::ThresholdStep {
                    value: Some(5.0),
                    color: Color::Reset,
                }],
                style: None,
            });
            let mut app = area_fill_app(panel);
            app.theme = crate::theme::builtin("solarized-light").unwrap();
            app.autogrid_color = None;
            app.threshold_marker = marker.to_string();
            let background = app.theme.background;
            let panel = &app.panels[0];
            let area = Rect::new(0, 0, 80, 20);
            let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();

            terminal
                .draw(|frame| {
                    frame.render_widget(
                        ratatui::widgets::Block::default().style(Style::default().bg(background)),
                        area,
                    );
                    render_graph_panel(frame, area, 0, panel, &app, Some(50.0), false);
                })
                .unwrap();

            let cells = terminal.backend().buffer().content();
            assert!(
                cells.iter().all(|cell| cell.bg == background),
                "{marker}: an overlay reset the background"
            );
            for (role, color) in [
                ("cursor", app.theme.cursor),
                ("axis", app.theme.axis),
                ("grid", app.theme.grid),
                ("threshold_default", app.theme.threshold_default),
            ] {
                assert!(
                    cells.iter().any(|cell| cell.fg == color),
                    "{marker}: no {role} cell"
                );
            }
        }
    }

    #[test]
    fn test_line_forced_points_are_visible_and_use_line_marker_cells() {
        let mut panel = area_fill_panel();
        panel.series[0].points = vec![(25.0, 2.0), (50.0, 8.0), (75.0, 4.0)];
        panel.options = PanelOptions::Graph(GraphOptions {
            draw_style: GraphDrawStyle::Line,
            show_points: GraphPointMode::Always,
            fill_opacity: None,
            axis_placement: GraphAxisPlacement::Visible,
            line_interpolation: None,
            stacking: GraphStackingMode::Off,
        });
        let app = area_fill_app(panel);
        let panel = &app.panels[0];
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();

        terminal
            .draw(|frame| {
                render_graph_panel(frame, Rect::new(0, 0, 80, 20), 0, panel, &app, None, false);
            })
            .unwrap();

        let y_bounds = calculate_y_bounds(panel);
        let chart_area = Rect::new(0, 0, 80, 18);
        let x_labels = vec![Span::raw("00:00:00"), Span::raw("00:01:40")];
        let chart_y_labels = vec![Span::raw("0"), Span::raw("10")];
        let plot = PlotBounds {
            left: chart_plot_left(
                chart_area,
                chart_y_label_width(&chart_y_labels),
                &x_labels,
                true,
            ),
            right: chart_area.right(),
            top: chart_area.top(),
            bottom: chart_area.bottom().saturating_sub(2),
        };
        let expected_cells: std::collections::HashSet<_> = panel.series[0]
            .points
            .iter()
            .filter_map(|(x, y)| point_to_braille_cell(*x, *y, [0.0, 100.0], y_bounds, plot))
            .collect();
        let visible_point_cells: std::collections::HashSet<_> = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .enumerate()
            .filter_map(|(index, cell)| {
                (cell.symbol() == "•").then_some(((index % 80) as u16, (index / 80) as u16))
            })
            .collect();

        assert!(!visible_point_cells.is_empty());
        assert_eq!(visible_point_cells, expected_cells);
    }

    #[test]
    fn targeted_annotation_renders_only_on_matching_title() {
        let mut app = area_fill_app(area_fill_panel());
        app.panels[0].title = "CPU".to_string();
        let mut deploy = crate::annotations::test_event_at(50.0, "deploy");
        deploy.target = crate::annotations::AnnotationTarget::PanelTitles(
            ["CPU".to_string()].into_iter().collect(),
        );
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![deploy]);
        let mut memory_panel = app.panels[0].clone();
        memory_panel.title = "Memory".to_string();

        let mut cpu = Terminal::new(TestBackend::new(80, 20)).unwrap();
        cpu.draw(|frame| {
            render_graph_panel(
                frame,
                Rect::new(0, 0, 80, 20),
                0,
                &app.panels[0],
                &app,
                None,
                false,
            );
        })
        .unwrap();
        let mut memory = Terminal::new(TestBackend::new(80, 20)).unwrap();
        memory
            .draw(|frame| {
                render_graph_panel(
                    frame,
                    Rect::new(0, 0, 80, 20),
                    1,
                    &memory_panel,
                    &app,
                    None,
                    false,
                );
            })
            .unwrap();

        let marker_count = |terminal: &Terminal<TestBackend>| {
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .filter(|cell| cell.symbol() == "┊")
                .count()
        };
        assert!(marker_count(&cpu) > 0);
        assert_eq!(marker_count(&memory), 0);
    }

    #[test]
    fn unknown_panel_preserves_graph_data_but_omits_annotation_markers() {
        let mut app = area_fill_app(area_fill_panel());
        app.panels[0].panel_type = PanelType::Unknown;
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(50.0, "deploy"),
        ]);
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        let mut rendered_cluster = None;

        terminal
            .draw(|frame| {
                rendered_cluster = crate::ui::panels::render_panel(
                    frame,
                    Rect::new(0, 0, 80, 20),
                    0,
                    &app.panels[0],
                    &app,
                    true,
                    None,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(
            buffer
                .content()
                .iter()
                .any(|cell| cell.style().fg == Some(app.theme.palette[0])),
            "unknown panels must retain legacy graph data rendering"
        );
        assert_eq!(
            buffer
                .content()
                .iter()
                .filter(|cell| cell.symbol() == "┊")
                .count(),
            0,
            "unknown panels must not render annotation markers"
        );
        assert!(rendered_cluster.is_none());
    }

    #[test]
    fn tag_filter_recalculates_same_column_count() {
        let mut app = area_fill_app(area_fill_panel());
        let mut deploy = crate::annotations::test_event_at(50.0, "deploy");
        deploy.tags = vec!["deploy".to_string()];
        let mut incident = crate::annotations::test_event_at(50.0, "incident");
        incident.tags = vec!["incident".to_string()];
        let mut rollback = crate::annotations::test_event_at(50.0, "rollback");
        rollback.tags = vec!["deploy".to_string()];
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            deploy, incident, rollback,
        ]);
        app.annotations
            .set_filter(crate::annotations::TagFilter::from_selected([
                "deploy".to_string()
            ]));
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();

        terminal
            .draw(|frame| {
                render_graph_panel(
                    frame,
                    Rect::new(0, 0, 80, 20),
                    0,
                    &app.panels[0],
                    &app,
                    None,
                    false,
                );
            })
            .unwrap();

        let chart_area = Rect::new(0, 0, 80, 18);
        let x_labels = vec![Span::raw("00:00:00"), Span::raw("00:01:40")];
        let chart_y_labels = vec![Span::raw("0"), Span::raw("10")];
        let plot = PlotBounds {
            left: chart_plot_left(
                chart_area,
                chart_y_label_width(&chart_y_labels),
                &x_labels,
                true,
            ),
            right: chart_area.right(),
            top: chart_area.top(),
            bottom: chart_area.bottom().saturating_sub(2),
        };
        let marker_x = labels::value_to_plot_x(50.0, [0.0, 100.0], plot).unwrap();

        assert_eq!(
            terminal
                .backend()
                .buffer()
                .cell((marker_x, plot.top))
                .unwrap()
                .symbol(),
            "2"
        );
    }

    #[test]
    fn annotation_cluster_renders_and_inspects_on_same_column() {
        let mut app = area_fill_app(area_fill_panel());
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(50.0, "deploy"),
            crate::annotations::test_event_at(50.1, "rollback"),
            crate::annotations::test_event_at(50.2, "resolved"),
        ]);
        app.cursor_x = Some(50.0);
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();

        terminal
            .draw(|frame| {
                render_graph_panel(
                    frame,
                    Rect::new(0, 0, 80, 20),
                    0,
                    &app.panels[0],
                    &app,
                    app.cursor_x,
                    false,
                );
            })
            .unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("3 events near"));
        assert!(rendered.contains("deploy"));
    }

    #[test]
    fn annotation_replaces_vertical_autogrid_but_preserves_standard_line_data() {
        let mut panel = area_fill_panel();
        panel.series[0].points = vec![(0.0, 8.0), (60.0, 8.0), (100.0, 8.0)];
        panel.options = PanelOptions::Graph(GraphOptions {
            draw_style: GraphDrawStyle::Line,
            show_points: GraphPointMode::Never,
            fill_opacity: None,
            axis_placement: GraphAxisPlacement::Visible,
            line_interpolation: None,
            stacking: GraphStackingMode::Off,
        });
        let mut app = area_fill_app(panel);
        let mut baseline = Terminal::new(TestBackend::new(80, 20)).unwrap();
        baseline
            .draw(|frame| {
                render_graph_panel(
                    frame,
                    Rect::new(0, 0, 80, 20),
                    0,
                    &app.panels[0],
                    &app,
                    None,
                    false,
                );
            })
            .unwrap();

        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(60.0, "deploy"),
        ]);
        let mut annotated = Terminal::new(TestBackend::new(80, 20)).unwrap();
        annotated
            .draw(|frame| {
                render_graph_panel(
                    frame,
                    Rect::new(0, 0, 80, 20),
                    0,
                    &app.panels[0],
                    &app,
                    None,
                    false,
                );
            })
            .unwrap();

        let panel = &app.panels[0];
        let y_bounds = calculate_y_bounds(panel);
        let chart_area = Rect::new(0, 0, 80, 18);
        let x_labels = vec![Span::raw("00:00:00"), Span::raw("00:01:40")];
        let chart_y_labels = vec![Span::raw("0"), Span::raw("10")];
        let plot = PlotBounds {
            left: chart_plot_left(
                chart_area,
                chart_y_label_width(&chart_y_labels),
                &x_labels,
                true,
            ),
            right: chart_area.right(),
            top: chart_area.top(),
            bottom: chart_area.bottom().saturating_sub(2),
        };
        let marker_x = labels::value_to_plot_x(60.0, [0.0, 100.0], plot).unwrap();
        let strong_cell = point_to_braille_cell(60.0, 8.0, [0.0, 100.0], y_bounds, plot).unwrap();
        assert_eq!(
            baseline
                .backend()
                .buffer()
                .cell(strong_cell)
                .unwrap()
                .style()
                .fg,
            Some(app.theme.palette[0]),
            "the collision cell must contain standard-line series data"
        );

        assert_eq!(
            annotated
                .backend()
                .buffer()
                .cell((marker_x, plot.top))
                .unwrap()
                .symbol(),
            "•"
        );
        assert!(
            (plot.top.saturating_add(1)..=plot.bottom).any(|y| {
                annotated
                    .backend()
                    .buffer()
                    .cell((marker_x, y))
                    .is_some_and(|cell| cell.symbol() == "┊")
            }),
            "annotation marker should replace vertical autogrid cells"
        );
        assert_eq!(
            annotated.backend().buffer().cell(strong_cell),
            baseline.backend().buffer().cell(strong_cell),
            "the standard line cell must remain visually dominant"
        );
    }

    #[test]
    fn annotation_details_keep_one_logical_line_per_reserved_row() {
        let mut app = area_fill_app(area_fill_panel());
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(vec![
            crate::annotations::test_event_at(50.0, "deploy"),
            crate::annotations::test_event_at(50.1, "rollback"),
            crate::annotations::test_event_at(50.2, "resolved"),
        ]);
        app.cursor_x = Some(50.0);
        let mut terminal = Terminal::new(TestBackend::new(30, 12)).unwrap();

        terminal
            .draw(|frame| {
                render_graph_panel(
                    frame,
                    Rect::new(0, 0, 30, 12),
                    0,
                    &app.panels[0],
                    &app,
                    app.cursor_x,
                    false,
                );
            })
            .unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("3 events near"));
        assert!(rendered.contains("deploy"));
    }

    #[test]
    fn annotation_details_are_bounded_by_terminal_legend_width() {
        let mut app = area_fill_app(area_fill_panel());
        app.annotations = crate::annotations::AnnotationState::from_events_for_test(
            (0..100)
                .map(|index| {
                    crate::annotations::test_event_at(
                        50.0,
                        &format!("event-{index:03}-{}", "x".repeat(80)),
                    )
                })
                .collect(),
        );
        app.cursor_x = Some(50.0);
        let width = 36;
        let height = 12;
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();

        terminal
            .draw(|frame| {
                render_graph_panel(
                    frame,
                    Rect::new(0, 0, width, height),
                    0,
                    &app.panels[0],
                    &app,
                    app.cursor_x,
                    false,
                );
            })
            .unwrap();

        let row_text = |y| {
            (0..width)
                .filter_map(|x| terminal.backend().buffer().cell((x, y)))
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        };
        let heading = row_text(height - 2);
        let details = row_text(height - 1);

        assert!(heading.starts_with("100 events near"));
        assert!(heading.ends_with('…'));
        assert!(details.starts_with("event-000-"));
        assert!(details.ends_with('…'));
        assert!(!details.contains("event-001-"));
        assert!(heading.chars().count() <= usize::from(width));
        assert!(details.chars().count() <= usize::from(width));
    }
}
