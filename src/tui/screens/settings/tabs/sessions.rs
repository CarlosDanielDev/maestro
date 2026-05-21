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
    // Place right after `default_mode` (idx 2 after default_model and
    // permission_mode were removed from SESSIONS_FIELDS). The bypass
    // toggle is now the sole UI for `sessions.permission_mode` —
    // per-provider permission_mode lives on the Providers tab.
    let insert_at = 3.min(fields.len());
    fields.insert(insert_at, bypass);
    fields
}
