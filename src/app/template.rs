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

//! Dashboard templates: the imported layout before repeats are expanded and
//! variables are interpolated into titles.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};

use super::state::{GridUnit, PanelState};
use super::variables::{format_prometheus_values, substitute_variables};
use crate::conditions::{ConditionGroup, ConditionTarget, Conditions};
use crate::dashboard::{
    DashboardAutoGrid, DashboardLayout, DashboardLayoutItem, DashboardRow, DashboardTab,
    DashboardTabs, LayoutWalk, MAX_LAYOUT_DEPTH, MAX_LAYOUT_NODES, MAX_LAYOUT_STEPS, Repeat,
    RepeatDirection, Repeats, RowId, SectionId, TabGroupId, WalkScope, node_count,
};
use crate::grafana::{SectionVariable, TemplateQueryVar};

/// Values that section query variables resolved to, per section copy and the
/// scope it was resolved in.
pub(crate) type ResolvedSections = HashMap<(SectionId, Scope), HashMap<String, Vec<String>>>;

/// Grafana's default number of horizontal repeat copies per row.
const DEFAULT_MAX_PER_ROW: u16 = 4;
const GRID_COLUMNS: i32 = 24;

/// A variable bound for an item and everything inside it: a repeat copy's
/// value, or a variable that a row or tab defines.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ScopeBinding {
    pub(crate) name: String,
    /// Raw selected values.
    pub(crate) values: Vec<String>,
    /// Query text replacing the formatted values, such as an `allValue`.
    pub(crate) formatted: Option<String>,
    /// Whether values are regex-escaped in queries.
    pub(crate) regex: bool,
    /// Whether `All` is selected.
    pub(crate) all: bool,
}

impl ScopeBinding {
    /// A repeat copy's binding: one value, formatted as that variable's only selection.
    pub(crate) fn repeat(name: &str, value: &str, regex: bool) -> Self {
        Self {
            name: name.to_string(),
            values: vec![value.to_string()],
            formatted: None,
            regex,
            all: false,
        }
    }

    /// The value substituted into queries.
    pub(crate) fn query_value(&self) -> String {
        self.formatted
            .clone()
            .unwrap_or_else(|| format_prometheus_values(&self.values, self.regex))
    }

    /// The value shown in titles, with several values joined like Grafana's text.
    fn text(&self) -> String {
        if self.values.is_empty() {
            self.formatted.clone().unwrap_or_default()
        } else {
            self.values.join(" + ")
        }
    }
}

/// Variables bound by enclosing repeats and sections, outermost first; later
/// bindings shadow earlier ones.
pub(crate) type Scope = Vec<ScopeBinding>;

pub(crate) fn find_binding<'s>(scope: &'s [ScopeBinding], name: &str) -> Option<&'s ScopeBinding> {
    scope.iter().rev().find(|binding| binding.name == name)
}

/// The dashboard's variables, as repeats and titles see them.
pub(crate) struct Variables<'a> {
    /// Values formatted for queries.
    pub(crate) formatted: &'a HashMap<String, String>,
    /// Raw selected values.
    pub(crate) values: &'a HashMap<String, Vec<String>>,
    /// Variables whose values are regex-escaped in queries.
    pub(crate) regex: &'a HashSet<String>,
    /// Resolved values of section query variables.
    pub(crate) sections: &'a ResolvedSections,
}

impl Variables<'_> {
    /// Interpolates `$name` references in a title. Variables bound in `scope`
    /// win; other variables show their selected values joined like Grafana's text.
    pub(crate) fn title(&self, title: &str, scope: &Scope) -> String {
        substitute_variables(title, |name| {
            if let Some(binding) = find_binding(scope, name) {
                return Some(Cow::Owned(binding.text()));
            }
            match self.values.get(name) {
                Some(values) if !values.is_empty() => Some(Cow::Owned(values.join(" + "))),
                _ => self
                    .formatted
                    .get(name)
                    .map(|value| Cow::Owned(value.clone())),
            }
        })
    }

    /// The values a repeat over `name` iterates, and whether they are regex-escaped.
    fn repeat_values<'s>(&'s self, scope: &'s Scope, name: &str) -> (&'s [String], bool) {
        match find_binding(scope, name) {
            Some(binding) => (&binding.values, binding.regex),
            None => (
                self.values.get(name).map_or(&[], Vec::as_slice),
                self.regex.contains(name),
            ),
        }
    }
}

/// Query variables for a panel: the dashboard's, overridden by the variables
/// bound in its scope.
pub(crate) fn scoped_vars<'a>(
    vars: &'a HashMap<String, String>,
    scope: Option<&Scope>,
) -> Cow<'a, HashMap<String, String>> {
    let Some(scope) = scope.filter(|scope| !scope.is_empty()) else {
        return Cow::Borrowed(vars);
    };
    let mut vars = vars.clone();
    for binding in scope {
        vars.insert(binding.name.clone(), binding.query_value());
    }
    Cow::Owned(vars)
}

/// The imported dashboard layout with its repeat settings and panel titles,
/// from which the displayed layout is rebuilt whenever variables change.
#[derive(Debug, Clone)]
pub(crate) struct DashboardTemplate {
    layout: DashboardLayout,
    repeats: Repeats,
    conditions: Conditions,
    sections: HashMap<SectionId, Vec<SectionVariable>>,
    panels: Vec<PanelTemplate>,
    clones: CloneIds,
}

#[derive(Debug, Clone)]
struct PanelTemplate {
    title: String,
    grid: Option<GridUnit>,
}

/// Ids handed out to repeat copies. A copy keeps its id, and with it its panel
/// state and row or tab state, for as long as its scope stays selected.
///
/// Ids a materialization no longer uses are forgotten when it finishes, and
/// their panel slots are reused, so memory follows the copies on screen rather
/// than every value a variable ever had.
#[derive(Debug, Clone, Default)]
struct CloneIds {
    panels: HashMap<(usize, Scope), usize>,
    rows: HashMap<(RowId, Scope), RowId>,
    tab_groups: HashMap<(TabGroupId, Scope), TabGroupId>,
    next_panel: usize,
    next_row: usize,
    next_tab_group: usize,
    /// Reclaimed panel slots below `next_panel`, reused lowest first.
    free_panels: BTreeSet<usize>,
    /// Keys used by the materialization in progress.
    used_panels: HashSet<(usize, Scope)>,
    used_rows: HashSet<(RowId, Scope)>,
    used_tab_groups: HashSet<(TabGroupId, Scope)>,
}

impl CloneIds {
    /// The slot for a copy of panel `source`, and whether it was just assigned,
    /// in which case the slot may hold another panel's old state.
    fn panel(&mut self, source: usize, scope: &Scope) -> (usize, bool) {
        let key = (source, scope.clone());
        self.used_panels.insert(key.clone());
        if let Some(&index) = self.panels.get(&key) {
            return (index, false);
        }
        let index = self
            .free_panels
            .pop_first()
            .unwrap_or_else(|| next(&mut self.next_panel));
        self.panels.insert(key, index);
        (index, true)
    }

    fn row(&mut self, source: RowId, scope: &Scope) -> RowId {
        let key = (source, scope.clone());
        self.used_rows.insert(key.clone());
        *self
            .rows
            .entry(key)
            .or_insert_with(|| RowId::new(next(&mut self.next_row)))
    }

    fn tab_group(&mut self, source: TabGroupId, scope: &Scope) -> TabGroupId {
        let key = (source, scope.clone());
        self.used_tab_groups.insert(key.clone());
        *self
            .tab_groups
            .entry(key)
            .or_insert_with(|| TabGroupId::new(next(&mut self.next_tab_group)))
    }

    /// Forgets the ids the finished materialization did not use, returning the
    /// panel slots it reclaimed. Trailing free slots are released entirely, down
    /// to `imported_panels`.
    fn finish(&mut self, imported_panels: usize) -> Vec<usize> {
        let used_panels = std::mem::take(&mut self.used_panels);
        let used_rows = std::mem::take(&mut self.used_rows);
        let used_tab_groups = std::mem::take(&mut self.used_tab_groups);
        let mut reclaimed = Vec::new();
        self.panels.retain(|key, index| {
            let used = used_panels.contains(key);
            if !used {
                reclaimed.push(*index);
            }
            used
        });
        self.rows.retain(|key, _| used_rows.contains(key));
        self.tab_groups
            .retain(|key, _| used_tab_groups.contains(key));
        self.free_panels.extend(reclaimed.iter().copied());
        while self.next_panel > imported_panels
            && self.free_panels.last() == Some(&(self.next_panel - 1))
        {
            self.free_panels.pop_last();
            self.next_panel -= 1;
        }
        reclaimed
    }
}

fn next(counter: &mut usize) -> usize {
    let value = *counter;
    *counter += 1;
    value
}

/// A panel of a materialized layout.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PanelInstance {
    /// Position in `AppState::panels`; repeat copies get indices past the imported panels.
    pub(crate) index: usize,
    /// The imported panel this instance shows.
    pub(crate) source: usize,
    pub(crate) title: String,
    pub(crate) grid: Option<GridUnit>,
    pub(crate) scope: Scope,
    /// Whether the slot was just assigned to this copy, so any state left in it
    /// belongs to another panel.
    pub(crate) fresh: bool,
}

/// A conditional rendering group attached to a materialized item.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MaterializedCondition {
    pub(crate) target: ConditionTarget,
    pub(crate) group: ConditionGroup,
    /// Repeat values the item was copied with, which its conditions see.
    pub(crate) scope: Scope,
}

/// A copy of a row or tab whose variables include Prometheus query variables.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SectionInstance {
    pub(crate) id: SectionId,
    /// Variables bound around the section, which its queries are expanded with.
    pub(crate) scope: Scope,
    pub(crate) queries: Vec<TemplateQueryVar>,
    /// Imported selections, which resolution keeps while they are offered.
    pub(crate) selected: HashMap<String, Vec<String>>,
}

/// A layout with repeats expanded, and the panels it shows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Materialized {
    pub(crate) layout: DashboardLayout,
    pub(crate) panels: Vec<PanelInstance>,
    pub(crate) conditions: Vec<MaterializedCondition>,
    pub(crate) sections: Vec<SectionInstance>,
    /// Panel slots no copy uses any more, whose state can be dropped.
    pub(crate) reclaimed: Vec<usize>,
    /// Panel slots in use or free for reuse; slots from here on are released.
    pub(crate) panel_slots: usize,
    /// Whether MAX_LAYOUT_NODES stopped a repeat before its last copy.
    pub(crate) truncated: bool,
}

impl DashboardTemplate {
    pub(crate) fn new(layout: DashboardLayout, repeats: Repeats, panels: &[PanelState]) -> Self {
        let (max_row, max_tab_group) = max_ids(&layout.items);
        Self {
            layout,
            repeats,
            conditions: Conditions::default(),
            sections: HashMap::new(),
            panels: panels
                .iter()
                .map(|panel| PanelTemplate {
                    title: panel.title.clone(),
                    grid: panel.grid,
                })
                .collect(),
            clones: CloneIds {
                next_panel: panels.len(),
                next_row: max_row.map_or(0, |id| id + 1),
                next_tab_group: max_tab_group.map_or(0, |id| id + 1),
                ..CloneIds::default()
            },
        }
    }

    /// How many repeat copy ids are remembered, across panels, rows, and tabs.
    #[cfg(test)]
    pub(crate) fn retained_ids(&self) -> usize {
        self.clones.panels.len() + self.clones.rows.len() + self.clones.tab_groups.len()
    }

    /// Adds conditional rendering, keyed by the same imported ids as the repeats.
    pub(crate) fn with_conditions(mut self, conditions: Conditions) -> Self {
        self.conditions = conditions;
        self
    }

    /// Adds the variables rows and tabs define, keyed by their imported ids.
    pub(crate) fn with_sections(
        mut self,
        sections: HashMap<SectionId, Vec<SectionVariable>>,
    ) -> Self {
        self.sections = sections;
        self
    }

    /// Expands repeats for the current variable values.
    ///
    /// The first copy of each repeated item keeps the item's own id, so a
    /// dashboard without repeats materializes to its imported layout.
    pub(crate) fn materialize(&mut self, variables: &Variables) -> Materialized {
        let mut builder = Builder {
            repeats: &self.repeats,
            conditions: &self.conditions,
            sections: &self.sections,
            section_instances: Vec::new(),
            templates: &self.panels,
            clones: &mut self.clones,
            variables,
            panels: Vec::new(),
            materialized_conditions: Vec::new(),
            nodes: 0,
            reserve: node_count(&self.layout.items),
            truncated: false,
        };
        let items = builder.items(&self.layout.items, &Scope::new(), true);
        let truncated = builder.truncated;
        let (panels, conditions, sections) = (
            builder.panels,
            builder.materialized_conditions,
            builder.section_instances,
        );
        let reclaimed = self.clones.finish(self.panels.len());
        Materialized {
            layout: DashboardLayout::new(items),
            panels,
            conditions,
            sections,
            reclaimed,
            panel_slots: self.clones.next_panel,
            truncated,
        }
    }
}

/// The largest row id and the largest tab group id in `items`.
fn max_ids(items: &[DashboardLayoutItem]) -> (Option<usize>, Option<usize>) {
    let mut rows = None;
    let mut tab_groups = None;
    for event in LayoutWalk::new(items, WalkScope::All) {
        match event.item {
            DashboardLayoutItem::Row(row) => rows = rows.max(Some(row.id.value())),
            DashboardLayoutItem::Tabs(group) => {
                tab_groups = tab_groups.max(Some(group.id.value()));
            }
            DashboardLayoutItem::Panel(_) | DashboardLayoutItem::AutoGrid(_) => {}
        }
    }
    (rows, tab_groups)
}

/// One copy of a repeated item: its scope, and whether it is the item itself.
struct RepeatCopy {
    scope: Scope,
    primary: bool,
}

struct Builder<'a, 'v> {
    repeats: &'a Repeats,
    conditions: &'a Conditions,
    sections: &'a HashMap<SectionId, Vec<SectionVariable>>,
    section_instances: Vec<SectionInstance>,
    templates: &'a [PanelTemplate],
    clones: &'a mut CloneIds,
    variables: &'a Variables<'v>,
    panels: Vec<PanelInstance>,
    materialized_conditions: Vec<MaterializedCondition>,
    /// The layout items that the builder made so far.
    nodes: usize,
    /// The layout items of the template, which the first copies can still add.
    reserve: usize,
    /// Whether MAX_LAYOUT_NODES stopped a repeat before its last copy.
    truncated: bool,
}

/// A list of template items that the builder expands, with the row or tab
/// copy that holds it.
struct BuildFrame<'a> {
    items: std::slice::Iter<'a, DashboardLayoutItem>,
    scope: Scope,
    primary: bool,
    output: Vec<DashboardLayoutItem>,
    /// The run of sibling grid panels that waits for `panel_group`.
    panels: Vec<usize>,
    parent: BuildParent<'a>,
}

impl<'a> BuildFrame<'a> {
    /// Starts the children of a copy.
    fn new(items: &'a [DashboardLayoutItem], copy: RepeatCopy, parent: BuildParent<'a>) -> Self {
        Self {
            items: items.iter(),
            scope: copy.scope,
            primary: copy.primary,
            output: Vec::new(),
            panels: Vec::new(),
            parent,
        }
    }

    /// This frame, without its items when `open_frames` frames already fill
    /// the stack. The builder leaves out the children past the depth limit.
    fn within_depth(mut self, open_frames: usize) -> Self {
        if open_frames >= MAX_LAYOUT_DEPTH {
            self.items = std::slice::Iter::default();
        }
        self
    }
}

/// The row or tab copy that holds a list of expanded items.
enum BuildParent<'a> {
    Root,
    Row(RowCopies<'a>),
    Tab(TabCopies<'a>),
}

/// The copies of a repeated item that the builder makes one at a time.
struct Copies {
    rest: std::vec::IntoIter<RepeatCopy>,
    made: usize,
    /// Whether the layout limit stopped the copies before the last value.
    cut: bool,
}

impl Copies {
    /// Starts the copies in `copies`.
    fn new(copies: Vec<RepeatCopy>) -> Self {
        Self {
            rest: copies.into_iter(),
            made: 0,
            cut: false,
        }
    }

    /// The next copy. A copy after the first needs `room` in the layout.
    fn next(&mut self, room: bool) -> Option<RepeatCopy> {
        if self.made > 0 && !room {
            self.cut = self.rest.next().is_some();
            return None;
        }
        self.made += 1;
        self.rest.next()
    }
}

/// A row whose copies the builder expands one at a time.
struct RowCopies<'a> {
    row: &'a DashboardRow,
    copies: Copies,
    /// The id of the copy whose children the builder expands now.
    id: RowId,
    /// The scope of that copy, with the variables of the row.
    scope: Scope,
}

/// A tab group whose tabs and tab copies the builder expands one at a time.
struct TabCopies<'a> {
    group: &'a DashboardTabs,
    id: TabGroupId,
    /// The scope and the primary flag of the copy that holds the tab group.
    outer: RepeatCopy,
    /// The position of the tab whose copies the builder makes now.
    position: usize,
    /// The copies of that tab, or `None` before the builder makes them.
    copies: Option<Copies>,
    /// The title of the tab copy whose children the builder expands now.
    title: String,
    tabs: Vec<DashboardTab>,
}

/// The frame of the next copy, or the finished item after the last copy.
enum CopyStep<'a> {
    Open(BuildFrame<'a>),
    Done(DashboardLayoutItem),
}

impl<'a> Builder<'a, '_> {
    /// Expands `items` and the items nested in their rows and tabs. A stack of
    /// frames replaces recursion and keeps the pre-order of the ids.
    fn items(
        &mut self,
        items: &'a [DashboardLayoutItem],
        scope: &Scope,
        primary: bool,
    ) -> Vec<DashboardLayoutItem> {
        let root = RepeatCopy {
            scope: scope.clone(),
            primary,
        };
        let mut frames = Vec::with_capacity(MAX_LAYOUT_DEPTH);
        frames.push(BuildFrame::new(items, root, BuildParent::Root));
        for _ in 0..MAX_LAYOUT_STEPS {
            let open_frames = frames.len();
            let Some(frame) = frames.last_mut() else {
                break;
            };
            if let Some(item) = frame.items.next() {
                let child = self.expand_item(item, frame);
                frames.extend(child.map(|child| child.within_depth(open_frames)));
            } else if let Some(output) = self.close_frame(&mut frames) {
                return output;
            }
        }
        frames
            .into_iter()
            .next()
            .map_or_else(Vec::new, |root| root.output)
    }

    /// Expands one item into `frame`. A row or a tab group gives the frame of
    /// the children of its first copy.
    fn expand_item(
        &mut self,
        item: &'a DashboardLayoutItem,
        frame: &mut BuildFrame<'a>,
    ) -> Option<BuildFrame<'a>> {
        let step = match item {
            DashboardLayoutItem::Panel(index) => {
                frame.panels.push(*index);
                return None;
            }
            DashboardLayoutItem::Row(row) => {
                self.flush_panels(frame);
                self.open_row(row, &frame.scope, frame.primary)
            }
            DashboardLayoutItem::Tabs(group) => {
                self.flush_panels(frame);
                self.open_tabs(group, &frame.scope, frame.primary)
            }
            DashboardLayoutItem::AutoGrid(grid) => {
                self.flush_panels(frame);
                self.nodes = self.nodes.saturating_add(1);
                let panels = grid
                    .panels
                    .iter()
                    .flat_map(|&source| self.panel_copies(source, &frame.scope, frame.primary))
                    .map(|(index, _)| index)
                    .collect();
                CopyStep::Done(DashboardLayoutItem::AutoGrid(DashboardAutoGrid {
                    panels,
                    ..grid.clone()
                }))
            }
        };
        match step {
            CopyStep::Open(child) => Some(child),
            CopyStep::Done(item) => {
                frame.output.push(item);
                None
            }
        }
    }

    /// Ends the top frame and adds its row or tab copy to the frame below.
    /// Gives the expanded items when the root frame ends.
    fn close_frame(
        &mut self,
        frames: &mut Vec<BuildFrame<'a>>,
    ) -> Option<Vec<DashboardLayoutItem>> {
        let Some(mut frame) = frames.pop() else {
            return Some(Vec::new());
        };
        self.flush_panels(&mut frame);
        let step = match frame.parent {
            BuildParent::Root => return Some(frame.output),
            BuildParent::Row(copies) => {
                let row = copies.row;
                let title = self.variables.title(&row.title, &copies.scope);
                let item = DashboardRow::new(
                    copies.id,
                    title,
                    row.collapsed,
                    row.hidden_header,
                    frame.output,
                );
                if let Some(outer) = frames.last_mut() {
                    outer.output.push(DashboardLayoutItem::Row(item));
                }
                self.next_row_copy(copies)
            }
            BuildParent::Tab(mut open) => {
                open.tabs.push(DashboardTab {
                    title: std::mem::take(&mut open.title),
                    children: frame.output,
                });
                Some(self.next_tab_copy(open))
            }
        };
        match step {
            Some(CopyStep::Open(next)) => frames.push(next.within_depth(frames.len())),
            Some(CopyStep::Done(item)) => {
                if let Some(outer) = frames.last_mut() {
                    outer.output.push(item);
                }
            }
            None => {}
        }
        None
    }

    /// Expands the run of grid panels that waits in `frame`.
    fn flush_panels(&mut self, frame: &mut BuildFrame<'a>) {
        let panels = std::mem::take(&mut frame.panels);
        self.panel_group(&panels, &frame.scope, frame.primary, &mut frame.output);
    }

    /// Whether the layout has room for a copy after the first. The first copy
    /// of each template item stays, so the template reserves its own size.
    fn has_room(&self) -> bool {
        self.nodes.saturating_add(self.reserve) < MAX_LAYOUT_NODES
    }

    /// Expands a run of sibling grid panels. A repeated panel grows to hold its
    /// copies, and later siblings move down by the height it adds, as Grafana's
    /// grid does when a repeat expands.
    fn panel_group(
        &mut self,
        sources: &[usize],
        scope: &Scope,
        primary: bool,
        output: &mut Vec<DashboardLayoutItem>,
    ) {
        let mut placed = Vec::new();
        let mut shifts = Vec::new();
        for &source in sources {
            let copies = self.panel_copies(source, scope, primary);
            if let Some(grid) = self.templates[source].grid {
                let bottom = copies
                    .iter()
                    .filter_map(|(index, _)| self.instance(*index)?.grid)
                    .map(|grid| grid.y + grid.h)
                    .max()
                    .unwrap_or(grid.y + grid.h);
                let added = bottom - (grid.y + grid.h);
                if added > 0 {
                    shifts.push((grid.y, added));
                }
            }
            placed.extend(copies.into_iter().map(|(index, _)| (source, index)));
        }

        for (source, index) in placed {
            let shift: i32 = match self.templates[source].grid {
                Some(grid) => shifts
                    .iter()
                    .filter(|(from, _)| grid.y > *from)
                    .map(|(_, added)| added)
                    .sum(),
                None => 0,
            };
            if shift > 0
                && let Some(grid) = self
                    .instance_mut(index)
                    .and_then(|panel| panel.grid.as_mut())
            {
                grid.y += shift;
            }
            output.push(DashboardLayoutItem::Panel(index));
        }
    }

    /// Records the panel instances a template panel expands to, returning their
    /// indices with whether each is the primary copy.
    fn panel_copies(&mut self, source: usize, scope: &Scope, primary: bool) -> Vec<(usize, bool)> {
        let repeat = self.repeats.panels.get(&source);
        let mut copies = self.copies(repeat, scope, primary);
        let room = MAX_LAYOUT_NODES.saturating_sub(self.nodes.saturating_add(self.reserve));
        self.truncated |= copies.len() > room.max(1);
        copies.truncate(room.max(1));
        self.nodes = self.nodes.saturating_add(copies.len());
        let count = copies.len();
        let template = &self.templates[source];
        let (title, grid) = (template.title.clone(), template.grid);
        copies
            .into_iter()
            .enumerate()
            .map(|(position, copy)| {
                let (index, fresh) = if copy.primary {
                    (source, false)
                } else {
                    self.clones.panel(source, &copy.scope)
                };
                let grid = match (grid, repeat) {
                    (Some(grid), Some(repeat)) => Some(repeat_grid(grid, repeat, position, count)),
                    (grid, _) => grid,
                };
                if let Some(group) = self.conditions.panels.get(&source) {
                    self.materialized_conditions.push(MaterializedCondition {
                        target: ConditionTarget::Panel(index),
                        group: group.clone(),
                        scope: copy.scope.clone(),
                    });
                }
                self.panels.push(PanelInstance {
                    index,
                    source,
                    title: self.variables.title(&title, &copy.scope),
                    grid,
                    scope: copy.scope,
                    fresh,
                });
                (index, copy.primary)
            })
            .collect()
    }

    /// Starts the copies of a row and gives the frame of the first copy.
    fn open_row(&mut self, row: &'a DashboardRow, scope: &Scope, primary: bool) -> CopyStep<'a> {
        let section = SectionId::Row(row.id);
        let repeat = self.repeats.rows.get(&row.id);
        let copies = RowCopies {
            row,
            copies: Copies::new(self.repeat_section(section, repeat, scope, primary)),
            id: row.id,
            scope: Scope::new(),
        };
        self.next_row_copy(copies).unwrap_or_else(|| {
            // `repeat_section` gives one copy at least, so this row is never empty.
            let empty = DashboardRow::new(row.id, "", row.collapsed, row.hidden_header, Vec::new());
            CopyStep::Done(DashboardLayoutItem::Row(empty))
        })
    }

    /// Starts the next copy of a row: its id, variables and condition. Gives
    /// the frame of its children, or `None` after the last copy.
    fn next_row_copy(&mut self, mut copies: RowCopies<'a>) -> Option<CopyStep<'a>> {
        let room = self.has_room();
        let Some(mut copy) = copies.copies.next(room) else {
            self.truncated |= copies.copies.cut;
            return None;
        };
        let row = copies.row;
        let section = SectionId::Row(row.id);
        let repeat = self.repeats.rows.get(&row.id);
        let id = if copy.primary {
            row.id
        } else {
            self.clones.row(row.id, &copy.scope)
        };
        self.nodes = self.nodes.saturating_add(1);
        self.bind_section(section, repeat, &mut copy.scope);
        if let Some(group) = self.conditions.rows.get(&row.id) {
            self.materialized_conditions.push(MaterializedCondition {
                target: ConditionTarget::Row(id),
                group: group.clone(),
                scope: copy.scope.clone(),
            });
        }
        copies.id = id;
        copies.scope.clone_from(&copy.scope);
        let frame = BuildFrame::new(&row.children, copy, BuildParent::Row(copies));
        Some(CopyStep::Open(frame))
    }

    /// Starts a tab group: its id, and the frame of its first tab copy.
    fn open_tabs(
        &mut self,
        group: &'a DashboardTabs,
        scope: &Scope,
        primary: bool,
    ) -> CopyStep<'a> {
        let id = if primary {
            group.id
        } else {
            self.clones.tab_group(group.id, scope)
        };
        self.nodes = self.nodes.saturating_add(1);
        let open = TabCopies {
            group,
            id,
            outer: RepeatCopy {
                scope: scope.clone(),
                primary,
            },
            position: 0,
            copies: None,
            title: String::new(),
            tabs: Vec::new(),
        };
        self.next_tab_copy(open)
    }

    /// Starts the next copy of the current tab, or the first copy of the next
    /// tab. Gives the finished tab group after the last tab.
    fn next_tab_copy(&mut self, mut open: TabCopies<'a>) -> CopyStep<'a> {
        let group = open.group;
        // Each pass makes a copy, or moves to the next tab.
        for _ in 0..group.tabs.len().saturating_add(2) {
            if let Some(copies) = open.copies.as_mut() {
                if let Some(copy) = copies.next(self.has_room()) {
                    return CopyStep::Open(self.open_tab_copy(open, copy));
                }
                self.truncated |= copies.cut;
                open.position += 1;
                open.copies = None;
            }
            if open.position >= group.tabs.len() {
                break;
            }
            let section = SectionId::Tab(group.id, open.position);
            let repeat = self.repeats.tabs.get(&(group.id, open.position));
            let (scope, primary) = (&open.outer.scope, open.outer.primary);
            open.copies = Some(Copies::new(
                self.repeat_section(section, repeat, scope, primary),
            ));
        }
        CopyStep::Done(finish_tabs(open))
    }

    /// Starts one copy of the current tab: its variables, condition and title.
    fn open_tab_copy(&mut self, mut open: TabCopies<'a>, mut copy: RepeatCopy) -> BuildFrame<'a> {
        let group = open.group;
        let key = (group.id, open.position);
        let repeat = self.repeats.tabs.get(&key);
        self.nodes = self.nodes.saturating_add(1);
        self.bind_section(
            SectionId::Tab(group.id, open.position),
            repeat,
            &mut copy.scope,
        );
        if let Some(condition) = self.conditions.tabs.get(&key) {
            self.materialized_conditions.push(MaterializedCondition {
                target: ConditionTarget::Tab(open.id, open.tabs.len()),
                group: condition.clone(),
                scope: copy.scope.clone(),
            });
        }
        let tab = group.tabs.get(open.position);
        let title = tab.map_or("", |tab| tab.title.as_str());
        open.title = self.variables.title(title, &copy.scope);
        let children = tab.map_or(&[][..], |tab| tab.children.as_slice());
        BuildFrame::new(children, copy, BuildParent::Tab(open))
    }

    /// The copies of a row or tab. A section's own variables are visible to its
    /// repeat, as Grafana looks variables up from the section itself.
    fn repeat_section(
        &self,
        section: SectionId,
        repeat: Option<&Repeat>,
        scope: &Scope,
        primary: bool,
    ) -> Vec<RepeatCopy> {
        let mut section_scope = scope.clone();
        section_scope.extend(self.section_bindings(section, scope));
        self.copies(repeat, &section_scope, primary)
            .into_iter()
            .map(|copy| {
                // Drop the section bindings again; `bind_section` adds them per copy.
                let mut copy_scope = scope.clone();
                copy_scope.extend(copy.scope.into_iter().skip(section_scope.len()));
                RepeatCopy {
                    scope: copy_scope,
                    primary: copy.primary,
                }
            })
            .collect()
    }

    /// Adds a section copy's variables to its scope, resolved in that copy's
    /// scope so they can depend on its repeat value. The repeat value itself is
    /// not shadowed.
    fn bind_section(&mut self, section: SectionId, repeat: Option<&Repeat>, scope: &mut Scope) {
        let Some(definitions) = self.sections.get(&section) else {
            return;
        };
        let queries: Vec<TemplateQueryVar> = definitions
            .iter()
            .filter_map(|definition| definition.query.clone())
            .collect();
        if !queries.is_empty() {
            self.section_instances.push(SectionInstance {
                id: section,
                scope: scope.clone(),
                queries,
                selected: definitions
                    .iter()
                    .filter(|definition| !definition.values.is_empty())
                    .map(|definition| (definition.name.clone(), definition.values.clone()))
                    .collect(),
            });
        }
        let bindings: Vec<ScopeBinding> = self
            .section_bindings(section, scope)
            .into_iter()
            .filter(|binding| repeat.is_none_or(|repeat| repeat.variable != binding.name))
            .collect();
        scope.extend(bindings);
    }

    /// Bindings for a section's variables: resolved values for query variables,
    /// otherwise the imported selection. Variables without a value are left out,
    /// so references to them fall through to the dashboard's variables.
    fn section_bindings(&self, section: SectionId, scope: &Scope) -> Vec<ScopeBinding> {
        let Some(definitions) = self.sections.get(&section) else {
            return Vec::new();
        };
        let resolved = self.variables.sections.get(&(section, scope.clone()));
        definitions
            .iter()
            .filter_map(|definition| {
                let values = resolved
                    .and_then(|resolved| resolved.get(&definition.name))
                    .unwrap_or(&definition.values)
                    .clone();
                let formatted = if definition.all {
                    definition
                        .all_value
                        .clone()
                        .or_else(|| values.is_empty().then(|| ".*".to_string()))
                } else {
                    None
                };
                (!values.is_empty() || formatted.is_some()).then(|| ScopeBinding {
                    name: definition.name.clone(),
                    values,
                    formatted,
                    regex: definition.regex,
                    all: definition.all,
                })
            })
            .collect()
    }

    /// The copies of an item repeated over its variable's selected values. An
    /// item without a repeat, or whose variable has no values, appears once.
    fn copies(&self, repeat: Option<&Repeat>, scope: &Scope, primary: bool) -> Vec<RepeatCopy> {
        let (values, regex) = repeat.map_or((&[][..], false), |repeat| {
            self.variables.repeat_values(scope, &repeat.variable)
        });
        let Some(repeat) = repeat.filter(|_| !values.is_empty()) else {
            return vec![RepeatCopy {
                scope: scope.clone(),
                primary,
            }];
        };
        values
            .iter()
            .enumerate()
            .map(|(position, value)| {
                let mut scope = scope.clone();
                scope.push(ScopeBinding::repeat(&repeat.variable, value, regex));
                RepeatCopy {
                    scope,
                    primary: primary && position == 0,
                }
            })
            .collect()
    }

    fn instance(&self, index: usize) -> Option<&PanelInstance> {
        self.panels.iter().rev().find(|panel| panel.index == index)
    }

    fn instance_mut(&mut self, index: usize) -> Option<&mut PanelInstance> {
        self.panels
            .iter_mut()
            .rev()
            .find(|panel| panel.index == index)
    }
}

/// The tab group of `open` with its finished tabs. The active tab of the
/// template stays active, or the last tab when the group has fewer tabs now.
fn finish_tabs(open: TabCopies<'_>) -> DashboardLayoutItem {
    let mut materialized = DashboardTabs::new(open.id, open.tabs);
    if let Some(active) = open.group.active {
        let last = materialized.tabs.len().checked_sub(1);
        materialized.active = last.map(|last| active.min(last));
    }
    DashboardLayoutItem::Tabs(materialized)
}

/// Grid position of copy `position` of `count` of a repeated panel.
///
/// Matches Grafana's `DashboardGridItem`: horizontal repeats span the full grid
/// width in rows of up to `maxPerRow` equal columns, and vertical repeats stack
/// at the panel's own width.
fn repeat_grid(grid: GridUnit, repeat: &Repeat, position: usize, count: usize) -> GridUnit {
    let position = i32::try_from(position).unwrap_or(i32::MAX);
    match repeat.direction {
        RepeatDirection::Vertical => GridUnit {
            y: grid.y + position * grid.h,
            ..grid
        },
        RepeatDirection::Horizontal => {
            let per_row = i32::from(repeat.max_per_row.unwrap_or(DEFAULT_MAX_PER_ROW).max(1))
                .min(i32::try_from(count).unwrap_or(i32::MAX))
                .max(1);
            let column = position % per_row;
            let row = position / per_row;
            // Spread the grid columns as evenly as whole units allow.
            let base = GRID_COLUMNS / per_row;
            let wider = GRID_COLUMNS % per_row;
            GridUnit {
                x: column * base + column.min(wider),
                y: grid.y + row * grid.h,
                w: base + i32::from(column < wider),
                h: grid.h,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(x: i32, y: i32, w: i32, h: i32) -> Option<GridUnit> {
        Some(GridUnit { x, y, w, h })
    }

    fn panels(specs: &[(&str, Option<GridUnit>)]) -> Vec<PanelState> {
        specs
            .iter()
            .map(|(title, grid)| {
                let mut panel = crate::app::default_queries(vec!["up".to_string()]).remove(0);
                panel.title = title.to_string();
                panel.grid = *grid;
                panel
            })
            .collect()
    }

    struct Vars {
        formatted: HashMap<String, String>,
        values: HashMap<String, Vec<String>>,
        regex: HashSet<String>,
        sections: ResolvedSections,
    }

    impl Vars {
        fn new(selections: &[(&str, &[&str])]) -> Self {
            let values: HashMap<String, Vec<String>> = selections
                .iter()
                .map(|(name, values)| {
                    (
                        name.to_string(),
                        values.iter().map(|value| value.to_string()).collect(),
                    )
                })
                .collect();
            let formatted = values
                .iter()
                .map(|(name, values)| (name.clone(), format_prometheus_values(values, true)))
                .collect();
            let regex = values.keys().cloned().collect();
            Self {
                formatted,
                values,
                regex,
                sections: ResolvedSections::new(),
            }
        }

        fn get(&self) -> Variables<'_> {
            Variables {
                formatted: &self.formatted,
                values: &self.values,
                regex: &self.regex,
                sections: &self.sections,
            }
        }
    }

    fn repeat(variable: &str, direction: RepeatDirection, max_per_row: Option<u16>) -> Repeat {
        Repeat {
            variable: variable.to_string(),
            direction,
            max_per_row,
        }
    }

    fn grids(materialized: &Materialized) -> Vec<(usize, Option<GridUnit>)> {
        materialized
            .panels
            .iter()
            .map(|panel| (panel.index, panel.grid))
            .collect()
    }

    /// `v2_grafana13_repeats.json` was authored through Grafana 13.2.3's V2 API: a
    /// horizontal grid repeat over an `All` selection, a repeated row holding an
    /// auto grid, and repeated tabs.
    #[test]
    fn grafana13_repeats_fixture_expands_like_grafana() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/grafana/v2_grafana13_repeats.json");
        let import = crate::grafana::load_grafana_dashboard(&path).unwrap();
        assert!(import.diagnostics.is_empty(), "{:?}", import.diagnostics);
        let panels: Vec<PanelState> = import
            .queries
            .iter()
            .map(|query| {
                let mut panel = crate::app::default_queries(vec!["up".to_string()]).remove(0);
                panel.title = query.title.clone();
                panel.grid = query.grid.map(|grid| GridUnit {
                    x: grid.x,
                    y: grid.y,
                    w: grid.w,
                    h: grid.h,
                });
                panel
            })
            .collect();
        let mut template =
            DashboardTemplate::new(import.layout.clone(), import.repeats.clone(), &panels);

        let materialized = template.materialize(&Variables {
            formatted: &import.vars,
            values: &import.var_values,
            regex: &import.regex_vars,
            sections: &ResolvedSections::new(),
        });

        let titles: Vec<_> = materialized
            .layout
            .items
            .iter()
            .map(|item| match item {
                DashboardLayoutItem::Row(row) => row.title.as_str(),
                item => panic!("expected rows, got {item:?}"),
            })
            .collect();
        assert_eq!(
            titles,
            ["Handlers", "Quantile 0.5", "Quantile 0.99", "By tab"]
        );

        let handlers: Vec<_> = materialized
            .panels
            .iter()
            .filter(|panel| panel.source <= 1)
            .map(|panel| (panel.title.as_str(), panel.grid))
            .collect();
        // Three handlers at two per row, then `Up` moved below the extra row.
        assert_eq!(
            handlers,
            [
                ("Requests /api/v1/query", grid(0, 0, 12, 6)),
                ("Requests /api/v1/query_range", grid(12, 0, 12, 6)),
                ("Requests /metrics", grid(0, 6, 12, 6)),
                ("Up", grid(0, 12, 24, 4)),
            ]
        );

        let DashboardLayoutItem::Row(by_tab) = &materialized.layout.items[3] else {
            unreachable!()
        };
        let [DashboardLayoutItem::Tabs(tabs)] = by_tab.children.as_slice() else {
            panic!("expected tabs, got {:?}", by_tab.children);
        };
        let tab_titles: Vec<_> = tabs.tabs.iter().map(|tab| tab.title.as_str()).collect();
        assert_eq!(
            tab_titles,
            ["/api/v1/query", "/api/v1/query_range", "/metrics"]
        );
    }

    #[test]
    fn layouts_without_repeats_materialize_unchanged_with_interpolated_titles() {
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(0),
            "Jobs: $job",
            true,
            false,
            vec![DashboardLayoutItem::Panel(0)],
        ))]);
        let mut template = DashboardTemplate::new(
            layout.clone(),
            Repeats::default(),
            &panels(&[("Up for ${job} / $job_name", grid(0, 0, 12, 4))]),
        );
        let vars = Vars::new(&[("job", &["api", "web"])]);

        let materialized = template.materialize(&vars.get());

        let DashboardLayoutItem::Row(row) = &materialized.layout.items[0] else {
            panic!("expected a row");
        };
        assert_eq!(row.title, "Jobs: api + web");
        assert!(row.collapsed);
        assert_eq!(row.children, [DashboardLayoutItem::Panel(0)]);
        // `$job_name` is a different, undefined variable and stays as written.
        assert_eq!(materialized.panels[0].title, "Up for api + web / $job_name");
        assert!(materialized.panels[0].scope.is_empty());
    }

    #[test]
    fn horizontal_repeats_fill_the_grid_width_and_push_later_panels_down() {
        let mut repeats = Repeats::default();
        repeats
            .panels
            .insert(0, repeat("dc", RepeatDirection::Horizontal, None));
        let mut template = DashboardTemplate::new(
            DashboardLayout::flat(2),
            repeats,
            &panels(&[("CPU $dc", grid(6, 0, 12, 4)), ("Below", grid(0, 4, 24, 4))]),
        );
        let vars = Vars::new(&[("dc", &["a", "b", "c", "d", "e"])]);

        let materialized = template.materialize(&vars.get());

        // Five copies wrap after Grafana's default four per row, each 6 units wide.
        assert_eq!(
            grids(&materialized),
            [
                (0, grid(0, 0, 6, 4)),
                (2, grid(6, 0, 6, 4)),
                (3, grid(12, 0, 6, 4)),
                (4, grid(18, 0, 6, 4)),
                (5, grid(0, 4, 6, 4)),
                (1, grid(0, 8, 24, 4)),
            ]
        );
        let titles: Vec<_> = materialized
            .panels
            .iter()
            .map(|p| p.title.as_str())
            .collect();
        assert_eq!(
            titles,
            ["CPU a", "CPU b", "CPU c", "CPU d", "CPU e", "Below"]
        );
        assert_eq!(
            materialized.panels[4].scope,
            [ScopeBinding::repeat("dc", "e", true)]
        );
    }

    #[test]
    fn horizontal_repeats_spread_uneven_columns_and_honor_max_per_row() {
        let mut repeats = Repeats::default();
        repeats
            .panels
            .insert(0, repeat("dc", RepeatDirection::Horizontal, Some(5)));
        let mut template = DashboardTemplate::new(
            DashboardLayout::flat(1),
            repeats,
            &panels(&[("CPU", grid(0, 2, 8, 3))]),
        );
        let vars = Vars::new(&[("dc", &["a", "b", "c", "d", "e"])]);

        let materialized = template.materialize(&vars.get());

        let widths: Vec<_> = materialized
            .panels
            .iter()
            .map(|panel| panel.grid.unwrap())
            .map(|grid| (grid.x, grid.w, grid.y))
            .collect();
        assert_eq!(
            widths,
            [(0, 5, 2), (5, 5, 2), (10, 5, 2), (15, 5, 2), (20, 4, 2)]
        );
    }

    #[test]
    fn vertical_repeats_stack_and_move_only_panels_starting_below_the_source() {
        let mut repeats = Repeats::default();
        repeats
            .panels
            .insert(0, repeat("dc", RepeatDirection::Vertical, None));
        let mut template = DashboardTemplate::new(
            DashboardLayout::flat(3),
            repeats,
            &panels(&[
                ("CPU", grid(0, 0, 12, 4)),
                ("Beside", grid(12, 0, 12, 4)),
                ("Below", grid(0, 4, 12, 4)),
            ]),
        );
        let vars = Vars::new(&[("dc", &["a", "b", "c"])]);

        let materialized = template.materialize(&vars.get());

        assert_eq!(
            grids(&materialized),
            [
                (0, grid(0, 0, 12, 4)),
                (3, grid(0, 4, 12, 4)),
                (4, grid(0, 8, 12, 4)),
                (1, grid(12, 0, 12, 4)),
                (2, grid(0, 12, 12, 4)),
            ]
        );
    }

    #[test]
    fn repeated_rows_copy_their_subtree_with_new_ids_and_scopes() {
        let mut repeats = Repeats::default();
        repeats.rows.insert(RowId::new(4), Repeat::new("dc"));
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(4),
            "Region $dc",
            false,
            false,
            vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
                TabGroupId::new(2),
                vec![DashboardTab {
                    title: "Tab".to_string(),
                    children: vec![DashboardLayoutItem::Panel(0)],
                }],
            ))],
        ))]);
        let mut template = DashboardTemplate::new(layout, repeats, &panels(&[("CPU $dc", None)]));
        let vars = Vars::new(&[("dc", &["eu", "us"])]);

        let materialized = template.materialize(&vars.get());

        let rows: Vec<_> = materialized
            .layout
            .items
            .iter()
            .map(|item| match item {
                DashboardLayoutItem::Row(row) => match row.children.as_slice() {
                    [DashboardLayoutItem::Tabs(tabs)] => (row.id, row.title.clone(), tabs.id),
                    children => panic!("unexpected row children {children:?}"),
                },
                item => panic!("unexpected item {item:?}"),
            })
            .collect();
        assert_eq!(
            rows,
            [
                (RowId::new(4), "Region eu".to_string(), TabGroupId::new(2)),
                (RowId::new(5), "Region us".to_string(), TabGroupId::new(3)),
            ]
        );
        let panels: Vec<_> = materialized
            .panels
            .iter()
            .map(|panel| (panel.index, panel.source, panel.title.as_str()))
            .collect();
        assert_eq!(panels, [(0, 0, "CPU eu"), (1, 0, "CPU us")]);
    }

    #[test]
    fn repeated_tabs_and_auto_grid_items_expand_in_place() {
        let mut repeats = Repeats::default();
        repeats
            .tabs
            .insert((TabGroupId::new(0), 0), Repeat::new("dc"));
        repeats.panels.insert(0, Repeat::new("host"));
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Tabs(DashboardTabs::new(
            TabGroupId::new(0),
            vec![DashboardTab {
                title: "$dc".to_string(),
                children: vec![DashboardLayoutItem::AutoGrid(DashboardAutoGrid {
                    panels: vec![0],
                    max_columns: 3,
                    min_column_width: 56,
                    row_height: 9,
                })],
            }],
        ))]);
        let mut template = DashboardTemplate::new(layout, repeats, &panels(&[("$host", None)]));
        let vars = Vars::new(&[("dc", &["eu", "us"]), ("host", &["a", "b"])]);

        let materialized = template.materialize(&vars.get());

        let DashboardLayoutItem::Tabs(group) = &materialized.layout.items[0] else {
            panic!("expected tabs");
        };
        let tabs: Vec<_> = group
            .tabs
            .iter()
            .map(|tab| match tab.children.as_slice() {
                [DashboardLayoutItem::AutoGrid(grid)] => (tab.title.as_str(), grid.panels.clone()),
                children => panic!("unexpected tab children {children:?}"),
            })
            .collect();
        assert_eq!(tabs, [("eu", vec![0, 1]), ("us", vec![2, 3])]);
        let scopes: Vec<_> = materialized
            .panels
            .iter()
            .map(|panel| panel.scope.clone())
            .collect();
        let scope = |dc: &str, host: &str| {
            vec![
                ScopeBinding::repeat("dc", dc, true),
                ScopeBinding::repeat("host", host, true),
            ]
        };
        assert_eq!(
            scopes,
            [
                scope("eu", "a"),
                scope("eu", "b"),
                scope("us", "a"),
                scope("us", "b")
            ]
        );
    }

    #[test]
    fn section_variables_shadow_dashboard_variables_inside_their_section() {
        let layout = DashboardLayout::new(vec![
            DashboardLayoutItem::Row(DashboardRow::new(
                RowId::new(0),
                "Env $env",
                false,
                false,
                vec![DashboardLayoutItem::Panel(0)],
            )),
            DashboardLayoutItem::Panel(1),
        ]);
        let sections = HashMap::from([(
            SectionId::Row(RowId::new(0)),
            vec![SectionVariable {
                name: "env".to_string(),
                values: vec!["staging".to_string()],
                regex: false,
                all: false,
                all_value: None,
                query: None,
            }],
        )]);
        let mut template = DashboardTemplate::new(
            layout,
            Repeats::default(),
            &panels(&[("CPU $env", None), ("Global $env", None)]),
        )
        .with_sections(sections);

        let materialized = template.materialize(&Vars::new(&[("env", &["prod"])]).get());

        let DashboardLayoutItem::Row(row) = &materialized.layout.items[0] else {
            panic!("expected a row");
        };
        assert_eq!(row.title, "Env staging");
        let titles: Vec<_> = materialized
            .panels
            .iter()
            .map(|p| p.title.as_str())
            .collect();
        assert_eq!(titles, ["CPU staging", "Global prod"]);
        let staging = &materialized.panels[0].scope;
        assert_eq!(staging.len(), 1);
        assert_eq!(staging[0].query_value(), "staging");
        assert!(materialized.panels[1].scope.is_empty());
        // Static sections have nothing to resolve.
        assert!(materialized.sections.is_empty());
    }

    #[test]
    fn copies_keep_their_ids_across_materializations() {
        let mut repeats = Repeats::default();
        repeats.panels.insert(0, Repeat::new("dc"));
        let mut template =
            DashboardTemplate::new(DashboardLayout::flat(1), repeats, &panels(&[("$dc", None)]));

        let first = template.materialize(&Vars::new(&[("dc", &["a", "b", "c"])]).get());
        let again = template.materialize(&Vars::new(&[("dc", &["a", "b", "c"])]).get());
        let fewer = template.materialize(&Vars::new(&[("dc", &["a", "c"])]).get());

        let indices = |materialized: &Materialized| -> Vec<(usize, bool)> {
            materialized
                .panels
                .iter()
                .map(|panel| (panel.index, panel.fresh))
                .collect()
        };
        assert_eq!(first.layout, again.layout);
        // New copies get fresh slots once, then keep them.
        assert_eq!(indices(&first), [(0, false), (1, true), (2, true)]);
        assert_eq!(indices(&again), [(0, false), (1, false), (2, false)]);
        // Dropping `b` reclaims its slot; `c` keeps its own.
        assert_eq!(indices(&fewer), [(0, false), (2, false)]);
        assert_eq!(fewer.reclaimed, [1]);
        assert_eq!(fewer.panel_slots, 3);

        // The free slot is reused before new ones, and trailing ones are released.
        let reused = template.materialize(&Vars::new(&[("dc", &["a", "d"])]).get());
        assert_eq!(indices(&reused), [(0, false), (1, true)]);
        assert_eq!(reused.reclaimed, [2]);
        assert_eq!(reused.panel_slots, 2);
    }

    #[test]
    fn repeats_without_selected_values_show_the_item_once() {
        let mut repeats = Repeats::default();
        repeats.panels.insert(0, Repeat::new("dc"));
        repeats.rows.insert(RowId::new(0), Repeat::new("dc"));
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(0),
            "Row",
            false,
            false,
            vec![DashboardLayoutItem::Panel(0)],
        ))]);
        let mut template =
            DashboardTemplate::new(layout.clone(), repeats, &panels(&[("$dc", None)]));

        let materialized = template.materialize(&Vars::new(&[]).get());

        assert_eq!(materialized.layout, layout);
        assert_eq!(materialized.panels[0].title, "$dc");
    }

    #[test]
    fn scoped_vars_format_repeat_values_like_a_single_selection() {
        let vars = HashMap::from([
            ("dc".to_string(), "(eu|us)".to_string()),
            ("job".to_string(), "api".to_string()),
        ]);
        let scope = vec![ScopeBinding::repeat("dc", "eu.west", true)];

        let scoped = scoped_vars(&vars, Some(&scope));

        assert_eq!(scoped.get("dc").map(String::as_str), Some("eu\\\\.west"));
        assert_eq!(scoped.get("job").map(String::as_str), Some("api"));
        assert!(matches!(scoped_vars(&vars, None), Cow::Borrowed(_)));
    }

    /// `count` values for a repeat variable.
    fn many_values(count: usize) -> Vec<String> {
        (0..count).map(|value| value.to_string()).collect()
    }

    #[test]
    fn panel_repeats_stop_at_the_node_limit() {
        let mut repeats = Repeats::default();
        repeats.panels.insert(0, Repeat::new("host"));
        let mut template = DashboardTemplate::new(
            DashboardLayout::flat(1),
            repeats,
            &panels(&[("$host", None)]),
        );
        let values = many_values(MAX_LAYOUT_NODES + 10);
        let mut vars = Vars::new(&[]);
        vars.values.insert("host".to_string(), values);

        let materialized = template.materialize(&vars.get());

        assert_eq!(materialized.panels.len(), MAX_LAYOUT_NODES - 1);
        assert_eq!(node_count(&materialized.layout.items), MAX_LAYOUT_NODES - 1);
        assert!(materialized.truncated);
    }

    #[test]
    fn row_and_tab_repeats_stop_at_the_node_limit_and_keep_first_copies() {
        let mut repeats = Repeats::default();
        repeats.rows.insert(RowId::new(0), Repeat::new("host"));
        repeats
            .tabs
            .insert((TabGroupId::new(0), 0), Repeat::new("host"));
        let layout = DashboardLayout::new(vec![
            DashboardLayoutItem::Row(DashboardRow::new(
                RowId::new(0),
                "$host",
                false,
                false,
                vec![DashboardLayoutItem::Panel(0)],
            )),
            DashboardLayoutItem::Tabs(DashboardTabs::new(
                TabGroupId::new(0),
                vec![DashboardTab {
                    title: "$host".to_string(),
                    children: vec![DashboardLayoutItem::Panel(1)],
                }],
            )),
            DashboardLayoutItem::Panel(2),
        ]);
        let titles = [("A", None), ("B", None), ("C", None)];
        let mut template = DashboardTemplate::new(layout, repeats, &panels(&titles));
        let mut vars = Vars::new(&[]);
        vars.values
            .insert("host".to_string(), many_values(MAX_LAYOUT_NODES));

        let materialized = template.materialize(&vars.get());

        let count = node_count(&materialized.layout.items);
        assert!(materialized.truncated);
        assert!(count <= MAX_LAYOUT_NODES, "{count}");
        assert!(count > MAX_LAYOUT_NODES - 8, "{count}");
        let items = &materialized.layout.items;
        assert!(matches!(items.last(), Some(DashboardLayoutItem::Panel(2))));
        let Some(DashboardLayoutItem::Tabs(group)) = items.iter().rev().nth(1) else {
            panic!("expected the tab group before the last panel");
        };
        assert_eq!(group.tabs.len(), 1);
        assert_eq!(group.tabs[0].children, vec![DashboardLayoutItem::Panel(1)]);
    }

    #[test]
    fn children_past_the_depth_limit_are_left_out() {
        let levels = MAX_LAYOUT_DEPTH + 2;
        let mut items = vec![DashboardLayoutItem::Panel(0)];
        for id in 1..levels {
            items = vec![DashboardLayoutItem::Row(DashboardRow::new(
                RowId::new(id),
                "Row",
                false,
                false,
                items,
            ))];
        }
        // Row 2 is at the deepest level that the builder opens.
        let mut repeats = Repeats::default();
        repeats.rows.insert(RowId::new(2), Repeat::new("dc"));
        let layout = DashboardLayout::new(items);
        let mut template = DashboardTemplate::new(layout, repeats, &panels(&[("A", None)]));
        let vars = Vars::new(&[("dc", &["eu", "us"])]);

        let materialized = template.materialize(&vars.get());

        let mut walk = LayoutWalk::new(&materialized.layout.items, WalkScope::All);
        let deepest: Vec<_> = walk
            .by_ref()
            .filter(|event| event.level == MAX_LAYOUT_DEPTH)
            .map(|event| event.item.clone())
            .collect();
        assert!(!walk.truncated());
        assert_eq!(deepest.len(), 2);
        assert!(deepest.iter().all(|item| matches!(
            item,
            DashboardLayoutItem::Row(row) if row.children.is_empty()
        )));
        assert!(materialized.panels.is_empty());
    }
}
