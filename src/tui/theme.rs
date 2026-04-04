use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::session::types::SessionStatus;

// ---------------------------------------------------------------------------
// Color capability detection
// ---------------------------------------------------------------------------

/// Terminal color capability level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorCapability {
    Basic,
    #[default]
    Extended,
    TrueColor,
}

impl ColorCapability {
    /// Detect terminal color capability from environment variables.
    pub fn detect() -> Self {
        Self::detect_from_env(|k| std::env::var(k).ok())
    }

    /// Testable detection: accepts an env-var reader closure.
    pub(crate) fn detect_from_env(get_env: impl Fn(&str) -> Option<String>) -> Self {
        let colorterm = get_env("COLORTERM").unwrap_or_default().to_lowercase();
        if colorterm == "truecolor" || colorterm == "24bit" {
            return Self::TrueColor;
        }
        let term = get_env("TERM").unwrap_or_default();
        if term.contains("256color") {
            return Self::Extended;
        }
        Self::Basic
    }

    /// Downgrade a color to fit within this capability level.
    pub fn downgrade(&self, color: Color) -> Color {
        match self {
            Self::TrueColor => color,
            Self::Extended => match color {
                Color::Rgb(r, g, b) => Color::Indexed(rgb_to_ansi256(r, g, b)),
                _ => color,
            },
            Self::Basic => match color {
                Color::Rgb(r, g, b) => rgb_to_basic(r, g, b),
                Color::Indexed(idx) => indexed_to_basic(idx),
                _ => color,
            },
        }
    }
}

/// Map an RGB value to the nearest ANSI 256-color index.
fn rgb_to_ansi256(r: u8, g: u8, b: u8) -> u8 {
    // Use the 6x6x6 color cube (indices 16-231)
    let ri = ((r as u16) * 5 / 255) as u8;
    let gi = ((g as u16) * 5 / 255) as u8;
    let bi = ((b as u16) * 5 / 255) as u8;
    16 + 36 * ri + 6 * gi + bi
}

/// Map an RGB value to the nearest basic ANSI color.
fn rgb_to_basic(r: u8, g: u8, b: u8) -> Color {
    // Simple luminance-based mapping to basic colors
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);

    if max < 48 {
        return Color::Black;
    }
    if min > 200 {
        return Color::White;
    }

    // Determine dominant channel
    if r > g && r > b {
        if r > 128 { Color::Red } else { Color::DarkGray }
    } else if g > r && g > b {
        if g > 128 {
            Color::Green
        } else {
            Color::DarkGray
        }
    } else if b > r && b > g {
        if b > 128 {
            Color::Blue
        } else {
            Color::DarkGray
        }
    } else if r > 128 && g > 128 {
        Color::Yellow
    } else if r > 128 && b > 128 {
        Color::Magenta
    } else if g > 128 && b > 128 {
        Color::Cyan
    } else {
        Color::Gray
    }
}

/// Map a 256-color index to the nearest basic ANSI color.
fn indexed_to_basic(idx: u8) -> Color {
    match idx {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::Gray,
        8 => Color::DarkGray,
        9 => Color::LightRed,
        10 => Color::LightGreen,
        11 => Color::LightYellow,
        12 => Color::LightBlue,
        13 => Color::LightMagenta,
        14 => Color::LightCyan,
        15 => Color::White,
        // For 16-231 (6x6x6 cube) and 232-255 (grayscale), approximate
        16..=231 => {
            let idx = idx - 16;
            let r = (idx / 36) * 51;
            let g = ((idx % 36) / 6) * 51;
            let b = (idx % 6) * 51;
            rgb_to_basic(r, g, b)
        }
        232..=255 => {
            let gray = 8 + (idx - 232) * 10;
            if gray < 64 {
                Color::Black
            } else if gray < 128 {
                Color::DarkGray
            } else if gray < 192 {
                Color::Gray
            } else {
                Color::White
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SerializableColor — ratatui Color with serde support
// ---------------------------------------------------------------------------

/// A color value that can be serialized to/from TOML.
/// Accepts: named colors ("red"), hex ("#ff0000"), or 256-color index (34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerializableColor(pub Color);

impl From<SerializableColor> for Color {
    fn from(sc: SerializableColor) -> Color {
        sc.0
    }
}

impl Serialize for SerializableColor {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Color::Rgb(r, g, b) => {
                serializer.serialize_str(&format!("#{:02x}{:02x}{:02x}", r, g, b))
            }
            Color::Indexed(idx) => serializer.serialize_u8(idx),
            other => serializer.serialize_str(color_to_name(other)),
        }
    }
}

impl<'de> Deserialize<'de> for SerializableColor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ColorVisitor;

        impl<'de> serde::de::Visitor<'de> for ColorVisitor {
            type Value = SerializableColor;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "a color name (\"red\"), hex string (\"#ff0000\"), or 256-color index (0-255)",
                )
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                if let Some(hex) = v.strip_prefix('#') {
                    if hex.len() != 6 {
                        return Err(E::custom(format!("invalid hex color: {}", v)));
                    }
                    let r = u8::from_str_radix(&hex[0..2], 16)
                        .map_err(|_| E::custom(format!("invalid hex color: {}", v)))?;
                    let g = u8::from_str_radix(&hex[2..4], 16)
                        .map_err(|_| E::custom(format!("invalid hex color: {}", v)))?;
                    let b = u8::from_str_radix(&hex[4..6], 16)
                        .map_err(|_| E::custom(format!("invalid hex color: {}", v)))?;
                    return Ok(SerializableColor(Color::Rgb(r, g, b)));
                }
                name_to_color(v)
                    .map(SerializableColor)
                    .ok_or_else(|| E::custom(format!("unknown color name: {}", v)))
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                if v > 255 {
                    return Err(E::custom(format!("color index out of range: {}", v)));
                }
                Ok(SerializableColor(Color::Indexed(v as u8)))
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                if !(0..=255).contains(&v) {
                    return Err(E::custom(format!("color index out of range: {}", v)));
                }
                Ok(SerializableColor(Color::Indexed(v as u8)))
            }
        }

        deserializer.deserialize_any(ColorVisitor)
    }
}

fn name_to_color(name: &str) -> Option<Color> {
    let normalized: String = name
        .chars()
        .filter_map(|c| {
            if c == '_' || c == '-' {
                None
            } else {
                Some(c.to_ascii_lowercase())
            }
        })
        .collect();
    match normalized.as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "lightred" => Some(Color::LightRed),
        "lightgreen" => Some(Color::LightGreen),
        "lightyellow" => Some(Color::LightYellow),
        "lightblue" => Some(Color::LightBlue),
        "lightmagenta" => Some(Color::LightMagenta),
        "lightcyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        _ => None,
    }
}

fn color_to_name(color: Color) -> &'static str {
    match color {
        Color::Black => "black",
        Color::Red => "red",
        Color::Green => "green",
        Color::Yellow => "yellow",
        Color::Blue => "blue",
        Color::Magenta => "magenta",
        Color::Cyan => "cyan",
        Color::Gray => "gray",
        Color::DarkGray => "darkgray",
        Color::LightRed => "lightred",
        Color::LightGreen => "lightgreen",
        Color::LightYellow => "lightyellow",
        Color::LightBlue => "lightblue",
        Color::LightMagenta => "lightmagenta",
        Color::LightCyan => "lightcyan",
        Color::White => "white",
        _ => "white", // fallback
    }
}

// ---------------------------------------------------------------------------
// Theme presets and configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreset {
    #[default]
    Dark,
    Light,
}

/// Top-level theme configuration, embeddable in maestro.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    #[serde(default)]
    pub preset: ThemePreset,
    #[serde(default)]
    pub overrides: ThemeOverrides,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            preset: ThemePreset::Dark,
            overrides: ThemeOverrides::default(),
        }
    }
}

/// Optional per-field color overrides. Applied on top of the preset.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeOverrides {
    pub branding_fg: Option<SerializableColor>,
    pub branding_bg: Option<SerializableColor>,
    pub text_primary: Option<SerializableColor>,
    pub text_secondary: Option<SerializableColor>,
    pub text_muted: Option<SerializableColor>,
    pub border_active: Option<SerializableColor>,
    pub border_inactive: Option<SerializableColor>,
    pub border_focused: Option<SerializableColor>,
    pub accent_success: Option<SerializableColor>,
    pub accent_warning: Option<SerializableColor>,
    pub accent_error: Option<SerializableColor>,
    pub accent_info: Option<SerializableColor>,
    pub accent_identifier: Option<SerializableColor>,
    pub gauge_low: Option<SerializableColor>,
    pub gauge_medium: Option<SerializableColor>,
    pub gauge_high: Option<SerializableColor>,
    pub gauge_background: Option<SerializableColor>,
    pub notification_critical: Option<SerializableColor>,
    pub notification_blocker: Option<SerializableColor>,
    pub notification_default: Option<SerializableColor>,
    pub keybind_key: Option<SerializableColor>,
    pub keybind_label_bg: Option<SerializableColor>,
    pub keybind_label_fg: Option<SerializableColor>,
}

// ---------------------------------------------------------------------------
// Resolved Theme (runtime-ready, all fields concrete)
// ---------------------------------------------------------------------------

/// The resolved, runtime-ready theme. All fields are concrete `Color` values.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    // Branding
    pub branding_fg: Color,
    pub branding_bg: Color,

    // Text hierarchy
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,

    // Borders
    pub border_active: Color,
    pub border_inactive: Color,
    pub border_focused: Color,

    // Status colors (mapped via status_color())
    pub status_running: Color,
    pub status_completed: Color,
    pub status_errored: Color,
    pub status_paused: Color,
    pub status_killed: Color,
    pub status_queued: Color,
    pub status_spawning: Color,
    pub status_stalled: Color,
    pub status_retrying: Color,
    pub status_gates_running: Color,
    pub status_needs_review: Color,
    pub status_ci_fix: Color,

    // Semantic accents
    pub accent_success: Color,
    pub accent_warning: Color,
    pub accent_error: Color,
    pub accent_info: Color,
    pub accent_identifier: Color,

    // Gauge thresholds
    pub gauge_low: Color,
    pub gauge_medium: Color,
    pub gauge_high: Color,
    pub gauge_background: Color,

    // Notification severity
    pub notification_critical: Color,
    pub notification_blocker: Color,
    pub notification_default: Color,

    // Help/keybinds
    pub keybind_key: Color,
    pub keybind_label_bg: Color,
    pub keybind_label_fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Dark theme preset — matches the current hardcoded colors.
    pub fn dark() -> Self {
        Self {
            branding_fg: Color::Black,
            branding_bg: Color::Green,
            text_primary: Color::White,
            text_secondary: Color::DarkGray,
            text_muted: Color::DarkGray,
            border_active: Color::Green,
            border_inactive: Color::DarkGray,
            border_focused: Color::White,
            status_running: Color::Green,
            status_completed: Color::Blue,
            status_errored: Color::Red,
            status_paused: Color::Yellow,
            status_killed: Color::Red,
            status_queued: Color::DarkGray,
            status_spawning: Color::Cyan,
            status_stalled: Color::Yellow,
            status_retrying: Color::Magenta,
            status_gates_running: Color::Cyan,
            status_needs_review: Color::LightYellow,
            status_ci_fix: Color::LightMagenta,
            accent_success: Color::Green,
            accent_warning: Color::Yellow,
            accent_error: Color::Red,
            accent_info: Color::Cyan,
            accent_identifier: Color::Cyan,
            gauge_low: Color::Green,
            gauge_medium: Color::Yellow,
            gauge_high: Color::Red,
            gauge_background: Color::DarkGray,
            notification_critical: Color::Red,
            notification_blocker: Color::LightRed,
            notification_default: Color::Yellow,
            keybind_key: Color::Yellow,
            keybind_label_bg: Color::DarkGray,
            keybind_label_fg: Color::Black,
        }
    }

    /// Light theme preset.
    pub fn light() -> Self {
        Self {
            branding_fg: Color::White,
            branding_bg: Color::Blue,
            text_primary: Color::Black,
            text_secondary: Color::DarkGray,
            text_muted: Color::Gray,
            border_active: Color::Blue,
            border_inactive: Color::Gray,
            border_focused: Color::Black,
            status_running: Color::Blue,
            status_completed: Color::Green,
            status_errored: Color::Red,
            status_paused: Color::Yellow,
            status_killed: Color::Red,
            status_queued: Color::Gray,
            status_spawning: Color::Cyan,
            status_stalled: Color::Yellow,
            status_retrying: Color::Magenta,
            status_gates_running: Color::Cyan,
            status_needs_review: Color::Yellow,
            status_ci_fix: Color::Magenta,
            accent_success: Color::Green,
            accent_warning: Color::Yellow,
            accent_error: Color::Red,
            accent_info: Color::Blue,
            accent_identifier: Color::Blue,
            gauge_low: Color::Green,
            gauge_medium: Color::Yellow,
            gauge_high: Color::Red,
            gauge_background: Color::Gray,
            notification_critical: Color::Red,
            notification_blocker: Color::LightRed,
            notification_default: Color::Yellow,
            keybind_key: Color::Blue,
            keybind_label_bg: Color::Gray,
            keybind_label_fg: Color::White,
        }
    }

    /// Build a theme from config, applying overrides on top of the preset.
    pub fn from_config(config: &ThemeConfig) -> Self {
        let mut theme = match config.preset {
            ThemePreset::Dark => Self::dark(),
            ThemePreset::Light => Self::light(),
        };

        macro_rules! apply_override {
            ($field:ident) => {
                if let Some(c) = config.overrides.$field {
                    theme.$field = c.into();
                }
            };
        }

        apply_override!(branding_fg);
        apply_override!(branding_bg);
        apply_override!(text_primary);
        apply_override!(text_secondary);
        apply_override!(text_muted);
        apply_override!(border_active);
        apply_override!(border_inactive);
        apply_override!(border_focused);
        apply_override!(accent_success);
        apply_override!(accent_warning);
        apply_override!(accent_error);
        apply_override!(accent_info);
        apply_override!(accent_identifier);
        apply_override!(gauge_low);
        apply_override!(gauge_medium);
        apply_override!(gauge_high);
        apply_override!(gauge_background);
        apply_override!(notification_critical);
        apply_override!(notification_blocker);
        apply_override!(notification_default);
        apply_override!(keybind_key);
        apply_override!(keybind_label_bg);
        apply_override!(keybind_label_fg);

        theme
    }

    /// Downgrade all colors to fit within the detected terminal capability.
    pub fn apply_capability(&mut self, cap: ColorCapability) {
        macro_rules! downgrade {
            ($($field:ident),+ $(,)?) => {
                $(self.$field = cap.downgrade(self.$field);)+
            };
        }
        downgrade!(
            branding_fg,
            branding_bg,
            text_primary,
            text_secondary,
            text_muted,
            border_active,
            border_inactive,
            border_focused,
            status_running,
            status_completed,
            status_errored,
            status_paused,
            status_killed,
            status_queued,
            status_spawning,
            status_stalled,
            status_retrying,
            status_gates_running,
            status_needs_review,
            status_ci_fix,
            accent_success,
            accent_warning,
            accent_error,
            accent_info,
            accent_identifier,
            gauge_low,
            gauge_medium,
            gauge_high,
            gauge_background,
            notification_critical,
            notification_blocker,
            notification_default,
            keybind_key,
            keybind_label_bg,
            keybind_label_fg,
        );
    }

    /// Map a session status to its themed color.
    pub fn status_color(&self, status: SessionStatus) -> Color {
        match status {
            SessionStatus::Running => self.status_running,
            SessionStatus::Completed => self.status_completed,
            SessionStatus::Errored => self.status_errored,
            SessionStatus::Paused => self.status_paused,
            SessionStatus::Killed => self.status_killed,
            SessionStatus::Queued => self.status_queued,
            SessionStatus::Spawning => self.status_spawning,
            SessionStatus::Stalled => self.status_stalled,
            SessionStatus::Retrying => self.status_retrying,
            SessionStatus::GatesRunning => self.status_gates_running,
            SessionStatus::NeedsReview => self.status_needs_review,
            SessionStatus::CiFix => self.status_ci_fix,
        }
    }

    /// Gauge color by percentage (0.0 - 1.0 scale, where values are pre-multiplied by 100).
    /// Matches the existing threshold logic in panels.rs.
    pub fn gauge_color(&self, pct: f64) -> Color {
        if pct > 70.0 {
            self.gauge_high
        } else if pct > 40.0 {
            self.gauge_medium
        } else {
            self.gauge_low
        }
    }

    /// Budget color by percentage (0-100 u8).
    /// Matches the existing logic: >= 90 is error, otherwise warning.
    pub fn budget_color(&self, pct: u8) -> Color {
        if pct >= 90 {
            self.accent_error
        } else {
            self.accent_warning
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    use crate::session::types::SessionStatus;

    // --- Theme Preset Constructors ---

    #[test]
    fn dark_preset_produces_non_default_colors() {
        let t = Theme::dark();
        assert_ne!(t.text_primary, Color::Reset);
        assert_ne!(t.border_active, Color::Reset);
        assert_ne!(t.status_running, Color::Reset);
    }

    #[test]
    fn light_preset_produces_different_colors_from_dark() {
        let dark = Theme::dark();
        let light = Theme::light();
        assert!(
            dark.text_primary != light.text_primary
                || dark.border_active != light.border_active
                || dark.status_running != light.status_running,
            "dark and light presets must differ in at least one field"
        );
    }

    #[test]
    fn dark_preset_is_default() {
        let default_theme = Theme::default();
        let dark = Theme::dark();
        assert_eq!(default_theme.text_primary, dark.text_primary);
        assert_eq!(default_theme.border_active, dark.border_active);
        assert_eq!(default_theme.status_running, dark.status_running);
        assert_eq!(default_theme.status_errored, dark.status_errored);
        assert_eq!(default_theme.gauge_low, dark.gauge_low);
    }

    // --- Theme::from_config ---

    #[test]
    fn from_config_dark_preset_no_overrides_equals_dark() {
        let cfg = ThemeConfig {
            preset: ThemePreset::Dark,
            overrides: ThemeOverrides::default(),
        };
        let t = Theme::from_config(&cfg);
        let dark = Theme::dark();
        assert_eq!(t.text_primary, dark.text_primary);
        assert_eq!(t.border_active, dark.border_active);
        assert_eq!(t.status_running, dark.status_running);
        assert_eq!(t.gauge_low, dark.gauge_low);
        assert_eq!(t.accent_success, dark.accent_success);
    }

    #[test]
    fn from_config_light_preset_no_overrides_equals_light() {
        let cfg = ThemeConfig {
            preset: ThemePreset::Light,
            overrides: ThemeOverrides::default(),
        };
        let t = Theme::from_config(&cfg);
        let light = Theme::light();
        assert_eq!(t.text_primary, light.text_primary);
        assert_eq!(t.border_active, light.border_active);
        assert_eq!(t.status_running, light.status_running);
    }

    #[test]
    fn from_config_override_replaces_single_field() {
        let mut overrides = ThemeOverrides::default();
        overrides.text_primary = Some(SerializableColor(Color::Magenta));
        let cfg = ThemeConfig {
            preset: ThemePreset::Dark,
            overrides,
        };
        let t = Theme::from_config(&cfg);
        assert_eq!(t.text_primary, Color::Magenta);
        assert_eq!(t.border_active, Theme::dark().border_active);
    }

    #[test]
    fn from_config_multiple_overrides_all_applied() {
        let mut overrides = ThemeOverrides::default();
        overrides.text_primary = Some(SerializableColor(Color::Magenta));
        overrides.border_active = Some(SerializableColor(Color::LightBlue));
        let cfg = ThemeConfig {
            preset: ThemePreset::Dark,
            overrides,
        };
        let t = Theme::from_config(&cfg);
        assert_eq!(t.text_primary, Color::Magenta);
        assert_eq!(t.border_active, Color::LightBlue);
        assert_eq!(t.status_running, Theme::dark().status_running);
    }

    // --- Theme::status_color ---

    #[test]
    fn status_color_running_returns_green() {
        assert_eq!(
            Theme::dark().status_color(SessionStatus::Running),
            Color::Green
        );
    }

    #[test]
    fn status_color_completed_returns_blue() {
        assert_eq!(
            Theme::dark().status_color(SessionStatus::Completed),
            Color::Blue
        );
    }

    #[test]
    fn status_color_errored_returns_red() {
        assert_eq!(
            Theme::dark().status_color(SessionStatus::Errored),
            Color::Red
        );
    }

    #[test]
    fn status_color_killed_returns_red() {
        assert_eq!(
            Theme::dark().status_color(SessionStatus::Killed),
            Color::Red
        );
    }

    #[test]
    fn status_color_covers_all_variants() {
        let t = Theme::dark();
        let all_variants = [
            SessionStatus::Queued,
            SessionStatus::Spawning,
            SessionStatus::Running,
            SessionStatus::Completed,
            SessionStatus::GatesRunning,
            SessionStatus::NeedsReview,
            SessionStatus::Errored,
            SessionStatus::Paused,
            SessionStatus::Killed,
            SessionStatus::Stalled,
            SessionStatus::Retrying,
            SessionStatus::CiFix,
        ];
        for variant in all_variants {
            let color = t.status_color(variant);
            assert_ne!(
                color,
                Color::Reset,
                "status_color({:?}) returned Color::Reset",
                variant
            );
        }
    }

    // --- Theme::gauge_color ---

    #[test]
    fn gauge_color_low_value_returns_green() {
        assert_eq!(Theme::dark().gauge_color(10.0), Color::Green);
    }

    #[test]
    fn gauge_color_mid_value_returns_yellow() {
        assert_eq!(Theme::dark().gauge_color(60.0), Color::Yellow);
    }

    #[test]
    fn gauge_color_high_value_returns_red() {
        assert_eq!(Theme::dark().gauge_color(90.0), Color::Red);
    }

    #[test]
    fn gauge_color_boundary_at_zero_does_not_panic() {
        let c = Theme::dark().gauge_color(0.0);
        assert_ne!(c, Color::Reset);
    }

    #[test]
    fn gauge_color_boundary_at_hundred_does_not_panic() {
        let c = Theme::dark().gauge_color(100.0);
        assert_ne!(c, Color::Reset);
    }

    #[test]
    fn gauge_color_exact_lower_threshold_is_deterministic() {
        let at_40 = Theme::dark().gauge_color(40.0);
        let at_40_1 = Theme::dark().gauge_color(40.1);
        // 40.0 is <= 40, so it's low tier (Green). 40.1 is > 40, so medium (Yellow).
        assert_eq!(at_40, Color::Green);
        assert_eq!(at_40_1, Color::Yellow);
    }

    // --- Theme::budget_color ---

    #[test]
    fn budget_color_below_threshold_returns_warning() {
        assert_eq!(Theme::dark().budget_color(75), Color::Yellow);
    }

    #[test]
    fn budget_color_at_threshold_returns_error() {
        assert_eq!(Theme::dark().budget_color(90), Color::Red);
    }

    #[test]
    fn budget_color_above_threshold_returns_error() {
        assert_eq!(Theme::dark().budget_color(95), Color::Red);
    }

    #[test]
    fn budget_color_at_zero_does_not_panic() {
        assert_ne!(Theme::dark().budget_color(0), Color::Reset);
    }

    #[test]
    fn budget_color_at_hundred_does_not_panic() {
        assert_ne!(Theme::dark().budget_color(100), Color::Reset);
    }

    // --- SerializableColor serde ---

    #[test]
    fn serializable_color_deserializes_named_red() {
        let toml_str = r#"text_primary = "red""#;
        let overrides: ThemeOverrides = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(overrides.text_primary, Some(SerializableColor(Color::Red)));
    }

    #[test]
    fn serializable_color_deserializes_hex_string() {
        let toml_str = r##"text_primary = "#ff0000""##;
        let overrides: ThemeOverrides = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(
            overrides.text_primary,
            Some(SerializableColor(Color::Rgb(255, 0, 0)))
        );
    }

    #[test]
    fn serializable_color_deserializes_indexed_integer() {
        let toml_str = r#"text_primary = 34"#;
        let overrides: ThemeOverrides = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(
            overrides.text_primary,
            Some(SerializableColor(Color::Indexed(34)))
        );
    }

    #[test]
    fn serializable_color_rejects_invalid_string() {
        let toml_str = r#"text_primary = "notacolor""#;
        let result: Result<ThemeOverrides, _> = toml::from_str(toml_str);
        assert!(result.is_err(), "expected error for invalid color name");
    }

    #[test]
    fn serializable_color_deserializes_all_standard_named_colors() {
        let names = [
            "black",
            "white",
            "red",
            "green",
            "blue",
            "yellow",
            "cyan",
            "magenta",
            "gray",
            "darkgray",
            "lightred",
            "lightgreen",
            "lightyellow",
            "lightblue",
            "lightmagenta",
            "lightcyan",
        ];
        for name in names {
            let toml_str = format!(r#"text_primary = "{name}""#);
            let result: Result<ThemeOverrides, _> = toml::from_str(&toml_str);
            assert!(result.is_ok(), "failed to deserialize color name: {name}");
        }
    }

    #[test]
    fn serializable_color_hex_uppercase_and_lowercase_are_equivalent() {
        let lower: ThemeOverrides = toml::from_str(r##"text_primary = "#ff0000""##).unwrap();
        let upper: ThemeOverrides = toml::from_str(r##"text_primary = "#FF0000""##).unwrap();
        assert_eq!(lower.text_primary, upper.text_primary);
    }

    // --- ThemePreset serde ---

    #[test]
    fn theme_preset_deserializes_dark_lowercase() {
        let cfg: ThemeConfig = toml::from_str(r#"preset = "dark""#).expect("parse failed");
        assert_eq!(cfg.preset, ThemePreset::Dark);
    }

    #[test]
    fn theme_preset_deserializes_light_lowercase() {
        let cfg: ThemeConfig = toml::from_str(r#"preset = "light""#).expect("parse failed");
        assert_eq!(cfg.preset, ThemePreset::Light);
    }

    #[test]
    fn theme_config_defaults_when_empty() {
        let cfg: ThemeConfig = toml::from_str("").expect("parse failed");
        assert_eq!(cfg.preset, ThemePreset::Dark);
        assert!(cfg.overrides.text_primary.is_none());
    }

    // --- ColorCapability::detect_from_env ---

    #[test]
    fn detect_truecolor_from_colorterm_truecolor() {
        let cap = ColorCapability::detect_from_env(|k| {
            if k == "COLORTERM" {
                Some("truecolor".into())
            } else {
                None
            }
        });
        assert_eq!(cap, ColorCapability::TrueColor);
    }

    #[test]
    fn detect_truecolor_from_colorterm_24bit() {
        let cap = ColorCapability::detect_from_env(|k| {
            if k == "COLORTERM" {
                Some("24bit".into())
            } else {
                None
            }
        });
        assert_eq!(cap, ColorCapability::TrueColor);
    }

    #[test]
    fn detect_extended_from_term_256color() {
        let cap = ColorCapability::detect_from_env(|k| {
            if k == "TERM" {
                Some("xterm-256color".into())
            } else {
                None
            }
        });
        assert_eq!(cap, ColorCapability::Extended);
    }

    #[test]
    fn detect_basic_when_no_env_vars_set() {
        let cap = ColorCapability::detect_from_env(|_| None);
        assert_eq!(cap, ColorCapability::Basic);
    }

    #[test]
    fn detect_truecolor_takes_priority_over_term_256color() {
        let cap = ColorCapability::detect_from_env(|k| match k {
            "COLORTERM" => Some("truecolor".into()),
            "TERM" => Some("xterm-256color".into()),
            _ => None,
        });
        assert_eq!(cap, ColorCapability::TrueColor);
    }

    #[test]
    fn detect_extended_from_screen_256color() {
        let cap = ColorCapability::detect_from_env(|k| {
            if k == "TERM" {
                Some("screen-256color".into())
            } else {
                None
            }
        });
        assert_eq!(cap, ColorCapability::Extended);
    }

    // --- ColorCapability::downgrade ---

    #[test]
    fn downgrade_rgb_is_unchanged_for_truecolor() {
        let c = ColorCapability::TrueColor.downgrade(Color::Rgb(255, 0, 0));
        assert_eq!(c, Color::Rgb(255, 0, 0));
    }

    #[test]
    fn downgrade_rgb_to_indexed_for_extended() {
        let c = ColorCapability::Extended.downgrade(Color::Rgb(255, 0, 0));
        assert!(
            matches!(c, Color::Indexed(_)),
            "expected Indexed, got {:?}",
            c
        );
    }

    #[test]
    fn downgrade_rgb_to_basic_for_basic() {
        let c = ColorCapability::Basic.downgrade(Color::Rgb(255, 0, 0));
        assert!(
            !matches!(c, Color::Rgb(_, _, _)) && !matches!(c, Color::Indexed(_)),
            "expected a basic named color, got {:?}",
            c
        );
    }

    #[test]
    fn downgrade_indexed_to_basic_for_basic() {
        let c = ColorCapability::Basic.downgrade(Color::Indexed(196));
        assert!(
            !matches!(c, Color::Rgb(_, _, _)) && !matches!(c, Color::Indexed(_)),
            "expected a basic named color, got {:?}",
            c
        );
    }

    #[test]
    fn downgrade_named_color_unchanged_for_extended() {
        assert_eq!(ColorCapability::Extended.downgrade(Color::Red), Color::Red);
    }

    #[test]
    fn downgrade_named_color_unchanged_for_truecolor() {
        assert_eq!(
            ColorCapability::TrueColor.downgrade(Color::Green),
            Color::Green
        );
    }
}
