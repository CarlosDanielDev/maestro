use crate::config::Config;
use crate::config::schema::schema_for_config;
use crate::flags::Flag;
use crate::flags::store::FeatureFlags;
use crate::tui::widgets::{ListEditor, NumberStepper, Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

pub(super) fn build_fields(config: &Config, flags: &FeatureFlags) -> Vec<SettingsField> {
    let caveman = field(WidgetKind::Toggle(Toggle::new(
        super::super::CAVEMAN_LABEL,
        false,
    )));

    if flags.is_enabled(Flag::SchemaDrivenSettings) {
        let concurrency_table = schema_for_config()
            .iter()
            .find(|t| t.name == "concurrency")
            .expect("concurrency schema must exist");
        let monitoring_table = schema_for_config()
            .iter()
            .find(|t| t.name == "monitoring")
            .expect("monitoring schema must exist");
        let mut concurrency_fields = from_schema(concurrency_table, config);
        let monitoring_fields = from_schema(monitoring_table, config);

        // Legacy order is [heavy_task_limit, work_tick_interval_secs,
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
        return out;
    }

    vec![
        field(WidgetKind::NumberStepper(NumberStepper::new(
            "heavy_task_limit",
            config.concurrency.heavy_task_limit as i64,
            1,
            10,
        ))),
        field(WidgetKind::NumberStepper(
            NumberStepper::new(
                "work_tick_interval_secs",
                config.monitoring.work_tick_interval_secs as i64,
                1,
                120,
            )
            .with_step(5),
        )),
        field(WidgetKind::ListEditor(ListEditor::new(
            "heavy_task_labels",
            config.concurrency.heavy_task_labels.clone(),
        ))),
        // Toggle here only receives Space; rendering is overlaid by caveman_row.
        caveman,
    ]
}
