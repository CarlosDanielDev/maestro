use crate::config::Config;
use crate::config::schema::schema_for_config;
use crate::tui::widgets::{Toggle, WidgetKind};

use super::{BYPASS_LABEL, field};
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    let table = schema_for_config()
        .iter()
        .find(|t| t.name == "sessions")
        .expect("sessions schema must exist");
    let mut fields = from_schema(table, config);
    let bypass = field(WidgetKind::Toggle(Toggle::new(
        BYPASS_LABEL,
        config.sessions.permission_mode == "bypassPermissions",
    )));
    // Place between `default_mode` (idx 3) and `permission_mode` (idx 4).
    let insert_at = 4.min(fields.len());
    fields.insert(insert_at, bypass);
    fields
}
