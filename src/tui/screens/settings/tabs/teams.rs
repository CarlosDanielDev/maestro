use crate::config::Config;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

use super::schema_table;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    from_schema(schema_table("teams"), config)
}
