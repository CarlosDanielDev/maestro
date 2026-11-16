//! Theme settings as they live in `maestro.toml` (`[tui.theme]`).
//!
//! Only the serializable data is here; the resolved runtime `Theme` and all
//! rendering stay in the TUI (`src/tui/theme.rs`), which re-exports these.

use serde::{Deserialize, Serialize};

/// A colour value from `maestro.toml`. Core owns this type so it needs no
/// terminal-UI crate; the TUI maps it to its own colour type
/// (`tui::theme::to_color`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
    Rgb(u8, u8, u8),
    Indexed(u8),
}

// ---------------------------------------------------------------------------
// SerializableColor — ThemeColor with serde support
// ---------------------------------------------------------------------------

/// A color value that can be serialized to/from TOML.
/// Accepts: named colors ("red"), hex ("#ff0000"), or 256-color index (34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerializableColor(pub ThemeColor);

impl Serialize for SerializableColor {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            ThemeColor::Rgb(r, g, b) => {
                serializer.serialize_str(&format!("#{:02x}{:02x}{:02x}", r, g, b))
            }
            ThemeColor::Indexed(idx) => serializer.serialize_u8(idx),
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
                    return Ok(SerializableColor(ThemeColor::Rgb(r, g, b)));
                }
                name_to_color(v)
                    .map(SerializableColor)
                    .ok_or_else(|| E::custom(format!("unknown color name: {}", v)))
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                if v > 255 {
                    return Err(E::custom(format!("color index out of range: {}", v)));
                }
                Ok(SerializableColor(ThemeColor::Indexed(v as u8)))
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                if !(0..=255).contains(&v) {
                    return Err(E::custom(format!("color index out of range: {}", v)));
                }
                Ok(SerializableColor(ThemeColor::Indexed(v as u8)))
            }
        }

        deserializer.deserialize_any(ColorVisitor)
    }
}

fn name_to_color(name: &str) -> Option<ThemeColor> {
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
        "black" => Some(ThemeColor::Black),
        "red" => Some(ThemeColor::Red),
        "green" => Some(ThemeColor::Green),
        "yellow" => Some(ThemeColor::Yellow),
        "blue" => Some(ThemeColor::Blue),
        "magenta" => Some(ThemeColor::Magenta),
        "cyan" => Some(ThemeColor::Cyan),
        "gray" | "grey" => Some(ThemeColor::Gray),
        "darkgray" | "darkgrey" => Some(ThemeColor::DarkGray),
        "lightred" => Some(ThemeColor::LightRed),
        "lightgreen" => Some(ThemeColor::LightGreen),
        "lightyellow" => Some(ThemeColor::LightYellow),
        "lightblue" => Some(ThemeColor::LightBlue),
        "lightmagenta" => Some(ThemeColor::LightMagenta),
        "lightcyan" => Some(ThemeColor::LightCyan),
        "white" => Some(ThemeColor::White),
        _ => None,
    }
}

fn color_to_name(color: ThemeColor) -> &'static str {
    match color {
        ThemeColor::Black => "black",
        ThemeColor::Red => "red",
        ThemeColor::Green => "green",
        ThemeColor::Yellow => "yellow",
        ThemeColor::Blue => "blue",
        ThemeColor::Magenta => "magenta",
        ThemeColor::Cyan => "cyan",
        ThemeColor::Gray => "gray",
        ThemeColor::DarkGray => "darkgray",
        ThemeColor::LightRed => "lightred",
        ThemeColor::LightGreen => "lightgreen",
        ThemeColor::LightYellow => "lightyellow",
        ThemeColor::LightBlue => "lightblue",
        ThemeColor::LightMagenta => "lightmagenta",
        ThemeColor::LightCyan => "lightcyan",
        ThemeColor::White => "white",
        // Rgb/Indexed serialize before reaching here.
        ThemeColor::Rgb(..) | ThemeColor::Indexed(_) => "white",
    }
}

// ---------------------------------------------------------------------------
// Theme presets and configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePreset {
    #[default]
    Dark,
    Light,
    Retro,
    /// Black/white base, saturated accents (low vision, projectors).
    HighContrast,
    /// Every colour `Reset`; also forced by `NO_COLOR` or `TERM=dumb`.
    NoColor,
}

/// Top-level theme configuration, embeddable in maestro.toml.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ThemeOverrides {
    pub branding_fg: Option<SerializableColor>,
    pub branding_bg: Option<SerializableColor>,
    pub text_primary: Option<SerializableColor>,
    pub text_secondary: Option<SerializableColor>,
    pub text_muted: Option<SerializableColor>,
    pub border_active: Option<SerializableColor>,
    pub border_inactive: Option<SerializableColor>,
    /// Retired (#1247): still parsed so old `maestro.toml` files load, never
    /// drawn. Focus uses `border_active`.
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
    pub selection_bg: Option<SerializableColor>,
    pub selection_fg: Option<SerializableColor>,
    pub title_accent: Option<SerializableColor>,
    pub fkey_badge_bg: Option<SerializableColor>,
    pub fkey_badge_fg: Option<SerializableColor>,
    pub surface_bar: Option<SerializableColor>,
    pub surface_overlay: Option<SerializableColor>,
    pub status_needs_pr: Option<SerializableColor>,
    pub status_conflict: Option<SerializableColor>,
}
