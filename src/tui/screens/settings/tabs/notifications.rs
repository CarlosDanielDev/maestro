use crate::config::Config;
use crate::config::schema::schema_for_config;
use crate::flags::Flag;
use crate::flags::store::FeatureFlags;
use crate::tui::widgets::{NumberStepper, TextInput, Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

pub(super) fn build_fields(config: &Config, flags: &FeatureFlags) -> Vec<SettingsField> {
    if flags.is_enabled(Flag::SchemaDrivenSettings) {
        let table = schema_for_config()
            .iter()
            .find(|t| t.name == "notifications")
            .expect("notifications schema must exist");
        return from_schema(table, config);
    }

    let n = &config.notifications;
    vec![
        field(WidgetKind::Toggle(Toggle::new("desktop", n.desktop))),
        field(WidgetKind::Toggle(Toggle::new("slack", n.slack))),
        field(WidgetKind::TextInput(TextInput::new(
            "slack_webhook_url",
            n.slack_webhook_url.as_deref().unwrap_or(""),
        ))),
        field(WidgetKind::NumberStepper(NumberStepper::new(
            "slack_rate_limit_per_min",
            n.slack_rate_limit_per_min as i64,
            1,
            60,
        ))),
    ]
}
