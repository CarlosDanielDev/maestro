use crate::config::Config;
use crate::config::schema::schema_for_config;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    let table = schema_for_config()
        .iter()
        .find(|t| t.name == "github")
        .expect("github schema must exist");
    from_schema(table, config)
}
