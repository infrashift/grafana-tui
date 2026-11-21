use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;

use super::model;
use crate::dashboard::{MAX_LAYOUT_DEPTH, MAX_LAYOUT_NODES, MAX_LAYOUT_STEPS};

#[derive(Debug, Deserialize)]
struct RawDashboard {
    title: Option<String>,
    refresh: Option<Value>,
    panels: Option<Vec<RawPanel>>,
    templating: Option<RawTemplating>,
}

#[derive(Debug, Deserialize)]
struct RawTemplating {
    list: Option<Vec<RawVar>>,
}

#[derive(Debug, Deserialize)]
struct RawVar {
    name: String,
    #[serde(rename = "type")]
    var_type: Option<String>,
    query: Option<RawVarQuery>,
    definition: Option<String>,
    regex: Option<String>,
    current: Option<RawVarCurrent>,
    #[serde(rename = "allValue")]
    all_value: Option<String>,
    multi: Option<bool>,
    #[serde(rename = "includeAll")]
    include_all: Option<bool>,
    options: Option<Vec<RawVarOption>>,
    refresh: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawVarOption {
    value: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawVarQuery {
    String(String),
    Object { query: Option<String> },
}

#[derive(Debug, Deserialize)]
struct RawVarCurrent {
    text: Option<Value>,
    value: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawPanel {
    #[serde(rename = "type")]
    panel_type: String,
    title: Option<String>,
    collapsed: Option<bool>,
    targets: Option<Vec<RawTarget>>,
    #[serde(rename = "gridPos")]
    grid_pos: Option<RawGridPos>,
    panels: Option<Vec<RawPanel>>,
    #[serde(rename = "fieldConfig")]
    field_config: Option<RawFieldConfig>,
    options: Option<RawPanelOptions>,
    repeat: Option<String>,
    #[serde(rename = "repeatDirection")]
    repeat_direction: Option<String>,
    #[serde(rename = "maxPerRow")]
    max_per_row: Option<Value>,
    /// Set on copies Grafana generated for a repeated panel.
    #[serde(rename = "repeatPanelId")]
    repeat_panel_id: Option<Value>,
    interval: Option<String>,
    #[serde(rename = "maxDataPoints")]
    max_data_points: Option<Value>,
}

impl RawPanel {
    fn repeat(&self) -> Option<model::Repeat> {
        let variable = self.repeat.as_deref().map(str::trim)?;
        if variable.is_empty() {
            return None;
        }
        let repeat = model::Repeat {
            variable: variable.to_string(),
            direction: match self.repeat_direction.as_deref() {
                Some("v") => model::RepeatDirection::Vertical,
                _ => model::RepeatDirection::Horizontal,
            },
            max_per_row: self
                .max_per_row
                .as_ref()
                .and_then(Value::as_u64)
                .and_then(|count| u16::try_from(count).ok())
                .filter(|count| *count > 0),
        };
        debug_assert_eq!(
            repeat.variable.trim(),
            repeat.variable,
            "the repeat variable is trimmed"
        );
        debug_assert!(
            repeat.max_per_row.is_none_or(|count| count > 0),
            "a max per row value is positive"
        );
        Some(repeat)
    }
}

#[derive(Debug, Deserialize)]
struct RawPanelOptions {
    #[serde(rename = "reduceOptions")]
    reduce_options: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawFieldConfig {
    defaults: Option<RawFieldConfigDefaults>,
}

#[derive(Debug, Deserialize)]
struct RawFieldConfigDefaults {
    unit: Option<String>,
    decimals: Option<usize>,
    #[serde(rename = "noValue")]
    no_value: Option<String>,
    min: Option<f64>,
    max: Option<f64>,
    thresholds: Option<RawThresholds>,
    custom: Option<RawCustom>,
    mappings: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawCustom {
    #[serde(rename = "drawStyle")]
    draw_style: Option<String>,
    #[serde(rename = "showPoints")]
    show_points: Option<String>,
    #[serde(rename = "fillOpacity")]
    fill_opacity: Option<u16>,
    #[serde(rename = "axisPlacement")]
    axis_placement: Option<String>,
    #[serde(rename = "lineInterpolation")]
    line_interpolation: Option<String>,
    stacking: Option<RawStacking>,
    #[serde(rename = "axisGridShow")]
    axis_grid_show: Option<bool>,
    #[serde(rename = "thresholdsStyle")]
    thresholds_style: Option<RawThresholdsStyle>,
}

#[derive(Debug, Deserialize)]
struct RawThresholdsStyle {
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawStacking {
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawThresholds {
    mode: Option<String>,
    steps: Option<Vec<RawThresholdStep>>,
}

#[derive(Debug, Deserialize)]
struct RawThresholdStep {
    value: Option<f64>,
    color: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawTarget {
    expr: Option<String>,
    #[serde(rename = "legendFormat")]
    legend_format: Option<String>,
    instant: Option<bool>,
    hide: Option<bool>,
    interval: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawGridPos {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

pub(super) fn adapt(value: Value) -> Result<model::Dashboard> {
    let raw: RawDashboard =
        serde_json::from_value(value).context("parsing Grafana Classic dashboard JSON")?;
    let mut dashboard = model::Dashboard {
        title: raw.title.unwrap_or_default(),
        refresh: raw
            .refresh
            .and_then(|value| value.as_str().map(str::to_owned)),
        ..model::Dashboard::default()
    };
    let variables = raw
        .templating
        .and_then(|templating| templating.list)
        .unwrap_or_default();
    let count = variables.len();
    dashboard.variables = variables
        .into_iter()
        .enumerate()
        .map(|(index, variable)| variable.normalize(index))
        .collect();
    debug_assert_eq!(
        dashboard.variables.len(),
        count,
        "each variable is normalized"
    );
    dashboard.layout = normalize_layout(raw.panels.unwrap_or_default(), "panels")?;
    debug_assert!(
        dashboard.layout.len() <= MAX_LAYOUT_NODES,
        "the top-level list is in the node limit"
    );
    debug_assert!(
        dashboard.diagnostics.is_empty(),
        "the Classic adapter gives no diagnostics"
    );
    Ok(dashboard)
}

impl RawVar {
    fn query_string(&self) -> Option<String> {
        let query = self
            .query
            .as_ref()
            .and_then(|query| match query {
                RawVarQuery::String(query) => Some(query.as_str()),
                RawVarQuery::Object { query } => query.as_deref(),
            })
            .or(self.definition.as_deref())?;
        let query = query.trim();
        let query = (!query.is_empty()).then(|| query.to_string());
        debug_assert!(
            query.as_deref().is_none_or(|query| !query.is_empty()),
            "a query is not empty"
        );
        debug_assert!(
            query.as_deref().is_none_or(|query| query.trim() == query),
            "a query is trimmed"
        );
        query
    }

    fn normalize(self, index: usize) -> model::Variable {
        let query = self.query_string();
        let mut options: Vec<String> = self
            .options
            .unwrap_or_default()
            .into_iter()
            .filter_map(|option| match option.value? {
                Value::String(value) => Some(value),
                Value::Number(value) => Some(value.to_string()),
                _ => None,
            })
            .filter(|value| value != "$__all")
            .collect();
        let saved = options.len();
        if options.is_empty()
            && self.var_type.as_deref() == Some("custom")
            && let Some(query) = query.as_deref()
        {
            options = crate::app::parse_custom_variable_values(query, false);
        }
        debug_assert!(
            saved == 0 || options.len() == saved,
            "custom values fill only an empty option list"
        );
        let variable = model::Variable {
            multi: self.multi.unwrap_or(false),
            include_all: self.include_all.unwrap_or(false),
            options,
            name: self.name,
            kind: self.var_type,
            current: self.current.map(|current| model::VariableCurrent {
                text: current.text,
                value: current.value,
            }),
            query_path: query
                .is_some()
                .then(|| format!("templating.list[{index}].query")),
            query,
            regex: self.regex,
            all_value: self.all_value,
            refresh: match self.refresh {
                Some(Value::Number(refresh)) => match refresh.as_u64() {
                    Some(0) => Some(model::VariableRefresh::Never),
                    Some(2) => Some(model::VariableRefresh::OnTimeRangeChange),
                    _ => Some(model::VariableRefresh::OnDashboardLoad),
                },
                // Dashboards from before Grafana 5 saved a boolean.
                Some(Value::Bool(false)) => Some(model::VariableRefresh::Never),
                _ => None,
            },
        };
        debug_assert_eq!(
            variable.query.is_some(),
            variable.query_path.is_some(),
            "a query has a path"
        );
        variable
    }
}

/// Normalizes a list of classic panels and the panels saved in its rows with
/// an explicit stack.
fn normalize_layout(panels: Vec<RawPanel>, path: &str) -> Result<Vec<model::LayoutNode>> {
    let mut frames = Vec::with_capacity(MAX_LAYOUT_DEPTH);
    frames.push(ClassicFrame::new(panels, path.to_string(), None));
    let mut nodes = 0usize;
    for _ in 0..MAX_LAYOUT_STEPS {
        let child_level = frames.len().saturating_add(1);
        let Some(frame) = frames.last_mut() else {
            break;
        };
        let Some((source_path, panel)) = frame.next_panel() else {
            if let Some(output) = close_classic_frame(&mut frames) {
                return Ok(output);
            }
            continue;
        };
        nodes = nodes.saturating_add(1);
        ensure!(
            nodes <= MAX_LAYOUT_NODES,
            "Grafana Classic dashboard at {source_path} holds more than the limit of \
             {MAX_LAYOUT_NODES} rows and panels"
        );
        if panel.panel_type != "row" {
            frame.push_panel(panel, source_path);
            continue;
        }
        frame.flush_expanded_row();
        ensure!(
            child_level <= MAX_LAYOUT_DEPTH,
            "Grafana Classic dashboard at {source_path} nests rows deeper than the limit of \
             {MAX_LAYOUT_DEPTH} levels"
        );
        let (row, row_base_y, saved) = classic_row_header(panel, source_path);
        let saved_path = format!("{}.panels", row.source_path);
        frames.push(ClassicFrame::new(
            saved,
            saved_path,
            Some((row, row_base_y)),
        ));
    }
    anyhow::bail!("Grafana Classic dashboard at {path} has more than {MAX_LAYOUT_STEPS} steps")
}

/// Ends the top frame. Gives the output of the top-level list, or `None` when
/// the frame was the saved panels of a row, which goes to the frame below.
fn close_classic_frame(frames: &mut Vec<ClassicFrame>) -> Option<Vec<model::LayoutNode>> {
    let mut frame = frames.pop()?;
    debug_assert!(
        frames.len() < MAX_LAYOUT_DEPTH,
        "the stack depth is in the limit"
    );
    frame.flush_expanded_row();
    let Some((mut row, row_base_y)) = frame.row else {
        debug_assert!(frames.is_empty(), "only the bottom frame has no row");
        return Some(frame.output);
    };
    row.children = frame
        .output
        .into_iter()
        .map(|node| normalize_child_y(node, row_base_y))
        .collect();
    let outer = frames.last_mut()?;
    debug_assert!(
        outer.expanded_row.is_none(),
        "the outer list has no open row"
    );
    if row.collapsed {
        outer.output.push(model::LayoutNode::Row(row));
    } else {
        outer.expanded_row = Some((row, row_base_y));
    }
    None
}

/// A list of classic panels that the normalizer reads, with the row that saved it.
struct ClassicFrame {
    panels: std::iter::Enumerate<std::vec::IntoIter<RawPanel>>,
    path: String,
    output: Vec<model::LayoutNode>,
    /// The open row that takes the panels below it, with its base y.
    expanded_row: Option<(model::Row, i32)>,
    /// The row whose saved panels this list holds, with its base y.
    row: Option<(model::Row, i32)>,
}

impl ClassicFrame {
    /// Starts a list of panels at `path`.
    fn new(panels: Vec<RawPanel>, path: String, row: Option<(model::Row, i32)>) -> Self {
        debug_assert!(!path.is_empty(), "a panel list has a path");
        debug_assert!(
            row.as_ref().is_none_or(|(row, _)| row.children.is_empty()),
            "a new row frame starts with no children"
        );
        Self {
            panels: panels.into_iter().enumerate(),
            path,
            output: Vec::new(),
            expanded_row: None,
            row,
        }
    }

    /// The next panel and its path. The function skips the copies that older
    /// Grafana saved for a repeat, because grafana-tui expands repeats itself.
    fn next_panel(&mut self) -> Option<(String, RawPanel)> {
        let path = &self.path;
        let next = self
            .panels
            .by_ref()
            .find(|(_, panel)| panel.repeat_panel_id.is_none())
            .map(|(index, panel)| (format!("{path}[{index}]"), panel));
        debug_assert!(
            next.as_ref()
                .is_none_or(|(source_path, _)| source_path.starts_with(path.as_str())),
            "a panel path extends the list path"
        );
        next
    }

    /// Adds a panel to the open row, or to the list when no row is open.
    fn push_panel(&mut self, panel: RawPanel, source_path: String) {
        debug_assert_ne!(panel.panel_type, "row", "a row is not a child panel");
        debug_assert!(
            source_path.starts_with(self.path.as_str()),
            "the panel path extends the list path"
        );
        if let Some((row, row_base_y)) = self.expanded_row.as_mut() {
            row.children.push(normalize_classic_panel(
                panel,
                source_path,
                Some(*row_base_y),
            ));
        } else {
            self.output
                .push(normalize_classic_panel(panel, source_path, None));
        }
    }

    /// Closes the open row and adds it to the list.
    fn flush_expanded_row(&mut self) {
        if let Some((row, _)) = self.expanded_row.take() {
            self.output.push(model::LayoutNode::Row(row));
        }
    }
}

/// A classic row with no children yet, its base y, and the panels it saved.
fn classic_row_header(panel: RawPanel, source_path: String) -> (model::Row, i32, Vec<RawPanel>) {
    debug_assert_eq!(panel.panel_type, "row", "the panel is a row");
    debug_assert!(!source_path.is_empty(), "the row has a source path");
    let collapsed = panel.collapsed.unwrap_or(false);
    let row_base_y = panel
        .grid_pos
        .as_ref()
        .map_or(0, |grid| grid.y.saturating_add(grid.h));
    let repeat = panel.repeat();
    let row = model::Row {
        title: panel.title.unwrap_or_default(),
        repeat,
        condition: None,
        variables: Vec::new(),
        collapsed,
        hidden_header: false,
        source_path,
        children: Vec::new(),
    };
    (row, row_base_y, panel.panels.unwrap_or_default())
}

fn normalize_child_y(node: model::LayoutNode, row_base_y: i32) -> model::LayoutNode {
    match node {
        model::LayoutNode::Panel(mut panel) => {
            if let Some(grid) = panel.grid.as_mut() {
                grid.y = grid.y.saturating_sub(row_base_y).max(0);
            }
            debug_assert!(
                panel.grid.is_none_or(|grid| grid.y >= 0),
                "a child y is not negative"
            );
            model::LayoutNode::Panel(panel)
        }
        model::LayoutNode::Row(row) => model::LayoutNode::Row(row),
        model::LayoutNode::Tabs(tabs) => model::LayoutNode::Tabs(tabs),
        model::LayoutNode::AutoGrid(grid) => model::LayoutNode::AutoGrid(grid),
    }
}

fn normalize_classic_panel(
    panel: RawPanel,
    source_path: String,
    row_base_y: Option<i32>,
) -> model::LayoutNode {
    debug_assert_ne!(panel.panel_type, "row", "a row is not a child panel");
    let repeat = panel.repeat();
    let target_count = panel.targets.as_ref().map_or(0, Vec::len);
    let field_defaults = panel.field_config.and_then(|config| {
        config
            .defaults
            .map(|defaults| classic_field_defaults(defaults, &source_path))
    });
    let normalized = model::Panel {
        repeat,
        condition: None,
        kind: panel.panel_type,
        title: panel.title.unwrap_or_default(),
        source_path: source_path.clone(),
        targets: classic_targets(panel.targets.unwrap_or_default(), &source_path),
        count_as_skipped_if_empty: false,
        grid: panel.grid_pos.map(|grid| model::GridPos {
            x: grid.x,
            y: row_base_y
                .map(|base| grid.y.saturating_sub(base).max(0))
                .unwrap_or(grid.y),
            w: grid.w,
            h: grid.h,
        }),
        field_defaults,
        reduce_options_path: panel
            .options
            .and_then(|options| options.reduce_options)
            .is_some_and(|options| model::reduce_options_are_custom(&options))
            .then(|| format!("{source_path}.options.reduceOptions")),
        transformations_path: None,
        min_interval: model::MinInterval::new(panel.interval, format!("{source_path}.interval")),
        max_data_points: model::max_data_points(panel.max_data_points.as_ref()),
    };
    debug_assert_eq!(
        normalized.targets.len(),
        target_count,
        "each target is normalized"
    );
    debug_assert!(
        row_base_y.is_none() || normalized.grid.is_none_or(|grid| grid.y >= 0),
        "a child y is not negative"
    );
    model::LayoutNode::Panel(normalized)
}

/// Converts the field config defaults of a classic panel.
fn classic_field_defaults(
    defaults: RawFieldConfigDefaults,
    source_path: &str,
) -> model::FieldDefaults {
    let steps = defaults
        .thresholds
        .as_ref()
        .and_then(|thresholds| thresholds.steps.as_ref())
        .map_or(0, Vec::len);
    let normalized = model::FieldDefaults {
        unit: defaults.unit,
        decimals: defaults.decimals,
        no_value: defaults.no_value,
        min: defaults.min,
        max: defaults.max,
        thresholds: defaults.thresholds.map(|thresholds| model::Thresholds {
            mode: thresholds.mode,
            steps: thresholds
                .steps
                .unwrap_or_default()
                .into_iter()
                .map(|step| model::ThresholdStep {
                    value: step.value,
                    color: step.color,
                })
                .collect(),
        }),
        custom: defaults.custom.map(|custom| model::GraphCustom {
            draw_style: custom.draw_style,
            show_points: custom.show_points,
            fill_opacity: custom.fill_opacity,
            axis_placement: custom.axis_placement,
            line_interpolation: custom.line_interpolation,
            stacking_mode: custom.stacking.and_then(|stacking| stacking.mode),
            axis_grid_show: custom.axis_grid_show,
            thresholds_style_mode: custom.thresholds_style.and_then(|style| style.mode),
        }),
        mappings_path: defaults
            .mappings
            .as_ref()
            .is_some_and(non_empty_json_value)
            .then(|| format!("{source_path}.fieldConfig.defaults.mappings")),
    };
    debug_assert_eq!(
        normalized
            .thresholds
            .as_ref()
            .map_or(0, |thresholds| thresholds.steps.len()),
        steps,
        "each threshold step is normalized"
    );
    debug_assert!(
        normalized
            .mappings_path
            .as_deref()
            .is_none_or(|path| path.starts_with(source_path)),
        "the mappings path extends the panel path"
    );
    normalized
}

/// Converts the targets of a classic panel, with the path of each field.
fn classic_targets(targets: Vec<RawTarget>, source_path: &str) -> Vec<model::Target> {
    let count = targets.len();
    let normalized: Vec<model::Target> = targets
        .into_iter()
        .enumerate()
        .map(|(index, target)| model::Target {
            expr: target.expr,
            expr_path: format!("{source_path}.targets[{index}].expr"),
            legend_format: target.legend_format,
            instant: target.instant,
            hidden: target.hide == Some(true),
            min_interval: model::MinInterval::new(
                target.interval,
                format!("{source_path}.targets[{index}].interval"),
            ),
        })
        .collect();
    debug_assert_eq!(normalized.len(), count, "each target is normalized");
    debug_assert!(
        normalized
            .iter()
            .all(|target| target.expr_path.starts_with(source_path)),
        "each target path extends the panel path"
    );
    normalized
}

fn non_empty_json_value(value: &Value) -> bool {
    match value {
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        Value::String(value) => !value.trim().is_empty(),
        Value::Null => false,
        Value::Bool(_) | Value::Number(_) => true,
    }
}
