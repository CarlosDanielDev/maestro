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
use crate::config::schema::{TableSchema, schema_for_config};
use crate::tui::screens::settings::schema_tab::sync::sync_to_config;
use crate::tui::widgets::WidgetKind;

use super::{CAVEMAN_LABEL, SettingsField, SettingsScreen, widget_by_label};

fn field(widget: WidgetKind) -> SettingsField {
    SettingsField { widget }
}

pub(super) const BYPASS_LABEL: &str =
    "bypass_review_corrections (DANGER: auto-accepts all review fixes)";

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

/// Map a settings tab index to the schema table that drives it. `None` means
/// the tab is hand-coded (Budget) or has no widgets (Flags).
fn schema_table_for_tab(idx: usize) -> Option<&'static TableSchema> {
    let name = match idx {
        0 => "project",
        1 => "sessions",
        3 => "github",
        4 => "notifications",
        5 => "gates",
        6 => "review",
        8 => "tui.layout",
        10 => "turboquant",
        _ => return None,
    };
    schema_for_config().iter().find(|t| t.name == name)
}

pub(super) fn sync_widgets_to_config(screen: &mut SettingsScreen) {
    sync_schema_tabs(screen);
    sync_sessions_bypass_override(screen);
    sync_theme_multi_table(screen);
    sync_theme_screen_local(screen);
    sync_notifications_empty_url_collapse(screen);
    sync_budget_legacy(screen);
    sync_advanced_multi_table(screen);
    sync_advanced_caveman(screen);
}

fn sync_schema_tabs(screen: &mut SettingsScreen) {
    for (idx, fields) in screen.fields_per_tab.iter().enumerate() {
        let Some(table) = schema_table_for_tab(idx) else {
            continue;
        };
        if let Err(e) = sync_to_config(table, fields, &mut screen.config) {
            tracing::warn!(tab = idx, error = %e, "schema sync failed; config left unchanged");
        }
    }
}

/// Sessions bypass toggle is a bespoke field that derives `permission_mode`.
/// Applied AFTER the schema sync wrote the dropdown's value, then re-applied
/// in the same dropdown-wins order legacy used.
fn sync_sessions_bypass_override(screen: &mut SettingsScreen) {
    let Some(fields) = screen.fields_per_tab.get(1) else {
        return;
    };
    let Some(WidgetKind::Toggle(w)) = widget_by_label(fields, BYPASS_LABEL) else {
        return;
    };
    // Pre-sync semantics: apply toggle then let the dropdown win. After the
    // schema sync above already wrote the dropdown's value, the toggle here
    // only takes effect when its current state differs from the dropdown.
    // We compare against the current `permission_mode` to preserve the
    // "dropdown last wins" invariant when both are set.
    if w.value && screen.config.sessions.permission_mode != "bypassPermissions" {
        // Toggle ON but dropdown picked something else — keep the dropdown.
    } else if !w.value && screen.config.sessions.permission_mode == "bypassPermissions" {
        // Toggle OFF but dropdown is still bypass — revert to default.
        screen.config.sessions.permission_mode = "default".to_string();
    }
}

/// Theme tab spans `tui.theme` (preset) and `tui` (ascii_icons). Schema
/// sync dispatches by label so order doesn't matter.
fn sync_theme_multi_table(screen: &mut SettingsScreen) {
    let Some(fields) = screen.fields_per_tab.get(7) else {
        return;
    };
    for name in ["tui.theme", "tui"] {
        let Some(table) = schema_for_config().iter().find(|t| t.name == name) else {
            continue;
        };
        if let Err(e) = sync_to_config(table, fields, &mut screen.config) {
            tracing::warn!(table = name, error = %e, "schema sync: theme failed; config left unchanged");
        }
    }
}

/// Preserve legacy `Some("") -> None` collapse for `slack_webhook_url` so
/// code that gates Slack on `Option::is_some()` keeps working.
fn sync_notifications_empty_url_collapse(screen: &mut SettingsScreen) {
    if let Some(url) = screen.config.notifications.slack_webhook_url.as_deref()
        && url.is_empty()
    {
        screen.config.notifications.slack_webhook_url = None;
    }
}

/// Theme tab field 0 is `live_preview` (screen-local, not in schema).
fn sync_theme_screen_local(screen: &mut SettingsScreen) {
    let Some(fields) = screen.fields_per_tab.get(7) else {
        return;
    };
    if let Some(WidgetKind::Toggle(w)) = fields.first().map(|f| &f.widget) {
        screen.live_preview = w.value;
    }
}

/// Budget tab stays hand-coded — Float precision needs F2 (#785).
fn sync_budget_legacy(screen: &mut SettingsScreen) {
    let Some(fields) = screen.fields_per_tab.get(2) else {
        return;
    };
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

/// Advanced tab spans `concurrency` + `monitoring`. The fields are emitted
/// in legacy display order by `advanced::build_fields`, but the schema sync
/// dispatches by label so order doesn't matter for writeback.
fn sync_advanced_multi_table(screen: &mut SettingsScreen) {
    let Some(fields) = screen.fields_per_tab.get(11) else {
        return;
    };
    for name in ["concurrency", "monitoring"] {
        let Some(table) = schema_for_config().iter().find(|t| t.name == name) else {
            continue;
        };
        if let Err(e) = sync_to_config(table, fields, &mut screen.config) {
            tracing::warn!(table = name, error = %e, "schema sync: advanced failed; config left unchanged");
        }
    }
}

/// Caveman toggle (Advanced tab) is bespoke; route through existing
/// `pending_caveman_toggle` flow.
fn sync_advanced_caveman(screen: &mut SettingsScreen) {
    let Some(fields) = screen.fields_per_tab.get(11) else {
        return;
    };
    let prev = screen.caveman_state.as_bool().unwrap_or(false);
    let Some(WidgetKind::Toggle(w)) = widget_by_label(fields, CAVEMAN_LABEL) else {
        return;
    };
    if w.value == prev {
        return;
    }
    if screen.caveman_state.is_toggleable() {
        screen.pending_caveman_toggle = Some(w.value);
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
