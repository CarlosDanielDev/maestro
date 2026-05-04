use crate::config::Config;
use crate::tui::widgets::{Dropdown, NumberStepper, Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    use crate::config::{ApplyTarget, QuantStrategy};
    let tq = &config.turboquant;
    let strategy_options: Vec<String> =
        vec!["turboquant".into(), "polarquant".into(), "qjl".into()];
    let strategy_idx = match tq.strategy {
        QuantStrategy::TurboQuant => 0,
        QuantStrategy::PolarQuant => 1,
        QuantStrategy::Qjl => 2,
    };
    let apply_options: Vec<String> = vec!["keys".into(), "values".into(), "both".into()];
    let apply_idx = match tq.apply_to {
        ApplyTarget::Keys => 0,
        ApplyTarget::Values => 1,
        ApplyTarget::Both => 2,
    };
    vec![
        field(WidgetKind::Toggle(Toggle::new("enabled", tq.enabled))),
        field(WidgetKind::NumberStepper(NumberStepper::new(
            "bit_width",
            tq.bit_width as i64,
            1,
            8,
        ))),
        field(WidgetKind::Dropdown(Dropdown::new(
            "strategy",
            strategy_options,
            strategy_idx,
        ))),
        field(WidgetKind::Dropdown(Dropdown::new(
            "apply_to",
            apply_options,
            apply_idx,
        ))),
        field(WidgetKind::Toggle(Toggle::new(
            "auto_on_overflow",
            tq.auto_on_overflow,
        ))),
    ]
}
