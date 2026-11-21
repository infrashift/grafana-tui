use std::collections::{HashMap, HashSet};

use crate::conditions::ConditionTarget;

mod walk;

pub(crate) use walk::{LayoutWalk, WalkScope, node_count};

/// The most levels of child lists in a layout. The top-level list is level 1.
///
/// The example dashboards and the test fixtures use 3 levels at most, and a
/// set of dashboards adds 1 level. The limit gives a large margin.
pub(crate) const MAX_LAYOUT_DEPTH: usize = 16;

/// The most rows, tab groups, tabs, auto grids and panels in a layout.
///
/// The largest example dashboard has 46 items. The limit gives a large margin.
pub(crate) const MAX_LAYOUT_NODES: usize = 4096;

/// The most steps of a loop that builds a layout: one step for each item and
/// one step for each child list.
pub(crate) const MAX_LAYOUT_STEPS: usize = 2 * MAX_LAYOUT_NODES + 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct RowId(usize);

impl RowId {
    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TabGroupId(usize);

impl TabGroupId {
    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> usize {
        self.0
    }
}

/// A row or tab that defines variables, by its ids from the import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SectionId {
    Row(RowId),
    /// A tab by its group and position among the group's tabs.
    Tab(TabGroupId, usize),
}

/// The direction in which the layout puts the copies of a repeated panel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum RepeatDirection {
    /// Side by side across the full grid width, with a new line after `max_per_row`.
    #[default]
    Horizontal,
    /// Stacked below each other at the source panel's width.
    Vertical,
}

/// Repeats an item once per selected value of a dashboard variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Repeat {
    pub(crate) variable: String,
    pub(crate) direction: RepeatDirection,
    /// Copies per row for horizontal panel repeats; Grafana defaults to 4.
    pub(crate) max_per_row: Option<u16>,
}

impl Repeat {
    #[cfg(test)]
    pub(crate) fn new(variable: impl Into<String>) -> Self {
        let variable = variable.into();
        debug_assert!(!variable.is_empty(), "a repeat names a variable");
        debug_assert!(
            !variable.starts_with('$'),
            "a repeat names the variable without `$`"
        );
        Self {
            variable,
            direction: RepeatDirection::default(),
            max_per_row: None,
        }
    }
}

/// Repeat settings of an imported layout, keyed by the ids from the import.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Repeats {
    pub(crate) panels: HashMap<usize, Repeat>,
    pub(crate) rows: HashMap<RowId, Repeat>,
    /// Keyed by tab group and the tab's position within it.
    pub(crate) tabs: HashMap<(TabGroupId, usize), Repeat>,
}

impl Repeats {
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.panels.is_empty() && self.rows.is_empty() && self.tabs.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DashboardItemId {
    Row(RowId),
    Tabs(TabGroupId),
    Panel(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DashboardLayoutItem {
    Row(DashboardRow),
    Tabs(DashboardTabs),
    Panel(usize),
    AutoGrid(DashboardAutoGrid),
}

/// Panels flowed row-major into equal-width columns (Grafana V2 `AutoGridLayout`).
///
/// Unlike rows and tabs, an auto grid is not selectable and has no header.
/// Its panels belong to the container around it, and only their position differs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DashboardAutoGrid {
    pub(crate) panels: Vec<usize>,
    pub(crate) max_columns: u16,
    /// Minimum column width in terminal cells.
    pub(crate) min_column_width: u16,
    /// Row height in dashboard grid units, the unit of `GridPos::h`.
    pub(crate) row_height: u16,
}

impl DashboardAutoGrid {
    /// Number of columns for `width` cells.
    ///
    /// Mirrors Grafana's CSS `repeat(auto-fit, minmax(...))` track list with a
    /// one-cell gap. It gives as many minimum-width columns as fit, at most
    /// `max_columns`. It never gives more columns than panels, because `auto-fit`
    /// collapses empty tracks, and the panels stretch to fill the row.
    pub(crate) fn column_count(&self, width: u16) -> u16 {
        let fitting = width.saturating_add(1) / self.min_column_width.saturating_add(1);
        let panels = u16::try_from(self.panels.len()).unwrap_or(u16::MAX);
        fitting.min(self.max_columns).min(panels).max(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DashboardTab {
    pub(crate) title: String,
    pub(crate) children: Vec<DashboardLayoutItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DashboardTabs {
    pub(crate) id: TabGroupId,
    pub(crate) tabs: Vec<DashboardTab>,
    pub(crate) active: Option<usize>,
}

impl DashboardTabs {
    pub(crate) fn new(id: TabGroupId, tabs: Vec<DashboardTab>) -> Self {
        let active = (!tabs.is_empty()).then_some(0);
        Self { id, tabs, active }
    }

    /// Whether the active tab is a tab of the group.
    pub(crate) fn active_in_range(&self) -> bool {
        self.active.is_none_or(|index| index < self.tabs.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DashboardRow {
    pub(crate) id: RowId,
    pub(crate) title: String,
    pub(crate) collapsed: bool,
    pub(crate) hidden_header: bool,
    pub(crate) children: Vec<DashboardLayoutItem>,
}

impl DashboardRow {
    pub(crate) fn new(
        id: RowId,
        title: impl Into<String>,
        collapsed: bool,
        hidden_header: bool,
        children: Vec<DashboardLayoutItem>,
    ) -> Self {
        debug_assert!(
            !children
                .iter()
                .any(|child| matches!(child, DashboardLayoutItem::Row(row) if row.id == id)),
            "a row does not hold a row with its own id"
        );
        debug_assert!(
            children.len() <= MAX_LAYOUT_NODES,
            "the row children are in the node limit"
        );
        Self {
            id,
            title: title.into(),
            collapsed,
            hidden_header,
            children,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleDashboardItem {
    pub(crate) id: DashboardItemId,
    pub(crate) depth: usize,
}

impl VisibleDashboardItem {
    pub(crate) const fn panel(index: usize, depth: usize) -> Self {
        debug_assert!(
            depth < MAX_LAYOUT_DEPTH,
            "the display depth is in the depth limit"
        );
        debug_assert!(
            index < MAX_LAYOUT_NODES,
            "the panel index is in the node limit"
        );
        Self {
            id: DashboardItemId::Panel(index),
            depth,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DashboardLayout {
    pub(crate) items: Vec<DashboardLayoutItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LayoutChange {
    pub(crate) newly_visible_panels: Vec<usize>,
}

impl DashboardLayout {
    pub(crate) fn new(items: Vec<DashboardLayoutItem>) -> Self {
        Self { items }
    }

    pub(crate) fn flat(panel_count: usize) -> Self {
        Self::new((0..panel_count).map(DashboardLayoutItem::Panel).collect())
    }

    pub(crate) fn row(&self, id: RowId) -> Option<&DashboardRow> {
        find_row(&self.items, id)
    }

    pub(crate) fn tabs(&self, id: TabGroupId) -> Option<&DashboardTabs> {
        find_tabs(&self.items, id)
    }

    pub(crate) fn visible_items(&self) -> Vec<VisibleDashboardItem> {
        let mut visible = Vec::new();
        collect_visible_items(&self.items, &mut visible);
        visible
    }

    pub(crate) fn visible_panel_indices(&self) -> Vec<usize> {
        let visible = self.visible_items();
        let count = visible.len();
        let panels: Vec<usize> = visible
            .into_iter()
            .filter_map(|item| match item.id {
                DashboardItemId::Panel(index) => Some(index),
                DashboardItemId::Row(_) | DashboardItemId::Tabs(_) => None,
            })
            .collect();
        debug_assert!(
            panels.len() <= count,
            "each visible panel is a visible item"
        );
        panels
    }

    /// Every panel in the layout, also those in collapsed rows and
    /// inactive tabs.
    pub(crate) fn panel_indices(&self) -> Vec<usize> {
        let mut panels = Vec::new();
        collect_panel_indices(&self.items, &mut panels);
        panels
    }

    /// Every panel in one tab of a tab group, also those in its collapsed
    /// rows and inactive inner tabs.
    pub(crate) fn tab_panel_indices(&self, id: TabGroupId, index: usize) -> Vec<usize> {
        let group = self.tabs(id);
        let tab = group.and_then(|group| group.tabs.get(index));
        let mut panels = Vec::new();
        if let Some(tab) = tab {
            collect_panel_indices(&tab.children, &mut panels);
        }
        debug_assert!(
            tab.is_some() || panels.is_empty(),
            "a missing tab has no panels"
        );
        debug_assert!(
            group.is_none_or(|group| index < group.tabs.len() || panels.is_empty()),
            "a tab index out of range gives no panels"
        );
        panels
    }

    /// The tab of group `id` that each row and tab section belongs to: the
    /// group's own tabs, and every row and tab nested inside them.
    pub(crate) fn section_owners(&self, id: TabGroupId) -> HashMap<SectionId, usize> {
        let mut owners = HashMap::new();
        if let Some(group) = self.tabs(id) {
            for (index, tab) in group.tabs.iter().enumerate() {
                owners.insert(SectionId::Tab(id, index), index);
                owners.extend(
                    LayoutWalk::new(&tab.children, WalkScope::All)
                        .flat_map(|event| item_sections(event.item))
                        .map(|section| (section, index)),
                );
            }
        }
        let count = self.tabs(id).map_or(0, |group| group.tabs.len());
        debug_assert!(owners.len() >= count, "each tab of the group owns itself");
        debug_assert!(
            owners.values().all(|&tab| tab < count),
            "each owner tab index is in range"
        );
        owners
    }

    pub(crate) fn toggle_row(&mut self, id: RowId) -> Option<LayoutChange> {
        let collapsed = !self.row(id)?.collapsed;
        self.set_row_collapsed(id, collapsed)
    }

    pub(crate) fn set_row_collapsed(&mut self, id: RowId, collapsed: bool) -> Option<LayoutChange> {
        let before = self.visible_panel_indices();
        *row_collapsed_mut(&mut self.items, id)? = collapsed;
        let after = self.visible_panel_indices();
        debug_assert!(
            collapsed || before.len() <= after.len(),
            "an expanded row hides no panel"
        );
        let change = LayoutChange {
            newly_visible_panels: after
                .into_iter()
                .filter(|panel| !before.contains(panel))
                .collect(),
        };
        debug_assert!(
            !collapsed || change.newly_visible_panels.is_empty(),
            "a collapsed row shows no new panel"
        );
        Some(change)
    }

    pub(crate) fn set_active_tab(&mut self, id: TabGroupId, index: usize) -> Option<LayoutChange> {
        let tabs = self.tabs(id)?;
        if index >= tabs.tabs.len() {
            return None;
        }
        debug_assert!(tabs.active_in_range(), "the active tab is in range");
        if tabs.active == Some(index) {
            return Some(LayoutChange::default());
        }
        let before = self.visible_panel_indices();
        *tabs_active_mut(&mut self.items, id)? = Some(index);
        let after = self.visible_panel_indices();
        let change = LayoutChange {
            newly_visible_panels: after
                .into_iter()
                .filter(|panel| !before.contains(panel))
                .collect(),
        };
        debug_assert!(
            change.newly_visible_panels.is_empty() || {
                let tab_panels = self.tab_panel_indices(id, index);
                change
                    .newly_visible_panels
                    .iter()
                    .all(|panel| tab_panels.contains(panel))
            },
            "the new panels are in the new tab"
        );
        Some(change)
    }

    pub(crate) fn first_tab_descendant(&self, id: TabGroupId) -> Option<DashboardItemId> {
        let group = self.tabs(id)?;
        debug_assert!(group.active_in_range(), "the active tab is in range");
        let tab = group.active.and_then(|index| group.tabs.get(index))?;
        let mut visible = Vec::new();
        collect_visible_items(&tab.children, &mut visible);
        debug_assert!(
            visible.first().is_none_or(|item| item.depth == 0),
            "the first item of a tab is at the top depth"
        );
        visible.first().map(|item| item.id)
    }

    pub(crate) fn first_visible(&self) -> Option<DashboardItemId> {
        self.visible_items().first().map(|item| item.id)
    }

    /// Copies row collapse state and active tabs from `previous` onto the rows and
    /// tab groups with the same ids. So a layout keeps what was open when the
    /// app rebuilds it.
    pub(crate) fn restore_state(&mut self, previous: &DashboardLayout) {
        let count = self.items.len();
        let mut rows = HashMap::new();
        let mut tabs = HashMap::new();
        collect_state(&previous.items, &mut rows, &mut tabs);
        apply_state(&mut self.items, &rows, &tabs);
        debug_assert_eq!(self.items.len(), count, "the state copy keeps the items");
        debug_assert!(
            active_tabs_in_range(&self.items),
            "each active tab is in range"
        );
    }

    /// This layout without the rows, tabs, and auto grid panels in `hidden`.
    ///
    /// This function drops the tab groups and auto grids that become empty. A tab
    /// group whose active tab does not show activates its first visible tab.
    pub(crate) fn without(&self, hidden: &HashSet<ConditionTarget>) -> DashboardLayout {
        let layout = DashboardLayout::new(filter_items(&self.items, hidden));
        debug_assert!(
            layout.items.len() <= self.items.len(),
            "the filter adds no top-level item"
        );
        debug_assert!(
            active_tabs_in_range(&layout.items),
            "each active tab is in range"
        );
        layout
    }

    /// Copies row collapse state and active tabs back from `displayed`, which
    /// is this layout `without` the `hidden` items. So the state survives when
    /// the app rebuilds the displayed layout for a new set of hidden items.
    pub(crate) fn sync_state_from(
        &mut self,
        displayed: &DashboardLayout,
        hidden: &HashSet<ConditionTarget>,
    ) {
        let count = self.items.len();
        let mut rows = HashMap::new();
        let mut tabs = HashMap::new();
        collect_state(&displayed.items, &mut rows, &mut tabs);
        sync_state(&mut self.items, &rows, &tabs, hidden);
        debug_assert_eq!(self.items.len(), count, "the state copy keeps the items");
        debug_assert!(
            active_tabs_in_range(&self.items),
            "each active tab is in range"
        );
    }

    pub(crate) fn nearest_visible_ancestor(&self, id: DashboardItemId) -> Option<DashboardItemId> {
        let visible = self.visible_items();
        if visible.iter().any(|item| item.id == id) {
            return Some(id);
        }

        if let Some(ancestors) = find_ancestors(&self.items, id) {
            debug_assert!(
                ancestors.len() < MAX_LAYOUT_DEPTH,
                "the ancestors are in the depth limit"
            );
            debug_assert!(!ancestors.contains(&id), "an item is not its own ancestor");
            for ancestor in ancestors.into_iter().rev() {
                if visible.iter().any(|item| item.id == ancestor) {
                    return Some(ancestor);
                }
            }
        }

        self.first_visible()
    }
}

/// A copy of `items` without the hidden items. A stack of child lists
/// replaces recursion. The filter changes each list on its own, so the order
/// of the lists does not change the result.
fn filter_items(
    items: &[DashboardLayoutItem],
    hidden: &HashSet<ConditionTarget>,
) -> Vec<DashboardLayoutItem> {
    let mut filtered = items.to_vec();
    let mut lists: Vec<&mut Vec<DashboardLayoutItem>> = vec![&mut filtered];
    for _ in 0..MAX_LAYOUT_STEPS {
        let Some(list) = lists.pop() else {
            break;
        };
        list.retain_mut(|item| keep_item(item, hidden));
        for item in list {
            match item {
                DashboardLayoutItem::Row(row) => lists.push(&mut row.children),
                DashboardLayoutItem::Tabs(group) => {
                    lists.extend(group.tabs.iter_mut().map(|tab| &mut tab.children));
                }
                DashboardLayoutItem::Panel(_) | DashboardLayoutItem::AutoGrid(_) => {}
            }
        }
    }
    debug_assert!(lists.is_empty(), "an imported layout is within the limits");
    debug_assert!(filtered.len() <= items.len(), "the filter adds no item");
    filtered
}

/// Whether `item` stays when the `hidden` items leave the layout. Removes the
/// hidden tabs and auto grid panels of `item`.
///
/// A tab group or an auto grid with nothing left goes. A tab group whose
/// active tab goes activates the first tab that stays.
fn keep_item(item: &mut DashboardLayoutItem, hidden: &HashSet<ConditionTarget>) -> bool {
    match item {
        DashboardLayoutItem::Panel(index) => !hidden.contains(&ConditionTarget::Panel(*index)),
        DashboardLayoutItem::Row(row) => !hidden.contains(&ConditionTarget::Row(row.id)),
        DashboardLayoutItem::Tabs(group) => {
            let id = group.id;
            let shown: Vec<usize> = (0..group.tabs.len())
                .filter(|&position| !hidden.contains(&ConditionTarget::Tab(id, position)))
                .collect();
            if shown.is_empty() {
                return false;
            }
            let active = group
                .active
                .and_then(|active| shown.iter().position(|&position| position == active))
                .unwrap_or(0);
            group.tabs = std::mem::take(&mut group.tabs)
                .into_iter()
                .enumerate()
                .filter(|(position, _)| shown.contains(position))
                .map(|(_, tab)| tab)
                .collect();
            group.active = Some(active);
            debug_assert_eq!(
                group.tabs.len(),
                shown.len(),
                "the group keeps the shown tabs"
            );
            debug_assert!(active < group.tabs.len(), "the active tab is in range");
            true
        }
        DashboardLayoutItem::AutoGrid(grid) => {
            grid.panels
                .retain(|index| !hidden.contains(&ConditionTarget::Panel(*index)));
            debug_assert!(
                grid.panels
                    .iter()
                    .all(|index| !hidden.contains(&ConditionTarget::Panel(*index))),
                "the grid keeps no hidden panel"
            );
            !grid.panels.is_empty()
        }
    }
}

/// Applies displayed row and tab state to a layout that also holds `hidden`
/// items. It changes each displayed active tab back to its position among
/// all of the group's tabs.
fn sync_state(
    items: &mut [DashboardLayoutItem],
    rows: &HashMap<RowId, bool>,
    tabs: &HashMap<TabGroupId, Option<usize>>,
    hidden: &HashSet<ConditionTarget>,
) {
    for section in sections_mut(items) {
        match section {
            SectionMut::Row { id, collapsed } => {
                if let Some(&value) = rows.get(&id) {
                    *collapsed = value;
                }
            }
            SectionMut::Tabs { id, count, active } => {
                if let Some(&Some(displayed)) = tabs.get(&id) {
                    let position = (0..count)
                        .filter(|&position| !hidden.contains(&ConditionTarget::Tab(id, position)))
                        .nth(displayed);
                    debug_assert!(
                        position.is_none_or(|index| index >= displayed),
                        "a displayed tab maps to the same or a later tab"
                    );
                    *active = position.or(*active);
                    debug_assert!(
                        active.is_none_or(|index| index < count),
                        "the active tab is in range"
                    );
                }
            }
        }
    }
}

/// Records the collapse state of each row and the active tab of each tab group.
fn collect_state(
    items: &[DashboardLayoutItem],
    rows: &mut HashMap<RowId, bool>,
    tabs: &mut HashMap<TabGroupId, Option<usize>>,
) {
    let before = rows.len().saturating_add(tabs.len());
    let mut walk = LayoutWalk::new(items, WalkScope::All);
    for event in walk.by_ref() {
        match event.item {
            DashboardLayoutItem::Row(row) => {
                rows.insert(row.id, row.collapsed);
            }
            DashboardLayoutItem::Tabs(group) => {
                tabs.insert(group.id, group.active);
            }
            DashboardLayoutItem::Panel(_) | DashboardLayoutItem::AutoGrid(_) => {}
        }
    }
    debug_assert!(!walk.truncated(), "an imported layout is within the limits");
    debug_assert!(
        rows.len().saturating_add(tabs.len()) >= before,
        "the state maps do not shrink"
    );
}

/// Copies the row collapse state and the active tabs in `rows` and `tabs`
/// to the rows and tab groups of `items` with the same ids.
fn apply_state(
    items: &mut [DashboardLayoutItem],
    rows: &HashMap<RowId, bool>,
    tabs: &HashMap<TabGroupId, Option<usize>>,
) {
    for section in sections_mut(items) {
        match section {
            SectionMut::Row { id, collapsed } => {
                if let Some(&value) = rows.get(&id) {
                    *collapsed = value;
                }
            }
            SectionMut::Tabs { id, count, active } => {
                if let Some(&value) = tabs.get(&id) {
                    *active = value
                        .map(|index| index.min(count.saturating_sub(1)))
                        .filter(|_| count > 0);
                    debug_assert!(
                        active.is_none_or(|index| index < count),
                        "the active tab is in range"
                    );
                    debug_assert_eq!(
                        active.is_some(),
                        value.is_some() && count > 0,
                        "a group with tabs keeps a saved active tab"
                    );
                }
            }
        }
    }
}

/// The state of a row or a tab group, borrowed for a change.
enum SectionMut<'a> {
    Row {
        id: RowId,
        collapsed: &'a mut bool,
    },
    Tabs {
        id: TabGroupId,
        count: usize,
        active: &'a mut Option<usize>,
    },
}

/// The state of every row and tab group of `items`, in no fixed order. A stack
/// of child lists replaces recursion.
fn sections_mut(items: &mut [DashboardLayoutItem]) -> Vec<SectionMut<'_>> {
    let mut sections = Vec::new();
    let mut lists = vec![items];
    for _ in 0..MAX_LAYOUT_STEPS {
        let Some(list) = lists.pop() else {
            break;
        };
        for item in list {
            match item {
                DashboardLayoutItem::Row(DashboardRow {
                    id,
                    collapsed,
                    children,
                    ..
                }) => {
                    sections.push(SectionMut::Row { id: *id, collapsed });
                    lists.push(children);
                }
                DashboardLayoutItem::Tabs(DashboardTabs { id, tabs, active }) => {
                    let count = tabs.len();
                    sections.push(SectionMut::Tabs {
                        id: *id,
                        count,
                        active,
                    });
                    lists.extend(tabs.iter_mut().map(|tab| tab.children.as_mut_slice()));
                }
                DashboardLayoutItem::Panel(_) | DashboardLayoutItem::AutoGrid(_) => {}
            }
        }
    }
    debug_assert!(lists.is_empty(), "an imported layout is within the limits");
    debug_assert!(
        sections.len() <= MAX_LAYOUT_NODES,
        "the sections are in the node limit"
    );
    sections
}

/// The collapse state of the row with `id`.
fn row_collapsed_mut(items: &mut [DashboardLayoutItem], id: RowId) -> Option<&mut bool> {
    let empty = items.is_empty();
    let mut found = sections_mut(items)
        .into_iter()
        .filter_map(|section| match section {
            SectionMut::Row { id: row, collapsed } if row == id => Some(collapsed),
            _ => None,
        });
    let collapsed = found.next();
    let others = found.count();
    debug_assert_eq!(others, 0, "the row id is unique");
    debug_assert!(!empty || collapsed.is_none(), "an empty layout has no row");
    collapsed
}

/// The active tab of the tab group with `id`.
fn tabs_active_mut(
    items: &mut [DashboardLayoutItem],
    id: TabGroupId,
) -> Option<&mut Option<usize>> {
    let mut found = sections_mut(items)
        .into_iter()
        .filter_map(|section| match section {
            SectionMut::Tabs {
                id: group,
                count,
                active,
            } if group == id => Some((active.is_none_or(|index| index < count), active)),
            _ => None,
        });
    let first = found.next();
    let others = found.count();
    debug_assert_eq!(others, 0, "the tab group id is unique");
    debug_assert!(
        first.as_ref().is_none_or(|(in_range, _)| *in_range),
        "the active tab is in range"
    );
    first.map(|(_, active)| active)
}

/// The rows and tabs that `item` itself defines.
fn item_sections(item: &DashboardLayoutItem) -> Vec<SectionId> {
    let (sections, tabs) = match item {
        DashboardLayoutItem::Row(row) => (vec![SectionId::Row(row.id)], 0),
        DashboardLayoutItem::Tabs(group) => (
            (0..group.tabs.len())
                .map(|index| SectionId::Tab(group.id, index))
                .collect(),
            group.tabs.len(),
        ),
        DashboardLayoutItem::Panel(_) | DashboardLayoutItem::AutoGrid(_) => (Vec::new(), 0),
    };
    debug_assert!(
        sections.len() <= 1 || matches!(item, DashboardLayoutItem::Tabs(_)),
        "only a tab group defines more than one section"
    );
    debug_assert!(
        sections.iter().all(|section| match section {
            SectionId::Tab(_, index) => *index < tabs,
            SectionId::Row(_) => true,
        }),
        "each tab section index is in range"
    );
    sections
}

/// Adds every panel of `items` to `panels` in pre-order.
fn collect_panel_indices(items: &[DashboardLayoutItem], panels: &mut Vec<usize>) {
    let before = panels.len();
    let mut walk = LayoutWalk::new(items, WalkScope::All);
    for event in walk.by_ref() {
        match event.item {
            DashboardLayoutItem::Panel(index) => panels.push(*index),
            DashboardLayoutItem::AutoGrid(grid) => panels.extend(&grid.panels),
            DashboardLayoutItem::Row(_) | DashboardLayoutItem::Tabs(_) => {}
        }
    }
    debug_assert!(!walk.truncated(), "an imported layout is within the limits");
    debug_assert!(panels.len() >= before, "the panel list does not shrink");
}

/// Whether the active tab of each tab group in `items` is in range.
fn active_tabs_in_range(items: &[DashboardLayoutItem]) -> bool {
    LayoutWalk::new(items, WalkScope::All).all(
        |event| !matches!(event.item, DashboardLayoutItem::Tabs(group) if !group.active_in_range()),
    )
}

/// The row with `id`, first in pre-order.
fn find_row(items: &[DashboardLayoutItem], id: RowId) -> Option<&DashboardRow> {
    let mut walk = LayoutWalk::new(items, WalkScope::All);
    let found = walk.find_map(|event| match event.item {
        DashboardLayoutItem::Row(row) if row.id == id => Some(row),
        _ => None,
    });
    debug_assert!(
        found.is_some() || !walk.truncated(),
        "a search for a missing row sees the full layout"
    );
    debug_assert!(
        found.is_none_or(|row| row.children.len() <= MAX_LAYOUT_NODES),
        "the row children are in the node limit"
    );
    found
}

/// The tab group with `id`, first in pre-order.
fn find_tabs(items: &[DashboardLayoutItem], id: TabGroupId) -> Option<&DashboardTabs> {
    let mut walk = LayoutWalk::new(items, WalkScope::All);
    let found = walk.find_map(|event| match event.item {
        DashboardLayoutItem::Tabs(group) if group.id == id => Some(group),
        _ => None,
    });
    debug_assert!(
        found.is_some() || !walk.truncated(),
        "a search for a missing tab group sees the full layout"
    );
    debug_assert!(
        found.is_none_or(DashboardTabs::active_in_range),
        "the active tab is in range"
    );
    found
}

/// Adds the items on screen to `visible`, in pre-order, with their display depth.
fn collect_visible_items(items: &[DashboardLayoutItem], visible: &mut Vec<VisibleDashboardItem>) {
    let before = visible.len();
    let mut walk = LayoutWalk::new(items, WalkScope::Visible);
    for event in walk.by_ref() {
        let depth = event.depth;
        match event.item {
            DashboardLayoutItem::Panel(index) => {
                visible.push(VisibleDashboardItem::panel(*index, depth));
            }
            DashboardLayoutItem::Row(row) if row.hidden_header => {}
            DashboardLayoutItem::Row(row) => visible.push(VisibleDashboardItem {
                id: DashboardItemId::Row(row.id),
                depth,
            }),
            DashboardLayoutItem::Tabs(group) => visible.push(VisibleDashboardItem {
                id: DashboardItemId::Tabs(group.id),
                depth,
            }),
            DashboardLayoutItem::AutoGrid(grid) => visible.extend(
                grid.panels
                    .iter()
                    .map(|&index| VisibleDashboardItem::panel(index, depth)),
            ),
        }
    }
    debug_assert!(!walk.truncated(), "an imported layout is within the limits");
    debug_assert!(visible.len() >= before, "the visible list does not shrink");
}

/// The rows and tab groups around `target`, outermost first, or `None` when
/// `items` does not hold `target`.
fn find_ancestors(
    items: &[DashboardLayoutItem],
    target: DashboardItemId,
) -> Option<Vec<DashboardItemId>> {
    let mut path = Vec::with_capacity(MAX_LAYOUT_DEPTH);
    for event in LayoutWalk::new(items, WalkScope::All) {
        // The list at level `n` has `n - 1` rows and tab groups around it.
        path.truncate(event.level.saturating_sub(1));
        debug_assert_eq!(
            path.len(),
            event.level.saturating_sub(1),
            "the path holds one ancestor for each level above the item"
        );
        debug_assert!(
            path.len() < MAX_LAYOUT_DEPTH,
            "the path is in the depth limit"
        );
        match event.item {
            DashboardLayoutItem::Panel(index) if target == DashboardItemId::Panel(*index) => {
                return Some(path);
            }
            DashboardLayoutItem::AutoGrid(grid)
                if grid
                    .panels
                    .iter()
                    .any(|&index| target == DashboardItemId::Panel(index)) =>
            {
                return Some(path);
            }
            DashboardLayoutItem::Row(row) => {
                if target == DashboardItemId::Row(row.id) {
                    return Some(path);
                }
                path.push(DashboardItemId::Row(row.id));
            }
            DashboardLayoutItem::Tabs(group) => {
                if target == DashboardItemId::Tabs(group.id) {
                    return Some(path);
                }
                path.push(DashboardItemId::Tabs(group.id));
            }
            DashboardLayoutItem::Panel(_) | DashboardLayoutItem::AutoGrid(_) => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_switch_returns_only_revealed_panels() {
        let id = TabGroupId::new(0);
        let mut layout = DashboardLayout::new(vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
            id,
            vec![
                DashboardTab {
                    title: "A".into(),
                    children: vec![DashboardLayoutItem::Panel(0)],
                },
                DashboardTab {
                    title: "B".into(),
                    children: vec![DashboardLayoutItem::Panel(1)],
                },
            ],
        ))]);
        assert_eq!(layout.visible_panel_indices(), vec![0]);
        assert_eq!(
            layout.set_active_tab(id, 1).unwrap().newly_visible_panels,
            vec![1]
        );
        assert_eq!(layout.visible_panel_indices(), vec![1]);
        assert!(
            layout
                .set_active_tab(id, 1)
                .unwrap()
                .newly_visible_panels
                .is_empty()
        );
        assert_eq!(layout.set_active_tab(id, 2), None);
        assert_eq!(
            layout.nearest_visible_ancestor(DashboardItemId::Panel(0)),
            Some(DashboardItemId::Tabs(id))
        );
    }

    #[test]
    fn tab_panels_and_section_owners_cover_everything_nested_in_a_tab() {
        let root = TabGroupId::new(0);
        let inner = TabGroupId::new(1);
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
            root,
            vec![
                DashboardTab {
                    title: "Nodes".into(),
                    children: vec![DashboardLayoutItem::Row(DashboardRow::new(
                        RowId::new(0),
                        "Collapsed",
                        true,
                        false,
                        vec![DashboardLayoutItem::Panel(0)],
                    ))],
                },
                DashboardTab {
                    title: "Consul".into(),
                    children: vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
                        inner,
                        vec![
                            DashboardTab {
                                title: "Active".into(),
                                children: vec![DashboardLayoutItem::Panel(1)],
                            },
                            DashboardTab {
                                title: "Inactive".into(),
                                children: vec![DashboardLayoutItem::Panel(2)],
                            },
                        ],
                    ))],
                },
            ],
        ))]);

        assert_eq!(layout.tab_panel_indices(root, 0), [0]);
        assert_eq!(layout.tab_panel_indices(root, 1), [1, 2]);
        assert!(layout.tab_panel_indices(root, 2).is_empty());

        let owners = layout.section_owners(root);
        assert_eq!(
            owners,
            HashMap::from([
                (SectionId::Tab(root, 0), 0),
                (SectionId::Row(RowId::new(0)), 0),
                (SectionId::Tab(root, 1), 1),
                (SectionId::Tab(inner, 0), 1),
                (SectionId::Tab(inner, 1), 1),
            ])
        );
    }

    #[test]
    fn nested_tab_and_row_state_survive_outer_switches() {
        let outer_id = TabGroupId::new(0);
        let inner_id = TabGroupId::new(1);
        let mut layout = DashboardLayout::new(vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
            outer_id,
            vec![
                DashboardTab {
                    title: "Nested".into(),
                    children: vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
                        inner_id,
                        vec![
                            DashboardTab {
                                title: "First".into(),
                                children: vec![DashboardLayoutItem::Row(DashboardRow::new(
                                    RowId::new(0),
                                    "Collapsed",
                                    true,
                                    false,
                                    vec![DashboardLayoutItem::Panel(0)],
                                ))],
                            },
                            DashboardTab {
                                title: "Second".into(),
                                children: vec![DashboardLayoutItem::Panel(1)],
                            },
                        ],
                    ))],
                },
                DashboardTab {
                    title: "Other".into(),
                    children: vec![DashboardLayoutItem::Panel(2)],
                },
            ],
        ))]);

        layout.set_active_tab(inner_id, 1).unwrap();
        layout.set_active_tab(outer_id, 1).unwrap();
        layout.set_active_tab(outer_id, 0).unwrap();

        assert_eq!(layout.tabs(inner_id).unwrap().active, Some(1));
        assert!(layout.row(RowId::new(0)).unwrap().collapsed);
        assert_eq!(layout.visible_panel_indices(), vec![1]);
    }

    #[test]
    fn empty_tab_groups_remain_visible_and_safe_to_enter() {
        let id = TabGroupId::new(0);
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
            id,
            vec![],
        ))]);

        assert_eq!(layout.first_visible(), Some(DashboardItemId::Tabs(id)));
        assert_eq!(layout.tabs(id).unwrap().active, None);
        assert_eq!(layout.first_tab_descendant(id), None);
        assert!(layout.visible_panel_indices().is_empty());
    }

    #[test]
    fn collapsed_parent_hides_descendants_and_preserves_nested_state() {
        let nested = DashboardRow::new(
            RowId::new(2),
            "Nested",
            true,
            false,
            vec![DashboardLayoutItem::Panel(2)],
        );
        let parent = DashboardRow::new(
            RowId::new(1),
            "Parent",
            false,
            false,
            vec![
                DashboardLayoutItem::Panel(0),
                DashboardLayoutItem::Row(nested),
                DashboardLayoutItem::Panel(1),
            ],
        );
        let mut layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(parent)]);

        assert_eq!(layout.visible_panel_indices(), vec![0, 1]);
        layout.set_row_collapsed(RowId::new(1), true).unwrap();
        assert_eq!(layout.visible_panel_indices(), Vec::<usize>::new());
        let change = layout.set_row_collapsed(RowId::new(1), false).unwrap();
        assert_eq!(change.newly_visible_panels, vec![0, 1]);
        assert!(layout.row(RowId::new(2)).unwrap().collapsed);
    }

    #[test]
    fn hidden_header_is_transparent_even_when_marked_collapsed() {
        let hidden = DashboardRow::new(
            RowId::new(1),
            "Hidden",
            true,
            true,
            vec![DashboardLayoutItem::Panel(0)],
        );
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(hidden)]);
        assert_eq!(layout.visible_panel_indices(), vec![0]);
        assert_eq!(
            layout.visible_items(),
            vec![VisibleDashboardItem::panel(0, 0)]
        );
    }

    fn auto_grid(panels: Vec<usize>) -> DashboardLayoutItem {
        DashboardLayoutItem::AutoGrid(DashboardAutoGrid {
            panels,
            max_columns: 3,
            min_column_width: 10,
            row_height: 4,
        })
    }

    fn conditional_layout() -> DashboardLayout {
        DashboardLayout::new(vec![
            DashboardLayoutItem::Row(DashboardRow::new(
                RowId::new(0),
                "Hidden row",
                false,
                false,
                vec![DashboardLayoutItem::Panel(0)],
            )),
            DashboardLayoutItem::Tabs(DashboardTabs {
                id: TabGroupId::new(0),
                tabs: vec![
                    DashboardTab {
                        title: "A".to_string(),
                        children: vec![DashboardLayoutItem::Row(DashboardRow::new(
                            RowId::new(1),
                            "In A",
                            false,
                            false,
                            vec![],
                        ))],
                    },
                    DashboardTab {
                        title: "B".to_string(),
                        children: vec![auto_grid(vec![1, 2])],
                    },
                    DashboardTab {
                        title: "C".to_string(),
                        children: vec![],
                    },
                ],
                active: Some(0),
            }),
        ])
    }

    #[test]
    fn hidden_conditional_items_are_left_out_of_the_layout() {
        let layout = conditional_layout();
        let hidden = HashSet::from([
            ConditionTarget::Row(RowId::new(0)),
            ConditionTarget::Tab(TabGroupId::new(0), 0),
            ConditionTarget::Panel(1),
        ]);

        let shown = layout.without(&hidden);

        let [DashboardLayoutItem::Tabs(group)] = shown.items.as_slice() else {
            panic!("expected only the tab group, got {:?}", shown.items);
        };
        let titles: Vec<_> = group.tabs.iter().map(|tab| tab.title.as_str()).collect();
        assert_eq!(titles, ["B", "C"]);
        // `hidden` holds the active tab, so the first visible tab is active.
        assert_eq!(group.active, Some(0));
        assert_eq!(group.tabs[0].children, [auto_grid(vec![2])]);
        assert_eq!(shown.visible_panel_indices(), [2]);
    }

    #[test]
    fn tab_groups_and_auto_grids_with_nothing_left_are_dropped() {
        let layout = DashboardLayout::new(vec![
            auto_grid(vec![0]),
            DashboardLayoutItem::Tabs(DashboardTabs::new(
                TabGroupId::new(0),
                vec![DashboardTab {
                    title: "Only".to_string(),
                    children: vec![],
                }],
            )),
        ]);
        let hidden = HashSet::from([
            ConditionTarget::Panel(0),
            ConditionTarget::Tab(TabGroupId::new(0), 0),
        ]);

        assert!(layout.without(&hidden).items.is_empty());
    }

    #[test]
    fn displayed_state_syncs_back_through_hidden_items() {
        let mut layout = conditional_layout();
        let hidden = HashSet::from([ConditionTarget::Tab(TabGroupId::new(0), 0)]);
        let mut displayed = layout.without(&hidden);
        // Activate "C", the second displayed tab, and collapse the first row.
        displayed.set_active_tab(TabGroupId::new(0), 1);
        displayed.set_row_collapsed(RowId::new(0), true);

        layout.sync_state_from(&displayed, &hidden);

        assert_eq!(layout.tabs(TabGroupId::new(0)).unwrap().active, Some(2));
        assert!(layout.row(RowId::new(0)).unwrap().collapsed);
        // When every tab shows again, "C" stays active.
        let shown = layout.without(&HashSet::new());
        assert_eq!(shown.tabs(TabGroupId::new(0)).unwrap().active, Some(2));
    }

    #[test]
    fn auto_grid_column_count_fits_minimum_widths_up_to_limits() {
        let DashboardLayoutItem::AutoGrid(grid) = auto_grid(vec![0, 1, 2, 3]) else {
            unreachable!()
        };

        // Each 10-cell column after the first also needs a one-cell gap.
        for (width, columns) in [(0, 1), (10, 1), (20, 1), (21, 2), (32, 3), (500, 3)] {
            assert_eq!(grid.column_count(width), columns, "{width}");
        }
        let two_panels = DashboardAutoGrid {
            panels: vec![0, 1],
            ..grid
        };
        assert_eq!(two_panels.column_count(500), 2);
    }

    #[test]
    fn auto_grid_panels_are_visible_at_the_enclosing_depth() {
        let layout = DashboardLayout::new(vec![
            auto_grid(vec![0, 1]),
            DashboardLayoutItem::Row(DashboardRow::new(
                RowId::new(0),
                "Row",
                false,
                false,
                vec![auto_grid(vec![2])],
            )),
        ]);

        assert_eq!(
            layout.visible_items(),
            [
                VisibleDashboardItem::panel(0, 0),
                VisibleDashboardItem::panel(1, 0),
                VisibleDashboardItem {
                    id: DashboardItemId::Row(RowId::new(0)),
                    depth: 0,
                },
                VisibleDashboardItem::panel(2, 1),
            ]
        );
    }

    #[test]
    fn collapsing_a_row_hides_its_auto_grid_and_selects_the_row() {
        let mut layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(0),
            "Row",
            false,
            false,
            vec![auto_grid(vec![0, 1])],
        ))]);

        layout.set_row_collapsed(RowId::new(0), true);

        assert_eq!(layout.visible_panel_indices(), Vec::<usize>::new());
        assert_eq!(
            layout.nearest_visible_ancestor(DashboardItemId::Panel(1)),
            Some(DashboardItemId::Row(RowId::new(0)))
        );
        assert_eq!(
            layout.set_row_collapsed(RowId::new(0), false),
            Some(LayoutChange {
                newly_visible_panels: vec![0, 1]
            })
        );
    }

    #[test]
    fn flat_layout_lists_each_panel_at_root_depth() {
        let layout = DashboardLayout::flat(3);

        assert_eq!(layout.visible_panel_indices().len(), 3);
        assert_eq!(
            layout.visible_items(),
            vec![
                VisibleDashboardItem::panel(0, 0),
                VisibleDashboardItem::panel(1, 0),
                VisibleDashboardItem::panel(2, 0),
            ]
        );
        assert_eq!(layout.first_visible(), Some(DashboardItemId::Panel(0)));
    }

    #[test]
    fn toggling_a_collapsed_row_reveals_panels_in_source_order() {
        let row = DashboardRow::new(
            RowId::new(1),
            "Collapsed",
            true,
            false,
            vec![DashboardLayoutItem::Panel(3), DashboardLayoutItem::Panel(1)],
        );
        let mut layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(row)]);

        let change = layout.toggle_row(RowId::new(1)).unwrap();

        assert_eq!(change.newly_visible_panels, vec![3, 1]);
        assert_eq!(layout.visible_panel_indices(), vec![3, 1]);
    }

    #[test]
    fn unknown_rows_do_not_change_the_layout() {
        let mut layout = DashboardLayout::flat(1);

        assert_eq!(layout.set_row_collapsed(RowId::new(99), true), None);
        assert_eq!(layout.toggle_row(RowId::new(99)), None);
        assert_eq!(layout.visible_panel_indices(), vec![0]);
    }

    #[test]
    fn nearest_visible_ancestor_prefers_the_target_then_visible_rows_then_first_item() {
        let collapsed_child = DashboardRow::new(
            RowId::new(2),
            "Child",
            true,
            false,
            vec![DashboardLayoutItem::Panel(2)],
        );
        let parent = DashboardRow::new(
            RowId::new(1),
            "Parent",
            false,
            false,
            vec![DashboardLayoutItem::Row(collapsed_child)],
        );
        let layout = DashboardLayout::new(vec![
            DashboardLayoutItem::Panel(0),
            DashboardLayoutItem::Row(parent),
        ]);

        assert_eq!(
            layout.nearest_visible_ancestor(DashboardItemId::Panel(0)),
            Some(DashboardItemId::Panel(0))
        );
        assert_eq!(
            layout.nearest_visible_ancestor(DashboardItemId::Panel(2)),
            Some(DashboardItemId::Row(RowId::new(2)))
        );
        assert_eq!(
            layout.nearest_visible_ancestor(DashboardItemId::Panel(99)),
            Some(DashboardItemId::Panel(0))
        );
    }
}
