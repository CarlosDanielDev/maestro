use crate::config::Config;
use crate::tui::widgets::{TextInput, Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    let r = &config.review;
    vec![
        field(WidgetKind::Toggle(Toggle::new("enabled", r.enabled))),
        field(WidgetKind::TextInput(TextInput::new("command", &r.command))),
    ]
}
