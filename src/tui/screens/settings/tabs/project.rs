use crate::config::Config;
use crate::config::schema::PROJECT_TABLE;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;
use crate::tui::widgets::{Toggle, WidgetKind};

use super::field;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    let mut fields = from_schema(&PROJECT_TABLE, config);
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
