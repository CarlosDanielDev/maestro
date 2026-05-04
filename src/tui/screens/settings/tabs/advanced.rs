use crate::config::Config;
use crate::tui::widgets::{ListEditor, NumberStepper, Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
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
        field(WidgetKind::Toggle(Toggle::new(
            super::super::CAVEMAN_LABEL,
            false,
        ))),
    ]
}
