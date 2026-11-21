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

use crate::app::{AppState, PanelState, PanelType};
use crate::num;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Sparkline},
};

pub(super) fn render_stat(frame: &mut Frame, area: Rect, p: &PanelState, app: &AppState) {
    debug_assert!(
        matches!(p.panel_type, PanelType::Stat),
        "the panel is a stat"
    );
    let theme = &app.theme;

    let visible_series = p.series.iter().find(|s| s.visible);
    let value = visible_series.and_then(|s| s.value);
    let color = value
        .and_then(|value| p.get_color_for_value(value))
        .unwrap_or(theme.palette_color(0));

    // Split area into value (top) and sparkline (bottom)
    let [value_area, sparkline_area] =
        Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);
    debug_assert!(
        value_area.height + sparkline_area.height <= area.height,
        "the value and the sparkline share the panel height"
    );

    // Grafana's noValue stands in for a null value and for no series at all.
    let val_str = visible_series
        .map(|_| p.display.format_value(value))
        .unwrap_or_else(|| p.display.no_data_text().to_string());
    let big_value = Paragraph::new(val_str)
        .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::NONE));

    frame.render_widget(big_value, value_area);

    // Render Sparkline
    if let Some(s) = visible_series {
        let data = sparkline_heights(&s.points);
        let sparkline = Sparkline::default()
            .block(Block::default().borders(Borders::NONE))
            .data(&data)
            .max(SPARKLINE_MAX)
            .style(Style::default().fg(color));
        frame.render_widget(sparkline, sparkline_area);
    }
}

const SPARKLINE_MAX: u64 = 100;

/// Bar heights for a sparkline, scaled between the series' minimum and
/// maximum. `Sparkline` takes unsigned integers. A direct cast of the values
/// flattens fractions and negatives.
fn sparkline_heights(points: &[(f64, f64)]) -> Vec<u64> {
    let (min, max) = points
        .iter()
        .map(|(_, value)| *value)
        .filter(|value| value.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), value| {
            (min.min(value), max.max(value))
        });
    let heights: Vec<u64> = points
        .iter()
        .map(|(_, value)| {
            if !value.is_finite() {
                0
            } else if max > min {
                // The lowest point keeps a sliver, so the line stays visible.
                let ratio = (value - min) / (max - min);
                1 + num::f64_to_u64((ratio * num::u64_to_f64(SPARKLINE_MAX - 1)).round())
            } else {
                SPARKLINE_MAX / 2
            }
        })
        .collect();
    debug_assert!(heights.len() == points.len(), "each point has one bar");
    debug_assert!(
        heights.iter().all(|height| *height <= SPARKLINE_MAX),
        "each bar is SPARKLINE_MAX high or less"
    );
    heights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparklines_scale_fractional_and_negative_values() {
        let points = [(0.0, -0.5), (1.0, 0.25), (2.0, 1.0)];
        assert_eq!(sparkline_heights(&points), [1, 51, 100]);
    }

    #[test]
    fn flat_sparklines_sit_in_the_middle() {
        let points = [(0.0, 0.3), (1.0, 0.3)];
        assert_eq!(sparkline_heights(&points), [50, 50]);
        assert!(sparkline_heights(&[]).is_empty());
    }
}
