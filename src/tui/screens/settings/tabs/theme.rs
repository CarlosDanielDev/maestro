use crate::config::Config;
use crate::config::schema::schema_for_config;
use crate::tui::widgets::{Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    let mut fields: Vec<SettingsField> = Vec::with_capacity(3);
    fields.push(field(WidgetKind::Toggle(Toggle::new(
        "live_preview",
        false,
    ))));
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
    fields
}
