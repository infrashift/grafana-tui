//! Characterization snapshots for the Power of 10 refactor (ISSUE-001).
//!
//! Each test loads an example dashboard on the same path as `main`. Then it
//! fills each panel with fixed synthetic series and draws the dashboard to a
//! TUI buffer and to SVG. The tests compare the output with the files in
//! `tests/snapshots/`. A refactor must not change the output.
//!
//! To record new snapshots after an intended change in the output, run
//! `UPDATE_SNAPSHOTS=1 cargo test snapshot_tests`. Then examine the diff.

#[cfg(test)]
mod tests {
    use crate::app::{AppState, SeriesView};
    use crate::dashboard::{DashboardLayoutItem, TabGroupId};
    use crate::export::ExportOptions;
    use crate::{app, build_import_context, grafana, panel_state, theme};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    /// The end of the time range in each snapshot: 2026-07-01T00:00:00Z.
    const VIEW_END: i64 = 1_782_864_000;
    /// The time range in each snapshot.
    const RANGE_SECS: u64 = 3600;
    /// The points in each synthetic series.
    const POINTS: usize = 60;
    /// The terminal size of each snapshot.
    const WIDTH: u16 = 160;
    const HEIGHT: u16 = 60;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// Loads one dashboard file into an app, as `main` does for one `--grafana-json`.
    fn load_app(path: &Path) -> AppState {
        let dashboard = grafana::load_grafana_dashboard(path).unwrap();
        let context = build_import_context(&dashboard, None, &[]);
        let panels: Vec<_> = dashboard.queries.into_iter().map(panel_state).collect();
        let template = app::DashboardTemplate::new(dashboard.layout, dashboard.repeats, &panels)
            .with_conditions(dashboard.conditions)
            .with_sections(dashboard.sections);
        let mut app = AppState::new(
            crate::prom::PromClient::new("http://127.0.0.1:9".to_string()),
            Duration::from_secs(RANGE_SECS),
            Duration::from_secs(5),
            Duration::from_secs(30),
            format!("{} (imported)", dashboard.title),
            panels,
            dashboard.skipped_panels,
            theme::Theme::default(),
            "dashed-line".to_string(),
            ExportOptions::default(),
        );
        app.vars = context.variables.vars;
        app.var_values = context.variables.var_values;
        app.regex_vars = context.variables.regex_vars;
        app.all_vars = context.variables.all_vars;
        app.variable_names = context.variables.names;
        app.query_vars = context.query_vars;
        app.apply_template(template);
        app.view_end_ts = VIEW_END;
        fill_series(&mut app);
        app
    }

    /// Gives each panel two fixed series for each query, with no randomness.
    fn fill_series(app: &mut AppState) {
        let start = (VIEW_END - RANGE_SECS as i64) as f64;
        let step = RANGE_SECS as f64 / POINTS as f64;
        for (panel_index, panel) in app.panels.iter_mut().enumerate() {
            let queries = panel.exprs.len().max(1);
            panel.series = (0..queries * 2)
                .map(|series_index| {
                    let phase = panel_index as f64 * 0.7 + series_index as f64 * 1.3;
                    let scale = 10.0 + 15.0 * series_index as f64;
                    let points: Vec<(f64, f64)> = (0..POINTS)
                        .map(|i| {
                            let x = start + i as f64 * step;
                            let y = scale * (1.5 + (i as f64 * 0.2 + phase).sin());
                            (x, (y * 100.0).round() / 100.0)
                        })
                        .collect();
                    SeriesView {
                        name: format!("series-{panel_index}-{series_index}"),
                        value: points.last().map(|point| point.1),
                        points,
                        visible: true,
                    }
                })
                .collect();
            panel.last_error = None;
        }
    }

    fn tui_text(app: &mut AppState) -> String {
        let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw_ui(frame, app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let mut text = (0..HEIGHT)
            .map(|y| {
                (0..WIDTH)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        text.push('\n');
        text
    }

    /// Compares `actual` with the snapshot file, or writes the file when
    /// `UPDATE_SNAPSHOTS` is set.
    fn check_snapshot(name: &str, actual: &str) {
        let path = root().join("tests/snapshots").join(name);
        if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, actual).unwrap();
            return;
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "{}: {error}; run UPDATE_SNAPSHOTS=1 cargo test snapshot_tests",
                path.display()
            )
        });
        if expected != actual {
            let line = expected
                .lines()
                .zip(actual.lines())
                .position(|(left, right)| left != right)
                .map_or_else(
                    || "a trailing line".to_string(),
                    |n| format!("line {}", n + 1),
                );
            panic!("{name} differs from the snapshot at {line}");
        }
    }

    /// The ids of the tab groups at the top level of the layout.
    fn top_tab_groups(app: &AppState) -> Vec<TabGroupId> {
        app.layout
            .items
            .iter()
            .filter_map(|item| match item {
                DashboardLayoutItem::Tabs(group) if group.tabs.len() > 1 => Some(group.id),
                _ => None,
            })
            .collect()
    }

    /// Draws one dashboard to the TUI and to SVG and checks both snapshots. When
    /// the layout has a tab group at the top level, it also checks the second tab.
    fn check_dashboard(relative: &str) {
        let mut app = load_app(&root().join(relative));
        let stem = relative.replace(['/', '.'], "_");
        check_snapshot(&format!("{stem}.tui.txt"), &tui_text(&mut app));
        let viewport = Rect::new(0, 0, WIDTH, HEIGHT);
        check_snapshot(
            &format!("{stem}.svg"),
            &crate::export::render_svg(&app, viewport),
        );
        if let Some(group) = top_tab_groups(&app).first().copied() {
            // The layout change only matters for a fetch, and each panel has data.
            let _change = app.layout.set_active_tab(group, 1);
            check_snapshot(&format!("{stem}.tab2.tui.txt"), &tui_text(&mut app));
        }
    }

    #[test]
    fn all_visualizations() {
        check_dashboard("examples/dashboards/all_visualizations.json");
    }

    #[test]
    fn v2_autogrid() {
        check_dashboard("examples/dashboards/grafana_v2_autogrid.json");
    }

    #[test]
    fn v2_compatibility() {
        check_dashboard("examples/dashboards/grafana_v2_compatibility.json");
    }

    #[test]
    fn v2_conditional() {
        check_dashboard("examples/dashboards/grafana_v2_conditional.json");
    }

    #[test]
    fn v2_repeats() {
        check_dashboard("examples/dashboards/grafana_v2_repeats.yaml");
    }

    #[test]
    fn v2_rows() {
        check_dashboard("examples/dashboards/grafana_v2_rows.json");
    }

    #[test]
    fn v2_sections() {
        check_dashboard("examples/dashboards/grafana_v2_sections.json");
    }

    #[test]
    fn v2_tabs() {
        check_dashboard("examples/dashboards/grafana_v2_tabs.json");
    }

    #[test]
    fn instant_queries() {
        check_dashboard("examples/dashboards/instant_queries.json");
    }

    #[test]
    fn prometheus_demo() {
        check_dashboard("examples/dashboards/prometheus_demo.json");
    }

    #[test]
    fn simple_test() {
        check_dashboard("examples/dashboards/simple_test.json");
    }

    #[test]
    fn thresholds_demo() {
        check_dashboard("examples/dashboards/thresholds_demo.json");
    }

    #[test]
    fn sshadowforge_operator() {
        check_dashboard("examples/demo/sshadowforge/00-operator.json");
    }

    #[test]
    fn sshadowforge_event_pipeline() {
        check_dashboard("examples/demo/sshadowforge/10-event-pipeline.json");
    }

    #[test]
    fn sshadowforge_nats() {
        check_dashboard("examples/demo/sshadowforge/20-nats-jetstream.json");
    }

    #[test]
    fn sshadowforge_builds() {
        check_dashboard("examples/demo/sshadowforge/30-builds.json");
    }

    #[test]
    fn sshadowforge_developer() {
        check_dashboard("examples/demo/sshadowforge/40-developer.json");
    }

    #[test]
    fn sshadowforge_resources() {
        check_dashboard("examples/demo/sshadowforge/50-resources.json");
    }

    #[test]
    fn hashistack_overview() {
        check_dashboard("examples/demo/hashistack-rdw/00-overview.json");
    }

    #[test]
    fn hashistack_nomad() {
        check_dashboard("examples/demo/hashistack-rdw/20-nomad.json");
    }
}
