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

use super::labels::{PlotBounds, value_to_plot_x};
use super::overlay::{is_blank_cell, overlay_cell_if_blank_or_weak_area_fill};
use crate::annotations::{AnnotationCluster, AnnotationEvent, cluster_projected_events};
use ratatui::prelude::*;

/// The marker for a cluster of `count` annotations: a dot, a digit, or `+` for 10 or more.
pub(crate) fn cluster_badge(count: usize) -> char {
    let badge = match count {
        0 => ' ',
        1 => '•',
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        9 => '9',
        _ => '+',
    };
    debug_assert!(
        count == 0 || !badge.is_whitespace(),
        "a cluster with events has a visible badge"
    );
    debug_assert!(
        !(2..=9).contains(&count) || badge.to_digit(10) == u32::try_from(count).ok(),
        "a digit badge shows the event count"
    );
    badge
}

pub(super) fn terminal_clusters<'a>(
    events: Vec<&'a AnnotationEvent>,
    x_bounds: [f64; 2],
    plot: PlotBounds,
) -> Vec<AnnotationCluster<'a>> {
    cluster_projected_events(events.into_iter().map(|event| {
        let x = value_to_plot_x(event.timestamp_secs(), x_bounds, plot);
        (x.map(u32::from), event)
    }))
}

pub(super) fn render_annotation_clusters(
    frame: &mut Frame,
    clusters: &[AnnotationCluster<'_>],
    plot: PlotBounds,
    strong_data: Option<&ratatui::buffer::Buffer>,
    color: Color,
) {
    debug_assert!(
        clusters.iter().all(|cluster| !cluster.events.is_empty()),
        "each cluster has an event"
    );
    debug_assert!(
        clusters
            .iter()
            .zip(clusters.iter().skip(1))
            .all(|(a, b)| a.coordinate < b.coordinate),
        "the clusters are in increasing column order"
    );
    for cluster in clusters {
        let Ok(x) = u16::try_from(cluster.coordinate) else {
            continue;
        };
        if x < plot.left || x >= plot.right {
            continue;
        }

        render_marker_cell(
            frame,
            strong_data,
            x,
            plot.top,
            cluster_badge(cluster.events.len()),
            color,
        );
        for y in plot.top.saturating_add(1)..=plot.bottom {
            render_marker_cell(frame, strong_data, x, y, '┊', color);
        }
    }
}

fn render_marker_cell(
    frame: &mut Frame,
    strong_data: Option<&ratatui::buffer::Buffer>,
    x: u16,
    y: u16,
    marker: char,
    color: Color,
) {
    debug_assert!(
        frame.area().contains(Position::new(x, y)),
        "the frame has each marker cell"
    );
    debug_assert!(
        strong_data.is_none_or(|buffer| buffer.cell((x, y)).is_some()),
        "the data mask covers each marker cell"
    );
    let Some(destination) = frame.buffer_mut().cell_mut((x, y)) else {
        return;
    };
    let empty_strong = ratatui::buffer::Cell::default();
    let strong = strong_data
        .and_then(|buffer| buffer.cell((x, y)))
        .unwrap_or(&empty_strong);
    overlay_marker_cell(destination, strong, marker, color);
}

fn overlay_marker_cell(
    destination: &mut ratatui::buffer::Cell,
    strong_data: &ratatui::buffer::Cell,
    marker: char,
    color: Color,
) {
    debug_assert!(!marker.is_whitespace(), "the marker is visible");
    let mut source = ratatui::buffer::Cell::default();
    source
        .set_char(marker)
        .set_style(Style::default().fg(color));
    overlay_cell_if_blank_or_weak_area_fill(destination, &source, strong_data);
    debug_assert!(
        !is_blank_cell(destination),
        "the cell is not blank after the marker overlay"
    );
}

pub(super) fn active_cluster<'a>(
    clusters: &'a [AnnotationCluster<'a>],
    cursor_x: Option<f64>,
    x_bounds: [f64; 2],
    plot: PlotBounds,
) -> Option<&'a AnnotationCluster<'a>> {
    let cursor_column = value_to_plot_x(cursor_x?, x_bounds, plot)?;
    clusters
        .iter()
        .find(|cluster| cluster.coordinate == u32::from(cursor_column))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn badge_matches_cluster_size() {
        assert_eq!(cluster_badge(1), '•');
        assert_eq!(cluster_badge(2), '2');
        assert_eq!(cluster_badge(9), '9');
        assert_eq!(cluster_badge(10), '+');
        let digits: String = (2..=9).map(cluster_badge).collect();
        assert_eq!(digits, "23456789");
    }

    #[test]
    fn marker_does_not_replace_strong_data_cell() {
        let mut destination = ratatui::buffer::Cell::default();
        destination.set_char('x');
        let strong = ratatui::buffer::Cell::default();

        overlay_marker_cell(&mut destination, &strong, '┊', Color::Yellow);

        assert_eq!(destination.symbol(), "x");
    }
}
