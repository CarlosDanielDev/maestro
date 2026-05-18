use crate::config::Config;
use crate::config::schema::schema_for_config;
use crate::flags::Flag;
use crate::flags::store::FeatureFlags;
use crate::tui::widgets::{Dropdown, Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

pub(super) fn build_fields(config: &Config, flags: &FeatureFlags) -> Vec<SettingsField> {
    if flags.is_enabled(Flag::SchemaDrivenSettings) {
        let mut fields: Vec<SettingsField> = Vec::with_capacity(3);
        fields.push(field(WidgetKind::Toggle(Toggle::new("live_preview", false))));
        let theme_table = schema_for_config()
            .iter()
            .find(|t| t.name == "tui.theme")
            .expect("tui.theme schema must exist");
        fields.extend(from_schema(theme_table, config));
        let tui_table = schema_for_config()
            .iter()
            .find(|t| t.name == "tui")
            .expect("tui schema must exist");
        fields.extend(from_schema(tui_table, config));
        return fields;
    }

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
