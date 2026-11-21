use crate::theme::Theme;
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TabSegment {
    pub(crate) rect: Rect,
    pub(crate) text: String,
    pub(crate) active: bool,
    pub(crate) activate: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TabBarGeometry {
    pub(crate) segments: Vec<TabSegment>,
}

pub(crate) fn tab_title(title: &str, index: usize) -> String {
    let sanitized = title
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let label = if sanitized.trim().is_empty() {
        format!("Tab {}", index + 1)
    } else {
        sanitized
    };
    debug_assert!(!label.trim().is_empty(), "a tab label has text");
    debug_assert!(
        !label.chars().any(char::is_control),
        "a tab label has no control characters"
    );
    label
}

pub(crate) fn truncate_title(title: &str, width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    let span = Span::raw(title);
    if span.width() <= usize::from(width) {
        return title.to_owned();
    }
    let budget = usize::from(width) - 1;
    let mut text = String::new();
    let mut used = 0;
    for grapheme in span.styled_graphemes(Style::default()) {
        let size = Span::raw(grapheme.symbol).width();
        if used + size > budget {
            break;
        }
        text.push_str(grapheme.symbol);
        used += size;
    }
    debug_assert!(used <= budget, "the kept graphemes fit the budget");
    text.push('…');
    debug_assert!(
        text_width(&text) <= usize::from(width),
        "the cut title fits the width"
    );
    text
}

fn text_width(text: &str) -> usize {
    Span::raw(text).width()
}

fn range_width(labels: &[String], active: usize, start: usize, end: usize) -> usize {
    debug_assert!(
        start <= end && end < labels.len(),
        "the range is inside the tab list"
    );
    let width = labels
        .iter()
        .enumerate()
        .take(end.saturating_add(1))
        .skip(start)
        .map(|(index, label)| text_width(label) + usize::from(index == active) * 2)
        .sum::<usize>()
        + end.saturating_sub(start)
        + usize::from(start > 0) * 2
        + usize::from(end + 1 < labels.len()) * 2;
    debug_assert!(
        width >= end - start,
        "the range has a separator between each two tabs"
    );
    width
}

/// Returns true when the segments go from left to right with no overlap.
fn segments_in_order(geometry: &TabBarGeometry) -> bool {
    let segments = &geometry.segments;
    let mut pairs = segments.iter().zip(segments.iter().skip(1));
    pairs.all(|(left, right)| left.rect.right() <= right.rect.x)
}

pub(crate) fn tab_bar_geometry(
    area: Rect,
    titles: &[String],
    active: Option<usize>,
    depth: usize,
) -> TabBarGeometry {
    let geometry = build_tab_bar(area, titles, active, depth);
    debug_assert!(
        geometry
            .segments
            .iter()
            .all(|segment| super::layout::rect_inside(area, segment.rect)),
        "each tab segment is inside the bar"
    );
    debug_assert!(
        segments_in_order(&geometry),
        "the tab segments go from left to right with no overlap"
    );
    debug_assert!(
        geometry
            .segments
            .iter()
            .filter(|segment| segment.active)
            .count()
            <= 1,
        "the bar shows one active tab or none"
    );
    geometry
}

/// Lays out the tab bar of `titles` in `area`, indented for `depth`.
fn build_tab_bar(
    area: Rect,
    titles: &[String],
    active: Option<usize>,
    depth: usize,
) -> TabBarGeometry {
    if area.is_empty() {
        return TabBarGeometry::default();
    }
    let mut geometry = TabBarGeometry::default();
    let indent = (depth.saturating_mul(2)).min(usize::from(area.width.saturating_sub(1)));
    let x = area.x.saturating_add(crate::num::usize_to_u16(indent));
    let available = usize::from(area.right().saturating_sub(x));
    if available == 0 {
        return geometry;
    }
    debug_assert!(x < area.right(), "the indent leaves a column for the tabs");
    let (false, Some(active)) = (titles.is_empty(), active) else {
        let text = truncate_title("No tabs", crate::num::usize_to_u16(available));
        geometry.segments.push(TabSegment {
            rect: Rect::new(x, area.y, crate::num::usize_to_u16(text_width(&text)), 1),
            text,
            active: false,
            activate: None,
        });
        return geometry;
    };

    let labels = titles
        .iter()
        .enumerate()
        .map(|(index, title)| tab_title(title, index))
        .collect::<Vec<_>>();
    let active = active.min(labels.len() - 1);
    debug_assert!(active < labels.len(), "the active tab is in the tab list");
    let mut segments = SegmentWriter { geometry, area, x };
    if area.width < 3 {
        let label = labels.get(active).map_or("", String::as_str);
        segments.push(format!("* {label}"), true, Some(active));
        return segments.geometry;
    }
    let (start, end) = visible_tabs(&labels, active, available);
    push_visible_tabs(&mut segments, &labels, active, (start, end));
    segments.geometry
}

/// Adds the tabs from `start` to `end`, with arrows for the tabs out of view.
fn push_visible_tabs(
    segments: &mut SegmentWriter,
    labels: &[String],
    active: usize,
    (start, end): (usize, usize),
) {
    debug_assert!(
        start <= active && active <= end && end < labels.len(),
        "the visible tabs include the active tab"
    );
    if start > 0 {
        segments.push("< ".to_string(), false, Some(active - 1));
    }
    for (index, label) in labels.iter().enumerate().take(end + 1).skip(start) {
        if index > start {
            segments.push(" ".to_string(), false, None);
        }
        let label = if index == active {
            format!("* {label}")
        } else {
            label.clone()
        };
        segments.push(label, index == active, Some(index));
    }
    if end + 1 < labels.len() {
        segments.push(" >".to_string(), false, Some(active + 1));
    }
    debug_assert!(
        segments.x <= segments.area.right(),
        "the tabs stop at the right edge of the bar"
    );
}

/// Returns the first and the last tab that fit in `available` columns around the active tab.
fn visible_tabs(labels: &[String], active: usize, available: usize) -> (usize, usize) {
    let mut start = active;
    let mut end = active;
    // Each pass adds a tab or ends the loop, so the tab count bounds it.
    for _ in 0..labels.len() {
        let mut grew = false;
        if start > 0 && range_width(labels, active, start - 1, end) <= available {
            start -= 1;
            grew = true;
        }
        if end + 1 < labels.len() && range_width(labels, active, start, end + 1) <= available {
            end += 1;
            grew = true;
        }
        if !grew {
            break;
        }
    }
    debug_assert!(
        start <= active && active <= end && end < labels.len(),
        "the visible tabs include the active tab"
    );
    debug_assert!(
        (start == active && end == active) || range_width(labels, active, start, end) <= available,
        "the added tabs fit the available width"
    );
    (start, end)
}

/// Adds tab bar segments from left to right until the bar is full.
struct SegmentWriter {
    geometry: TabBarGeometry,
    area: Rect,
    x: u16,
}

impl SegmentWriter {
    /// Adds one segment, cut short to the remaining width of the bar.
    fn push(&mut self, text: String, selected: bool, activate: Option<usize>) {
        debug_assert!(
            self.x >= self.area.x && self.x <= self.area.right(),
            "the writer is inside the bar"
        );
        let remaining = self.area.right().saturating_sub(self.x);
        if remaining == 0 {
            return;
        }
        let text = truncate_title(&text, remaining);
        let width = crate::num::usize_to_u16(text_width(&text).min(usize::from(remaining)));
        if width == 0 {
            return;
        }
        self.geometry.segments.push(TabSegment {
            rect: Rect::new(self.x, self.area.y, width, 1),
            text,
            active: selected,
            activate,
        });
        self.x = self.x.saturating_add(width);
        debug_assert!(
            self.x <= self.area.right(),
            "the segment stops at the right edge of the bar"
        );
    }
}

pub(crate) fn tab_at(geometry: &TabBarGeometry, position: Position) -> Option<usize> {
    debug_assert!(
        segments_in_order(geometry),
        "the tab segments go from left to right with no overlap"
    );
    geometry
        .segments
        .iter()
        .find(|segment| segment.rect.contains(position))
        .and_then(|segment| segment.activate)
}

pub(crate) fn tab_style(theme: &Theme, active: bool, focused: bool) -> Style {
    let mut style = Style::default().fg(if active { theme.title } else { theme.text });
    if active {
        style = style.add_modifier(Modifier::BOLD);
    }
    if focused {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    debug_assert!(
        style.add_modifier.contains(Modifier::BOLD) == active,
        "only the active tab is bold"
    );
    debug_assert!(
        style.add_modifier.contains(Modifier::UNDERLINED) == focused,
        "only a focused bar is underlined"
    );
    style
}

pub(crate) fn render_tab_bar(
    frame: &mut Frame,
    geometry: &TabBarGeometry,
    theme: &Theme,
    focused: bool,
) {
    debug_assert!(
        segments_in_order(geometry),
        "the tab segments go from left to right with no overlap"
    );
    debug_assert!(
        geometry
            .segments
            .iter()
            .all(|segment| segment.rect.height == 1),
        "each tab segment is one row high"
    );
    for segment in &geometry.segments {
        frame.render_widget(
            Line::styled(
                segment.text.as_str(),
                tab_style(theme, segment.active, focused),
            ),
            segment.rect,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{layout::Rect, text::Span};

    #[test]
    fn tabs_overflow_keeps_active_label_clickable() {
        let titles = vec!["Overview".into(), "服务".into(), "Latency".into()];
        let area = Rect::new(2, 3, 12, 1);
        let geometry = tab_bar_geometry(area, &titles, Some(2), 0);
        let active = geometry.segments.iter().find(|part| part.active).unwrap();
        assert_eq!(active.activate, Some(2));
        assert!(!active.text.is_empty());
        assert!(Span::raw(&active.text).width() <= usize::from(active.rect.width));
        assert_eq!(tab_at(&geometry, active.rect.as_position()), Some(2));
        let left_overflow = geometry.segments.first().unwrap();
        assert_eq!(left_overflow.text, "< ");
        assert_eq!(left_overflow.activate, Some(1));
        assert_eq!(tab_at(&geometry, left_overflow.rect.as_position()), Some(1));
        assert!(
            geometry
                .segments
                .iter()
                .all(|part| part.rect.x >= area.x && part.rect.right() <= area.right())
        );
    }

    #[test]
    fn tabs_truncation_preserves_clusters() {
        assert_eq!(truncate_title("服务延迟", 5), "服务…");
        assert_eq!(truncate_title("e\u{301}xyz", 2), "e\u{301}…");
        assert_eq!(truncate_title("Latency", 0), "");
        assert_eq!(tab_title("  ", 2), "Tab 3");
    }
}
