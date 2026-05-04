use crate::config::Config;
use crate::tui::widgets::{NumberStepper, TextInput, Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
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
