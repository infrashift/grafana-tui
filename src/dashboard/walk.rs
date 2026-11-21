//! A bounded walk over a layout tree with an explicit stack in place of recursion.

use std::slice;

use super::{DashboardLayoutItem, DashboardTab, MAX_LAYOUT_DEPTH, MAX_LAYOUT_STEPS};

/// The items that a layout walk visits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WalkScope {
    /// Every item, also the items in collapsed rows and inactive tabs.
    All,
    /// The items on screen. The walk skips collapsed rows and inactive tabs.
    Visible,
}

/// One item that a layout walk visits.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WalkEvent<'a> {
    pub(crate) item: &'a DashboardLayoutItem,
    /// The display depth. The children of a row with a hidden header stay at
    /// the depth of the row.
    pub(crate) depth: usize,
    /// The level of the list that holds the item. The top-level list is level 1.
    pub(crate) level: usize,
}

/// One open child list of a walk.
struct Frame<'a> {
    items: slice::Iter<'a, DashboardLayoutItem>,
    /// The tabs of the same group that the walk visits after `items`.
    tabs: slice::Iter<'a, DashboardTab>,
    depth: usize,
}

/// A pre-order walk over a layout. The stack holds at most MAX_LAYOUT_DEPTH
/// frames, one for each open child list.
pub(crate) struct LayoutWalk<'a> {
    scope: WalkScope,
    frames: Vec<Frame<'a>>,
    /// The item that the walk gave last. The next step opens its children.
    last: Option<WalkEvent<'a>>,
    truncated: bool,
}

impl<'a> LayoutWalk<'a> {
    /// Starts a walk over `items` at depth 0.
    pub(crate) fn new(items: &'a [DashboardLayoutItem], scope: WalkScope) -> Self {
        let mut frames = Vec::with_capacity(MAX_LAYOUT_DEPTH);
        frames.push(Frame {
            items: items.iter(),
            tabs: slice::Iter::default(),
            depth: 0,
        });
        Self {
            scope,
            frames,
            last: None,
            truncated: false,
        }
    }

    /// Whether the walk skipped items at MAX_LAYOUT_DEPTH or MAX_LAYOUT_STEPS.
    pub(crate) fn truncated(&self) -> bool {
        self.truncated
    }

    /// Opens the children of `event` that the scope includes.
    fn open(&mut self, event: WalkEvent<'a>) {
        let empty = slice::Iter::default();
        let (items, tabs, depth) = match (event.item, self.scope) {
            (DashboardLayoutItem::Row(row), _) if row.hidden_header => {
                (row.children.iter(), empty, event.depth)
            }
            (DashboardLayoutItem::Row(row), scope) if scope == WalkScope::All || !row.collapsed => {
                (row.children.iter(), empty, event.depth + 1)
            }
            (DashboardLayoutItem::Tabs(group), WalkScope::All) => {
                (slice::Iter::default(), group.tabs.iter(), event.depth + 1)
            }
            (DashboardLayoutItem::Tabs(group), WalkScope::Visible) => {
                match group.active.and_then(|index| group.tabs.get(index)) {
                    Some(tab) => (tab.children.iter(), empty, event.depth + 1),
                    None => return,
                }
            }
            _ => return,
        };
        if items.as_slice().is_empty() && tabs.as_slice().is_empty() {
            return;
        }
        if self.frames.len() >= MAX_LAYOUT_DEPTH {
            self.truncated = true;
            return;
        }
        self.frames.push(Frame { items, tabs, depth });
    }
}

impl<'a> Iterator for LayoutWalk<'a> {
    type Item = WalkEvent<'a>;

    fn next(&mut self) -> Option<WalkEvent<'a>> {
        if let Some(last) = self.last.take() {
            self.open(last);
        }
        // Each pass gives an item, moves to the next tab, or closes a frame.
        for _ in 0..MAX_LAYOUT_STEPS {
            let level = self.frames.len();
            let frame = self.frames.last_mut()?;
            if let Some(item) = frame.items.next() {
                let event = WalkEvent {
                    item,
                    depth: frame.depth,
                    level,
                };
                self.last = Some(event);
                return Some(event);
            }
            if let Some(tab) = frame.tabs.next() {
                frame.items = tab.children.iter();
            } else {
                self.frames.pop();
            }
        }
        self.truncated = true;
        self.frames.clear();
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::{DashboardItemId, DashboardRow, DashboardTabs, RowId, TabGroupId};

    fn row(
        id: usize,
        collapsed: bool,
        hidden: bool,
        children: Vec<DashboardLayoutItem>,
    ) -> DashboardLayoutItem {
        DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(id),
            "Row",
            collapsed,
            hidden,
            children,
        ))
    }

    fn tabs(
        id: usize,
        active: Option<usize>,
        tabs: Vec<Vec<DashboardLayoutItem>>,
    ) -> DashboardLayoutItem {
        let tabs = tabs
            .into_iter()
            .map(|children| DashboardTab {
                title: "Tab".into(),
                children,
            })
            .collect();
        let mut group = DashboardTabs::new(TabGroupId::new(id), tabs);
        group.active = active;
        DashboardLayoutItem::Tabs(group)
    }

    fn id(item: &DashboardLayoutItem) -> DashboardItemId {
        match item {
            DashboardLayoutItem::Row(row) => DashboardItemId::Row(row.id),
            DashboardLayoutItem::Tabs(group) => DashboardItemId::Tabs(group.id),
            DashboardLayoutItem::Panel(index) => DashboardItemId::Panel(*index),
            DashboardLayoutItem::AutoGrid(grid) => DashboardItemId::Panel(grid.panels[0]),
        }
    }

    fn walk(
        items: &[DashboardLayoutItem],
        scope: WalkScope,
    ) -> Vec<(DashboardItemId, usize, usize)> {
        LayoutWalk::new(items, scope)
            .map(|event| (id(event.item), event.depth, event.level))
            .collect()
    }

    fn sample() -> Vec<DashboardLayoutItem> {
        vec![
            DashboardLayoutItem::Panel(0),
            row(0, true, false, vec![DashboardLayoutItem::Panel(1)]),
            row(1, true, true, vec![DashboardLayoutItem::Panel(2)]),
            tabs(
                0,
                Some(1),
                vec![
                    vec![DashboardLayoutItem::Panel(3)],
                    vec![],
                    vec![row(2, false, false, vec![DashboardLayoutItem::Panel(4)])],
                ],
            ),
            DashboardLayoutItem::Panel(5),
        ]
    }

    #[test]
    fn all_scope_visits_every_item_in_pre_order() {
        use DashboardItemId::{Panel, Row, Tabs};
        assert_eq!(
            walk(&sample(), WalkScope::All),
            vec![
                (Panel(0), 0, 1),
                (Row(RowId::new(0)), 0, 1),
                (Panel(1), 1, 2),
                (Row(RowId::new(1)), 0, 1),
                (Panel(2), 0, 2),
                (Tabs(TabGroupId::new(0)), 0, 1),
                (Panel(3), 1, 2),
                (Row(RowId::new(2)), 1, 2),
                (Panel(4), 2, 3),
                (Panel(5), 0, 1),
            ]
        );
    }

    #[test]
    fn visible_scope_skips_collapsed_rows_and_inactive_tabs() {
        use DashboardItemId::{Panel, Row, Tabs};
        // A hidden header keeps its children on screen at its own depth.
        assert_eq!(
            walk(&sample(), WalkScope::Visible),
            vec![
                (Panel(0), 0, 1),
                (Row(RowId::new(0)), 0, 1),
                (Row(RowId::new(1)), 0, 1),
                (Panel(2), 0, 2),
                (Tabs(TabGroupId::new(0)), 0, 1),
                (Panel(5), 0, 1),
            ]
        );
        let no_active = [tabs(1, None, vec![vec![DashboardLayoutItem::Panel(7)]])];
        assert_eq!(walk(&no_active, WalkScope::Visible).len(), 1);
    }

    /// Rows nested `levels` deep: the innermost list holds one panel.
    fn nested_rows(levels: usize) -> Vec<DashboardLayoutItem> {
        let mut items = vec![DashboardLayoutItem::Panel(0)];
        for id in 1..levels {
            items = vec![row(id, false, false, items)];
        }
        items
    }

    #[test]
    fn walk_stops_at_the_depth_limit() {
        let deep = nested_rows(MAX_LAYOUT_DEPTH);
        let mut walk = LayoutWalk::new(&deep, WalkScope::All);
        assert_eq!(walk.by_ref().count(), MAX_LAYOUT_DEPTH);
        assert!(!walk.truncated());

        let too_deep = nested_rows(MAX_LAYOUT_DEPTH + 4);
        let mut walk = LayoutWalk::new(&too_deep, WalkScope::All);
        let levels: Vec<usize> = walk.by_ref().map(|event| event.level).collect();
        assert_eq!(levels.len(), MAX_LAYOUT_DEPTH);
        assert_eq!(levels.last(), Some(&MAX_LAYOUT_DEPTH));
        assert!(walk.truncated());
    }

    #[test]
    fn empty_child_lists_open_no_frame() {
        let mut items = vec![tabs(0, Some(0), vec![]), row(0, false, false, vec![])];
        for id in 1..MAX_LAYOUT_DEPTH {
            items = vec![row(id, false, false, items)];
        }
        let mut walk = LayoutWalk::new(&items, WalkScope::All);
        assert_eq!(walk.by_ref().count(), MAX_LAYOUT_DEPTH + 1);
        assert!(!walk.truncated());
    }

    #[test]
    fn walk_stops_after_the_step_limit() {
        let empty = vec![vec![]; MAX_LAYOUT_STEPS + 1];
        let items = [tabs(0, Some(0), empty), DashboardLayoutItem::Panel(0)];
        let mut walk = LayoutWalk::new(&items, WalkScope::All);
        assert_eq!(walk.by_ref().count(), 1);
        assert!(walk.truncated());
        assert!(walk.next().is_none());
    }
}
