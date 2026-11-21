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

use super::view::threshold_steps;
use crate::app::{PanelState, YAxisMode};

pub(crate) fn calculate_y_bounds(p: &PanelState) -> [f64; 2] {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut has_data = false;

    for s in &p.series {
        if !s.visible {
            continue;
        }
        for &(_, v) in &s.points {
            observe_value(v, &mut min, &mut max, &mut has_data);
        }
    }

    observe_thresholds(p, &mut min, &mut max, &mut has_data);

    let explicit_min = p.min.filter(|value| value.is_finite());
    let explicit_max = p.max.filter(|value| value.is_finite());

    if !has_data {
        return fallback_bounds(explicit_min, explicit_max);
    }

    let [min, max] = adjusted_range(p, [min, max], explicit_min, explicit_max);
    if max <= min {
        return fallback_bounds(Some(min), explicit_max.filter(|value| *value > min));
    }

    // Add 5% of the range as padding. Halves keep the range finite for data
    // near f64::MAX, and the clamps keep each bound finite.
    let padding = (max / 2.0 - min / 2.0) * 0.1;
    let bounds = [
        if explicit_min.is_some() {
            min
        } else {
            (min - padding).max(f64::MIN)
        },
        if explicit_max.is_some() {
            max
        } else {
            (max + padding).min(f64::MAX)
        },
    ];
    let [lower, upper] = bounds;
    debug_assert!(
        lower <= upper,
        "the lower bound is a number and not above the upper bound"
    );
    debug_assert!(
        lower.is_finite() && upper.is_finite(),
        "the bounds are finite"
    );
    debug_assert!(
        explicit_min.is_none_or(|value| value.to_bits() == lower.to_bits()),
        "an explicit minimum stays the lower bound"
    );
    bounds
}

/// Widens a flat data range, applies the zero-based mode and applies the explicit limits.
fn adjusted_range(
    p: &PanelState,
    [mut min, mut max]: [f64; 2],
    explicit_min: Option<f64>,
    explicit_max: Option<f64>,
) -> [f64; 2] {
    if min == max {
        min -= 1.0;
        max += 1.0;
    }

    if p.y_axis_mode == YAxisMode::ZeroBased {
        if min > 0.0 {
            min = 0.0;
        } else if max < 0.0 {
            max = 0.0;
        }
    }

    if let Some(value) = explicit_min {
        min = value;
    }
    if let Some(value) = explicit_max {
        max = value;
    }
    [min, max]
}

fn observe_thresholds(p: &PanelState, min: &mut f64, max: &mut f64, has_data: &mut bool) {
    for (value, _) in threshold_steps(p) {
        observe_value(value, min, max, has_data);
    }
}

fn observe_value(value: f64, min: &mut f64, max: &mut f64, has_data: &mut bool) {
    if !value.is_finite() {
        return;
    }

    *min = min.min(value);
    *max = max.max(value);
    *has_data = true;
    debug_assert!(
        *min <= value && value <= *max,
        "the observed range holds the value"
    );
    debug_assert!(
        min.is_finite() && max.is_finite(),
        "the observed range is finite"
    );
}

fn fallback_bounds(explicit_min: Option<f64>, explicit_max: Option<f64>) -> [f64; 2] {
    debug_assert!(
        explicit_min.is_none_or(f64::is_finite) && explicit_max.is_none_or(f64::is_finite),
        "the explicit limits are finite"
    );
    let bounds = match (explicit_min, explicit_max) {
        (Some(min), Some(max)) if max > min => [min, max],
        (Some(min), _) => [min, min + 1.0],
        (_, Some(max)) => [max - 1.0, max],
        _ => [0.0, 1.0],
    };
    let [lower, upper] = bounds;
    debug_assert!(
        lower.is_finite() && upper.is_finite() && lower <= upper,
        "the fallback bounds are finite and in order"
    );
    bounds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_near_the_float_limits_gives_finite_bounds() {
        let mut panel = create_test_panel();
        panel.series = vec![crate::app::SeriesView {
            name: "edge".to_string(),
            value: None,
            points: vec![(0.0, -f64::MAX), (1.0, f64::MAX)],
            visible: true,
        }];

        let [lower, upper] = calculate_y_bounds(&panel);

        assert_eq!([lower, upper], [f64::MIN, f64::MAX]);
    }
    use crate::app::{
        GraphOptions, PanelOptions, SeriesView, ThresholdMode, ThresholdStep, Thresholds,
    };
    use ratatui::style::Color;

    fn create_test_panel() -> PanelState {
        PanelState {
            title: "test".to_string(),
            exprs: vec![],
            legends: vec![],
            query_modes: vec![],
            series: vec![],
            last_error: None,
            last_url: None,
            last_samples: 0,
            grid: None,
            y_axis_mode: YAxisMode::Auto,
            panel_type: crate::app::PanelType::Graph,
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

    #[test]
    fn test_calculate_y_bounds_basic() {
        let mut p = create_test_panel();
        p.series.push(SeriesView {
            name: "test".to_string(),
            value: None,
            points: vec![(0.0, 10.0), (1.0, 20.0)],
            visible: true,
        });

        let bounds = calculate_y_bounds(&p);
        assert!(bounds[0] < 10.0);
        assert!(bounds[1] > 20.0);
    }

    #[test]
    fn test_calculate_y_bounds_nan() {
        let mut p = create_test_panel();
        p.series.push(SeriesView {
            name: "test".to_string(),
            value: None,
            points: vec![(0.0, 10.0), (1.0, f64::NAN), (2.0, 20.0)],
            visible: true,
        });

        let bounds = calculate_y_bounds(&p);
        assert!(bounds[0] < 10.0); // The bounds ignore NAN
        assert!(bounds[1] > 20.0);
    }

    #[test]
    fn test_calculate_y_bounds_infinity() {
        let mut p = create_test_panel();
        p.series.push(SeriesView {
            name: "test".to_string(),
            value: None,
            points: vec![(0.0, 10.0), (1.0, f64::INFINITY), (2.0, 20.0)],
            visible: true,
        });

        let bounds = calculate_y_bounds(&p);
        assert!(bounds[0] < 10.0); // The bounds ignore INFINITY
        assert!(bounds[1] > 20.0);
    }

    #[test]
    fn test_calculate_y_bounds_zero_based() {
        let mut p = create_test_panel();
        p.y_axis_mode = YAxisMode::ZeroBased;
        p.series.push(SeriesView {
            name: "test".to_string(),
            value: None,
            points: vec![(0.0, 10.0), (1.0, 20.0)],
            visible: true,
        });

        let bounds = calculate_y_bounds(&p);
        // Range is 0.0 to 20.0. Padding is 5% of 20.0 = 1.0.
        // So min is 0.0 - 1.0 = -1.0.
        assert_eq!(bounds[0], -1.0);
        assert!(bounds[1] > 20.0);
    }

    #[test]
    fn test_calculate_y_bounds_respects_explicit_min() {
        let mut p = create_test_panel();
        p.min = Some(0.0);
        p.series.push(SeriesView {
            name: "requests".to_string(),
            value: None,
            points: vec![(0.0, 4.5), (1.0, 11_200.0)],
            visible: true,
        });

        let bounds = calculate_y_bounds(&p);

        assert_eq!(bounds[0], 0.0);
        assert!(bounds[1] > 11_200.0);
    }

    #[test]
    fn test_calculate_y_bounds_includes_threshold_lines() {
        let mut p = create_test_panel();
        p.min = Some(0.0);
        p.series.push(SeriesView {
            name: "latency".to_string(),
            value: None,
            points: vec![(0.0, 0.5), (1.0, 1.0)],
            visible: true,
        });
        p.thresholds = Some(Thresholds {
            mode: ThresholdMode::Absolute,
            steps: vec![
                ThresholdStep {
                    value: None,
                    color: Color::Green,
                },
                ThresholdStep {
                    value: Some(5.0),
                    color: Color::Red,
                },
            ],
            style: Some("line".to_string()),
        });

        let bounds = calculate_y_bounds(&p);

        assert_eq!(bounds[0], 0.0);
        assert!(bounds[1] > 5.0);
    }
}
