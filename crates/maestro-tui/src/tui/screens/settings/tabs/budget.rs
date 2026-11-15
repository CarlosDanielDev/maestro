use crate::config::Config;
use crate::config::schema::BUDGET_TABLE;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    from_schema(&BUDGET_TABLE, config)
}
