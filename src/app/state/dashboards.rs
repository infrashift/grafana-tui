//! Several dashboards in one session. Each dashboard is a tab of one tab
//! group pinned above the content. Only the active dashboard queries
//! Prometheus: its panels, as for any tab, and its variables, which are its
//! tab's variables.

use super::AppState;
use crate::app::template::SectionInstance;
use crate::dashboard::{DashboardItemId, SectionId, TabGroupId};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// One dashboard of several loaded together.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DashboardInfo {
    /// The dashboard's own title, for the header.
    pub(crate) title: String,
    pub(crate) path: PathBuf,
    /// How often it refreshes while it is the active dashboard.
    pub(crate) refresh_every: Duration,
    /// Panels its import skipped.
    pub(crate) skipped_panels: usize,
}

/// Dashboards loaded together, one tab each in the tab group `group`.
#[derive(Debug, Clone)]
pub(crate) struct DashboardSet {
    pub(crate) group: TabGroupId,
    pub(crate) dashboards: Vec<DashboardInfo>,
    /// The dashboard each row and tab section belongs to, by imported id.
    section_owner: HashMap<SectionId, usize>,
    /// Where the user left each dashboard, to return there.
    views: Vec<DashboardView>,
}

#[derive(Debug, Clone, Copy, Default)]
struct DashboardView {
    selected_item: Option<DashboardItemId>,
    vertical_scroll: usize,
}

impl DashboardSet {
    pub(crate) fn new(
        group: TabGroupId,
        dashboards: Vec<DashboardInfo>,
        section_owner: HashMap<SectionId, usize>,
    ) -> Self {
        debug_assert!(
            dashboards.len() <= crate::limits::MAX_DASHBOARD_FILES,
            "the dashboards are in the file limit"
        );
        debug_assert!(
            section_owner
                .values()
                .all(|&owner| owner < dashboards.len()),
            "each section belongs to a loaded dashboard"
        );
        let views = vec![DashboardView::default(); dashboards.len()];
        Self {
            group,
            dashboards,
            section_owner,
            views,
        }
    }
}

/// Where the selection goes when another dashboard becomes active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DashboardFocus {
    /// The dashboard tab bar, as after a click on a tab.
    Bar,
    /// Where the user left the dashboard, or its first item.
    Content,
}

/// Tab labels for dashboard titles: without a `"Prefix / "` that every title
/// shares, so "HashiStack / Nodes" and "HashiStack / Vault" become "Nodes" and
/// "Vault".
pub(crate) fn dashboard_tab_labels(titles: &[String]) -> Vec<String> {
    let prefix = titles.first().and_then(|first| {
        let end = first.rfind(" / ")? + " / ".len();
        let prefix = first.get(..end)?;
        titles
            .iter()
            .all(|title| title.len() > prefix.len() && title.starts_with(prefix))
            .then_some(prefix)
    });
    debug_assert!(
        prefix.is_none_or(|prefix| prefix.ends_with(" / ")),
        "the shared prefix ends with the separator"
    );
    let labels: Vec<String> = titles
        .iter()
        .map(|title| {
            prefix
                .and_then(|prefix| title.strip_prefix(prefix))
                .unwrap_or(title)
        })
        .map(str::to_string)
        .collect();
    debug_assert_eq!(labels.len(), titles.len(), "each title has one label");
    debug_assert!(
        labels
            .iter()
            .zip(titles)
            .all(|(label, title)| title.ends_with(label.as_str())
                && (title.is_empty() || !label.is_empty())),
        "each label is the end of its title, and is not empty"
    );
    labels
}

/// Returns `active` moved by `direction` and wrapped to `count`, or `None` when `count` is 0.
fn wrapped_index(active: usize, direction: isize, count: usize) -> Option<usize> {
    let step = direction.unsigned_abs().checked_rem(count)?;
    let active = active % count;
    debug_assert!(
        step < count && active < count,
        "the step and the start are in range"
    );
    let next = if direction < 0 {
        (active + count - step) % count
    } else {
        (active + step) % count
    };
    debug_assert!(next < count, "the wrapped index is in range");
    Some(next)
}

impl AppState {
    /// Shows several dashboards, one per tab of `set.group`, and starts with the
    /// first. Call this function after `apply_template`.
    pub(crate) fn set_dashboards(&mut self, set: DashboardSet) {
        debug_assert_eq!(
            self.layout.tabs(set.group).map(|group| group.tabs.len()),
            Some(set.dashboards.len()),
            "the tab group has one tab for each dashboard"
        );
        debug_assert_eq!(
            set.views.len(),
            set.dashboards.len(),
            "each dashboard has a view"
        );
        let group = set.group;
        self.dashboards = Some(set);
        self.apply_dashboard_info(0);
        self.selected_item = self
            .layout
            .first_tab_descendant(group)
            .or(Some(DashboardItemId::Tabs(group)));
        self.vertical_scroll = 0;
        debug_assert_eq!(
            self.active_dashboard(),
            Some(0),
            "the first dashboard is active"
        );
    }

    /// The tab group that holds one tab per dashboard, with several dashboards.
    pub(crate) fn dashboard_group(&self) -> Option<TabGroupId> {
        self.dashboards.as_ref().map(|set| set.group)
    }

    /// What an empty tab of group `group` says.
    pub(crate) fn empty_tab_message(&self, group: TabGroupId) -> &'static str {
        if self.dashboard_group() == Some(group) {
            "No supported panels in this dashboard"
        } else {
            "No supported panels in this tab"
        }
    }

    /// The index of the dashboard shown, with several dashboards.
    pub(crate) fn active_dashboard(&self) -> Option<usize> {
        self.layout.tabs(self.dashboard_group()?)?.active
    }

    /// Shows the next dashboard, or the previous for a negative `direction`,
    /// and wraps around at the ends.
    pub(crate) fn cycle_dashboard(&mut self, direction: isize) -> Result<()> {
        let (Some(set), Some(active)) = (&self.dashboards, self.active_dashboard()) else {
            return Ok(());
        };
        debug_assert!(
            active < set.dashboards.len(),
            "the active dashboard is in range"
        );
        let Some(next) = wrapped_index(active, direction, set.dashboards.len()) else {
            return Ok(());
        };
        debug_assert!(
            next < set.dashboards.len(),
            "the next dashboard is in range"
        );
        self.activate_dashboard(next, DashboardFocus::Content)
    }

    /// Shows dashboard `index`. Only it queries Prometheus from now on, and it
    /// refreshes at once and reloads its variables, as when a dashboard opens.
    pub(crate) fn activate_dashboard(&mut self, index: usize, focus: DashboardFocus) -> Result<()> {
        let (Some(group), Some(previous)) = (self.dashboard_group(), self.active_dashboard())
        else {
            return Ok(());
        };
        let count = self
            .dashboards
            .as_ref()
            .map_or(0, |set| set.dashboards.len());
        if index == previous || index >= count {
            if focus == DashboardFocus::Bar {
                self.selected_item = Some(DashboardItemId::Tabs(group));
            }
            return Ok(());
        }
        debug_assert!(previous < count, "the previous dashboard is in range");
        self.save_dashboard_view(previous);
        self.layout.set_active_tab(group, index);
        // The unfiltered layout holds the panels that data conditions keep in
        // the fetch, so it must show the same dashboard.
        if self.template.is_some() {
            self.unfiltered_layout
                .sync_state_from(&self.layout, &self.hidden_items);
        }
        self.apply_dashboard_info(index);
        self.refreshes.dashboard_switched();

        match focus {
            DashboardFocus::Bar => self.selected_item = Some(DashboardItemId::Tabs(group)),
            DashboardFocus::Content => {
                let view = self
                    .dashboards
                    .as_ref()
                    .and_then(|set| set.views.get(index).copied())
                    .unwrap_or_default();
                self.selected_item = view
                    .selected_item
                    .or_else(|| self.layout.first_tab_descendant(group));
                self.vertical_scroll = view.vertical_scroll;
            }
        }
        self.ensure_selection_visible();
        self.start_refresh();
        self.reconcile_visible_annotation_targets();
        debug_assert_eq!(
            self.active_dashboard(),
            Some(index),
            "the dashboard is active"
        );
        Ok(())
    }

    /// Keeps the selection and the scroll of dashboard `previous`, to return there.
    fn save_dashboard_view(&mut self, previous: usize) {
        let view = DashboardView {
            selected_item: self.selected_item,
            vertical_scroll: self.vertical_scroll,
        };
        debug_assert!(
            self.dashboards
                .as_ref()
                .is_some_and(|set| previous < set.views.len()),
            "the previous dashboard has a view"
        );
        if let Some(slot) = self
            .dashboards
            .as_mut()
            .and_then(|set| set.views.get_mut(previous))
        {
            *slot = view;
        }
        debug_assert!(
            self.dashboards
                .as_ref()
                .and_then(|set| set.views.get(previous))
                .is_some_and(|saved| saved.selected_item == view.selected_item),
            "the view of the previous dashboard is saved"
        );
    }

    /// Shows dashboard `index`'s title and refreshes at its interval.
    fn apply_dashboard_info(&mut self, index: usize) {
        debug_assert!(
            self.active_dashboard().is_none_or(|active| active == index),
            "the dashboard at the index is the active one"
        );
        let Some(info) = self
            .dashboards
            .as_ref()
            .and_then(|set| set.dashboards.get(index))
        else {
            return;
        };
        self.title = format!("{} (imported)", info.title);
        self.skipped_panels = info.skipped_panels;
        self.refresh_every = info.refresh_every;
        debug_assert!(
            self.dashboards
                .as_ref()
                .is_some_and(|set| index < set.dashboards.len()),
            "the dashboard index is in range"
        );
    }

    /// The row and tab copies whose variables the app queries: all of them,
    /// except those of dashboards that do not show.
    pub(super) fn live_section_instances(&self) -> Vec<SectionInstance> {
        let Some(set) = &self.dashboards else {
            return self.section_instances.clone();
        };
        let active = self.active_dashboard();
        debug_assert!(
            set.section_owner
                .values()
                .all(|&owner| owner < set.dashboards.len()),
            "each section belongs to a loaded dashboard"
        );
        let live: Vec<SectionInstance> = self
            .section_instances
            .iter()
            .filter(|instance| {
                set.section_owner
                    .get(&instance.id)
                    .is_none_or(|&owner| Some(owner) == active)
            })
            .cloned()
            .collect();
        debug_assert!(
            live.len() <= self.section_instances.len(),
            "the live sections are a part of the sections"
        );
        live
    }

    /// Panels of the dashboard shown, with several dashboards, also those
    /// in its collapsed rows, inactive tabs, or hidden by conditions.
    pub(super) fn active_dashboard_panel_indices(&self) -> Option<Vec<usize>> {
        let group = self.dashboard_group()?;
        let active = self.active_dashboard()?;
        Some(self.unfiltered_layout.tab_panel_indices(group, active))
    }
}

/// Several dashboards loaded as `main` loads them, for tests.
#[cfg(test)]
pub(crate) mod test_support {
    use super::AppState;
    use crate::export::ExportOptions;
    use crate::prom;
    use crate::theme::Theme;
    use std::time::Duration;

    /// A V2 dashboard with one range panel that queries `expr`, and an `instance`
    /// variable that `instance_query` resolves. It refreshes every `auto_refresh`.
    pub(crate) fn v2_dashboard(
        title: &str,
        auto_refresh: &str,
        expr: &str,
        instance_query: &str,
    ) -> String {
        assert!(!title.is_empty(), "the test dashboard has a title");
        assert!(
            humantime::parse_duration(auto_refresh).is_ok(),
            "the auto refresh of the test dashboard is a duration"
        );
        serde_json::json!({
            "apiVersion": "dashboard.grafana.app/v2",
            "kind": "Dashboard",
            "spec": {
                "title": title,
                "elements": {"panel-1": {"kind": "Panel", "spec": {
                    "id": 1,
                    "title": "Panel",
                    "data": {"kind": "QueryGroup", "spec": {"queries": [{
                        "kind": "PanelQuery",
                        "spec": {"refId": "A", "query": {
                            "kind": "DataQuery", "group": "prometheus", "spec": {"expr": expr}
                        }}
                    }]}},
                    "vizConfig": {"kind": "VizConfig", "group": "timeseries", "spec": {}}
                }}},
                "layout": {"kind": "GridLayout", "spec": {"items": [{
                    "kind": "GridLayoutItem",
                    "spec": {"x": 0, "y": 0, "width": 24, "height": 8,
                             "element": {"kind": "ElementReference", "name": "panel-1"}}
                }]}},
                "variables": [{"kind": "QueryVariable", "spec": {
                    "name": "instance",
                    "query": {"kind": "DataQuery", "group": "prometheus",
                              "spec": {"query": instance_query}}
                }}],
                "timeSettings": {"from": "now-6h", "to": "now", "autoRefresh": auto_refresh}
            }
        })
        .to_string()
    }

    /// The app `main` builds for several dashboards.
    pub(crate) fn dashboards_app(url: &str, documents: &[&str]) -> AppState {
        let set = crate::grafana::parse_grafana_dashboards(documents).unwrap();
        let setup = crate::build_dashboard_set(set, None, &[], None, None);
        let mut app = AppState::new(
            prom::PromClient::new(url.to_string()),
            Duration::from_secs(300),
            Duration::from_secs(5),
            Duration::from_secs(1),
            "grafana-tui".to_string(),
            setup.panels,
            0,
            Theme::default(),
            "dashed".to_string(),
            ExportOptions::default(),
        );
        app.variable_names = setup.variables.names;
        app.apply_template(setup.template);
        app.set_dashboards(setup.set);
        assert_eq!(
            app.dashboards.as_ref().map(|set| set.dashboards.len()),
            Some(documents.len()),
            "each document is a dashboard"
        );
        assert_eq!(
            app.active_dashboard(),
            Some(0),
            "the first dashboard is active"
        );
        app
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_index_wraps_both_ways_and_rejects_no_dashboards() {
        assert_eq!(wrapped_index(0, 1, 0), None);
        assert_eq!(wrapped_index(2, 1, 3), Some(0));
        assert_eq!(wrapped_index(0, -1, 3), Some(2));
        assert_eq!(wrapped_index(1, -7, 3), Some(0));
        assert_eq!(wrapped_index(1, 7, 3), Some(2));
        assert_eq!(wrapped_index(4, 0, 3), Some(1));
    }

    #[test]
    fn tab_labels_drop_a_shared_title_prefix() {
        let titles = |titles: &[&str]| titles.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        assert_eq!(
            dashboard_tab_labels(&titles(&["HashiStack / Nodes", "HashiStack / Vault"])),
            ["Nodes", "Vault"]
        );
        // Kept when not every title shares it, or when a label without it is empty.
        assert_eq!(
            dashboard_tab_labels(&titles(&["HashiStack / Nodes", "Prometheus"])),
            ["HashiStack / Nodes", "Prometheus"]
        );
        assert_eq!(
            dashboard_tab_labels(&titles(&["A / B", "A / "])),
            ["A / B", "A / "]
        );
        assert_eq!(dashboard_tab_labels(&titles(&["Nodes"])), ["Nodes"]);
    }
}
