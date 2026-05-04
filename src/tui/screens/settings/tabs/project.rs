use crate::config::Config;
use crate::tui::widgets::{TextInput, Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    vec![
        field(WidgetKind::TextInput(TextInput::new(
            "repo",
            &config.project.repo,
        ))),
        field(WidgetKind::TextInput(TextInput::new(
            "base_branch",
            &config.project.base_branch,
        ))),
        field(WidgetKind::Toggle(Toggle::new(
            "Reset Settings (re-detect project stack)",
            false,
        ))),
    ]
}
