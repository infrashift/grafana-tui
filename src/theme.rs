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

mod builtin;
mod custom;

use anyhow::{Result, bail};
use ratatui::style::Color;

pub(crate) use builtin::{DEFAULT_THEME, builtin, builtin_family, builtin_names};
pub(crate) use custom::{ThemeSpec, custom_themes};

/// Semantic UI colors. Renderers pick a role, never a literal color, so every
/// theme restyles the whole UI and the SVG export.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Theme {
    pub(crate) name: String,
    /// Fill behind the whole frame.
    pub(crate) background: Color,
    /// Fill behind popups and modals.
    pub(crate) surface: Color,
    pub(crate) text: Color,
    /// Hints and other secondary text.
    pub(crate) text_muted: Color,
    pub(crate) title: Color,
    pub(crate) border: Color,
    pub(crate) border_focused: Color,
    pub(crate) selection_fg: Color,
    pub(crate) selection_bg: Color,
    pub(crate) error: Color,
    pub(crate) warning: Color,
    pub(crate) success: Color,
    pub(crate) axis: Color,
    /// Automatic grid lines and labels, unless `autogrid_color` overrides it.
    pub(crate) grid: Color,
    /// Inspect-mode cursor line.
    pub(crate) cursor: Color,
    pub(crate) gauge_track: Color,
    /// Heatmap low, mid and high bands.
    pub(crate) heatmap: [Color; 3],
    /// Heatmap cells without a finite value.
    pub(crate) heatmap_empty: Color,
    pub(crate) annotation: Color,
    /// Threshold steps whose dashboard color has no terminal equivalent.
    pub(crate) threshold_default: Color,
    /// Series colors. Never empty.
    pub(crate) palette: Vec<Color>,
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_base(DEFAULT_THEME, &builtin::TOKYO_NIGHT)
    }
}

/// Every theme that the user can select: the built-ins in display order, then
/// the other user themes. A user theme replaces the built-in of the same name.
pub(crate) fn catalog(custom: &[Theme]) -> Vec<Theme> {
    let find_custom = |name: &str| {
        custom
            .iter()
            .find(|theme| theme.name.eq_ignore_ascii_case(name))
    };
    let mut themes: Vec<Theme> = builtin::builtin_themes()
        .map(|theme| find_custom(&theme.name).cloned().unwrap_or(theme))
        .collect();
    let builtin_count = themes.len();
    debug_assert!(
        builtin_count == builtin_names().count(),
        "the catalog has each built-in theme once"
    );
    for theme in custom {
        if !builtin_names().any(|name| name.eq_ignore_ascii_case(&theme.name)) {
            themes.push(theme.clone());
        }
    }
    debug_assert!(
        themes.len() <= builtin_count.saturating_add(custom.len()),
        "the catalog adds each user theme no more than once"
    );
    debug_assert!(
        themes.iter().all(|theme| !theme.palette.is_empty()),
        "each catalog theme has a palette color"
    );
    themes
}

/// Finds a theme in `catalog` by name or built-in alias. The search ignores case.
pub(crate) fn resolve(name: &str, catalog: &[Theme]) -> Result<Theme> {
    let find = |name: &str| {
        catalog
            .iter()
            .find(|theme| theme.name.eq_ignore_ascii_case(name))
    };
    debug_assert!(!catalog.is_empty(), "the theme catalog is not empty");
    match find(name).or_else(|| builtin::alias_target(name).and_then(find)) {
        Some(theme) => {
            debug_assert!(
                !theme.palette.is_empty(),
                "the resolved theme has a palette color"
            );
            Ok(theme.clone())
        }
        None => bail!(
            "unknown theme `{name}`; available themes: {}",
            catalog
                .iter()
                .map(|theme| theme.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

impl Theme {
    /// Replaces a threshold step color the terminal cannot show.
    pub(crate) fn threshold_color(&self, color: Color) -> Color {
        let shown = if color == Color::Reset {
            self.threshold_default
        } else {
            color
        };
        debug_assert!(
            shown == color || color == Color::Reset,
            "the function replaces only the reset color"
        );
        debug_assert!(
            shown != Color::Reset || self.threshold_default == Color::Reset,
            "a reset color stays only when the theme default is reset"
        );
        shown
    }
}

/// The ANSI theme: follows the terminal's own palette and background.
fn terminal() -> Theme {
    let theme = Theme {
        name: "terminal".to_string(),
        background: Color::Reset,
        surface: Color::Reset,
        text: Color::Reset,
        text_muted: Color::DarkGray,
        title: Color::Cyan,
        border: Color::DarkGray,
        border_focused: Color::Yellow,
        selection_fg: Color::Cyan,
        selection_bg: Color::Reset,
        error: Color::Red,
        warning: Color::Yellow,
        success: Color::Green,
        axis: Color::Gray,
        grid: Color::DarkGray,
        cursor: Color::White,
        gauge_track: Color::DarkGray,
        heatmap: [Color::Cyan, Color::Yellow, Color::Red],
        heatmap_empty: Color::DarkGray,
        annotation: Color::Yellow,
        threshold_default: Color::Reset,
        palette: vec![
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::Red,
            Color::LightGreen,
            Color::LightYellow,
            Color::LightBlue,
            Color::LightMagenta,
            Color::LightCyan,
            Color::LightRed,
        ],
    };
    debug_assert!(
        !theme.palette.is_empty(),
        "the terminal palette has a color"
    );
    debug_assert!(
        has_distinct_colors(&theme.palette),
        "the terminal palette has no duplicate colors"
    );
    theme
}

/// Tells if each color in `palette` is different from all the colors before it.
fn has_distinct_colors(palette: &[Color]) -> bool {
    let first = |color: &Color| palette.iter().position(|other| other == color);
    palette
        .iter()
        .enumerate()
        .all(|(index, color)| first(color) == Some(index))
}

/// The few colors that a palette-based theme starts from.
#[derive(Debug, Clone, Copy)]
struct Base {
    bg: u32,
    /// Popups and modals.
    bg_alt: u32,
    /// Selected rows and gauge tracks.
    highlight: u32,
    /// Grid lines and empty heatmap cells.
    subtle: u32,
    border: u32,
    muted: u32,
    fg: u32,
    title: u32,
    focus: u32,
    red: u32,
    orange: u32,
    yellow: u32,
    green: u32,
    cyan: u32,
    blue: u32,
    purple: u32,
}

const fn rgb(hex: u32) -> Color {
    let [_, r, g, b] = hex.to_be_bytes();
    Color::Rgb(r, g, b)
}

impl Theme {
    /// Returns the palette color at `index`, wrapped to the length of the palette.
    pub(crate) fn palette_color(&self, index: usize) -> Color {
        debug_assert!(
            !self.palette.is_empty(),
            "a theme palette has at least one color"
        );
        let wrapped = index.checked_rem(self.palette.len()).unwrap_or(0);
        debug_assert!(
            wrapped < self.palette.len(),
            "the wrapped index is in the palette"
        );
        self.palette.get(wrapped).copied().unwrap_or(self.text)
    }

    fn from_base(name: &str, base: &Base) -> Self {
        debug_assert!(!name.is_empty(), "a built-in theme has a name");
        let mut palette = Vec::new();
        for hex in [
            base.blue,
            base.green,
            base.yellow,
            base.purple,
            base.cyan,
            base.orange,
            base.red,
        ] {
            let color = rgb(hex);
            if !palette.contains(&color) {
                palette.push(color);
            }
        }
        debug_assert!(
            (1..=7).contains(&palette.len()),
            "the palette has one to seven base colors"
        );
        debug_assert!(
            has_distinct_colors(&palette),
            "the palette has no duplicate colors"
        );
        Self {
            name: name.to_string(),
            background: rgb(base.bg),
            surface: rgb(base.bg_alt),
            text: rgb(base.fg),
            text_muted: rgb(base.muted),
            title: rgb(base.title),
            border: rgb(base.border),
            border_focused: rgb(base.focus),
            selection_fg: rgb(base.title),
            selection_bg: rgb(base.highlight),
            error: rgb(base.red),
            warning: rgb(base.yellow),
            success: rgb(base.green),
            axis: rgb(base.muted),
            grid: rgb(base.subtle),
            cursor: rgb(base.fg),
            gauge_track: rgb(base.highlight),
            heatmap: [rgb(base.blue), rgb(base.yellow), rgb(base.red)],
            heatmap_empty: rgb(base.subtle),
            annotation: rgb(base.focus),
            threshold_default: rgb(base.orange),
            palette,
        }
    }
}

/// Parses the two hex digits at byte `start` of `c`. A bad digit or a split character gives 0.
fn hex_byte(c: &str, start: usize) -> u8 {
    c.get(start..start.saturating_add(2))
        .and_then(|digits| u8::from_str_radix(digits, 16).ok())
        .unwrap_or(0)
}

/// Converts a Grafana color, `#rrggbb` or a named color, to a terminal color.
/// An unknown name gives `Color::Reset`.
pub(crate) fn parse_grafana_color(c: &str) -> Color {
    if c.starts_with('#') && c.len() >= 7 {
        let color = Color::Rgb(hex_byte(c, 1), hex_byte(c, 3), hex_byte(c, 5));
        debug_assert!(
            c.get(1..7)
                .filter(|hex| hex.chars().all(|digit| digit.is_ascii_hexdigit()))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .is_none_or(|value| rgb(value) == color),
            "six hex digits give the color of a direct parse"
        );
        return color;
    }

    match c.to_lowercase().as_str() {
        "green" | "dark-green" => Color::Green,
        "super-light-green" | "light-green" => Color::LightGreen,
        "yellow" | "dark-yellow" => Color::Yellow,
        "super-light-yellow" | "light-yellow" => Color::LightYellow,
        "red" | "dark-red" => Color::Red,
        "super-light-red" | "light-red" => Color::LightRed,
        "blue" | "dark-blue" => Color::Blue,
        "super-light-blue" | "light-blue" => Color::LightBlue,
        "purple" | "dark-purple" => Color::Magenta,
        "super-light-purple" | "light-purple" => Color::LightMagenta,
        "orange" | "dark-orange" => Color::Rgb(255, 165, 0),
        "light-orange" => Color::Rgb(255, 200, 100),
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "dark-gray" | "dark-grey" => Color::DarkGray,
        "white" => Color::White,
        "black" => Color::Black,
        _ => Color::Reset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grafana_hex_colors_with_split_characters_do_not_panic() {
        assert_eq!(parse_grafana_color("#a0b1c2"), Color::Rgb(0xa0, 0xb1, 0xc2));
        assert_eq!(parse_grafana_color("#aé123"), Color::Rgb(0, 0, 0x23));
        assert_eq!(parse_grafana_color("#zz11é"), Color::Rgb(0, 0x11, 0));
    }

    #[test]
    fn palette_colors_wrap_to_the_palette_length() {
        let theme = Theme::default();
        let count = theme.palette.len();
        assert_eq!(theme.palette_color(count + 1), theme.palette[1]);
        assert_eq!(theme.palette_color(0), theme.palette[0]);
    }

    #[test]
    fn every_builtin_resolves_with_a_usable_palette() {
        for name in builtin_names() {
            let theme = builtin(name).unwrap_or_else(|| panic!("{name} should resolve"));
            assert_eq!(theme.name, name);
            assert!(!theme.palette.is_empty(), "{name} palette is empty");
            if name != "terminal" {
                assert_ne!(
                    theme.background,
                    Color::Reset,
                    "{name} paints no background"
                );
                assert_ne!(theme.text, Color::Reset, "{name} has no text color");
            }
        }
    }

    #[test]
    fn lookup_ignores_case() {
        assert_eq!(
            builtin("Dracula").map(|t| t.name),
            Some("dracula".to_string())
        );
    }

    #[test]
    fn unknown_names_are_rejected_with_the_available_names() {
        let error = resolve("nope", &catalog(&[])).unwrap_err().to_string();
        assert!(error.contains("unknown theme `nope`"), "{error}");
        assert!(error.contains("catppuccin-latte"), "{error}");
        assert!(error.contains("gruvbox-light-soft"), "{error}");
    }

    #[test]
    fn aliases_resolve_to_canonical_flavors() {
        for (alias, canonical) in [
            ("default", "tokyo-night"),
            ("tokyo-night-night", "tokyo-night"),
            ("catppuccin", "catppuccin-mocha"),
            ("Catppuccin", "catppuccin-mocha"),
            ("gruvbox", "gruvbox-dark"),
            ("gruvbox-dark-medium", "gruvbox-dark"),
            ("gruvbox-light-medium", "gruvbox-light"),
        ] {
            assert_eq!(
                resolve(alias, &catalog(&[])).unwrap().name,
                canonical,
                "{alias}"
            );
        }
        assert_eq!(Theme::default().name, DEFAULT_THEME);
        assert_eq!(Some(Theme::default()), builtin(DEFAULT_THEME));
    }

    #[test]
    fn user_themes_shadow_builtins_in_place_and_append_the_rest() {
        let shadow = Theme {
            name: "gruvbox-dark".to_string(),
            title: Color::Rgb(1, 2, 3),
            ..builtin("gruvbox-dark").unwrap()
        };
        let extra = Theme {
            name: "Mine".to_string(),
            ..Theme::default()
        };
        let themes = catalog(&[shadow.clone(), extra.clone()]);

        let names: Vec<_> = themes.iter().map(|theme| theme.name.as_str()).collect();
        let builtins: Vec<_> = builtin_names().collect();
        assert_eq!(names[..builtins.len()], builtins[..]);
        assert_eq!(names[builtins.len()..], ["Mine"]);
        assert_eq!(resolve("gruvbox-dark", &themes).unwrap(), shadow);
        // Aliases reach the user's override of their target.
        assert_eq!(resolve("gruvbox", &themes).unwrap(), shadow);
        assert_eq!(resolve("mine", &themes).unwrap(), extra);
        assert!(
            resolve("nope", &themes)
                .unwrap_err()
                .to_string()
                .contains("Mine")
        );
    }

    #[test]
    fn every_builtin_has_a_family() {
        for name in builtin_names() {
            assert!(builtin_family(name).is_some(), "{name}");
        }
        assert_eq!(builtin_family("Catppuccin-Latte"), Some("Catppuccin"));
        assert_eq!(builtin_family("mine"), None);
    }

    #[test]
    fn aliases_point_at_builtins_without_shadowing_them() {
        for (alias, target) in builtin::ALIASES {
            assert!(
                builtin_names().any(|name| name == *target),
                "{alias} -> {target}"
            );
            assert!(
                builtin_names().all(|name| name != *alias),
                "{alias} shadows a builtin"
            );
        }
    }

    #[test]
    fn light_flavors_paint_light_backgrounds() {
        let luma = |color| match color {
            Color::Rgb(r, g, b) => {
                0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b)
            }
            other => panic!("expected rgb, got {other:?}"),
        };
        for name in builtin_names().filter(|name| *name != "terminal") {
            let theme = builtin(name).unwrap();
            let light = ["day", "latte", "light"]
                .iter()
                .any(|flavor| name.contains(flavor));
            assert_eq!(luma(theme.background) > 128.0, light, "{name}");
            // Text must contrast with the background it sits on.
            assert!(
                (luma(theme.text) - luma(theme.background)).abs() > 80.0,
                "{name}"
            );
        }
    }

    #[test]
    fn derived_palettes_skip_duplicate_accents() {
        let base = Base {
            cyan: 0x66d9ef,
            blue: 0x66d9ef,
            ..builtin::TOKYO_NIGHT
        };
        let theme = Theme::from_base("dupes", &base);
        assert_eq!(theme.palette.len(), 6);
        for name in builtin_names() {
            let palette = builtin(name).unwrap().palette;
            for (index, color) in palette.iter().enumerate() {
                assert!(
                    !palette[..index].contains(color),
                    "{name} repeats {color:?}"
                );
            }
        }
    }

    #[test]
    fn threshold_color_replaces_only_reset() {
        let theme = builtin("tokyo-night").unwrap();
        assert_eq!(theme.threshold_color(Color::Reset), theme.threshold_default);
        assert_eq!(theme.threshold_color(Color::Red), Color::Red);
    }
}
