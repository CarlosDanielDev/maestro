use crate::config::Config;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;
use crate::tui::widgets::{Dropdown, WidgetKind};

use super::{DEFAULT_PROVIDER_LABEL, field, schema_table};

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    let mut options: Vec<String> = config.agents.entries.keys().cloned().collect();
    if options.is_empty() {
        options.push("claude".to_string());
    }
    let selected = options
        .iter()
        .position(|id| id == &config.agents.default)
        .unwrap_or(0);

    let mut fields = Vec::with_capacity(2);
    // Inject the "Default provider" dropdown at the top so users can pick
    // which `[agents.<id>]` entry is the runtime default without leaving
    // the tab. Sync writeback lives in `sync_agents_default_override`.
    fields.push(field(WidgetKind::Dropdown(Dropdown::new(
        DEFAULT_PROVIDER_LABEL,
        options,
        selected,
    ))));
    fields.extend(from_schema(schema_table("agents"), config));
    fields
}
