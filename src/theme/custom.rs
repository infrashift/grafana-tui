//! User-defined themes from the `[themes.<name>]` tables of the config file.

use super::{DEFAULT_THEME, Theme, builtin};
use anyhow::{Context, Result, anyhow, bail};
use ratatui::style::Color;
use serde::Deserialize;
use std::collections::BTreeMap;

/// A `[themes.<name>]` table: a built-in theme to start from plus role overrides.
///
/// Each single-color field has the name of the `Theme` field that it overrides.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ThemeSpec {
    /// Built-in theme to start from; the default theme when omitted.
    pub(crate) extends: Option<String>,
    /// Override of the `background` color role.
    pub(crate) background: Option<String>,
    /// Override of the `surface` color role.
    pub(crate) surface: Option<String>,
    /// Override of the `text` color role.
    pub(crate) text: Option<String>,
    /// Override of the `text_muted` color role.
    pub(crate) text_muted: Option<String>,
    /// Override of the `title` color role.
    pub(crate) title: Option<String>,
    /// Override of the `border` color role.
    pub(crate) border: Option<String>,
    /// Override of the `border_focused` color role.
    pub(crate) border_focused: Option<String>,
    /// Override of the `selection_fg` color role.
    pub(crate) selection_fg: Option<String>,
    /// Override of the `selection_bg` color role.
    pub(crate) selection_bg: Option<String>,
    /// Override of the `error` color role.
    pub(crate) error: Option<String>,
    /// Override of the `warning` color role.
    pub(crate) warning: Option<String>,
    /// Override of the `success` color role.
    pub(crate) success: Option<String>,
    /// Override of the `axis` color role.
    pub(crate) axis: Option<String>,
    /// Override of the `grid` color role.
    pub(crate) grid: Option<String>,
    /// Override of the `cursor` color role.
    pub(crate) cursor: Option<String>,
    /// Override of the `gauge_track` color role.
    pub(crate) gauge_track: Option<String>,
    /// Override of the `heatmap_empty` color role.
    pub(crate) heatmap_empty: Option<String>,
    /// Override of the `annotation` color role.
    pub(crate) annotation: Option<String>,
    /// Override of the `threshold_default` color role.
    pub(crate) threshold_default: Option<String>,
    /// Low, mid and high heatmap bands.
    pub(crate) heatmap: Option<Vec<String>>,
    pub(crate) palette: Option<Vec<String>>,
}

/// The number of single-color roles that a `[themes.<name>]` table can override.
const ROLE_COUNT: usize = 19;

/// One single-color role: its TOML key, the configured value and the `Theme` field to set.
type RoleSlot<'a> = (&'static str, Option<&'a String>, &'a mut Color);

impl ThemeSpec {
    /// Pairs each single-color key with its `Theme` field, in declaration order.
    fn role_slots<'a>(&'a self, theme: &'a mut Theme) -> [RoleSlot<'a>; ROLE_COUNT] {
        [
            (
                "background",
                self.background.as_ref(),
                &mut theme.background,
            ),
            ("surface", self.surface.as_ref(), &mut theme.surface),
            ("text", self.text.as_ref(), &mut theme.text),
            (
                "text_muted",
                self.text_muted.as_ref(),
                &mut theme.text_muted,
            ),
            ("title", self.title.as_ref(), &mut theme.title),
            ("border", self.border.as_ref(), &mut theme.border),
            (
                "border_focused",
                self.border_focused.as_ref(),
                &mut theme.border_focused,
            ),
            (
                "selection_fg",
                self.selection_fg.as_ref(),
                &mut theme.selection_fg,
            ),
            (
                "selection_bg",
                self.selection_bg.as_ref(),
                &mut theme.selection_bg,
            ),
            ("error", self.error.as_ref(), &mut theme.error),
            ("warning", self.warning.as_ref(), &mut theme.warning),
            ("success", self.success.as_ref(), &mut theme.success),
            ("axis", self.axis.as_ref(), &mut theme.axis),
            ("grid", self.grid.as_ref(), &mut theme.grid),
            ("cursor", self.cursor.as_ref(), &mut theme.cursor),
            (
                "gauge_track",
                self.gauge_track.as_ref(),
                &mut theme.gauge_track,
            ),
            (
                "heatmap_empty",
                self.heatmap_empty.as_ref(),
                &mut theme.heatmap_empty,
            ),
            (
                "annotation",
                self.annotation.as_ref(),
                &mut theme.annotation,
            ),
            (
                "threshold_default",
                self.threshold_default.as_ref(),
                &mut theme.threshold_default,
            ),
        ]
    }

    /// Parses each configured single-color role into `theme`, and names the key on error.
    fn apply_roles(&self, theme: &mut Theme) -> Result<()> {
        for (role, value, slot) in self.role_slots(theme) {
            if let Some(value) = value {
                *slot = parse_theme_color(value).with_context(|| format!("`{role}`"))?;
            }
        }
        Ok(())
    }
}

/// Builds every configured theme, so a mistake in any of them surfaces at startup.
pub(crate) fn custom_themes(specs: &BTreeMap<String, ThemeSpec>) -> Result<Vec<Theme>> {
    specs
        .iter()
        .map(|(name, spec)| build(name, spec).with_context(|| format!("theme `{name}`")))
        .collect()
}

fn build(name: &str, spec: &ThemeSpec) -> Result<Theme> {
    if name.trim().is_empty() {
        bail!("theme names cannot be empty");
    }
    let extends = spec.extends.as_deref().unwrap_or(DEFAULT_THEME);
    let mut theme =
        builtin(extends).ok_or_else(|| anyhow!("`extends`: unknown built-in theme `{extends}`"))?;
    theme.name = name.to_string();
    spec.apply_roles(&mut theme)?;
    if let Some(bands) = &spec.heatmap {
        let [low, mid, high] = bands.as_slice() else {
            bail!(
                "`heatmap`: expected 3 colors (low, mid, high), got {}",
                bands.len()
            );
        };
        let [low, mid, high] =
            [low, mid, high].map(|band| parse_theme_color(band).context("`heatmap`"));
        theme.heatmap = [low?, mid?, high?];
    }
    if let Some(palette) = &spec.palette {
        if palette.is_empty() {
            bail!("`palette`: needs at least one color");
        }
        theme.palette = palette
            .iter()
            .map(|color| parse_theme_color(color))
            .collect::<Result<_>>()
            .context("`palette`")?;
    }
    Ok(theme)
}

/// Parses `#rrggbb`, an ANSI color name, or `reset` (the terminal's own color).
pub(crate) fn parse_theme_color(value: &str) -> Result<Color> {
    if let Some(hex) = value.strip_prefix('#')
        && hex.len() == 6
        && hex.chars().all(|digit| digit.is_ascii_hexdigit())
    {
        let rgb = u32::from_str_radix(hex, 16).with_context(|| format!("`{value}`"))?;
        return Ok(super::rgb(rgb));
    }
    Ok(match value.to_ascii_lowercase().as_str() {
        "reset" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" | "purple" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "dark-gray" | "dark-grey" => Color::DarkGray,
        "light-red" => Color::LightRed,
        "light-green" => Color::LightGreen,
        "light-yellow" => Color::LightYellow,
        "light-blue" => Color::LightBlue,
        "light-magenta" | "light-purple" => Color::LightMagenta,
        "light-cyan" => Color::LightCyan,
        "white" => Color::White,
        _ => bail!(
            "invalid color `{value}`; expected #rrggbb, an ANSI name such as `light-blue`, or `reset`"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deserializes and builds `[themes.*]` tables the way the config loader does.
    fn load(toml: &str) -> Result<Vec<Theme>> {
        #[derive(Deserialize)]
        struct Wrapper {
            themes: BTreeMap<String, ThemeSpec>,
        }
        custom_themes(&toml::from_str::<Wrapper>(toml)?.themes)
    }

    #[test]
    fn overrides_merge_onto_the_extended_builtin() {
        let themes = load(
            r##"
            [themes.my-dark]
            extends = "catppuccin-latte"
            title = "#010203"
            grid = "dark-gray"
            heatmap = ["blue", "#aabbcc", "reset"]
            palette = ["#ff0000", "light-cyan"]
            "##,
        )
        .unwrap();

        let latte = builtin("catppuccin-latte").unwrap();
        let theme = &themes[0];
        assert_eq!(theme.name, "my-dark");
        assert_eq!(theme.title, Color::Rgb(1, 2, 3));
        assert_eq!(theme.grid, Color::DarkGray);
        assert_eq!(
            theme.heatmap,
            [Color::Blue, Color::Rgb(0xaa, 0xbb, 0xcc), Color::Reset]
        );
        assert_eq!(theme.palette, vec![Color::Rgb(255, 0, 0), Color::LightCyan]);
        assert_eq!(theme.background, latte.background);
        assert_eq!(theme.text, latte.text);
    }

    #[test]
    fn extends_defaults_to_the_default_theme() {
        let themes = load("[themes.plain]\n").unwrap();
        let default = Theme::default();
        assert_eq!(
            themes[0],
            Theme {
                name: "plain".to_string(),
                ..default
            }
        );
    }

    #[test]
    fn mistakes_name_the_theme_and_key() {
        for (toml, expected) in [
            ("[themes.a]\ntitel = \"red\"\n", "unknown field `titel`"),
            (
                "[themes.a]\nextends = \"nope\"\n",
                "theme `a`: `extends`: unknown built-in theme `nope`",
            ),
            (
                "[themes.a]\ntitle = \"#12345\"\n",
                "theme `a`: `title`: invalid color `#12345`",
            ),
            (
                "[themes.a]\ngrid = \"#+12345\"\n",
                "theme `a`: `grid`: invalid color `#+12345`",
            ),
            (
                "[themes.a]\npalette = []\n",
                "theme `a`: `palette`: needs at least one color",
            ),
            (
                "[themes.a]\npalette = [\"red\", \"bogus\"]\n",
                "theme `a`: `palette`: invalid color `bogus`",
            ),
            (
                "[themes.a]\nheatmap = [\"red\"]\n",
                "theme `a`: `heatmap`: expected 3 colors (low, mid, high), got 1",
            ),
        ] {
            let error = format!("{:#}", load(toml).unwrap_err());
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn color_names_are_case_insensitive() {
        assert_eq!(parse_theme_color("Light-Blue").unwrap(), Color::LightBlue);
        assert_eq!(
            parse_theme_color("#ABCDEF").unwrap(),
            Color::Rgb(0xab, 0xcd, 0xef)
        );
        assert_eq!(parse_theme_color("RESET").unwrap(), Color::Reset);
    }
}
