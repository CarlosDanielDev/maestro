//! Schema for `[tui.theme.overrides]` — 28 optional color overrides applied
//! on top of the preset (see `src/tui/theme.rs::ThemeOverrides`).
//!
//! Every override accepts a named color (`red`, `darkgray`, `lightcyan`, …),
//! a hex string (`#RRGGBB`), or a 256-color index (`0..=255`). Validation
//! lives in `SerializableColor::deserialize`; the schema layer accepts free
//! strings and renders the default as `unset` (the `DefaultValue::Str("")`
//! sentinel).

use super::{DefaultValue, FieldKind, FieldSchema};

const fn override_field(key: &'static str, label: &'static str, help: &'static str) -> FieldSchema {
    FieldSchema {
        key,
        label,
        help,
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    }
}

pub(super) const TUI_THEME_OVERRIDES_FIELDS: &[FieldSchema] = &[
    override_field(
        "branding_fg",
        "Branding FG",
        "Foreground color of the maestro branding badge — name, hex, or 256-color index",
    ),
    override_field(
        "branding_bg",
        "Branding BG",
        "Background color of the maestro branding badge — name, hex, or 256-color index",
    ),
    override_field(
        "text_primary",
        "Text Primary",
        "Primary text color — name, hex, or 256-color index",
    ),
    override_field(
        "text_secondary",
        "Text Secondary",
        "Secondary text color (subdued labels) — name, hex, or 256-color index",
    ),
    override_field(
        "text_muted",
        "Text Muted",
        "Muted text color (deprecated or low-priority text) — name, hex, or 256-color index",
    ),
    override_field(
        "border_active",
        "Border Active",
        "Active panel border color — name, hex, or 256-color index",
    ),
    override_field(
        "border_inactive",
        "Border Inactive",
        "Inactive panel border color — name, hex, or 256-color index",
    ),
    override_field(
        "border_focused",
        "Border Focused",
        "Focused panel border color — name, hex, or 256-color index",
    ),
    override_field(
        "accent_success",
        "Accent Success",
        "Success accent color (gates passed, completion) — name, hex, or 256-color index",
    ),
    override_field(
        "accent_warning",
        "Accent Warning",
        "Warning accent color — name, hex, or 256-color index",
    ),
    override_field(
        "accent_error",
        "Accent Error",
        "Error accent color — name, hex, or 256-color index",
    ),
    override_field(
        "accent_info",
        "Accent Info",
        "Info accent color — name, hex, or 256-color index",
    ),
    override_field(
        "accent_identifier",
        "Accent Identifier",
        "Identifier accent color (IDs, session keys) — name, hex, or 256-color index",
    ),
    override_field(
        "gauge_low",
        "Gauge Low",
        "Low-tier gauge color (under 40 percent) — name, hex, or 256-color index",
    ),
    override_field(
        "gauge_medium",
        "Gauge Medium",
        "Medium-tier gauge color — name, hex, or 256-color index",
    ),
    override_field(
        "gauge_high",
        "Gauge High",
        "High-tier gauge color — name, hex, or 256-color index",
    ),
    override_field(
        "gauge_background",
        "Gauge Background",
        "Gauge background color — name, hex, or 256-color index",
    ),
    override_field(
        "notification_critical",
        "Notification Critical",
        "Critical notification color — name, hex, or 256-color index",
    ),
    override_field(
        "notification_blocker",
        "Notification Blocker",
        "Blocker notification color — name, hex, or 256-color index",
    ),
    override_field(
        "notification_default",
        "Notification Default",
        "Default notification color — name, hex, or 256-color index",
    ),
    override_field(
        "keybind_key",
        "Keybind Key",
        "Keybind hint key color — name, hex, or 256-color index",
    ),
    override_field(
        "keybind_label_bg",
        "Keybind Label BG",
        "Keybind hint label background — name, hex, or 256-color index",
    ),
    override_field(
        "keybind_label_fg",
        "Keybind Label FG",
        "Keybind hint label foreground — name, hex, or 256-color index",
    ),
    override_field(
        "selection_bg",
        "Selection BG",
        "Selected-row background color — name, hex, or 256-color index",
    ),
    override_field(
        "selection_fg",
        "Selection FG",
        "Selected-row foreground color — name, hex, or 256-color index",
    ),
    override_field(
        "title_accent",
        "Title Accent",
        "Title bar accent color — name, hex, or 256-color index",
    ),
    override_field(
        "fkey_badge_bg",
        "F-key Badge BG",
        "F-key badge background color — name, hex, or 256-color index",
    ),
    override_field(
        "fkey_badge_fg",
        "F-key Badge FG",
        "F-key badge foreground color — name, hex, or 256-color index",
    ),
];
