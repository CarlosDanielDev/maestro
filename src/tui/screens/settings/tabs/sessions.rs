use crate::config::Config;
use crate::tui::widgets::{Toggle, WidgetKind};

use super::{BYPASS_LABEL, field, schema_table};
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    let mut fields = from_schema(schema_table("sessions"), config);
    let bypass = field(WidgetKind::Toggle(Toggle::new(
        BYPASS_LABEL,
        config.sessions.permission_mode == "bypassPermissions",
    )));
    // Place between `default_mode` (idx 3) and `permission_mode` (idx 4).
    let insert_at = 4.min(fields.len());
    fields.insert(insert_at, bypass);
    fields
}
