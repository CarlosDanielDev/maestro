use crate::config::Config;
use crate::tui::widgets::{Dropdown, NumberStepper, TextInput, Toggle, WidgetKind};

use super::field;
use crate::tui::screens::settings::SettingsField;

pub(super) fn build_fields(config: &Config) -> Vec<SettingsField> {
    let s = &config.sessions;
    let permission_options: Vec<String> = vec![
        "default",
        "acceptEdits",
        "bypassPermissions",
        "dontAsk",
        "plan",
        "auto",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let perm_idx = permission_options
        .iter()
        .position(|p| p == &s.permission_mode)
        .unwrap_or(0);

    vec![
        field(WidgetKind::NumberStepper(NumberStepper::new(
            "max_concurrent",
            s.max_concurrent as i64,
            1,
            20,
        ))),
        field(WidgetKind::NumberStepper(
            NumberStepper::new("stall_timeout_secs", s.stall_timeout_secs as i64, 30, 3600)
                .with_step(30),
        )),
        field(WidgetKind::TextInput(TextInput::new(
            "default_model",
            &s.default_model,
        ))),
        field(WidgetKind::TextInput(TextInput::new(
            "default_mode",
            &s.default_mode,
        ))),
        field(WidgetKind::Toggle(Toggle::new(
            "bypass_review_corrections (DANGER: auto-accepts all review fixes)",
            s.permission_mode == "bypassPermissions",
        ))),
        field(WidgetKind::Dropdown(Dropdown::new(
            "permission_mode",
            permission_options,
            perm_idx,
        ))),
        field(WidgetKind::NumberStepper(NumberStepper::new(
            "max_retries",
            s.max_retries as i64,
            0,
            10,
        ))),
        field(WidgetKind::NumberStepper(
            NumberStepper::new("retry_cooldown_secs", s.retry_cooldown_secs as i64, 0, 600)
                .with_step(10),
        )),
        // Hollow retry policy (#275) — dropdown + per-intent steppers.
        field(WidgetKind::Dropdown(Dropdown::new(
            "hollow_retry.policy",
            vec!["always".into(), "intent-aware".into(), "never".into()],
            match s.hollow_retry.policy {
                crate::config::HollowRetryPolicy::Always => 0,
                crate::config::HollowRetryPolicy::IntentAware => 1,
                crate::config::HollowRetryPolicy::Never => 2,
            },
        ))),
        field(WidgetKind::NumberStepper(NumberStepper::new(
            "hollow_retry.work_max_retries",
            s.hollow_retry.work_max_retries as i64,
            0,
            10,
        ))),
        field(WidgetKind::NumberStepper(NumberStepper::new(
            "hollow_retry.consultation_max_retries",
            s.hollow_retry.consultation_max_retries as i64,
            0,
            10,
        ))),
        // Context Overflow sub-section
        field(WidgetKind::NumberStepper(
            NumberStepper::new(
                "overflow_threshold_pct",
                s.context_overflow.overflow_threshold_pct as i64,
                10,
                100,
            )
            .with_step(5),
        )),
        field(WidgetKind::Toggle(Toggle::new(
            "auto_fork",
            s.context_overflow.auto_fork,
        ))),
        field(WidgetKind::NumberStepper(
            NumberStepper::new(
                "commit_prompt_pct",
                s.context_overflow.commit_prompt_pct as i64,
                10,
                100,
            )
            .with_step(5),
        )),
        field(WidgetKind::NumberStepper(NumberStepper::new(
            "max_fork_depth",
            s.context_overflow.max_fork_depth as i64,
            1,
            20,
        ))),
        // Conflict sub-section
        field(WidgetKind::Toggle(Toggle::new(
            "conflict_enabled",
            s.conflict.enabled,
        ))),
        field(WidgetKind::Dropdown(Dropdown::new(
            "conflict_policy",
            vec!["warn".into(), "pause".into(), "kill".into()],
            match s.conflict.policy {
                crate::config::ConflictPolicy::Warn => 0,
                crate::config::ConflictPolicy::Pause => 1,
                crate::config::ConflictPolicy::Kill => 2,
            },
        ))),
    ]
}
