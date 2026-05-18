use crate::config::Config;
use crate::config::schema::schema_for_config;
use crate::flags::Flag;
use crate::flags::store::FeatureFlags;
use crate::tui::widgets::{NumberStepper, TextInput, Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

pub(super) fn build_fields(config: &Config, flags: &FeatureFlags) -> Vec<SettingsField> {
    if flags.is_enabled(Flag::SchemaDrivenSettings) {
        let table = schema_for_config()
            .iter()
            .find(|t| t.name == "gates")
            .expect("gates schema must exist");
        return from_schema(table, config);
    }

    let g = &config.gates;
    vec![
        field(WidgetKind::Toggle(Toggle::new("enabled", g.enabled))),
        field(WidgetKind::TextInput(TextInput::new(
            "test_command",
            &g.test_command,
        ))),
        field(WidgetKind::NumberStepper(
            NumberStepper::new(
                "ci_poll_interval_secs",
                g.ci_poll_interval_secs as i64,
                5,
                300,
            )
            .with_step(5),
        )),
        field(WidgetKind::NumberStepper(
            NumberStepper::new("ci_max_wait_secs", g.ci_max_wait_secs as i64, 60, 7200)
                .with_step(60),
        )),
        field(WidgetKind::Toggle(Toggle::new(
            "ci_auto_fix.enabled",
            g.ci_auto_fix.enabled,
        ))),
        field(WidgetKind::NumberStepper(NumberStepper::new(
            "ci_auto_fix.max_retries",
            g.ci_auto_fix.max_retries as i64,
            0,
            10,
        ))),
    ]
}
