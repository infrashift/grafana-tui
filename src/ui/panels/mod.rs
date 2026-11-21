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

mod bar_gauge;
mod gauge;
mod graph;
mod heatmap;
mod stat;
mod table;

use super::tabs::truncate_title;
use crate::app::{AppState, PanelState, PanelType};
use crate::theme::Theme;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use bar_gauge::render_bar_gauge;
use gauge::render_gauge;
use graph::render_graph_panel;
pub(crate) use graph::{calculate_y_bounds, cluster_badge, svg_time_ticks, svg_value_ticks};
use heatmap::render_heatmap;
use stat::render_stat;
use table::render_table;

/// How serious a panel's data notice is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoticeLevel {
    Error,
    Warning,
}

/// A marker shown after the title of a panel that still has data to show but
/// whose latest fetch failed or returned warnings. The footer shows the
/// details for the selected panel.
pub(crate) fn data_notice(p: &PanelState) -> Option<(&'static str, NoticeLevel)> {
    let notice = if p.last_error.is_some() {
        let text = if p.notices.stale {
            "⚠ stale: queries failed"
        } else {
            "⚠ query failed"
        };
        Some((text, NoticeLevel::Error))
    } else if !p.notices.warnings.is_empty() {
        Some(("⚠ warning", NoticeLevel::Warning))
    } else {
        None
    };
    debug_assert!(
        notice.is_none_or(|(text, _)| text.starts_with(NOTICE_ICON)),
        "a notice starts with its icon"
    );
    debug_assert!(
        notice.is_none_or(|(_, level)| (level == NoticeLevel::Error) == p.last_error.is_some()),
        "only a failed fetch gives an error notice"
    );
    notice
}

/// Title columns kept before a notice shrinks to its icon.
const MIN_TITLE_COLUMNS: usize = 6;

/// The notice icon alone, for panels too narrow for the whole notice.
const NOTICE_ICON: &str = "⚠";

/// Fits a panel title and its notice in `columns`, with a space between
/// them. The title is cut short first, then the notice shrinks to its icon,
/// so the notice stays in view in narrow panels.
pub(crate) fn fit_panel_title(
    title: &str,
    notice: Option<&'static str>,
    columns: usize,
) -> (String, Option<&'static str>) {
    let to_u16 = |columns: usize| u16::try_from(columns).unwrap_or(u16::MAX);
    if let Some(notice) = notice {
        let title_width = Span::raw(title).width();
        for badge in [notice, NOTICE_ICON] {
            let room = columns.saturating_sub(Span::raw(badge).width() + 1);
            if room >= title_width.min(MIN_TITLE_COLUMNS) {
                let fitted = truncate_title(title, to_u16(room));
                debug_assert!(
                    Span::raw(fitted.as_str()).width() <= room,
                    "the title leaves room for the notice"
                );
                return (fitted, Some(badge));
            }
        }
    }
    let fitted = truncate_title(title, to_u16(columns));
    debug_assert!(
        Span::raw(fitted.as_str()).width() <= columns,
        "the title fits the columns"
    );
    (fitted, None)
}

/// Renders a single panel.
///
/// This function handles:
/// - Drawing the panel border and title.
/// - Rendering the chart with data series.
/// - Drawing the legend (if space permits).
/// - Handling inspection mode (cursor line and values).
/// - Displaying error messages if the panel has an error.
pub(crate) fn render_panel(
    frame: &mut Frame,
    area: Rect,
    panel_index: usize,
    p: &PanelState,
    app: &AppState,
    is_selected: bool,
    cursor_x: Option<f64>,
) -> Option<Vec<crate::annotations::AnnotationEvent>> {
    debug_assert!(
        panel_index < app.panels.len(),
        "the panel is in the panel list"
    );
    let theme = &app.theme;
    let border_style = if is_selected {
        Style::default().fg(theme.border_focused)
    } else {
        Style::default().fg(theme.border)
    };

    if let Some(err) = &p.last_error
        && p.series.is_empty()
    {
        render_error_panel(frame, area, p, err, border_style, theme);
        return None;
    }

    if !p.has_loaded() && !p.exprs.is_empty() {
        render_loading_panel(frame, area, p, border_style, theme);
        return None;
    }

    // Render the outer block (Panel container)
    let block = panel_block(p, area, border_style, theme);
    frame.render_widget(block.clone(), area);

    let inner_area = block.inner(area);
    debug_assert!(
        super::layout::rect_inside(area, inner_area),
        "the panel body is inside the panel"
    );
    let body = PanelBody {
        area: inner_area,
        panel_index,
        is_selected,
        cursor_x,
    };
    render_panel_body(frame, &body, p, app)
}

/// Draws a panel that has an error and no data to show.
fn render_error_panel(
    frame: &mut Frame,
    area: Rect,
    p: &PanelState,
    err: &str,
    border_style: Style,
    theme: &Theme,
) {
    debug_assert!(p.last_error.is_some(), "an error panel has an error");
    debug_assert!(p.series.is_empty(), "an error panel has no data to show");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            format!("{} — ERROR", p.title),
            Style::default().fg(theme.error),
        ));
    let para = Paragraph::new(err.to_string())
        .block(block)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(theme.text));
    frame.render_widget(para, area);
}

/// Draws a panel whose first fetch is not complete.
fn render_loading_panel(
    frame: &mut Frame,
    area: Rect,
    p: &PanelState,
    border_style: Style,
    theme: &Theme,
) {
    debug_assert!(!p.has_loaded(), "a loading panel has no first result");
    debug_assert!(!p.exprs.is_empty(), "a loading panel has queries");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            p.title.clone(),
            Style::default().fg(theme.title),
        ));
    let para = Paragraph::new("Loading…")
        .block(block)
        .style(Style::default().fg(theme.text));
    frame.render_widget(para, area);
}

/// Builds the bordered panel block with the title and the notice badge.
fn panel_block(p: &PanelState, area: Rect, border_style: Style, theme: &Theme) -> Block<'static> {
    let notice = data_notice(p);
    let (title_text, badge) = fit_panel_title(
        &p.title,
        notice.map(|(text, _)| text),
        usize::from(area.width.saturating_sub(2)),
    );
    let mut title = vec![Span::styled(title_text, Style::default().fg(theme.title))];
    if let (Some(badge), Some((_, level))) = (badge, notice) {
        let color = match level {
            NoticeLevel::Error => theme.error,
            NoticeLevel::Warning => theme.warning,
        };
        title.push(Span::styled(
            format!(" {badge}"),
            Style::default().fg(color),
        ));
    }
    debug_assert!(
        badge.is_none() || notice.is_some(),
        "only a panel with a notice has a badge"
    );
    debug_assert!(
        title.len() <= 2,
        "the title has the text and one badge at most"
    );
    Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Line::from(title))
}

/// The inner area of a panel and the state that its body renderer uses.
struct PanelBody {
    area: Rect,
    panel_index: usize,
    is_selected: bool,
    cursor_x: Option<f64>,
}

/// Draws the body of a panel with the renderer for its panel type.
fn render_panel_body(
    frame: &mut Frame,
    body: &PanelBody,
    p: &PanelState,
    app: &AppState,
) -> Option<Vec<crate::annotations::AnnotationEvent>> {
    debug_assert!(
        body.panel_index < app.panels.len(),
        "the panel is in the panel list"
    );
    let inner_area = body.area;
    let events = match p.panel_type {
        PanelType::Graph => render_graph_panel(
            frame,
            inner_area,
            body.panel_index,
            p,
            app,
            body.cursor_x,
            body.is_selected,
        ),
        PanelType::Unknown => {
            render_graph_panel(
                frame,
                inner_area,
                body.panel_index,
                p,
                app,
                body.cursor_x,
                false,
            );
            None
        }
        PanelType::Gauge => {
            render_gauge(frame, inner_area, p, app);
            None
        }
        PanelType::BarGauge => {
            render_bar_gauge(frame, inner_area, p, app);
            None
        }
        PanelType::Table => {
            render_table(frame, inner_area, p, app);
            None
        }
        PanelType::Stat => {
            render_stat(frame, inner_area, p, app);
            None
        }
        PanelType::Heatmap => {
            render_heatmap(frame, inner_area, p, app);
            None
        }
    };
    let graph = matches!(p.panel_type, PanelType::Graph);
    debug_assert!(events.is_none() || graph, "only a graph shows events");
    events
}

#[cfg(test)]
mod tests {
    use super::fit_panel_title;

    #[test]
    fn titles_shorten_before_the_notice_does() {
        let title = "Upstream connect failures";
        let notice = Some("⚠ warning");

        assert_eq!(
            fit_panel_title(title, notice, 40),
            (title.to_string(), notice)
        );
        // 20 columns leave 10 for the title beside " ⚠ warning".
        assert_eq!(
            fit_panel_title(title, notice, 20),
            ("Upstream …".to_string(), notice)
        );
        // Too narrow for 6 title columns: the notice shrinks to its icon.
        assert_eq!(
            fit_panel_title(title, notice, 12),
            ("Upstream …".to_string(), Some("⚠"))
        );
        // Short titles keep the whole notice for as long as they fit.
        assert_eq!(
            fit_panel_title("CPU", notice, 14),
            ("CPU".to_string(), notice)
        );
        assert_eq!(
            fit_panel_title(title, None, 9),
            ("Upstream…".to_string(), None)
        );
        assert_eq!(fit_panel_title(title, notice, 0), (String::new(), None));
    }
}
