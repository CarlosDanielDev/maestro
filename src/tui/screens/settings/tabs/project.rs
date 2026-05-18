use crate::config::Config;
use crate::config::schema::PROJECT_TABLE;
use crate::flags::Flag;
use crate::flags::store::FeatureFlags;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;
use crate::tui::widgets::{TextInput, Toggle, WidgetKind};

use super::field;

pub(super) fn build_fields(config: &Config, flags: &FeatureFlags) -> Vec<SettingsField> {
    let mut fields = if flags.is_enabled(Flag::SchemaDrivenSettings) {
        from_schema(&PROJECT_TABLE, config)
    } else {
        vec![
            field(WidgetKind::TextInput(TextInput::new(
                "repo",
                &config.project.repo,
            ))),
            field(WidgetKind::TextInput(TextInput::new(
                "base_branch",
                &config.project.base_branch,
            ))),
        ]
    };
    fields.push(field(WidgetKind::Toggle(Toggle::new(
        "Reset Settings (re-detect project stack)",
        false,
    ))));
    fields.push(field(WidgetKind::Toggle(Toggle::new(
        "Normalize Agent Config (add [agents])",
        false,
    ))));
    fields
}
