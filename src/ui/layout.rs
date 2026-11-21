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

use crate::{
    app::{AppMode, AppState, GridUnit, PanelState},
    dashboard::{
        DashboardAutoGrid, DashboardItemId, DashboardLayoutItem, DashboardTabs, MAX_LAYOUT_DEPTH,
        MAX_LAYOUT_STEPS, RowId, TabGroupId,
    },
    num,
};
use ratatui::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DashboardRect {
    pub(crate) id: DashboardItemId,
    pub(crate) rect: Rect,
    pub(crate) disclosure_rect: Option<Rect>,
    pub(crate) kind: DashboardRectKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DashboardRectKind {
    Row {
        row_id: RowId,
        depth: usize,
        collapsed: bool,
    },
    Tabs {
        group_id: TabGroupId,
        depth: usize,
    },
    TabEmpty {
        group_id: TabGroupId,
    },
    Panel {
        index: usize,
    },
}

/// Returns true when `inner` is empty or lies inside `outer`.
pub(crate) fn rect_inside(outer: Rect, inner: Rect) -> bool {
    let inside = inner.x >= outer.x && inner.y >= outer.y;
    inner.is_empty()
        || (inside && inner.right() <= outer.right() && inner.bottom() <= outer.bottom())
}

pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    debug_assert!(
        percent_x <= 100 && percent_y <= 100,
        "the percentages are 100 or less"
    );
    let [_, middle, _] = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .areas(area);

    let [_, center, _] = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .areas(middle);
    debug_assert!(
        rect_inside(area, center),
        "the centered rect is inside the area"
    );
    debug_assert!(
        center.width <= area.width && center.height <= area.height,
        "the centered rect is not larger than the area"
    );
    center
}

/// Returns a list of (Rect, panel_index) for all panels to be rendered.
pub(crate) fn calculate_grid_layout(area: Rect, app: &AppState) -> Vec<(Rect, usize)> {
    let mut results = Vec::new();

    // Grafana uses a 24-column grid; y/h units are arbitrary grid rows.
    let grid_cols: u16 = 24;
    let cell_w = std::cmp::max(1, area.width / grid_cols);
    // Heuristic: choose a usable cell height from terminal height (min 3 rows per h-unit)
    let cell_h = std::cmp::max(3, area.height / 24);

    // Render grid-backed panels with scroll offset
    let scroll_offset = num::usize_to_u16(app.vertical_scroll).saturating_mul(cell_h);
    let cells = GridCells {
        area,
        cell_w,
        cell_h,
        scroll_offset,
    };

    let shown = app.flat_panel_indices();
    for &i in &shown {
        let Some(p) = app.panels.get(i) else {
            continue;
        };
        if let Some(rect) = p.grid.and_then(|g| scrolled_grid_rect(&cells, g)) {
            results.push((rect, i));
        }
    }

    results.extend(grid_extras_layout(area, app, &shown, cell_h));
    debug_assert!(
        results.iter().all(|(rect, _)| rect_inside(area, *rect)),
        "each grid panel is inside the area"
    );
    debug_assert!(
        results.iter().all(|(_, index)| *index < app.panels.len()),
        "each grid panel is in the panel list"
    );
    results
}

/// The cell size and the scroll offset of a scrolled Grafana grid.
struct GridCells {
    area: Rect,
    cell_w: u16,
    cell_h: u16,
    scroll_offset: u16,
}

/// Returns the clamped rect of a grid panel, or `None` when the panel is not in view.
fn scrolled_grid_rect(cells: &GridCells, g: GridUnit) -> Option<Rect> {
    debug_assert!(
        cells.cell_w >= 1 && cells.cell_h >= 1,
        "a grid cell is one column and one row or more"
    );
    let area = cells.area;
    if g.x < 0 || g.y < 0 || g.w <= 0 || g.h <= 0 {
        return None;
    }
    let x = area
        .x
        .saturating_add(num::i32_to_u16(g.x).saturating_mul(cells.cell_w));
    let y_absolute = num::i32_to_u16(g.y).saturating_mul(cells.cell_h);

    // Apply scroll offset
    if y_absolute < cells.scroll_offset {
        // Panel is scrolled out of view at the top
        return None;
    }
    let y = area
        .y
        .saturating_add(y_absolute.saturating_sub(cells.scroll_offset));

    let w = num::i32_to_u16(g.w).saturating_mul(cells.cell_w);
    let h = num::i32_to_u16(g.h).saturating_mul(cells.cell_h);

    // Skip panels that are completely below the visible area
    if y >= area.bottom() {
        return None;
    }

    // Clamp to area
    let rect = Rect {
        x,
        y,
        width: w.min(area.right().saturating_sub(x)),
        height: h.min(area.bottom().saturating_sub(y)),
    };
    debug_assert!(
        rect_inside(area, rect),
        "the clamped panel is inside the area"
    );
    (rect.width >= 8 && rect.height >= 4).then_some(rect)
}

/// Lays out the panels without a grid position in two columns below the grid.
fn grid_extras_layout(
    area: Rect,
    app: &AppState,
    shown: &[usize],
    cell_h: u16,
) -> Vec<(Rect, usize)> {
    // Extras (panels without grid)
    let extras: Vec<(usize, &PanelState)> = shown
        .iter()
        .filter_map(|&i| app.panels.get(i).map(|p| (i, p)))
        .filter(|(_, p)| p.grid.is_none())
        .collect();
    if extras.is_empty() {
        return Vec::new();
    }
    // Place extras in a vertical stack under the grid.
    let max_y_h = shown
        .iter()
        .filter_map(|&i| {
            let g = app.panels.get(i)?.grid?;
            Some(g.y + g.h)
        })
        .max()
        .unwrap_or(0);

    let start_y_px = area
        .y
        .saturating_add(num::i32_to_u16(max_y_h).saturating_mul(cell_h));

    if start_y_px >= area.bottom() {
        return Vec::new();
    }
    let extras_area = Rect {
        x: area.x,
        y: start_y_px,
        width: area.width,
        height: area.bottom().saturating_sub(start_y_px),
    };
    debug_assert!(
        rect_inside(area, extras_area),
        "the extras area is inside the area"
    );

    // Reuse two-column layout for extras
    // We need to pass the subset of panels but keep their original indices.
    let extra_indices: Vec<usize> = extras.iter().map(|(i, _)| *i).collect();
    let placed = calculate_two_column_layout_subset(extras_area, app, &extra_indices);
    debug_assert!(
        placed.len() <= extra_indices.len(),
        "each extra panel has one rect or none"
    );
    placed
}

pub(crate) fn calculate_two_column_layout(area: Rect, app: &AppState) -> Vec<(Rect, usize)> {
    calculate_two_column_layout_subset(area, app, &app.flat_panel_indices())
}

pub(crate) fn calculate_two_column_layout_subset(
    area: Rect,
    app: &AppState,
    panel_indices: &[usize],
) -> Vec<(Rect, usize)> {
    let mut results = Vec::new();
    if panel_indices.is_empty() {
        return results;
    }

    let [left_column, right_column] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);

    let panel_height = 12u16;
    let rows_fit = usize::from((area.height / panel_height).saturating_mul(2).max(1));

    // Scroll handling
    // If we are rendering the main list (not extras), we use app.vertical_scroll.
    // If we are rendering extras, we might want independent scroll or just show what fits.
    // For now, use app.vertical_scroll only if we are rendering the full list (heuristic).
    // Or better: always use it, but clamp it.

    let start = app
        .vertical_scroll
        .min(panel_indices.len().saturating_sub(rows_fit));
    let end = (start + rows_fit).min(panel_indices.len());
    debug_assert!(
        start <= end && end <= panel_indices.len(),
        "the visible window is inside the panel list"
    );

    let visible_indices = panel_indices.get(start..end).unwrap_or_default();

    let mut left_indices = Vec::new();
    let mut right_indices = Vec::new();

    for (i, &original_idx) in visible_indices.iter().enumerate() {
        if i % 2 == 0 {
            left_indices.push(original_idx);
        } else {
            right_indices.push(original_idx);
        }
    }

    stack_column(left_column, &left_indices, panel_height, &mut results);
    stack_column(right_column, &right_indices, panel_height, &mut results);

    debug_assert!(
        results.len() <= rows_fit && results.iter().all(|(rect, _)| rect_inside(area, *rect)),
        "the columns show the panels that fit, inside the area"
    );
    results
}

/// Stacks panels of `panel_height` rows from the top of `column`.
fn stack_column(
    column: Rect,
    indices: &[usize],
    panel_height: u16,
    results: &mut Vec<(Rect, usize)>,
) {
    debug_assert!(panel_height >= 1, "a panel is one row high or more");
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(panel_height); indices.len()])
        .split(column);
    debug_assert!(
        chunks.len() == indices.len(),
        "the layout gives one chunk for each panel"
    );
    debug_assert!(
        chunks.iter().all(|chunk| rect_inside(column, *chunk)),
        "each chunk is inside the column"
    );
    for (rect, &idx) in chunks.iter().zip(indices.iter()) {
        results.push((*rect, idx));
    }
}

fn dashboard_inner_area(area: Rect) -> Rect {
    let [_, body, _] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(2),
    ])
    .areas(area);

    let inner = body.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    debug_assert!(
        rect_inside(area, inner),
        "the dashboard body is inside the area"
    );
    debug_assert!(
        inner.width <= area.width.saturating_sub(2),
        "the margin takes one column on each side"
    );
    inner
}

pub(crate) fn visible_dashboard_rects(area: Rect, app: &AppState) -> Vec<DashboardRect> {
    let inner_area = dashboard_inner_area(area);

    if app.mode == AppMode::Fullscreen || app.mode == AppMode::FullscreenInspect {
        return app
            .selected_panel_index()
            .map(|index| {
                vec![DashboardRect {
                    id: DashboardItemId::Panel(index),
                    rect: inner_area,
                    disclosure_rect: None,
                    kind: DashboardRectKind::Panel { index },
                }]
            })
            .unwrap_or_default();
    }

    let structurally_flat = app
        .layout
        .items
        .iter()
        .all(|item| matches!(item, DashboardLayoutItem::Panel(_)));
    if structurally_flat {
        return flat_dashboard_rects(inner_area, app);
    }

    let projection = project_dashboard(inner_area, app);
    let pinned = projection.pinned.is_some();

    let scroll_offset = u16::try_from(app.vertical_scroll)
        .unwrap_or(u16::MAX)
        .saturating_mul(projection.cell_h);
    let rects: Vec<DashboardRect> = projection
        .pinned
        .into_iter()
        .chain(
            projection
                .items
                .into_iter()
                .filter_map(|item| clip_scrolled_rect(item, projection.content, scroll_offset)),
        )
        .collect();
    debug_assert!(
        rects.iter().all(|item| rect_inside(inner_area, item.rect)),
        "each visible item is inside the dashboard body"
    );
    debug_assert!(
        !pinned
            || rects
                .first()
                .is_some_and(|item| matches!(item.kind, DashboardRectKind::Tabs { .. })),
        "the pinned tab bar comes first"
    );
    rects
}

/// Lays out a dashboard of panels only: on the Grafana grid when a panel has
/// a grid position, else in two columns.
fn flat_dashboard_rects(inner_area: Rect, app: &AppState) -> Vec<DashboardRect> {
    let panel_rects = if app.flat_panel_indices().into_iter().any(|index| {
        app.panels
            .get(index)
            .is_some_and(|panel| panel.grid.is_some())
    }) {
        calculate_grid_layout(inner_area, app)
    } else {
        calculate_two_column_layout(inner_area, app)
    };
    let rects: Vec<DashboardRect> = panel_rects
        .into_iter()
        .map(|(rect, index)| panel_rect(index, rect))
        .collect();
    debug_assert!(
        rects
            .iter()
            .all(|item| matches!(item.kind, DashboardRectKind::Panel { .. })),
        "a flat dashboard shows panels only"
    );
    debug_assert!(
        rects.iter().all(|item| rect_inside(inner_area, item.rect)),
        "each panel is inside the dashboard body"
    );
    rects
}

/// The dashboard laid out before scrolling.
struct Projection {
    /// The bar of the tab group holding one tab per dashboard, which stays at
    /// the top when several dashboards are loaded.
    pinned: Option<DashboardRect>,
    /// Where the scrolled items show: the area below the pinned bar.
    content: Rect,
    /// The items, positioned as if `content` were tall enough for them all.
    items: Vec<DashboardRect>,
    cell_h: u16,
}

fn project_dashboard(area: Rect, app: &AppState) -> Projection {
    if let Some(group) = pinned_dashboard_group(app) {
        return project_pinned(group, area, app);
    }
    let cell_h = std::cmp::max(3, area.height / 24);
    let mut items = Vec::new();
    project_layout_items(&app.layout.items, 0, area, area.y, cell_h, app, &mut items);
    let projection = Projection {
        pinned: None,
        content: area,
        items,
        cell_h,
    };
    debug_assert!(
        projection
            .items
            .iter()
            .all(|item| item.rect.y >= projection.content.y),
        "each item starts at or below the top of the content"
    );
    debug_assert!(
        projection.items.iter().all(|item| {
            item.rect.x >= area.x && item.rect.right() <= area.right() || item.rect.is_empty()
        }),
        "each item is inside the columns of the area"
    );
    projection
}

/// Lays out the pinned tab bar of several loaded dashboards, and the shown
/// dashboard below it.
fn project_pinned(group: &DashboardTabs, area: Rect, app: &AppState) -> Projection {
    debug_assert!(
        group.active.is_none_or(|active| active < group.tabs.len()),
        "the active dashboard is in the tab list"
    );
    let bar = Rect::new(area.x, area.y, area.width, area.height.min(1));
    let content = Rect::new(
        area.x,
        area.y.saturating_add(bar.height),
        area.width,
        area.height.saturating_sub(bar.height),
    );
    debug_assert!(
        rect_inside(area, bar) && rect_inside(area, content),
        "the bar and the content are inside the area"
    );
    let cell_h = std::cmp::max(3, content.height / 24);
    let mut items = Vec::new();
    // The shown dashboard's items, laid out as a dashboard of its own.
    match group.active.and_then(|index| group.tabs.get(index)) {
        Some(tab) if !tab.children.is_empty() => {
            project_layout_items(
                &tab.children,
                0,
                content,
                content.y,
                cell_h,
                app,
                &mut items,
            );
        }
        _ => items.push(DashboardRect {
            id: DashboardItemId::Tabs(group.id),
            rect: Rect::new(content.x, content.y, content.width, 1),
            disclosure_rect: None,
            kind: DashboardRectKind::TabEmpty { group_id: group.id },
        }),
    }
    Projection {
        pinned: Some(DashboardRect {
            id: DashboardItemId::Tabs(group.id),
            rect: bar,
            disclosure_rect: None,
            kind: DashboardRectKind::Tabs {
                group_id: group.id,
                depth: 0,
            },
        }),
        content,
        items,
        cell_h,
    }
}

/// The tab group of several loaded dashboards, which is pinned above them.
fn pinned_dashboard_group(app: &AppState) -> Option<&crate::dashboard::DashboardTabs> {
    let group = app.dashboard_group()?;
    let pinned = match app.layout.items.as_slice() {
        [DashboardLayoutItem::Tabs(tabs)] if tabs.id == group => Some(tabs),
        _ => None,
    };
    debug_assert!(
        pinned.is_none_or(|tabs| tabs.active.is_none_or(|active| active < tabs.tabs.len())),
        "the active dashboard is in the tab list"
    );
    pinned
}

pub(crate) fn scroll_selected_into_view(area: Rect, app: &mut AppState) {
    let Some(selected) = app.selected_item else {
        return;
    };
    if app
        .layout
        .items
        .iter()
        .all(|item| matches!(item, DashboardLayoutItem::Panel(_)))
    {
        app.scroll_to_selected_panel();
        return;
    }
    if app.dashboard_group().map(DashboardItemId::Tabs) == Some(selected) {
        // The dashboards' tab bar is pinned, so it is always in view.
        return;
    }
    let inner = dashboard_inner_area(area);
    if inner.is_empty() {
        return;
    }
    let Projection {
        content: inner,
        items,
        cell_h,
        ..
    } = project_dashboard(inner, app);
    if inner.is_empty() {
        return;
    }
    let Some(item) = items.into_iter().find(|item| {
        item.id == selected && !matches!(item.kind, DashboardRectKind::TabEmpty { .. })
    }) else {
        return;
    };
    let cell_h = usize::from(cell_h.max(1));
    let top_delta = usize::from(item.rect.y.saturating_sub(inner.y));
    let bottom_delta = usize::from(item.rect.bottom().saturating_sub(inner.bottom()));
    let lower = bottom_delta.div_ceil(cell_h);
    let upper = top_delta / cell_h;
    app.vertical_scroll = if item.rect.height > inner.height {
        upper
    } else {
        app.vertical_scroll.clamp(lower, upper.max(lower))
    };
    debug_assert!(
        app.vertical_scroll <= upper.max(lower),
        "the scroll does not pass the top of the selected item"
    );
    debug_assert!(
        app.vertical_scroll >= lower.min(upper),
        "the scroll reaches the bottom of the selected item when it fits"
    );
}

/// Lays out `items` and their children from `cursor_y` down, and gives the y
/// below them. A stack of child lists replaces recursion.
fn project_layout_items(
    items: &[DashboardLayoutItem],
    depth: usize,
    area: Rect,
    mut cursor_y: u16,
    cell_h: u16,
    app: &AppState,
    output: &mut Vec<DashboardRect>,
) -> u16 {
    let (start_y, first_new) = (cursor_y, output.len());
    let mut frames = Vec::with_capacity(MAX_LAYOUT_DEPTH);
    frames.push(ProjectFrame::new(items, depth));
    for _ in 0..MAX_LAYOUT_STEPS {
        let open_frames = frames.len();
        let Some(frame) = frames.last_mut() else {
            break;
        };
        let Some(item) = frame.items.next() else {
            if !frame.panels.is_empty() {
                cursor_y = project_panel_group(area, cursor_y, cell_h, app, &frame.panels, output);
            }
            frames.pop();
            continue;
        };
        if let DashboardLayoutItem::Panel(index) = item {
            frame.panels.push(*index);
            continue;
        }
        if !frame.panels.is_empty() {
            let panels = std::mem::take(&mut frame.panels);
            cursor_y = project_panel_group(area, cursor_y, cell_h, app, &panels, output);
        }
        let header = ItemCursor {
            depth: frame.depth,
            area,
            cursor_y,
            cell_h,
        };
        let (next_y, nested) = project_item_header(item, &header, output);
        cursor_y = next_y;
        if let Some((children, child_depth)) = nested
            && open_frames < MAX_LAYOUT_DEPTH
        {
            frames.push(ProjectFrame::new(children, child_depth));
        }
    }
    debug_assert!(cursor_y >= start_y, "the layout cursor moves down only");
    debug_assert!(
        output
            .iter()
            .skip(first_new)
            .all(|item| item.rect.y >= start_y),
        "each new item starts at or below the first row"
    );
    cursor_y
}

/// A list of items to lay out, with the run of panels that waits for its grid.
struct ProjectFrame<'a> {
    items: std::slice::Iter<'a, DashboardLayoutItem>,
    depth: usize,
    panels: Vec<usize>,
}

impl<'a> ProjectFrame<'a> {
    /// Starts a list of items at `depth`.
    fn new(items: &'a [DashboardLayoutItem], depth: usize) -> Self {
        debug_assert!(
            depth <= MAX_LAYOUT_DEPTH,
            "a nested list is not deeper than MAX_LAYOUT_DEPTH"
        );
        Self {
            items: items.iter(),
            depth,
            panels: Vec::new(),
        }
    }
}

/// The position of the next item in a dashboard projection.
struct ItemCursor {
    depth: usize,
    area: Rect,
    cursor_y: u16,
    cell_h: u16,
}

/// Child items to lay out below a header, and the depth for them.
type NestedItems<'a> = Option<(&'a [DashboardLayoutItem], usize)>;

/// Lays out a non-panel item and returns the next y and the child items to lay out.
fn project_item_header<'a>(
    item: &'a DashboardLayoutItem,
    at: &ItemCursor,
    output: &mut Vec<DashboardRect>,
) -> (u16, NestedItems<'a>) {
    let projected = match item {
        DashboardLayoutItem::Panel(_) => (at.cursor_y, None),
        DashboardLayoutItem::AutoGrid(grid) => (
            project_auto_grid(at.area, at.cursor_y, at.cell_h, grid, output),
            None,
        ),
        DashboardLayoutItem::Row(row) if row.hidden_header => {
            (at.cursor_y, Some((row.children.as_slice(), at.depth)))
        }
        DashboardLayoutItem::Row(row) => {
            let rect = Rect::new(at.area.x, at.cursor_y, at.area.width, 1);
            output.push(DashboardRect {
                id: DashboardItemId::Row(row.id),
                rect,
                disclosure_rect: Some(Rect::new(rect.x, rect.y, rect.width.min(1), 1)),
                kind: DashboardRectKind::Row {
                    row_id: row.id,
                    depth: at.depth,
                    collapsed: row.collapsed,
                },
            });
            let nested = (!row.collapsed).then_some((row.children.as_slice(), at.depth + 1));
            (at.cursor_y.saturating_add(1), nested)
        }
        DashboardLayoutItem::Tabs(group) => project_tabs_header(group, at, output),
    };
    debug_assert!(
        projected.0 >= at.cursor_y,
        "the layout cursor moves down only"
    );
    debug_assert!(
        projected.1.is_none_or(|(_, depth)| depth <= at.depth + 1),
        "a child list is one level deeper at most"
    );
    projected
}

/// Lays out a tab group bar, and the empty tab message when the active tab is empty.
fn project_tabs_header<'a>(
    group: &'a DashboardTabs,
    at: &ItemCursor,
    output: &mut Vec<DashboardRect>,
) -> (u16, NestedItems<'a>) {
    debug_assert!(
        group.active.is_none_or(|active| active < group.tabs.len()),
        "the active tab is in the tab list"
    );
    debug_assert!(
        at.depth <= MAX_LAYOUT_DEPTH,
        "the tab group is not deeper than MAX_LAYOUT_DEPTH"
    );
    let mut cursor_y = at.cursor_y;
    let rect = Rect::new(at.area.x, cursor_y, at.area.width, 1);
    output.push(DashboardRect {
        id: DashboardItemId::Tabs(group.id),
        rect,
        disclosure_rect: None,
        kind: DashboardRectKind::Tabs {
            group_id: group.id,
            depth: at.depth,
        },
    });
    cursor_y = cursor_y.saturating_add(1);
    let Some(tab) = group.active.and_then(|index| group.tabs.get(index)) else {
        return (cursor_y, None);
    };
    if !tab.children.is_empty() {
        return (cursor_y, Some((tab.children.as_slice(), at.depth + 1)));
    }
    let rect = Rect::new(at.area.x, cursor_y, at.area.width, 1);
    output.push(DashboardRect {
        id: DashboardItemId::Tabs(group.id),
        rect,
        disclosure_rect: None,
        kind: DashboardRectKind::TabEmpty { group_id: group.id },
    });
    (cursor_y.saturating_add(1), None)
}

fn project_panel_group(
    area: Rect,
    origin_y: u16,
    cell_h: u16,
    app: &AppState,
    panel_indices: &[usize],
    output: &mut Vec<DashboardRect>,
) -> u16 {
    debug_assert!(cell_h >= 1, "a grid row is one terminal row or more");
    let has_grid = panel_indices.iter().any(|&index| {
        app.panels
            .get(index)
            .is_some_and(|panel| panel.grid.is_some())
    });
    if !has_grid {
        return project_panel_columns(area, origin_y, panel_indices, output);
    }

    let cell_w = std::cmp::max(1, area.width / 24);
    let mut grid_height = 0u16;
    let mut extras = Vec::new();
    for &index in panel_indices {
        let Some(panel) = app.panels.get(index) else {
            continue;
        };
        let Some(grid) = panel.grid else {
            extras.push(index);
            continue;
        };
        if grid.x < 0 || grid.y < 0 || grid.w <= 0 || grid.h <= 0 {
            continue;
        }
        if let Some(rect) = project_grid_rect(area, origin_y, cell_w, cell_h, grid) {
            output.push(panel_rect(index, rect));
        }
        grid_height = grid_height.max(
            num::i32_to_u16(grid.y)
                .saturating_add(num::i32_to_u16(grid.h))
                .saturating_mul(cell_h),
        );
    }

    let extras_origin = origin_y.saturating_add(grid_height);
    let next_y = project_panel_columns(area, extras_origin, &extras, output);
    debug_assert!(next_y >= origin_y, "the layout cursor moves down only");
    next_y
}

/// Returns the rect of a grid panel, or `None` when it is too small to draw.
fn project_grid_rect(
    area: Rect,
    origin_y: u16,
    cell_w: u16,
    cell_h: u16,
    grid: GridUnit,
) -> Option<Rect> {
    debug_assert!(
        grid.x >= 0 && grid.y >= 0 && grid.w > 0 && grid.h > 0,
        "the caller skips a grid position that is negative or empty"
    );
    debug_assert!(
        cell_w >= 1 && cell_h >= 1,
        "a grid cell is one column and one row or more"
    );
    let x = area
        .x
        .saturating_add(num::i32_to_u16(grid.x).saturating_mul(cell_w));
    let y = origin_y.saturating_add(num::i32_to_u16(grid.y).saturating_mul(cell_h));
    let rect = Rect::new(
        x,
        y,
        num::i32_to_u16(grid.w)
            .saturating_mul(cell_w)
            .min(area.right().saturating_sub(x)),
        num::i32_to_u16(grid.h).saturating_mul(cell_h),
    );
    debug_assert!(
        rect.is_empty() || rect.right() <= area.right(),
        "the panel is inside the columns of the area"
    );
    (rect.width >= 8 && rect.height >= 4).then_some(rect)
}

/// Lays out panels in two columns from `origin_y` and returns the y below the last row.
fn project_panel_columns(
    area: Rect,
    origin_y: u16,
    panel_indices: &[usize],
    output: &mut Vec<DashboardRect>,
) -> u16 {
    let first_new = output.len();
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(Rect::new(area.x, origin_y, area.width, u16::MAX - origin_y));
    for (position, &index) in panel_indices.iter().enumerate() {
        let row = num::usize_to_u16(position / 2);
        let column = if position % 2 == 0 { left } else { right };
        let rect = Rect::new(
            column.x,
            origin_y.saturating_add(row.saturating_mul(12)),
            column.width,
            12,
        );
        output.push(panel_rect(index, rect));
    }
    debug_assert!(
        output
            .iter()
            .skip(first_new)
            .all(|item| item.rect.width <= area.width.div_ceil(2)),
        "each column is half of the area or less"
    );
    let rows = num::usize_to_u16(panel_indices.len().div_ceil(2));
    let next_y = origin_y.saturating_add(rows.saturating_mul(12));
    debug_assert!(next_y >= origin_y, "the layout cursor moves down only");
    next_y
}

/// Lays out auto grid panels row-major in equal-width columns of `row_height` grid
/// units each, returning the y coordinate below the last row.
fn project_auto_grid(
    area: Rect,
    origin_y: u16,
    cell_h: u16,
    grid: &DashboardAutoGrid,
    output: &mut Vec<DashboardRect>,
) -> u16 {
    if grid.panels.is_empty() {
        return origin_y;
    }
    let columns = grid.column_count(area.width);
    debug_assert!(columns >= 1, "the auto grid has one column or more");
    let column_rects = Layout::horizontal(vec![
        Constraint::Ratio(1, u32::from(columns));
        usize::from(columns)
    ])
    .split(Rect::new(area.x, origin_y, area.width, 1));
    debug_assert_eq!(
        column_rects.len(),
        usize::from(columns),
        "the layout gives one rect for each column"
    );
    let row_height = grid.row_height.saturating_mul(cell_h);
    for (position, &index) in grid.panels.iter().enumerate() {
        let row = u16::try_from(position / usize::from(columns)).unwrap_or(u16::MAX);
        let Some(column) = column_rects.get(position % usize::from(columns)) else {
            continue;
        };
        output.push(panel_rect(
            index,
            Rect::new(
                column.x,
                origin_y.saturating_add(row.saturating_mul(row_height)),
                column.width,
                row_height,
            ),
        ));
    }
    let rows = u16::try_from(grid.panels.len().div_ceil(usize::from(columns))).unwrap_or(u16::MAX);
    let next_y = origin_y.saturating_add(rows.saturating_mul(row_height));
    debug_assert!(next_y >= origin_y, "the layout cursor moves down only");
    next_y
}

fn panel_rect(index: usize, rect: Rect) -> DashboardRect {
    DashboardRect {
        id: DashboardItemId::Panel(index),
        rect,
        disclosure_rect: None,
        kind: DashboardRectKind::Panel { index },
    }
}

fn clip_scrolled_rect(
    mut item: DashboardRect,
    area: Rect,
    scroll_offset: u16,
) -> Option<DashboardRect> {
    let columns = (item.rect.x, item.rect.width);
    let top = item.rect.y.saturating_sub(scroll_offset);
    let bottom = item.rect.bottom().saturating_sub(scroll_offset);
    if bottom <= area.y || top >= area.bottom() {
        return None;
    }
    item.rect.y = top.max(area.y);
    item.rect.height = bottom.min(area.bottom()).saturating_sub(item.rect.y);
    item.disclosure_rect = item
        .disclosure_rect
        .and_then(|rect| {
            rect.y
                .checked_sub(scroll_offset)
                .map(|y| Rect { y, ..rect })
        })
        .filter(|rect| {
            area.contains(Position {
                x: rect.x,
                y: rect.y,
            })
        });
    debug_assert!(
        item.rect.y >= area.y && item.rect.bottom() <= area.bottom(),
        "the clipped item is inside the rows of the area"
    );
    debug_assert!(
        item.disclosure_rect
            .is_none_or(|rect| rect.y >= item.rect.y && rect.y < item.rect.bottom()),
        "the disclosure target is on a row of its item"
    );
    debug_assert!(
        (item.rect.x, item.rect.width) == columns,
        "the scroll changes only the rows of the item"
    );
    Some(item)
}

/// Determines which visible dashboard item is located at the given coordinates.
///
/// # Arguments
///
/// * `app` - The application state.
/// * `area` - The total area available for charts.
/// * `x` - The x-coordinate of the mouse event.
/// * `y` - The y-coordinate of the mouse event.
///
/// # Returns
///
/// A copy of the hit rectangle, including its disclosure target for rows.
pub(crate) fn hit_test(app: &AppState, area: Rect, x: u16, y: u16) -> Option<DashboardRect> {
    let inner_area = dashboard_inner_area(area);

    if !inner_area.contains(ratatui::layout::Position { x, y }) {
        return None;
    }

    let hit = visible_dashboard_rects(area, app).into_iter().find(|item| {
        !matches!(item.kind, DashboardRectKind::TabEmpty { .. })
            && item.rect.contains(ratatui::layout::Position { x, y })
    });
    debug_assert!(
        hit.is_none_or(|item| rect_inside(inner_area, item.rect)),
        "the hit item is inside the dashboard body"
    );
    hit
}

/// Layout helpers for tests in other modules.
#[cfg(test)]
pub(crate) mod test_support {
    use super::{AppState, DashboardRectKind, Rect, visible_dashboard_rects};

    /// Returns the rectangle and index of each visible panel.
    pub(crate) fn visible_panel_rects(area: Rect, app: &AppState) -> Vec<(Rect, usize)> {
        visible_dashboard_rects(area, app)
            .into_iter()
            .filter_map(|item| match item.kind {
                DashboardRectKind::Panel { index } => Some((item.rect, index)),
                DashboardRectKind::Row { .. }
                | DashboardRectKind::Tabs { .. }
                | DashboardRectKind::TabEmpty { .. } => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{GridUnit, PanelOptions, PanelType, YAxisMode},
        dashboard::{
            DashboardAutoGrid, DashboardItemId, DashboardLayout, DashboardLayoutItem, DashboardRow,
            DashboardTab, DashboardTabs, RowId, TabGroupId,
        },
        export::ExportOptions,
        prom::PromClient,
        theme::Theme,
        ui::DisplayFormat,
    };
    use std::time::Duration;

    fn panel(title: &str, grid: Option<GridUnit>) -> PanelState {
        PanelState {
            title: title.to_string(),
            exprs: vec![],
            legends: vec![],
            query_modes: vec![],
            series: vec![],
            last_error: None,
            last_url: None,
            last_samples: 0,
            grid,
            y_axis_mode: YAxisMode::Auto,
            panel_type: PanelType::Graph,
            thresholds: None,
            min: None,
            max: None,
            autogrid: None,
            display: DisplayFormat::default(),
            options: PanelOptions::None,
            resolution: Default::default(),
            notices: Default::default(),
        }
    }

    fn app_with(panels: Vec<PanelState>, layout: DashboardLayout) -> AppState {
        let mut app = AppState::new(
            PromClient::new("http://127.0.0.1:9".to_string()),
            Duration::from_secs(300),
            Duration::from_secs(5),
            Duration::from_secs(60),
            "Rows".to_string(),
            panels,
            0,
            Theme::default(),
            "dashed".to_string(),
            ExportOptions::default(),
        );
        app.apply_layout(layout);
        app
    }

    fn nested_row_app() -> AppState {
        app_with(
            vec![
                panel(
                    "Visible child",
                    Some(GridUnit {
                        x: 0,
                        y: 0,
                        w: 24,
                        h: 2,
                    }),
                ),
                panel(
                    "Collapsed child",
                    Some(GridUnit {
                        x: 0,
                        y: 0,
                        w: 24,
                        h: 2,
                    }),
                ),
            ],
            DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
                RowId::new(0),
                "Expanded",
                false,
                false,
                vec![
                    DashboardLayoutItem::Panel(0),
                    DashboardLayoutItem::Row(DashboardRow::new(
                        RowId::new(1),
                        "Nested",
                        true,
                        false,
                        vec![DashboardLayoutItem::Panel(1)],
                    )),
                ],
            ))]),
        )
    }

    #[test]
    fn row_projection_offsets_nested_grid_panels_and_collapses_subtrees() {
        let app = nested_row_app();

        let rects = visible_dashboard_rects(Rect::new(0, 0, 120, 50), &app);

        assert_eq!(rects.len(), 3);
        assert!(matches!(
            rects[0].kind,
            DashboardRectKind::Row {
                row_id,
                depth: 0,
                collapsed: false,
            } if row_id == RowId::new(0)
        ));
        assert_eq!(rects[0].rect.height, 1);
        assert!(matches!(
            rects[1].kind,
            DashboardRectKind::Panel { index: 0 }
        ));
        assert_eq!(rects[1].id, DashboardItemId::Panel(0));
        assert_eq!(rects[1].rect.y, rects[0].rect.bottom());
        assert!(matches!(
            rects[2].kind,
            DashboardRectKind::Row {
                row_id,
                depth: 1,
                collapsed: true,
            } if row_id == RowId::new(1)
        ));
        assert_eq!(rects[2].rect.y, rects[1].rect.bottom());
        assert!(
            rects
                .iter()
                .all(|item| item.id != DashboardItemId::Panel(1))
        );
    }

    fn auto_grid_app(panel_count: usize, trailing_row: bool) -> AppState {
        let mut items = vec![DashboardLayoutItem::AutoGrid(DashboardAutoGrid {
            panels: (0..panel_count).collect(),
            max_columns: 3,
            min_column_width: 56,
            row_height: 2,
        })];
        if trailing_row {
            items.push(DashboardLayoutItem::Row(DashboardRow::new(
                RowId::new(0),
                "After",
                true,
                false,
                vec![],
            )));
        }
        app_with(
            (0..panel_count)
                .map(|index| panel(&format!("Panel {index}"), None))
                .collect(),
            DashboardLayout::new(items),
        )
    }

    fn panel_rects(rects: &[DashboardRect]) -> Vec<Rect> {
        rects
            .iter()
            .filter(|item| matches!(item.kind, DashboardRectKind::Panel { .. }))
            .map(|item| item.rect)
            .collect()
    }

    #[test]
    fn auto_grid_reflows_columns_with_terminal_width() {
        let app = auto_grid_app(4, false);

        // The dashboard body is the terminal width minus a one-cell border, and each
        // 56-cell column needs a one-cell gap: 118 cells fit two, 198 fit three.
        for (width, columns) in [(80, 1), (120, 2), (200, 3)] {
            let area = Rect::new(0, 0, width, 50);
            let inner = dashboard_inner_area(area);
            let rects = panel_rects(&visible_dashboard_rects(area, &app));

            assert_eq!(rects.len(), 4, "{width}");
            let first_row: Vec<_> = rects.iter().filter(|rect| rect.y == rects[0].y).collect();
            assert_eq!(first_row.len(), columns, "{width}");
            assert_eq!(first_row[0].x, inner.x, "{width}");
            assert_eq!(first_row.last().unwrap().right(), inner.right(), "{width}");
            // Row-major order: the panel after a full row starts the next one.
            assert_eq!(rects[columns].x, inner.x, "{width}");
            assert_eq!(rects[columns].y, rects[0].bottom(), "{width}");
            // Two grid units of three rows each.
            assert!(rects.iter().all(|rect| rect.height == 6), "{width}");
        }
    }

    #[test]
    fn auto_grid_stretches_panels_when_fewer_than_columns() {
        let app = auto_grid_app(2, false);
        let area = Rect::new(0, 0, 200, 50);
        let inner = dashboard_inner_area(area);

        let rects = panel_rects(&visible_dashboard_rects(area, &app));

        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].y, rects[1].y);
        assert_eq!(rects[0].width + rects[1].width, inner.width);
        assert_eq!(rects[1].right(), inner.right());
    }

    #[test]
    fn content_after_an_auto_grid_starts_below_its_last_row() {
        let app = auto_grid_app(3, true);

        let rects = visible_dashboard_rects(Rect::new(0, 0, 120, 50), &app);

        let panels = panel_rects(&rects);
        let row = rects
            .iter()
            .find(|item| item.id == DashboardItemId::Row(RowId::new(0)))
            .unwrap();
        assert_eq!(
            row.rect.y,
            panels.iter().map(|rect| rect.bottom()).max().unwrap()
        );
    }

    #[test]
    fn hidden_header_row_is_layout_transparent() {
        let grid = GridUnit {
            x: 3,
            y: 2,
            w: 9,
            h: 2,
        };
        let hidden = app_with(
            vec![panel("Panel", Some(grid))],
            DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
                RowId::new(0),
                "Hidden",
                true,
                true,
                vec![DashboardLayoutItem::Panel(0)],
            ))]),
        );
        let flat = app_with(vec![panel("Panel", Some(grid))], DashboardLayout::flat(1));

        let hidden_rects = visible_dashboard_rects(Rect::new(0, 0, 120, 50), &hidden);
        let flat_rects = visible_dashboard_rects(Rect::new(0, 0, 120, 50), &flat);

        assert_eq!(hidden_rects.len(), 1);
        assert_eq!(hidden_rects[0].id, DashboardItemId::Panel(0));
        assert_eq!(hidden_rects[0].rect, flat_rects[0].rect);
        assert!(hidden_rects[0].disclosure_rect.is_none());
    }

    #[test]
    fn sibling_hidden_header_rows_flow_their_local_grid_children() {
        let grid = GridUnit {
            x: 0,
            y: 0,
            w: 24,
            h: 2,
        };
        let app = app_with(
            vec![
                panel("First child", Some(grid)),
                panel("Second child", Some(grid)),
            ],
            DashboardLayout::new(vec![
                DashboardLayoutItem::Row(DashboardRow::new(
                    RowId::new(0),
                    "Hidden first",
                    false,
                    true,
                    vec![DashboardLayoutItem::Panel(0)],
                )),
                DashboardLayoutItem::Row(DashboardRow::new(
                    RowId::new(1),
                    "Hidden second",
                    false,
                    true,
                    vec![DashboardLayoutItem::Panel(1)],
                )),
            ]),
        );

        let rects = visible_dashboard_rects(Rect::new(0, 0, 120, 50), &app);

        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].id, DashboardItemId::Panel(0));
        assert_eq!(rects[1].id, DashboardItemId::Panel(1));
        assert!(rects[0].rect.bottom() <= rects[1].rect.y);
    }

    #[test]
    fn several_dashboards_pin_their_tab_bar_above_the_scrolled_content() {
        use crate::app::dashboard_test_support::{dashboards_app, v2_dashboard};
        let nodes = v2_dashboard("Ops / Nodes", "30s", "up", "label_values(up, instance)");
        let consul = v2_dashboard("Ops / Consul", "30s", "up", "label_values(up, instance)");
        let mut app = dashboards_app("http://127.0.0.1:9", &[&nodes, &consul]);
        let group = app.dashboard_group().unwrap();
        let area = Rect::new(0, 0, 120, 50);
        let inner = dashboard_inner_area(area);

        let rects = visible_dashboard_rects(area, &app);
        assert!(matches!(
            rects[0].kind,
            DashboardRectKind::Tabs { group_id, depth: 0 } if group_id == group
        ));
        assert_eq!(rects[0].rect, Rect::new(inner.x, inner.y, inner.width, 1));
        // The dashboard's own items follow, laid out as a dashboard of their own.
        assert_eq!(rects[1].id, DashboardItemId::Panel(0));
        assert_eq!(rects[1].rect.y, inner.y + 1);
        let panel = rects[1].rect;

        // Scrolling moves the content under the bar, which stays.
        app.vertical_scroll = 1;
        let rects = visible_dashboard_rects(area, &app);
        assert_eq!(rects[0].rect, Rect::new(inner.x, inner.y, inner.width, 1));
        assert_eq!(rects[1].rect.y, inner.y + 1);
        assert!(rects[1].rect.height < panel.height);
        let hit = hit_test(&app, area, inner.x + 3, inner.y).unwrap();
        assert_eq!(hit.id, DashboardItemId::Tabs(group));

        // The bar is always in view, so selecting it does not scroll.
        app.selected_item = Some(DashboardItemId::Tabs(group));
        scroll_selected_into_view(area, &mut app);
        assert_eq!(app.vertical_scroll, 1);
    }

    #[test]
    fn tabs_project_only_active_content_below_the_bar() {
        let grid = GridUnit {
            x: 0,
            y: 0,
            w: 24,
            h: 2,
        };
        let id = TabGroupId::new(0);
        let app = app_with(
            vec![panel("CPU", Some(grid)), panel("Memory", Some(grid))],
            DashboardLayout::new(vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
                id,
                vec![
                    DashboardTab {
                        title: "CPU".into(),
                        children: vec![DashboardLayoutItem::Panel(0)],
                    },
                    DashboardTab {
                        title: "Memory".into(),
                        children: vec![DashboardLayoutItem::Panel(1)],
                    },
                ],
            ))]),
        );

        let rects = visible_dashboard_rects(Rect::new(0, 0, 120, 50), &app);

        assert!(matches!(
            rects[0].kind,
            DashboardRectKind::Tabs {
                group_id,
                depth: 0
            } if group_id == id
        ));
        assert_eq!(rects[1].id, DashboardItemId::Panel(0));
        assert!(rects[0].rect.bottom() <= rects[1].rect.y);
        assert!(
            rects
                .iter()
                .all(|item| item.id != DashboardItemId::Panel(1))
        );
    }

    #[test]
    fn empty_active_tab_gets_decorative_non_hit_testable_line() {
        let id = TabGroupId::new(0);
        let app = app_with(
            vec![],
            DashboardLayout::new(vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
                id,
                vec![DashboardTab {
                    title: "Empty".into(),
                    children: vec![],
                }],
            ))]),
        );
        let area = Rect::new(0, 0, 80, 20);
        let rects = visible_dashboard_rects(area, &app);
        assert_eq!(rects.len(), 2);
        assert!(
            matches!(rects[1].kind, DashboardRectKind::TabEmpty { group_id } if group_id == id)
        );
        assert!(hit_test(&app, area, rects[1].rect.x, rects[1].rect.y).is_none());
    }

    #[test]
    fn row_free_projection_preserves_legacy_panel_vector_growth() {
        let mut app = app_with(vec![panel("CPU", None)], DashboardLayout::flat(1));
        app.panels.push(panel("Memory", None));

        let rects = visible_dashboard_rects(Rect::new(0, 0, 120, 50), &app);
        let panel_indices = rects
            .into_iter()
            .filter_map(|item| match item.kind {
                DashboardRectKind::Panel { index } => Some(index),
                DashboardRectKind::Row { .. }
                | DashboardRectKind::Tabs { .. }
                | DashboardRectKind::TabEmpty { .. } => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(panel_indices, vec![0, 1]);
    }

    #[test]
    fn hit_test_returns_row_and_separate_disclosure_target() {
        let app = nested_row_app();
        let area = Rect::new(0, 0, 100, 40);
        let row = visible_dashboard_rects(area, &app)
            .into_iter()
            .find(|item| item.id == DashboardItemId::Row(RowId::new(0)))
            .unwrap();
        let disclosure = row.disclosure_rect.unwrap();

        let disclosure_hit = hit_test(&app, area, disclosure.x, disclosure.y).unwrap();
        let header_hit = hit_test(&app, area, row.rect.right() - 1, row.rect.y).unwrap();

        assert_eq!(disclosure_hit.id, DashboardItemId::Row(RowId::new(0)));
        assert_eq!(disclosure_hit.disclosure_rect, Some(disclosure));
        assert_eq!(header_hit.id, DashboardItemId::Row(RowId::new(0)));
        assert!(!disclosure.contains(Position {
            x: row.rect.right() - 1,
            y: row.rect.y,
        }));
    }
}
