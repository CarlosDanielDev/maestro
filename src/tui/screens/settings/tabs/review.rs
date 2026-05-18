use crate::config::Config;
use crate::config::schema::schema_for_config;
use crate::flags::Flag;
use crate::flags::store::FeatureFlags;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;
use crate::tui::widgets::{TextInput, Toggle, WidgetKind};

use super::field;

pub(super) fn build_fields(config: &Config, flags: &FeatureFlags) -> Vec<SettingsField> {
    if flags.is_enabled(Flag::SchemaDrivenSettings) {
        let table = schema_for_config()
            .iter()
            .find(|t| t.name == "review")
            .expect("review schema must exist");
        return from_schema(table, config);
    }

    let r = &config.review;
    vec![
        field(WidgetKind::Toggle(Toggle::new("enabled", r.enabled))),
        field(WidgetKind::TextInput(TextInput::new("command", &r.command))),
    ]
}
