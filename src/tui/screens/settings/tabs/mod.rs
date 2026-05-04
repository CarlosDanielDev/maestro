pub mod advanced;
pub mod budget;
pub mod flags;
pub mod gates;
pub mod github;
pub mod layout;
pub mod notifications;
pub mod project;
pub mod review;
pub mod sessions;
pub mod theme;
pub mod turboquant;

use crate::config::Config;
use crate::tui::widgets::WidgetKind;

use super::{SettingsField, SettingsScreen};

fn field(widget: WidgetKind) -> SettingsField {
    SettingsField { widget }
}

pub(super) fn build_fields(config: &Config) -> Vec<Vec<SettingsField>> {
    vec![
        project::build_fields(config),
        sessions::build_fields(config),
        budget::build_fields(config),
        github::build_fields(config),
        notifications::build_fields(config),
        gates::build_fields(config),
        review::build_fields(config),
        theme::build_fields(config),
        layout::build_fields(config),
        vec![],
        turboquant::build_fields(config),
        advanced::build_fields(config),
    ]
}

pub(super) fn sync_widgets_to_config(screen: &mut SettingsScreen) {
    // Project (tab 0)
    if let Some(fields) = screen.fields_per_tab.first() {
        if let Some(WidgetKind::TextInput(w)) = fields.first().map(|f| &f.widget) {
            screen.config.project.repo = w.value.clone();
        }
        if let Some(WidgetKind::TextInput(w)) = fields.get(1).map(|f| &f.widget) {
            screen.config.project.base_branch = w.value.clone();
        }
    }

    // Sessions (tab 1) — looked up by label so widget reordering
    // cannot silently drop a sync. New widgets only need to appear in
    // `build_sessions_fields`; no index bookkeeping here.
    if let Some(fields) = screen.fields_per_tab.get(1) {
        let s = &mut screen.config.sessions;
        if let Some(WidgetKind::NumberStepper(w)) = super::widget_by_label(fields, "max_concurrent")
        {
            s.max_concurrent = w.value as usize;
        }
        if let Some(WidgetKind::NumberStepper(w)) =
            super::widget_by_label(fields, "stall_timeout_secs")
        {
            s.stall_timeout_secs = w.value as u64;
        }
        if let Some(WidgetKind::TextInput(w)) = super::widget_by_label(fields, "default_model") {
            s.default_model = w.value.clone();
        }
        if let Some(WidgetKind::TextInput(w)) = super::widget_by_label(fields, "default_mode") {
            s.default_mode = w.value.clone();
        }
        // Apply the bypass toggle FIRST so the permission_mode dropdown
        // can override it if the user explicitly picked a non-bypass
        // value (e.g. "acceptEdits"). Toggle ON → bypassPermissions;
        // toggle OFF → "default" only if currently bypass (so users
        // who picked "acceptEdits" via the dropdown aren't reset).
        if let Some(WidgetKind::Toggle(w)) = super::widget_by_label(
            fields,
            "bypass_review_corrections (DANGER: auto-accepts all review fixes)",
        ) {
            if w.value {
                s.permission_mode = "bypassPermissions".to_string();
            } else if s.permission_mode == "bypassPermissions" {
                s.permission_mode = "default".to_string();
            }
        }
        if let Some(WidgetKind::Dropdown(w)) = super::widget_by_label(fields, "permission_mode") {
            s.permission_mode = w.selected_value().to_string();
        }
        if let Some(WidgetKind::NumberStepper(w)) = super::widget_by_label(fields, "max_retries") {
            s.max_retries = w.value as u32;
        }
        if let Some(WidgetKind::NumberStepper(w)) =
            super::widget_by_label(fields, "retry_cooldown_secs")
        {
            s.retry_cooldown_secs = w.value as u64;
        }
        if let Some(WidgetKind::Dropdown(w)) = super::widget_by_label(fields, "hollow_retry.policy")
        {
            s.hollow_retry.policy = match w.selected {
                0 => crate::config::HollowRetryPolicy::Always,
                1 => crate::config::HollowRetryPolicy::IntentAware,
                _ => crate::config::HollowRetryPolicy::Never,
            };
        }
        if let Some(WidgetKind::NumberStepper(w)) =
            super::widget_by_label(fields, "hollow_retry.work_max_retries")
        {
            s.hollow_retry.work_max_retries = w.value as u32;
        }
        if let Some(WidgetKind::NumberStepper(w)) =
            super::widget_by_label(fields, "hollow_retry.consultation_max_retries")
        {
            s.hollow_retry.consultation_max_retries = w.value as u32;
        }
        if let Some(WidgetKind::NumberStepper(w)) =
            super::widget_by_label(fields, "overflow_threshold_pct")
        {
            s.context_overflow.overflow_threshold_pct = w.value as u8;
        }
        if let Some(WidgetKind::Toggle(w)) = super::widget_by_label(fields, "auto_fork") {
            s.context_overflow.auto_fork = w.value;
        }
        if let Some(WidgetKind::NumberStepper(w)) =
            super::widget_by_label(fields, "commit_prompt_pct")
        {
            s.context_overflow.commit_prompt_pct = w.value as u8;
        }
        if let Some(WidgetKind::NumberStepper(w)) = super::widget_by_label(fields, "max_fork_depth")
        {
            s.context_overflow.max_fork_depth = w.value as u8;
        }
        if let Some(WidgetKind::Toggle(w)) = super::widget_by_label(fields, "conflict_enabled") {
            s.conflict.enabled = w.value;
        }
        if let Some(WidgetKind::Dropdown(w)) = super::widget_by_label(fields, "conflict_policy") {
            s.conflict.policy = match w.selected {
                0 => crate::config::ConflictPolicy::Warn,
                1 => crate::config::ConflictPolicy::Pause,
                _ => crate::config::ConflictPolicy::Kill,
            };
        }
    }

    // Budget (tab 2) — values stored as x10 for decimal precision
    if let Some(fields) = screen.fields_per_tab.get(2) {
        if let Some(WidgetKind::NumberStepper(w)) = fields.first().map(|f| &f.widget) {
            screen.config.budget.per_session_usd = w.value as f64 / 10.0;
        }
        if let Some(WidgetKind::NumberStepper(w)) = fields.get(1).map(|f| &f.widget) {
            screen.config.budget.total_usd = w.value as f64 / 10.0;
        }
        if let Some(WidgetKind::NumberStepper(w)) = fields.get(2).map(|f| &f.widget) {
            screen.config.budget.alert_threshold_pct = w.value as u8;
        }
    }

    // GitHub (tab 3)
    if let Some(fields) = screen.fields_per_tab.get(3) {
        let g = &mut screen.config.github;
        if let Some(WidgetKind::ListEditor(w)) = fields.first().map(|f| &f.widget) {
            g.issue_filter_labels = w.items.clone();
        }
        if let Some(WidgetKind::Toggle(w)) = fields.get(1).map(|f| &f.widget) {
            g.auto_pr = w.value;
        }
        if let Some(WidgetKind::NumberStepper(w)) = fields.get(2).map(|f| &f.widget) {
            g.cache_ttl_secs = w.value as u64;
        }
        if let Some(WidgetKind::Toggle(w)) = fields.get(3).map(|f| &f.widget) {
            g.auto_merge = w.value;
        }
        if let Some(WidgetKind::Dropdown(w)) = fields.get(4).map(|f| &f.widget) {
            g.merge_method = match w.selected {
                0 => crate::config::MergeMethod::Merge,
                1 => crate::config::MergeMethod::Squash,
                _ => crate::config::MergeMethod::Rebase,
            };
        }
    }

    // Notifications (tab 4)
    if let Some(fields) = screen.fields_per_tab.get(4) {
        let n = &mut screen.config.notifications;
        if let Some(WidgetKind::Toggle(w)) = fields.first().map(|f| &f.widget) {
            n.desktop = w.value;
        }
        if let Some(WidgetKind::Toggle(w)) = fields.get(1).map(|f| &f.widget) {
            n.slack = w.value;
        }
        if let Some(WidgetKind::TextInput(w)) = fields.get(2).map(|f| &f.widget) {
            n.slack_webhook_url = if w.value.is_empty() {
                None
            } else {
                Some(w.value.clone())
            };
        }
        if let Some(WidgetKind::NumberStepper(w)) = fields.get(3).map(|f| &f.widget) {
            n.slack_rate_limit_per_min = w.value as u32;
        }
    }

    // Gates (tab 5)
    if let Some(fields) = screen.fields_per_tab.get(5) {
        let g = &mut screen.config.gates;
        if let Some(WidgetKind::Toggle(w)) = fields.first().map(|f| &f.widget) {
            g.enabled = w.value;
        }
        if let Some(WidgetKind::TextInput(w)) = fields.get(1).map(|f| &f.widget) {
            g.test_command = w.value.clone();
        }
        if let Some(WidgetKind::NumberStepper(w)) = fields.get(2).map(|f| &f.widget) {
            g.ci_poll_interval_secs = w.value as u64;
        }
        if let Some(WidgetKind::NumberStepper(w)) = fields.get(3).map(|f| &f.widget) {
            g.ci_max_wait_secs = w.value as u64;
        }
        if let Some(WidgetKind::Toggle(w)) = fields.get(4).map(|f| &f.widget) {
            g.ci_auto_fix.enabled = w.value;
        }
        if let Some(WidgetKind::NumberStepper(w)) = fields.get(5).map(|f| &f.widget) {
            g.ci_auto_fix.max_retries = w.value as u32;
        }
    }

    // Review (tab 6)
    if let Some(fields) = screen.fields_per_tab.get(6) {
        let r = &mut screen.config.review;
        if let Some(WidgetKind::Toggle(w)) = fields.first().map(|f| &f.widget) {
            r.enabled = w.value;
        }
        if let Some(WidgetKind::TextInput(w)) = fields.get(1).map(|f| &f.widget) {
            r.command = w.value.clone();
        }
    }

    // Theme (tab 7)
    if let Some(fields) = screen.fields_per_tab.get(7) {
        if let Some(WidgetKind::Toggle(w)) = fields.first().map(|f| &f.widget) {
            screen.live_preview = w.value;
        }
        if let Some(WidgetKind::Dropdown(w)) = fields.get(1).map(|f| &f.widget) {
            screen.config.tui.theme.preset = match w.selected {
                0 => crate::tui::theme::ThemePreset::Dark,
                1 => crate::tui::theme::ThemePreset::Light,
                _ => crate::tui::theme::ThemePreset::Retro,
            };
        }
        if let Some(WidgetKind::Toggle(w)) = fields.get(2).map(|f| &f.widget) {
            screen.config.tui.ascii_icons = w.value;
        }
    }

    // Layout (tab 8)
    if let Some(fields) = screen.fields_per_tab.get(8) {
        let l = &mut screen.config.tui.layout;
        if let Some(WidgetKind::Dropdown(w)) = fields.first().map(|f| &f.widget) {
            l.mode = match w.selected {
                0 => crate::config::LayoutMode::Vertical,
                _ => crate::config::LayoutMode::Horizontal,
            };
        }
        if let Some(WidgetKind::Dropdown(w)) = fields.get(1).map(|f| &f.widget) {
            l.density = match w.selected {
                0 => crate::config::Density::Default,
                1 => crate::config::Density::Comfortable,
                _ => crate::config::Density::Compact,
            };
        }
        if let Some(WidgetKind::NumberStepper(w)) = fields.get(2).map(|f| &f.widget) {
            l.preview_ratio = w.value as u8;
        }
        if let Some(WidgetKind::NumberStepper(w)) = fields.get(3).map(|f| &f.widget) {
            l.activity_log_height = w.value as u8;
        }
    }

    // TurboQuant (tab 10 — Flags tab at 9 has no widgets)
    if let Some(fields) = screen.fields_per_tab.get(10) {
        let tq = &mut screen.config.turboquant;
        if let Some(WidgetKind::Toggle(w)) = fields.first().map(|f| &f.widget) {
            tq.enabled = w.value;
        }
        if let Some(WidgetKind::NumberStepper(w)) = fields.get(1).map(|f| &f.widget) {
            tq.bit_width = w.value as u8;
        }
        if let Some(WidgetKind::Dropdown(w)) = fields.get(2).map(|f| &f.widget) {
            tq.strategy = match w.selected {
                0 => crate::config::QuantStrategy::TurboQuant,
                1 => crate::config::QuantStrategy::PolarQuant,
                _ => crate::config::QuantStrategy::Qjl,
            };
        }
        if let Some(WidgetKind::Dropdown(w)) = fields.get(3).map(|f| &f.widget) {
            tq.apply_to = match w.selected {
                0 => crate::config::ApplyTarget::Keys,
                1 => crate::config::ApplyTarget::Values,
                _ => crate::config::ApplyTarget::Both,
            };
        }
        if let Some(WidgetKind::Toggle(w)) = fields.get(4).map(|f| &f.widget) {
            tq.auto_on_overflow = w.value;
        }
    }

    // Advanced (tab 11 — after TurboQuant)
    let mut caveman_change: Option<bool> = None;
    if let Some(fields) = screen.fields_per_tab.get(11) {
        if let Some(WidgetKind::NumberStepper(w)) = fields.first().map(|f| &f.widget) {
            screen.config.concurrency.heavy_task_limit = w.value as usize;
        }
        if let Some(WidgetKind::NumberStepper(w)) = fields.get(1).map(|f| &f.widget) {
            screen.config.monitoring.work_tick_interval_secs = w.value as u64;
        }
        if let Some(WidgetKind::ListEditor(w)) = fields.get(2).map(|f| &f.widget) {
            screen.config.concurrency.heavy_task_labels = w.items.clone();
        }
        let prev = screen.caveman_state.as_bool().unwrap_or(false);
        if let Some(WidgetKind::Toggle(w)) = super::widget_by_label(fields, super::CAVEMAN_LABEL)
            && w.value != prev
        {
            caveman_change = Some(w.value);
        }
    }
    if let Some(new_value) = caveman_change {
        if screen.caveman_state.is_toggleable() {
            screen.pending_caveman_toggle = Some(new_value);
        } else {
            let label = screen.caveman_state.label().into_owned();
            let state = screen.caveman_state.clone();
            screen.set_caveman_state(state);
            screen.show_caveman_status(format!(
                "caveman_mode is unreadable ({}); fix the file before toggling.",
                label
            ));
        }
    }
}
