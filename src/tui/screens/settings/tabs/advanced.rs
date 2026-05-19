use crate::config::Config;
use crate::tui::widgets::{Toggle, WidgetKind};

use super::{field, schema_table};
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    let caveman = field(WidgetKind::Toggle(Toggle::new(
        super::super::CAVEMAN_LABEL,
        false,
    )));
    let mut concurrency_fields = from_schema(schema_table("concurrency"), config);
    let monitoring_fields = from_schema(schema_table("monitoring"), config);

    // Legacy order: [heavy_task_limit, work_tick_interval_secs,
    // heavy_task_labels, caveman_mode]. Reassemble by label.
    let mut out: Vec<SettingsField> = Vec::with_capacity(4);
    if let Some(idx) = concurrency_fields
        .iter()
        .position(|f| f.widget.label() == "heavy_task_limit")
    {
        out.push(concurrency_fields.remove(idx));
    }
    out.extend(monitoring_fields);
    if let Some(idx) = concurrency_fields
        .iter()
        .position(|f| f.widget.label() == "heavy_task_labels")
    {
        out.push(concurrency_fields.remove(idx));
    }
    out.push(caveman);
    out
}
