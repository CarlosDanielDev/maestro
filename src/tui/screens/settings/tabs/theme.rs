use crate::config::Config;
use crate::tui::widgets::{Dropdown, Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    use crate::tui::theme::ThemePreset;
    let preset_options: Vec<String> = vec!["dark".into(), "light".into(), "retro".into()];
    let preset_idx = match config.tui.theme.preset {
        ThemePreset::Dark => 0,
        ThemePreset::Light => 1,
        ThemePreset::Retro => 2,
    };
    vec![
        field(WidgetKind::Toggle(Toggle::new("live_preview", false))),
        field(WidgetKind::Dropdown(Dropdown::new(
            "preset",
            preset_options,
            preset_idx,
        ))),
        field(WidgetKind::Toggle(Toggle::new(
            "ascii_icons",
            config.tui.ascii_icons,
        ))),
    ]
}
