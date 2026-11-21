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

use crate::limits::{MAX_CONFIG_BYTES, MAX_DASHBOARD_FILES, MAX_USER_VARS};
use anyhow::{Context, Result, ensure};
use directories::ProjectDirs;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default, Clone)]
pub(crate) struct Config {
    pub(crate) prometheus_url: Option<String>,
    pub(crate) refresh_rate: Option<u64>,
    pub(crate) time_range: Option<String>,
    pub(crate) step: Option<String>,
    pub(crate) scrape_interval: Option<String>,
    pub(crate) theme: Option<String>,
    pub(crate) transparent_background: Option<bool>,
    /// One dashboard file, or a list of them shown one per tab.
    #[serde(default, alias = "grafana_dashboard", deserialize_with = "one_or_many")]
    pub(crate) grafana_json: Vec<PathBuf>,
    pub(crate) annotations_file: Option<PathBuf>,
    pub(crate) annotations_command: Option<crate::annotations::AnnotationCommandConfig>,
    pub(crate) threshold_marker: Option<String>,
    pub(crate) export_dir: Option<PathBuf>,
    pub(crate) export_format: Option<crate::export::ExportFormat>,
    pub(crate) record_max_frames: Option<usize>,
    pub(crate) autogrid: Option<bool>,
    pub(crate) autogrid_color: Option<String>,
    pub(crate) vars: Option<HashMap<String, String>>,
    /// TLS files for the Prometheus connection.
    #[serde(default)]
    pub(crate) tls: crate::prom::TlsFiles,
    /// User-defined themes, keyed by name.
    #[serde(default)]
    pub(crate) themes: BTreeMap<String, crate::theme::ThemeSpec>,
}

impl Config {
    pub(crate) fn load(cli_path: Option<PathBuf>) -> Result<Self> {
        let config_path = cli_path
            .map(|p| expand_path(&p))
            .or_else(Self::get_config_path);
        if let Some(path) = config_path
            && path.exists()
        {
            let content = crate::limits::read_text_capped(&path, MAX_CONFIG_BYTES)
                .with_context(|| format!("reading config file: {}", path.display()))?;
            debug_assert!(
                u64::try_from(content.len()).is_ok_and(|len| len <= MAX_CONFIG_BYTES),
                "the config text is not larger than its limit"
            );
            let config: Config = toml::from_str(&content)?;
            // The command line can replace the dashboard files of the config,
            // but a config over a limit is not valid.
            let files = config.grafana_json.len();
            ensure!(
                files <= MAX_DASHBOARD_FILES,
                "{files} dashboard files are more than the limit of {MAX_DASHBOARD_FILES}"
            );
            let vars = config.vars.as_ref().map_or(0, HashMap::len);
            ensure!(
                vars <= MAX_USER_VARS,
                "{vars} config vars are more than the limit of {MAX_USER_VARS}"
            );
            return Ok(config);
        }
        Ok(Config::default())
    }

    /// Returns the first config file that exists: `config.toml` or
    /// `grafana-tui.toml` in the config directory, then `./grafana-tui.toml`.
    fn get_config_path() -> Option<PathBuf> {
        let mut candidates = Vec::with_capacity(3);
        if let Some(base_dirs) = ProjectDirs::from("", "", "grafana-tui") {
            candidates.push(base_dirs.config_dir().join("config.toml"));
            candidates.push(base_dirs.config_dir().join("grafana-tui.toml"));
        }
        candidates.push(PathBuf::from("./grafana-tui.toml"));
        debug_assert!(
            (1..=3).contains(&candidates.len()),
            "the search has one to three candidate paths"
        );
        let found = candidates.into_iter().find(|path| path.exists());
        debug_assert!(
            found
                .as_ref()
                .is_none_or(|path| path.extension().is_some_and(|ext| ext == "toml")),
            "a config path has the toml extension"
        );
        found
    }
}

/// Reads a path, or a list of paths, as a list.
fn one_or_many<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<PathBuf>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(PathBuf),
        Many(Vec<PathBuf>),
    }
    let (paths, single) = match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(path) => (vec![path], true),
        OneOrMany::Many(paths) => (paths, false),
    };
    debug_assert!(
        !single || paths.len() == 1,
        "one path in the config gives a list of one"
    );
    Ok(paths)
}

/// Expands a path that starts with `~` to the user's home directory.
pub(crate) fn expand_path(path: &std::path::Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str.starts_with("~")
        && let Some(dirs) = directories::UserDirs::new()
    {
        let home = dirs.home_dir();
        debug_assert!(
            !home.as_os_str().is_empty(),
            "the home directory has a name"
        );
        if path_str == "~" {
            return home.to_path_buf();
        }
        if let Some(rest) = path_str
            .strip_prefix("~/")
            .or_else(|| path_str.strip_prefix("~\\"))
        {
            let expanded = home.join(rest);
            debug_assert!(
                expanded.as_os_str().len() >= rest.len(),
                "the expanded path keeps the rest of the input path"
            );
            return expanded;
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_config_file_over_its_limit_names_the_file_and_the_limit() {
        let limit = crate::limits::MAX_CONFIG_BYTES;
        let path = crate::limits::tests::sparse_file("big.toml", limit + 1);
        let error = Config::load(Some(path.clone())).unwrap_err();
        assert_eq!(
            format!("{error:#}"),
            format!(
                "reading config file: {}: the file is larger than the limit of {limit} bytes",
                path.display()
            )
        );
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_config_deserializes_theme_tables() {
        let config: Config = toml::from_str(
            r##"
            theme = "mine"

            [themes.mine]
            extends = "catppuccin-latte"
            title = "#ff9e64"
            palette = ["#7aa2f7", "green"]
            "##,
        )
        .unwrap();

        let spec = &config.themes["mine"];
        assert_eq!(spec.extends.as_deref(), Some("catppuccin-latte"));
        assert_eq!(spec.title.as_deref(), Some("#ff9e64"));
        assert_eq!(spec.palette.as_ref().map(Vec::len), Some(2));
        assert!(Config::default().themes.is_empty());
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
            prometheus_url = "http://localhost:9090"
            refresh_rate = 5000
            theme = "dracula"
            transparent_background = true
            export_format = "svg"
            autogrid = false
            autogrid_color = "gray"
            annotations_file = "~/events.jsonl"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.prometheus_url,
            Some("http://localhost:9090".to_string())
        );
        assert_eq!(config.refresh_rate, Some(5000));
        assert_eq!(config.theme, Some("dracula".to_string()));
        assert_eq!(config.transparent_background, Some(true));
        assert_eq!(config.export_format, Some(crate::export::ExportFormat::Svg));
        assert_eq!(config.autogrid, Some(false));
        assert_eq!(config.autogrid_color, Some("gray".to_string()));
        assert_eq!(
            config.annotations_file,
            Some(PathBuf::from("~/events.jsonl"))
        );
    }

    #[test]
    fn parses_the_tls_table() {
        let config: Config = toml::from_str(
            r#"
            prometheus_url = "https://10.60.1.21:9090"

            [tls]
            ca_cert = "~/certs/ca.pem"
            client_cert = "/etc/grafana-tui/client.pem"
            client_key = "/etc/grafana-tui/client.key"
            "#,
        )
        .unwrap();

        assert_eq!(config.tls.ca_cert, Some(PathBuf::from("~/certs/ca.pem")));
        assert_eq!(
            config.tls.client_cert,
            Some(PathBuf::from("/etc/grafana-tui/client.pem"))
        );
        assert_eq!(
            config.tls.client_key,
            Some(PathBuf::from("/etc/grafana-tui/client.key"))
        );
        assert!(Config::default().tls.is_empty());
    }

    #[test]
    fn misspelled_tls_options_are_errors() {
        let error = toml::from_str::<Config>("[tls]\nca_file = \"ca.pem\"\n")
            .unwrap_err()
            .to_string();

        assert!(error.contains("unknown field `ca_file`"), "{error}");
        assert!(error.contains("ca_cert"), "{error}");
    }

    #[test]
    fn parses_annotation_command_table_and_defaults() {
        let config: Config = toml::from_str(
            r#"
            [annotations_command]
            program = "./provider"
            args = ["--environment", "prod"]
            "#,
        )
        .unwrap();

        let command = config.annotations_command.unwrap();
        assert_eq!(command.program, "./provider");
        assert_eq!(command.args, ["--environment", "prod"]);
        assert_eq!(command.timeout, Duration::from_secs(10));
    }

    #[test]
    fn parses_annotation_command_human_timeout() {
        let config: Config = toml::from_str(
            r#"
            [annotations_command]
            program = "./provider"
            timeout = "750ms"
            "#,
        )
        .unwrap();

        assert_eq!(
            config.annotations_command.unwrap().timeout,
            Duration::from_millis(750)
        );
    }

    #[test]
    fn grafana_dashboard_is_an_alias_for_grafana_json() {
        let config: Config = toml::from_str(r#"grafana_dashboard = "dash.yaml""#).unwrap();

        assert_eq!(config.grafana_json, [PathBuf::from("dash.yaml")]);
    }

    #[test]
    fn grafana_json_takes_one_file_or_a_list() {
        let config: Config =
            toml::from_str(r#"grafana_json = ["nodes.json", "consul.yaml"]"#).unwrap();
        assert_eq!(
            config.grafana_json,
            [PathBuf::from("nodes.json"), PathBuf::from("consul.yaml")]
        );

        let config: Config = toml::from_str("").unwrap();
        assert!(config.grafana_json.is_empty());
    }

    #[test]
    fn test_expand_path() {
        if let Some(dirs) = directories::UserDirs::new() {
            let home = dirs.home_dir();

            // Test ~
            let p = PathBuf::from("~");
            assert_eq!(expand_path(&p), home);

            // Test ~/foo
            let p = PathBuf::from("~/foo/bar.json");
            assert_eq!(expand_path(&p), home.join("foo/bar.json"));

            // Test path without ~
            let p = PathBuf::from("./local.json");
            assert_eq!(expand_path(&p), p);

            // Test /absolute
            let p = PathBuf::from("/etc/config");
            assert_eq!(expand_path(&p), p);
        }
    }
}
