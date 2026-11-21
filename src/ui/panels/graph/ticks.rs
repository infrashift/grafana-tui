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

//! Grid tick generators for the TUI graph and the SVG export.
//!
//! Each loop has a fixed bound (rule R2 of ADR-0002). A float step that does
//! not move the tick stops the generator, so a step under half an ulp cannot
//! hang it.

use super::autogrid::nice_grid_step;

/// The most ticks on one axis. An exported plot is narrower than 4096 pixels,
/// so more grid lines add nothing.
pub(crate) const MAX_TICKS: usize = 4096;

/// The most step changes in the search for a coarse time step. Doubling one
/// day goes past `f64::MAX` in fewer than 1024 changes.
const MAX_STEP_CHANGES: usize = 1100;

/// One minute in seconds.
const MINUTE: f64 = 60.0;
/// One hour in seconds.
const HOUR: f64 = 60.0 * MINUTE;
/// One day in seconds.
const DAY: f64 = 24.0 * HOUR;

/// The multiples of `step` strictly between `start` and `end`, at most `limit`.
///
/// The ticks come from repeated adds of `step`, so the values are the same as
/// before the bound. The loop stops when a step does not move the tick.
fn interior_ticks(start: f64, end: f64, step: f64, limit: usize) -> Vec<f64> {
    let mut ticks = Vec::new();
    if !(start.is_finite() && end.is_finite() && step.is_finite() && step > 0.0) {
        return ticks;
    }
    let mut tick = (start / step).ceil() * step;
    if tick <= start {
        tick += step;
    }
    for _ in 0..limit {
        if tick >= end {
            break;
        }
        if tick > start {
            ticks.push(tick);
        }
        let next = tick + step;
        if next <= tick {
            break;
        }
        tick = next;
    }
    debug_assert!(
        ticks_are_sorted(&ticks, limit),
        "the ticks obey the limit and are in increasing order"
    );
    debug_assert!(
        ticks.iter().all(|tick| *tick > start && *tick < end),
        "each tick is strictly between the bounds"
    );
    ticks
}

/// Tells if `ticks` has `limit` or fewer values, each in increasing order.
fn ticks_are_sorted(ticks: &[f64], limit: usize) -> bool {
    ticks.len() <= limit && ticks.iter().zip(ticks.iter().skip(1)).all(|(a, b)| a < b)
}

/// How `coarse_step` makes a time step coarser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepGrowth {
    /// The next TUI time step, then double the step.
    TuiTimeSteps,
    /// Double the step.
    Double,
}

impl StepGrowth {
    /// The step after `step` for this growth.
    fn next(self, step: f64) -> f64 {
        debug_assert!(
            step.is_finite() && step > 0.0,
            "the step is finite and positive"
        );
        let next = match self {
            Self::TuiTimeSteps => next_time_grid_step(step),
            Self::Double => step * 2.0,
        };
        debug_assert!(next > step, "the next step is coarser");
        next
    }
}

/// Returns `step`, made coarser with `growth` until `max_ticks` or fewer fit
/// between `start` and `end`.
fn coarse_step(start: f64, end: f64, mut step: f64, max_ticks: usize, growth: StepGrowth) -> f64 {
    debug_assert!(
        start.is_finite() && end.is_finite() && start < end,
        "the range is finite and not empty"
    );
    debug_assert!(
        step.is_finite() && step > 0.0,
        "the first step is finite and positive"
    );
    debug_assert!(max_ticks > 0, "the tick limit is positive");
    for _ in 0..MAX_STEP_CHANGES {
        if interior_ticks(start, end, step, max_ticks + 1).len() <= max_ticks {
            break;
        }
        step = growth.next(step);
    }
    step
}

/// The value grid ticks of a TUI graph that is `chart_height` rows high.
pub(super) fn calculate_value_grid_ticks(y_bounds: [f64; 2], chart_height: u16) -> Vec<f64> {
    let [min, max] = y_bounds;
    if !min.is_finite() || !max.is_finite() || max <= min || chart_height < 4 {
        return Vec::new();
    }

    let target_lines = (usize::from(chart_height) / 6).clamp(2, 4);
    let step = nice_grid_step(max - min, target_lines);
    debug_assert!(
        step > 0.0 || !(max - min).is_finite(),
        "the grid step is positive for a finite range"
    );
    let ticks = interior_ticks(min, max, step, MAX_TICKS);
    debug_assert!(
        ticks_are_sorted(&ticks, target_lines.saturating_add(1)),
        "the value grid has about the target number of lines, in order"
    );
    ticks
}

/// The time grid ticks of a TUI graph that is `chart_width` columns wide.
pub(super) fn calculate_time_grid_ticks(start: f64, end: f64, chart_width: u16) -> Vec<f64> {
    if !start.is_finite() || !end.is_finite() || end <= start || chart_width < 8 {
        return Vec::new();
    }

    let max_ticks = (usize::from(chart_width) / 20).clamp(3, 8);
    let base = base_time_grid_step(end - start);
    let step = coarse_step(start, end, base, max_ticks, StepGrowth::TuiTimeSteps);
    debug_assert!(step >= base, "the coarse step is not finer than the base");
    let ticks = interior_ticks(start, end, step, MAX_TICKS);
    debug_assert!(
        ticks_are_sorted(&ticks, max_ticks),
        "the time grid obeys its tick limit, in order"
    );
    ticks
}

/// The first time step that the TUI tries for a time range of `range` seconds.
fn base_time_grid_step(range: f64) -> f64 {
    let step = if range <= 10.0 * MINUTE {
        MINUTE
    } else if range <= 30.0 * MINUTE {
        5.0 * MINUTE
    } else if range <= 90.0 * MINUTE {
        30.0 * MINUTE
    } else if range <= 3.0 * HOUR {
        HOUR
    } else if range <= 6.0 * HOUR {
        2.0 * HOUR
    } else if range <= 12.0 * HOUR {
        3.0 * HOUR
    } else if range <= DAY {
        6.0 * HOUR
    } else if range <= 2.0 * DAY {
        12.0 * HOUR
    } else {
        DAY
    };
    debug_assert!(
        (MINUTE..=DAY).contains(&step),
        "the first step is one minute to one day"
    );
    debug_assert!(
        step <= range.max(MINUTE),
        "the first step is not wider than the range"
    );
    step
}

/// The next coarser TUI time step after `step`.
fn next_time_grid_step(step: f64) -> f64 {
    const STEPS: [f64; 10] = [
        60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0, 10800.0, 21600.0, 43200.0, 86400.0,
    ];

    debug_assert!(step > 0.0, "the time step is positive");
    let next = STEPS
        .iter()
        .copied()
        .find(|candidate| *candidate > step)
        .unwrap_or(step * 2.0);
    debug_assert!(next > step, "the next time step is coarser");
    next
}

/// The value grid ticks of an exported graph.
pub(crate) fn svg_value_ticks(min: f64, max: f64) -> Vec<f64> {
    if !min.is_finite() || !max.is_finite() || max <= min {
        return Vec::new();
    }

    let step = nice_grid_step(max - min, 3);
    debug_assert!(
        step > 0.0 || !(max - min).is_finite(),
        "the grid step is positive for a finite range"
    );
    let ticks = interior_ticks(min, max, step, MAX_TICKS);
    debug_assert!(
        ticks_are_sorted(&ticks, 4),
        "the value grid has about three lines, in order"
    );
    ticks
}

/// The time grid ticks of an exported graph. A range with more than
/// `MAX_TICKS` day ticks gets a step of two days or more.
pub(crate) fn svg_time_ticks(start: f64, end: f64) -> Vec<f64> {
    if !start.is_finite() || !end.is_finite() || end <= start {
        return Vec::new();
    }

    let range = end - start;
    let base = if range <= 10.0 * MINUTE {
        MINUTE
    } else if range <= 90.0 * MINUTE {
        30.0 * MINUTE
    } else if range <= 6.0 * HOUR {
        HOUR
    } else if range <= DAY {
        6.0 * HOUR
    } else {
        DAY
    };
    let step = coarse_step(start, end, base, MAX_TICKS, StepGrowth::Double);
    debug_assert!(step >= base, "the coarse step is not finer than the base");
    let ticks = interior_ticks(start, end, step, MAX_TICKS);
    debug_assert!(
        ticks_are_sorted(&ticks, MAX_TICKS),
        "the time grid obeys the tick limit, in order"
    );
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_value_grid_ticks_round_values() {
        let ticks = calculate_value_grid_ticks([329.0, 1287.0], 20);
        assert_eq!(ticks, vec![500.0, 1000.0]);
    }

    #[test]
    fn test_calculate_value_grid_ticks_excludes_boundaries() {
        let ticks = calculate_value_grid_ticks([0.0, 100.0], 20);
        assert!(!ticks.contains(&0.0));
        assert!(!ticks.contains(&100.0));
    }

    #[test]
    fn test_calculate_value_grid_ticks_invalid_ranges() {
        assert!(calculate_value_grid_ticks([1.0, 1.0], 20).is_empty());
        assert!(calculate_value_grid_ticks([2.0, 1.0], 20).is_empty());
        assert!(calculate_value_grid_ticks([f64::NAN, 1.0], 20).is_empty());
        assert!(calculate_value_grid_ticks([0.0, 1.0], 3).is_empty());
    }

    #[test]
    fn test_calculate_time_grid_ticks_two_hour_window() {
        let start = 41_820.0; // 11:37 UTC
        let end = start + 2.0 * 60.0 * 60.0;

        let ticks = calculate_time_grid_ticks(start, end, 80);

        assert_eq!(ticks, vec![43_200.0, 46_800.0]); // 12:00, 13:00 UTC
    }

    #[test]
    fn test_calculate_time_grid_ticks_one_hour_window() {
        let start = 44_520.0; // 12:22 UTC
        let end = start + 60.0 * 60.0;

        let ticks = calculate_time_grid_ticks(start, end, 80);

        assert_eq!(ticks, vec![45_000.0, 46_800.0]); // 12:30, 13:00 UTC
    }

    #[test]
    fn test_calculate_time_grid_ticks_five_minute_window() {
        let start = 43_335.0; // 12:02:15 UTC
        let end = start + 5.0 * 60.0;

        let ticks = calculate_time_grid_ticks(start, end, 120);

        assert_eq!(
            ticks,
            vec![43_380.0, 43_440.0, 43_500.0, 43_560.0, 43_620.0]
        );
    }

    #[test]
    fn each_time_range_gets_its_base_step() {
        let cases = [
            (20.0 * MINUTE, 5.0 * MINUTE),
            (5.0 * HOUR, 2.0 * HOUR),
            (10.0 * HOUR, 3.0 * HOUR),
            (20.0 * HOUR, 6.0 * HOUR),
            (40.0 * HOUR, 12.0 * HOUR),
        ];
        for (range, step) in cases {
            assert_eq!(base_time_grid_step(range), step, "{range}");
        }
        let ticks = svg_time_ticks(0.0, 20.0 * HOUR);
        assert_eq!(ticks, vec![6.0 * HOUR, 12.0 * HOUR, 18.0 * HOUR]);
    }

    #[test]
    fn invalid_ranges_have_no_ticks() {
        assert!(calculate_time_grid_ticks(1.0, 1.0, 80).is_empty());
        assert!(calculate_time_grid_ticks(0.0, 60.0, 7).is_empty());
        assert!(svg_value_ticks(f64::NAN, 1.0).is_empty());
        assert!(svg_time_ticks(2.0, 1.0).is_empty());
    }

    #[test]
    fn the_tui_time_step_grows_past_one_day_for_long_ranges() {
        let ticks = calculate_time_grid_ticks(0.0, 30.0 * DAY, 80);
        assert_eq!(ticks, vec![8.0 * DAY, 16.0 * DAY, 24.0 * DAY]);
    }

    #[test]
    fn the_generator_stops_at_its_limit() {
        let ticks = interior_ticks(0.0, 1e9, 1.0, MAX_TICKS);
        assert_eq!(ticks.len(), MAX_TICKS);
        assert_eq!(ticks.first(), Some(&1.0));
        assert_eq!(ticks.last(), Some(&4096.0));
    }

    #[test]
    fn the_generator_rejects_a_step_that_is_not_positive_and_finite() {
        for step in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(interior_ticks(0.0, 10.0, step, MAX_TICKS).is_empty());
        }
        assert!(interior_ticks(f64::NEG_INFINITY, 10.0, 1.0, MAX_TICKS).is_empty());
    }

    #[test]
    fn a_step_under_half_an_ulp_ends_the_value_grid() {
        // The ulp of 1e20 is 16384. The step of 5000 does not move a tick.
        let start = 1e20;
        let end = start + 16384.0;
        assert!(end > start);
        assert!(calculate_value_grid_ticks([start, end], 24).is_empty());
        assert!(svg_value_ticks(start, end).is_empty());
    }

    #[test]
    fn an_infinite_time_range_ends_the_time_grid() {
        assert!(calculate_time_grid_ticks(-f64::MAX, f64::MAX, 80).is_empty());
        assert!(svg_time_ticks(-f64::MAX, f64::MAX).is_empty());
    }

    #[test]
    fn a_long_export_range_gets_a_coarser_time_step() {
        let days = 4096.0;
        let ticks = svg_time_ticks(0.0, days * DAY);
        assert_eq!(ticks.len(), 4095);
        assert_eq!(ticks.first(), Some(&DAY));

        let ticks = svg_time_ticks(0.0, 2.0 * days * DAY);
        assert_eq!(ticks.len(), 4095);
        assert_eq!(ticks.first(), Some(&(2.0 * DAY)));
    }
}
