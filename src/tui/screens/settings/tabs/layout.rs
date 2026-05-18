use crate::config::Config;
use crate::config::schema::schema_for_config;
use crate::flags::Flag;
use crate::flags::store::FeatureFlags;
use crate::tui::widgets::{Dropdown, NumberStepper, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;
use crate::tui::screens::settings::schema_tab::build::from_schema;

pub(super) fn build_fields(config: &Config, flags: &FeatureFlags) -> Vec<SettingsField> {
    if flags.is_enabled(Flag::SchemaDrivenSettings) {
        let table = schema_for_config()
            .iter()
            .find(|t| t.name == "tui.layout")
            .expect("tui.layout schema must exist");
        return from_schema(table, config);
    }

    use crate::config::{Density, LayoutMode};
    let mode_options: Vec<String> = vec!["vertical".into(), "horizontal".into()];
    let mode_idx = match config.tui.layout.mode {
        LayoutMode::Vertical => 0,
        LayoutMode::Horizontal => 1,
    };
    let density_options: Vec<String> =
        vec!["default".into(), "comfortable".into(), "compact".into()];
    let density_idx = match config.tui.layout.density {
        Density::Default => 0,
        Density::Comfortable => 1,
        Density::Compact => 2,
    };
    vec![
        field(WidgetKind::Dropdown(Dropdown::new(
            "mode",
            mode_options,
            mode_idx,
        ))),
        field(WidgetKind::Dropdown(Dropdown::new(
            "density",
            density_options,
            density_idx,
        ))),
        field(WidgetKind::NumberStepper(
            NumberStepper::new(
                "preview_ratio",
                config.tui.layout.preview_ratio as i64,
                10,
                90,
            )
            .with_step(5),
        )),
        field(WidgetKind::NumberStepper(
            NumberStepper::new(
                "activity_log_height",
                config.tui.layout.activity_log_height as i64,
                10,
                50,
            )
            .with_step(5),
        )),
    ]
}
