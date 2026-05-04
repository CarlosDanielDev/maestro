use crate::tui::theme::ThemeConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub ascii_icons: bool,
    /// Show the Clawd mascot companion in the TUI.
    #[serde(default = "default_show_mascot")]
    pub show_mascot: bool,
    /// Visual style for the mascot: `"sprite"` (pixel art, default) or
    /// `"ascii"` (legacy Unicode block-character art).
    #[serde(default)]
    pub mascot_style: crate::mascot::MascotStyle,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            theme: ThemeConfig::default(),
            layout: LayoutConfig::default(),
            ascii_icons: false,
            show_mascot: default_show_mascot(),
            mascot_style: crate::mascot::MascotStyle::default(),
        }
    }
}

fn default_show_mascot() -> bool {
    true
}

/// Layout configuration for the Issues screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutConfig {
    /// Panel arrangement mode.
    #[serde(default)]
    pub mode: LayoutMode,
    /// Information density level.
    #[serde(default)]
    pub density: Density,
    /// Percentage of width (horizontal) or height (vertical) for preview panel.
    #[serde(default = "default_preview_ratio")]
    pub preview_ratio: u8,
    /// Percentage of height for activity log panel.
    #[serde(default = "default_activity_log_height")]
    pub activity_log_height: u8,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            mode: LayoutMode::default(),
            density: Density::default(),
            preview_ratio: default_preview_ratio(),
            activity_log_height: default_activity_log_height(),
        }
    }
}

/// Panel arrangement mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutMode {
    #[default]
    Vertical,
    Horizontal,
}

/// Information density level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Default,
    Comfortable,
    Compact,
}

fn default_preview_ratio() -> u8 {
    50
}

fn default_activity_log_height() -> u8 {
    25
}
