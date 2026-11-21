use anyhow::Result;

use super::{
    DashboardImport, GridPos, ImportDiagnostic, QueryPanel, SectionVariable, TemplateQueryVar,
    VariableRefresh, model,
};
use anyhow::ensure;

use crate::limits::MAX_DASHBOARD_VARIABLES;

use crate::dashboard::{
    DashboardLayoutItem, MAX_LAYOUT_DEPTH, MAX_LAYOUT_NODES, MAX_LAYOUT_STEPS, SectionId,
};

pub(super) fn finish(dashboard: model::Dashboard) -> Result<DashboardImport> {
    let model::Dashboard {
        title,
        refresh,
        variables,
        layout,
        skipped_panels,
        diagnostics,
    } = dashboard;
    let mut out = DashboardImport {
        title,
        refresh_rate_ms: refresh.as_deref().and_then(parse_refresh_rate_ms),
        skipped_panels,
        diagnostics,
        ..DashboardImport::default()
    };
    let variable_names: std::collections::HashSet<String> = variables
        .iter()
        .map(|variable| variable.name.clone())
        .collect();
    out.variable_names.clone_from(&variable_names);
    let mut ids = LayoutIds {
        variable_names,
        ..LayoutIds::default()
    };
    ids.count_variables(variables.len())?;
    import_variables(&mut out, variables);
    out.layout =
        crate::dashboard::DashboardLayout::new(import_layout_nodes(layout, &mut out, &mut ids, 1)?);
    Ok(out)
}

/// Imports several dashboards as the tabs of one tab group, one tab per
/// dashboard. Each dashboard's variables become its tab's variables, so they
/// apply only inside that tab. Returns the import and the group's id.
pub(super) fn finish_many(
    dashboards: Vec<model::Dashboard>,
) -> Result<(DashboardImport, crate::dashboard::TabGroupId)> {
    let mut out = DashboardImport::default();
    let mut ids = LayoutIds::default();
    let root = crate::dashboard::TabGroupId::new(ids.next_tabs);
    ids.next_tabs += 1;
    ids.count(dashboards.len().saturating_add(1))?;
    let mut tabs = Vec::with_capacity(dashboards.len());
    for (index, dashboard) in dashboards.into_iter().enumerate() {
        out.skipped_panels += dashboard.skipped_panels;
        // Each dashboard's repeats can only use its own variables.
        ids.variable_names = dashboard
            .variables
            .iter()
            .map(|variable| variable.name.clone())
            .collect();
        ids.variables = 0;
        ids.count_variables(dashboard.variables.len())?;
        if !dashboard.variables.is_empty() {
            out.sections.insert(
                SectionId::Tab(root, index),
                import_section_variables(dashboard.variables),
            );
        }
        tabs.push(crate::dashboard::DashboardTab {
            title: dashboard.title,
            children: import_layout_nodes(dashboard.layout, &mut out, &mut ids, 2)?,
        });
    }
    out.layout =
        crate::dashboard::DashboardLayout::new(vec![crate::dashboard::DashboardLayoutItem::Tabs(
            crate::dashboard::DashboardTabs::new(root, tabs),
        )]);
    Ok((out, root))
}

fn import_variables(out: &mut DashboardImport, variables: Vec<model::Variable>) {
    for variable in variables {
        import_variable(out, variable);
    }
}

/// Imports one variable: its selection, its query value and its query.
fn import_variable(out: &mut DashboardImport, variable: model::Variable) {
    let select_all = current_is_all(variable.current.as_ref());
    if select_all {
        out.all_vars.insert(variable.name.clone());
    }
    let regex_values = variable.multi || variable.include_all;
    if regex_values {
        out.regex_vars.insert(variable.name.clone());
    }
    let selected = record_selection(out, &variable, select_all, regex_values);

    // A `Never` refresh keeps the saved options, as Grafana does; only a
    // variable saved without any is resolved once so the dashboard works.
    let saved = if select_all {
        !variable.options.is_empty()
    } else {
        !selected.is_empty()
    };
    let resolves = match variable.refresh {
        Some(model::VariableRefresh::Never) => !saved,
        _ => true,
    };
    // An explicit multi-value selection is kept as chosen; otherwise query
    // variables resolve against Prometheus like Grafana does on load.
    if variable.kind.as_deref() == Some("query")
        && resolves
        && selected.len() <= 1
        && let (Some(query), Some(query_path)) = (variable.query, variable.query_path)
    {
        out.query_vars.push(TemplateQueryVar {
            name: variable.name,
            query,
            regex: variable.regex.filter(|regex| !regex.trim().is_empty()),
            query_path,
            select_all,
            all_value: variable.all_value,
            regex_values,
            refresh: match variable.refresh {
                Some(model::VariableRefresh::OnTimeRangeChange) => {
                    VariableRefresh::OnTimeRangeChange
                }
                _ => VariableRefresh::OnLoad,
            },
        });
    }
}

/// Records the selected values of a variable and its query value. Returns the selected values.
fn record_selection(
    out: &mut DashboardImport,
    variable: &model::Variable,
    select_all: bool,
    regex_values: bool,
) -> Vec<String> {
    let mut selected = selected_values(variable.current.as_ref());
    if selected.is_empty() && !select_all && variable.kind.as_deref() != Some("query") {
        // Grafana selects the first option when nothing is saved.
        selected.extend(variable.options.first().cloned());
    }
    if select_all {
        // `All` covers every option; known options let repeats iterate them now,
        // and dynamic query variables fill them in when they resolve.
        if !variable.options.is_empty() {
            out.var_values
                .insert(variable.name.clone(), variable.options.clone());
        }
        let value = variable.all_value.clone().unwrap_or_else(|| {
            if variable.options.is_empty() {
                ".*".to_string()
            } else {
                crate::app::format_prometheus_values(&variable.options, true)
            }
        });
        out.vars.insert(variable.name.clone(), value);
    } else if !selected.is_empty() {
        out.var_values
            .insert(variable.name.clone(), selected.clone());
        out.vars.insert(
            variable.name.clone(),
            crate::app::format_prometheus_values(&selected, regex_values),
        );
    }
    selected
}

/// Imports the variables a row or tab defines, with the same selection and
/// formatting rules as dashboard variables.
fn import_section_variables(variables: Vec<model::Variable>) -> Vec<SectionVariable> {
    let names: Vec<String> = variables
        .iter()
        .map(|variable| variable.name.clone())
        .collect();
    let all_values: std::collections::HashMap<String, Option<String>> = variables
        .iter()
        .map(|variable| (variable.name.clone(), variable.all_value.clone()))
        .collect();
    let mut imported = DashboardImport::default();
    import_variables(&mut imported, variables);
    names
        .into_iter()
        .map(|name| SectionVariable {
            values: imported.var_values.remove(&name).unwrap_or_default(),
            regex: imported.regex_vars.contains(&name),
            all: imported.all_vars.contains(&name),
            all_value: all_values.get(&name).cloned().flatten(),
            query: imported
                .query_vars
                .iter()
                .find(|query_var| query_var.name == name)
                .cloned(),
            name,
        })
        .collect()
}

/// Non-empty values selected by a variable's `current` value, or its text as a
/// fallback, excluding Grafana's `$__all` marker.
fn selected_values(current: Option<&model::VariableCurrent>) -> Vec<String> {
    let Some(current) = current else {
        return Vec::new();
    };
    let values = |value: Option<&serde_json::Value>| -> Vec<String> {
        let values = match value {
            Some(serde_json::Value::String(value)) => vec![value.clone()],
            Some(serde_json::Value::Number(value)) => vec![value.to_string()],
            Some(serde_json::Value::Array(values)) => values
                .iter()
                .filter_map(|value| match value {
                    serde_json::Value::String(value) => Some(value.clone()),
                    serde_json::Value::Number(value) => Some(value.to_string()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        values
            .into_iter()
            .filter(|value| !value.is_empty() && value != "$__all")
            .collect()
    };
    let selected = values(current.value.as_ref());
    if selected.is_empty() {
        values(current.text.as_ref())
    } else {
        selected
    }
}

/// Whether `All` is selected: the value is Grafana's `$__all` marker, or, when
/// there is no value, the text says `All`. An explicit value list wins over text.
fn current_is_all(current: Option<&model::VariableCurrent>) -> bool {
    current.is_some_and(|current| match current.value.as_ref() {
        None | Some(serde_json::Value::Null) => value_is_all(current.text.as_ref()),
        Some(serde_json::Value::String(value)) if value.is_empty() => {
            value_is_all(current.text.as_ref())
        }
        value => value_is_all(value),
    })
}

/// Whether a variable value, or one string in a value list, selects `All`.
fn value_is_all(value: Option<&serde_json::Value>) -> bool {
    match value {
        Some(serde_json::Value::String(value)) => text_is_all(value),
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .any(|value| matches!(value, serde_json::Value::String(text) if text_is_all(text))),
        _ => false,
    }
}

/// Whether a text is Grafana's `$__all` marker or the word `All`.
fn text_is_all(text: &str) -> bool {
    text == "$__all" || text.eq_ignore_ascii_case("all")
}

fn import_panel(panel: model::Panel, out: &mut DashboardImport) -> Result<Option<usize>> {
    let panel_type = panel_type_for(&panel.kind);
    if panel_type == crate::app::PanelType::Unknown {
        report_unsupported_panel(&panel, out);
        return Ok(None);
    }

    let queries = collect_panel_queries(panel.targets, panel_type, out);
    let resolution = crate::app::QueryResolution {
        min_interval: checked_min_interval(panel.min_interval, out),
        max_data_points: panel.max_data_points,
        target_min_intervals: queries.min_intervals,
    };
    report_ignored_panel_fields(panel.transformations_path, panel.reduce_options_path, out);
    let fields = field_settings_from(panel.field_defaults, out);

    if queries.exprs.is_empty() {
        if panel.count_as_skipped_if_empty {
            out.skipped_panels += 1;
        }
        return Ok(None);
    }
    let options = match panel_type {
        crate::app::PanelType::Graph => crate::app::PanelOptions::Graph(fields.graph_options),
        _ => crate::app::PanelOptions::None,
    };
    let index = out.queries.len();
    out.queries.push(QueryPanel {
        title: panel.title,
        exprs: queries.exprs,
        expr_paths: queries.expr_paths,
        legends: queries.legends,
        query_modes: queries.query_modes,
        grid: panel.grid.map(|grid| GridPos {
            x: grid.x,
            y: grid.y,
            w: grid.w,
            h: grid.h,
        }),
        panel_type,
        thresholds: fields.thresholds,
        min: fields.min,
        max: fields.max,
        autogrid: fields.autogrid,
        display: fields.display,
        options,
        resolution,
    });
    Ok(Some(index))
}

/// Returns the panel type for a Grafana panel kind.
fn panel_type_for(kind: &str) -> crate::app::PanelType {
    match kind {
        "graph" | "timeseries" => crate::app::PanelType::Graph,
        "stat" => crate::app::PanelType::Stat,
        "gauge" => crate::app::PanelType::Gauge,
        "bargauge" => crate::app::PanelType::BarGauge,
        "table" => crate::app::PanelType::Table,
        "heatmap" => crate::app::PanelType::Heatmap,
        _ => crate::app::PanelType::Unknown,
    }
}

/// Counts and reports a panel of an unknown kind. Rows and empty kinds are not reported.
fn report_unsupported_panel(panel: &model::Panel, out: &mut DashboardImport) {
    if panel.kind.is_empty() || panel.kind == "row" {
        return;
    }
    out.skipped_panels += 1;
    out.diagnostics.push(ImportDiagnostic::new(
        "skipped_panel",
        panel.source_path.clone(),
        format!(
            "unsupported panel type `{}` skipped for panel `{}`",
            panel.kind, panel.title
        ),
    ));
}

/// The visible queries of a panel, in parallel lists.
struct PanelQueries {
    exprs: Vec<String>,
    expr_paths: Vec<String>,
    legends: Vec<Option<String>>,
    query_modes: Vec<crate::app::QueryMode>,
    min_intervals: Vec<Option<String>>,
}

/// Collects the visible targets that have an expression.
fn collect_panel_queries(
    targets: Vec<model::Target>,
    panel_type: crate::app::PanelType,
    out: &mut DashboardImport,
) -> PanelQueries {
    let mut queries = PanelQueries {
        exprs: Vec::new(),
        expr_paths: Vec::new(),
        legends: Vec::new(),
        query_modes: Vec::new(),
        min_intervals: Vec::new(),
    };
    for target in targets {
        if target.hidden {
            continue;
        }
        if let Some(expr) = target.expr {
            queries.exprs.push(expr);
            queries.expr_paths.push(target.expr_path);
            queries.legends.push(target.legend_format);
            queries
                .query_modes
                .push(query_mode_for_target(target.instant, panel_type));
            queries
                .min_intervals
                .push(checked_min_interval(target.min_interval, out));
        }
    }
    queries
}

/// Reports the panel fields that grafana-tui does not support.
fn report_ignored_panel_fields(
    transformations_path: Option<String>,
    reduce_options_path: Option<String>,
    out: &mut DashboardImport,
) {
    if let Some(path) = transformations_path {
        out.diagnostics.push(ImportDiagnostic::new(
            "ignored_field",
            path,
            "`transformations` are not supported yet; queries will run without Grafana transformations",
        ));
    }

    if let Some(path) = reduce_options_path {
        out.diagnostics.push(ImportDiagnostic::new(
            "ignored_field",
            path,
            "`options.reduceOptions` is not supported yet; grafana-tui will use default value selection",
        ));
    }
}

/// The display settings of a panel from its field config defaults.
struct PanelFieldSettings {
    thresholds: Option<crate::app::Thresholds>,
    min: Option<f64>,
    max: Option<f64>,
    autogrid: Option<bool>,
    display: crate::ui::DisplayFormat,
    graph_options: crate::app::GraphOptions,
}

/// Converts the field config defaults of a panel, and reports value mappings.
fn field_settings_from(
    defaults: Option<model::FieldDefaults>,
    out: &mut DashboardImport,
) -> PanelFieldSettings {
    let Some(defaults) = defaults else {
        return PanelFieldSettings {
            thresholds: None,
            min: None,
            max: None,
            autogrid: None,
            display: crate::ui::DisplayFormat::default(),
            graph_options: crate::app::GraphOptions::default(),
        };
    };
    if let Some(path) = defaults.mappings_path {
        out.diagnostics.push(ImportDiagnostic::new(
            "ignored_field",
            path,
            "`fieldConfig.defaults.mappings` is not supported yet; value mappings will be ignored",
        ));
    }
    PanelFieldSettings {
        graph_options: graph_options_from_custom(defaults.custom.as_ref()),
        display: crate::ui::DisplayFormat {
            unit: defaults.unit,
            decimals: defaults.decimals,
            no_value: defaults.no_value,
        },
        min: defaults.min,
        max: defaults.max,
        autogrid: defaults
            .custom
            .as_ref()
            .and_then(|custom| custom.axis_grid_show),
        thresholds: thresholds_from_model(defaults.thresholds, defaults.custom.as_ref()),
    }
}

/// Imports layout nodes and the nodes nested in their rows and tabs with an
/// explicit stack. `base_level` is the level of `nodes`.
fn import_layout_nodes(
    nodes: Vec<model::LayoutNode>,
    out: &mut DashboardImport,
    ids: &mut LayoutIds,
    base_level: usize,
) -> Result<Vec<DashboardLayoutItem>> {
    let mut frames = Vec::with_capacity(MAX_LAYOUT_DEPTH);
    frames.push(ImportFrame::new(nodes, ImportParent::Root));
    for _ in 0..MAX_LAYOUT_STEPS {
        let child_level = base_level.saturating_add(frames.len());
        let Some(frame) = frames.last_mut() else {
            break;
        };
        if let Some(node) = frame.nodes.next() {
            if let Some(child) = import_node(node, &mut frame.items, out, ids, child_level)? {
                frames.push(child);
            }
        } else if let Some(items) = close_import_frame(&mut frames, out, ids, base_level)? {
            return Ok(items);
        }
    }
    anyhow::bail!("the dashboard layout has more than {MAX_LAYOUT_STEPS} import steps")
}

/// Imports one node into `items`. A row or a tab group gives the frame of its
/// first child list, at level `child_level`.
fn import_node(
    node: model::LayoutNode,
    items: &mut Vec<DashboardLayoutItem>,
    out: &mut DashboardImport,
    ids: &mut LayoutIds,
    child_level: usize,
) -> Result<Option<ImportFrame>> {
    ids.count(1)?;
    match node {
        model::LayoutNode::Panel(panel) => {
            if let Some(index) = import_placed_panel(panel, out, ids)? {
                items.push(DashboardLayoutItem::Panel(index));
            }
            Ok(None)
        }
        model::LayoutNode::AutoGrid(grid) => {
            ids.count(grid.panels.len())?;
            items.extend(import_auto_grid(grid, out, ids)?);
            Ok(None)
        }
        model::LayoutNode::Row(mut row) => {
            check_depth(child_level, &row.source_path)?;
            ids.count_variables(row.variables.len())?;
            let id = register_row(&mut row, out, ids);
            let header = RowHeader {
                id,
                title: row.title,
                collapsed: row.collapsed,
                hidden_header: row.hidden_header,
            };
            Ok(Some(ImportFrame::new(
                row.children,
                ImportParent::Row(header),
            )))
        }
        model::LayoutNode::Tabs(group) => {
            let id = crate::dashboard::TabGroupId::new(ids.next_tabs);
            ids.next_tabs += 1;
            let open = OpenTabs {
                id,
                title: String::new(),
                tabs: Vec::with_capacity(group.tabs.len()),
                rest: group.tabs.into_iter().enumerate(),
            };
            match open_next_tab(open, out, ids, child_level)? {
                TabStep::Open(frame) => Ok(Some(frame)),
                TabStep::Done(item) => {
                    items.push(item);
                    Ok(None)
                }
            }
        }
    }
}

/// Ends the top frame and adds its row or tab to the frame below. Gives the
/// items of the top-level list when the root frame ends.
fn close_import_frame(
    frames: &mut Vec<ImportFrame>,
    out: &mut DashboardImport,
    ids: &mut LayoutIds,
    base_level: usize,
) -> Result<Option<Vec<DashboardLayoutItem>>> {
    let Some(ImportFrame { items, parent, .. }) = frames.pop() else {
        return Ok(Some(Vec::new()));
    };
    let finished = match parent {
        ImportParent::Root => return Ok(Some(items)),
        ImportParent::Row(row) => DashboardLayoutItem::Row(crate::dashboard::DashboardRow::new(
            row.id,
            row.title,
            row.collapsed,
            row.hidden_header,
            items,
        )),
        ImportParent::Tab(mut open) => {
            open.tabs.push(crate::dashboard::DashboardTab {
                title: std::mem::take(&mut open.title),
                children: items,
            });
            let child_level = base_level.saturating_add(frames.len());
            match open_next_tab(open, out, ids, child_level)? {
                TabStep::Open(frame) => {
                    frames.push(frame);
                    return Ok(None);
                }
                TabStep::Done(item) => item,
            }
        }
    };
    if let Some(outer) = frames.last_mut() {
        outer.items.push(finished);
    }
    Ok(None)
}

/// Registers the next tab of `open` and gives the frame of its children, or
/// gives the finished tab group after the last tab.
fn open_next_tab(
    mut open: OpenTabs,
    out: &mut DashboardImport,
    ids: &mut LayoutIds,
    child_level: usize,
) -> Result<TabStep> {
    let Some((index, mut tab)) = open.rest.next() else {
        return Ok(TabStep::Done(DashboardLayoutItem::Tabs(
            crate::dashboard::DashboardTabs::new(open.id, open.tabs),
        )));
    };
    check_depth(child_level, &tab.source_path)?;
    ids.count(1)?;
    ids.count_variables(tab.variables.len())?;
    register_tab(&mut tab, (open.id, index), out, ids);
    open.title = tab.title;
    let children = tab.children;
    Ok(TabStep::Open(ImportFrame::new(
        children,
        ImportParent::Tab(open),
    )))
}

/// Fails when a child list at `level` nests deeper than MAX_LAYOUT_DEPTH.
fn check_depth(level: usize, path: &str) -> Result<()> {
    ensure!(
        level <= MAX_LAYOUT_DEPTH,
        "the dashboard layout at {path} nests rows and tabs deeper than the limit of \
         {MAX_LAYOUT_DEPTH} levels"
    );
    Ok(())
}

/// A list of layout nodes that the importer reads, with the row or tab that holds it.
struct ImportFrame {
    nodes: std::vec::IntoIter<model::LayoutNode>,
    items: Vec<DashboardLayoutItem>,
    parent: ImportParent,
}

impl ImportFrame {
    /// Starts a list of nodes inside `parent`.
    fn new(nodes: Vec<model::LayoutNode>, parent: ImportParent) -> Self {
        Self {
            nodes: nodes.into_iter(),
            items: Vec::new(),
            parent,
        }
    }
}

/// The row or tab that holds a list of imported items.
enum ImportParent {
    Root,
    Row(RowHeader),
    Tab(OpenTabs),
}

/// A row with no children yet.
struct RowHeader {
    id: crate::dashboard::RowId,
    title: String,
    collapsed: bool,
    hidden_header: bool,
}

/// A tab group that the importer fills one tab at a time.
struct OpenTabs {
    id: crate::dashboard::TabGroupId,
    /// The title of the tab whose children the importer reads now.
    title: String,
    tabs: Vec<crate::dashboard::DashboardTab>,
    rest: std::iter::Enumerate<std::vec::IntoIter<model::Tab>>,
}

/// The frame of the next tab, or the finished tab group.
enum TabStep {
    Open(ImportFrame),
    Done(DashboardLayoutItem),
}

/// Imports a panel with its repeat and condition. Returns its query index.
fn import_placed_panel(
    mut panel: model::Panel,
    out: &mut DashboardImport,
    ids: &LayoutIds,
) -> Result<Option<usize>> {
    let repeat = ids.checked_repeat(panel.repeat.take(), &panel.source_path, out);
    let condition = panel.condition.take();
    let Some(index) = import_panel(panel, out)? else {
        return Ok(None);
    };
    if let Some(repeat) = repeat {
        out.repeats.panels.insert(index, repeat);
    }
    if let Some(condition) = condition {
        out.conditions.panels.insert(index, condition);
    }
    Ok(Some(index))
}

/// Gives a row its id and records its variables, repeat and condition.
///
/// The caller imports the children of the row.
fn register_row(
    row: &mut model::Row,
    out: &mut DashboardImport,
    ids: &mut LayoutIds,
) -> crate::dashboard::RowId {
    let id = crate::dashboard::RowId::new(ids.next_row);
    ids.next_row += 1;
    // A row's variables apply to the row itself, so they are known
    // before its own repeat is checked.
    let variables = std::mem::take(&mut row.variables);
    if !variables.is_empty() {
        ids.variable_names
            .extend(variables.iter().map(|variable| variable.name.clone()));
        out.sections
            .insert(SectionId::Row(id), import_section_variables(variables));
    }
    if let Some(repeat) = ids.checked_repeat(row.repeat.take(), &row.source_path, out) {
        out.repeats.rows.insert(id, repeat);
    }
    if let Some(condition) = row.condition.take() {
        out.conditions.rows.insert(id, condition);
    }
    id
}

/// Records the variables, repeat and condition of the tab at `key`.
///
/// The caller imports the children of the tab.
fn register_tab(
    tab: &mut model::Tab,
    key: (crate::dashboard::TabGroupId, usize),
    out: &mut DashboardImport,
    ids: &mut LayoutIds,
) {
    let (id, index) = key;
    let variables = std::mem::take(&mut tab.variables);
    if !variables.is_empty() {
        ids.variable_names
            .extend(variables.iter().map(|variable| variable.name.clone()));
        out.sections.insert(
            SectionId::Tab(id, index),
            import_section_variables(variables),
        );
    }
    if let Some(repeat) = ids.checked_repeat(tab.repeat.take(), &tab.source_path, out) {
        out.repeats.tabs.insert(key, repeat);
    }
    if let Some(condition) = tab.condition.take() {
        out.conditions.tabs.insert(key, condition);
    }
}

/// Imports the panels of an auto grid. An auto grid without panels gives `None`.
fn import_auto_grid(
    grid: model::AutoGrid,
    out: &mut DashboardImport,
    ids: &LayoutIds,
) -> Result<Option<crate::dashboard::DashboardLayoutItem>> {
    let mut panels = Vec::with_capacity(grid.panels.len());
    for panel in grid.panels {
        if let Some(index) = import_placed_panel(panel, out, ids)? {
            panels.push(index);
        }
    }
    if panels.is_empty() {
        return Ok(None);
    }
    Ok(Some(crate::dashboard::DashboardLayoutItem::AutoGrid(
        crate::dashboard::DashboardAutoGrid {
            panels,
            max_columns: grid.max_columns,
            min_column_width: auto_grid_column_cells(grid.column_width_px),
            row_height: auto_grid_row_units(grid.row_height_px),
        },
    )))
}

/// Grafana CSS pixels per terminal column, used to size auto grid columns.
///
/// This is Grafana's 8px design-system spacing unit, close to a typical monospace
/// cell width, so column counts match what Grafana shows at a similar pixel width.
const AUTO_GRID_PX_PER_COLUMN: f64 = 8.0;
/// Height of one fixed-grid unit in Grafana: a 30px cell plus its 8px margin.
const GRID_UNIT_PX: f64 = 38.0;
const GRID_MARGIN_PX: f64 = 8.0;

fn auto_grid_column_cells(width_px: f64) -> u16 {
    (width_px / AUTO_GRID_PX_PER_COLUMN)
        .round()
        .clamp(1.0, f64::from(u16::MAX)) as u16
}

/// Converts a pixel height to the fixed-grid units used by `GridPos::h`, where `h`
/// units span `h * 30px + (h - 1) * 8px`.
fn auto_grid_row_units(height_px: f64) -> u16 {
    ((height_px + GRID_MARGIN_PX) / GRID_UNIT_PX)
        .round()
        .clamp(1.0, f64::from(u16::MAX)) as u16
}

#[derive(Default)]
struct LayoutIds {
    next_row: usize,
    next_tabs: usize,
    variable_names: std::collections::HashSet<String>,
    /// The layout items imported so far, across every dashboard of a set.
    nodes: usize,
    /// The variables of the dashboard so far, with those of its rows and tabs.
    variables: usize,
}

impl LayoutIds {
    /// Adds `added` items to the count, and fails over MAX_LAYOUT_NODES.
    fn count(&mut self, added: usize) -> Result<()> {
        self.nodes = self.nodes.saturating_add(added);
        ensure!(
            self.nodes <= MAX_LAYOUT_NODES,
            "the dashboard layout holds more than the limit of {MAX_LAYOUT_NODES} rows, tabs \
             and panels"
        );
        Ok(())
    }

    /// Adds `added` variables to the count, and fails over `MAX_DASHBOARD_VARIABLES`.
    fn count_variables(&mut self, added: usize) -> Result<()> {
        self.variables = self.variables.saturating_add(added);
        ensure!(
            self.variables <= MAX_DASHBOARD_VARIABLES,
            "the dashboard holds more than the limit of {MAX_DASHBOARD_VARIABLES} variables"
        );
        Ok(())
    }

    /// Keeps a repeat whose variable the dashboard defines; otherwise the item is
    /// shown once, as Grafana does, with a diagnostic.
    fn checked_repeat(
        &self,
        repeat: Option<model::Repeat>,
        source_path: &str,
        out: &mut DashboardImport,
    ) -> Option<model::Repeat> {
        let repeat = repeat?;
        if self.variable_names.contains(&repeat.variable) {
            return Some(repeat);
        }
        out.diagnostics.push(ImportDiagnostic::new(
            "unknown_repeat_variable",
            source_path,
            format!(
                "repeat variable `{}` is not defined; the item is shown once",
                repeat.variable
            ),
        ));
        None
    }
}

/// Keeps a min interval that is a duration or references a variable, which
/// resolves when queries run; anything else is dropped with a diagnostic.
fn checked_min_interval(
    interval: Option<model::MinInterval>,
    out: &mut DashboardImport,
) -> Option<String> {
    let interval = interval?;
    if interval.text.contains('$') || crate::app::parse_min_interval(&interval.text).is_some() {
        return Some(interval.text);
    }
    out.diagnostics.push(ImportDiagnostic::new(
        "ignored_field",
        interval.path,
        format!(
            "min interval `{}` is not a duration; the default query step is used",
            interval.text
        ),
    ));
    None
}

fn query_mode_for_target(
    instant: Option<bool>,
    panel_type: crate::app::PanelType,
) -> crate::app::QueryMode {
    match instant {
        Some(true) => crate::app::QueryMode::Instant,
        Some(false) => crate::app::QueryMode::Range,
        None => default_query_mode_for_panel(panel_type),
    }
}

fn default_query_mode_for_panel(panel_type: crate::app::PanelType) -> crate::app::QueryMode {
    match panel_type {
        crate::app::PanelType::Gauge
        | crate::app::PanelType::BarGauge
        | crate::app::PanelType::Table => crate::app::QueryMode::Instant,
        _ => crate::app::QueryMode::Range,
    }
}

fn graph_options_from_custom(custom: Option<&model::GraphCustom>) -> crate::app::GraphOptions {
    let Some(custom) = custom else {
        return crate::app::GraphOptions::default();
    };
    crate::app::GraphOptions {
        draw_style: parse_graph_draw_style(custom.draw_style.as_deref()),
        show_points: parse_graph_point_mode(custom.show_points.as_deref()),
        fill_opacity: custom.fill_opacity.map(|value| value.min(100) as u8),
        axis_placement: parse_graph_axis_placement(custom.axis_placement.as_deref()),
        line_interpolation: custom
            .line_interpolation
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .cloned(),
        stacking: parse_graph_stacking_mode(custom.stacking_mode.as_deref()),
    }
}

fn thresholds_from_model(
    thresholds: Option<model::Thresholds>,
    custom: Option<&model::GraphCustom>,
) -> Option<crate::app::Thresholds> {
    let thresholds = thresholds?;
    let mode = match thresholds.mode.as_deref() {
        Some("percentage") => crate::app::ThresholdMode::Percentage,
        _ => crate::app::ThresholdMode::Absolute,
    };
    let mut steps: Vec<_> = thresholds
        .steps
        .into_iter()
        .map(|step| {
            let color = step.color.unwrap_or_else(|| "green".to_string());
            crate::app::ThresholdStep {
                value: step.value,
                color: crate::theme::parse_grafana_color(&color),
            }
        })
        .collect();
    steps.sort_by(|a, b| {
        let a_value = a.value.unwrap_or(f64::NEG_INFINITY);
        let b_value = b.value.unwrap_or(f64::NEG_INFINITY);
        a_value
            .partial_cmp(&b_value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    (!steps.is_empty()).then(|| crate::app::Thresholds {
        mode,
        steps,
        style: Some(
            custom
                .and_then(|custom| custom.thresholds_style_mode.clone())
                .unwrap_or_else(|| "line".to_string()),
        ),
    })
}

fn parse_graph_draw_style(value: Option<&str>) -> crate::app::GraphDrawStyle {
    match value {
        Some("points") => crate::app::GraphDrawStyle::Points,
        Some("bars") => crate::app::GraphDrawStyle::Bars,
        _ => crate::app::GraphDrawStyle::Line,
    }
}

fn parse_graph_point_mode(value: Option<&str>) -> crate::app::GraphPointMode {
    match value {
        Some("always") => crate::app::GraphPointMode::Always,
        Some("never") => crate::app::GraphPointMode::Never,
        _ => crate::app::GraphPointMode::Auto,
    }
}

fn parse_graph_axis_placement(value: Option<&str>) -> crate::app::GraphAxisPlacement {
    match value {
        Some("hidden") => crate::app::GraphAxisPlacement::Hidden,
        _ => crate::app::GraphAxisPlacement::Visible,
    }
}

fn parse_graph_stacking_mode(value: Option<&str>) -> crate::app::GraphStackingMode {
    match value {
        Some("normal") => crate::app::GraphStackingMode::Normal,
        Some("percent") => crate::app::GraphStackingMode::Percent,
        _ => crate::app::GraphStackingMode::Off,
    }
}

fn parse_refresh_rate_ms(refresh: &str) -> Option<u64> {
    let refresh = refresh.trim();
    if refresh.is_empty()
        || refresh.eq_ignore_ascii_case("false")
        || refresh.eq_ignore_ascii_case("off")
    {
        return None;
    }
    let duration = humantime::parse_duration(refresh).ok()?;
    u64::try_from(duration.as_millis()).ok()
}
