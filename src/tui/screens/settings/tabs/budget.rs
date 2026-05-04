use crate::config::Config;
use crate::tui::widgets::{NumberStepper, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    let b = &config.budget;
    vec![
        field(WidgetKind::NumberStepper(
            NumberStepper::new(
                "per_session_usd",
                (b.per_session_usd * 10.0) as i64,
                1,
                1000,
            )
            .with_step(5),
        )),
        field(WidgetKind::NumberStepper(
            NumberStepper::new("total_usd", (b.total_usd * 10.0) as i64, 1, 10000).with_step(50),
        )),
        field(WidgetKind::NumberStepper(
            NumberStepper::new("alert_threshold_pct", b.alert_threshold_pct as i64, 10, 100)
                .with_step(5),
        )),
    ]
}
