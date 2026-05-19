use crate::config::Config;
use crate::tui::widgets::{Toggle, WidgetKind};

use super::{field, schema_table};
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    let mut fields: Vec<SettingsField> = Vec::with_capacity(3);
    fields.push(field(WidgetKind::Toggle(Toggle::new(
        "live_preview",
        false,
    ))));
    fields.extend(from_schema(schema_table("tui.theme"), config));
    fields.extend(from_schema(schema_table("tui"), config));
    fields
}
