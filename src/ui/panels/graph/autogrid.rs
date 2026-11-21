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

/// A grid step of 1, 2 or 5 times a power of 10 that gives about `target_lines` lines.
pub(super) fn nice_grid_step(range: f64, target_lines: usize) -> f64 {
    if range <= 0.0 || !range.is_finite() || target_lines == 0 {
        return 0.0;
    }

    let raw_step = range / crate::num::usize_to_f64(target_lines);
    let magnitude = 10_f64.powf(raw_step.log10().floor());
    let fraction = raw_step / magnitude;
    let nice_fraction = if fraction <= 1.0 {
        1.0
    } else if fraction <= 2.0 {
        2.0
    } else if fraction <= 5.0 {
        5.0
    } else {
        10.0
    };
    let step = nice_fraction * magnitude;
    // With a subnormal magnitude or an overflow, the checks are not precise.
    let precise = magnitude.is_normal() && step.is_finite();
    debug_assert!(
        !precise || step >= raw_step * (1.0 - 1e-9),
        "the nice step is not finer than the raw step"
    );
    debug_assert!(
        !precise || step <= raw_step * 2.5 * (1.0 + 1e-9),
        "the nice step is no more than 2.5 times the raw step"
    );
    step
}

pub(super) fn build_autogrid_datasets(
    x_bounds: [f64; 2],
    y_bounds: [f64; 2],
    time_ticks: &[f64],
    value_ticks: &[f64],
    plot_width: u16,
    plot_height: u16,
) -> Vec<Vec<(f64, f64)>> {
    if x_bounds[1] <= x_bounds[0] || y_bounds[1] <= y_bounds[0] {
        return Vec::new();
    }

    let mut datasets = Vec::new();
    let vertical_samples = usize::from(plot_height).saturating_mul(4).max(2);
    let horizontal_samples = usize::from(plot_width).saturating_mul(2).max(2);

    for tick in time_ticks {
        if *tick <= x_bounds[0] || *tick >= x_bounds[1] {
            continue;
        }
        datasets.push(
            (0..=vertical_samples)
                .map(|i| {
                    let y = interpolate(y_bounds[0], y_bounds[1], i, vertical_samples);
                    (*tick, y)
                })
                .collect(),
        );
    }

    for tick in value_ticks {
        if *tick <= y_bounds[0] || *tick >= y_bounds[1] {
            continue;
        }
        datasets.push(
            (0..=horizontal_samples)
                .map(|i| {
                    let x = interpolate(x_bounds[0], x_bounds[1], i, horizontal_samples);
                    (x, *tick)
                })
                .collect(),
        );
    }
    datasets
}

fn interpolate(start: f64, end: f64, index: usize, total: usize) -> f64 {
    start + (end - start) * crate::num::usize_to_f64(index) / crate::num::usize_to_f64(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_autogrid_datasets() {
        let datasets = build_autogrid_datasets([0.0, 10.0], [0.0, 10.0], &[5.0], &[5.0], 10, 5);

        assert_eq!(datasets.len(), 2);
        assert_eq!(datasets[0].first(), Some(&(5.0, 0.0)));
        assert_eq!(datasets[0].last(), Some(&(5.0, 10.0)));
        assert_eq!(datasets[0].len(), 21);
        assert_eq!(datasets[1].first(), Some(&(0.0, 5.0)));
        assert_eq!(datasets[1].last(), Some(&(10.0, 5.0)));
        assert_eq!(datasets[1].len(), 21);
    }
}
