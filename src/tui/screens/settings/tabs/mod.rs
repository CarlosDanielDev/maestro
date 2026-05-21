pub mod advanced;
pub mod agents;
pub mod budget;
pub mod flags;
pub mod gates;
pub mod github;
pub mod layout;
pub mod modes;
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

/// Look up a schema table by name. Panics with a uniform message if the
/// `const SCHEMA` registry does not contain `name` — a programmer error
/// caught at SettingsScreen::new time, never at runtime from user input.
pub(super) fn schema_table(name: &'static str) -> &'static TableSchema {
    schema_for_config()
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("{name} schema must exist in const SCHEMA registry"))
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
        agents::build_fields(config),
        modes::build_fields(config),
        theme::build_fields(config),
        layout::build_fields(config),
        vec![],
        turboquant::build_fields(config),
        advanced::build_fields(config),
    ]
}

/// Map a settings tab index to the schema table that drives it. `None`
/// means the tab has no widgets (Flags, idx 11) or spans more than one
/// TOML table — Theme (idx 9) and Advanced (idx 13) are handled by
/// `sync_multi_table` below.
fn schema_table_for_tab(idx: usize) -> Option<&'static TableSchema> {
    let name = match idx {
        0 => "project",
        1 => "sessions",
        2 => "budget",
        3 => "github",
        4 => "notifications",
        5 => "gates",
        6 => "review",
        7 => "agents",
        8 => "modes",
        10 => "tui.layout",
        12 => "turboquant",
        _ => return None,
    };
    Some(schema_table(name))
}

pub(super) fn sync_widgets_to_config(screen: &mut SettingsScreen) {
    sync_schema_tabs(screen);
    sync_sessions_bypass_override(screen);
    sync_multi_table(9, &["tui.theme", "tui"], screen);
    sync_theme_screen_local(screen);
    sync_notifications_empty_url_collapse(screen);
    sync_multi_table(13, &["concurrency", "monitoring"], screen);
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

/// Sessions bypass toggle is the sole UI surface for
/// `sessions.permission_mode` after the schema-driven dropdown was
/// removed (per-provider permission_mode lives on the Providers tab).
/// Toggle ON → "bypassPermissions"; toggle OFF → "default" (only when
/// the current value is the bypass sentinel, so a non-default
/// permission_mode set via TOML hand-edit is preserved on a toggle
/// that's already off).
fn sync_sessions_bypass_override(screen: &mut SettingsScreen) {
    let Some(fields) = screen.fields_per_tab.get(1) else {
        return;
    };
    let Some(WidgetKind::Toggle(w)) = widget_by_label(fields, BYPASS_LABEL) else {
        return;
    };
    if w.value {
        screen.config.sessions.permission_mode = "bypassPermissions".to_string();
    } else if screen.config.sessions.permission_mode == "bypassPermissions" {
        screen.config.sessions.permission_mode = "default".to_string();
    }
}

/// Sync a tab whose widgets span multiple TOML tables (Theme spans
/// `tui.theme` + `tui`; Advanced spans `concurrency` + `monitoring`).
/// `sync_to_config` dispatches by label so the order of names does not
/// affect correctness — order matters only for the matching-tab snapshot.
fn sync_multi_table(tab_idx: usize, table_names: &[&'static str], screen: &mut SettingsScreen) {
    let Some(fields) = screen.fields_per_tab.get(tab_idx) else {
        return;
    };
    for name in table_names {
        let table = schema_table(name);
        if let Err(e) = sync_to_config(table, fields, &mut screen.config) {
            tracing::warn!(table = name, tab = tab_idx, error = %e, "schema sync: multi-table failed; config left unchanged");
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
    let Some(fields) = screen.fields_per_tab.get(9) else {
        return;
    };
    if let Some(WidgetKind::Toggle(w)) = fields.first().map(|f| &f.widget) {
        screen.live_preview = w.value;
    }
}

/// Caveman toggle (Advanced tab) is bespoke; route through existing
/// `pending_caveman_toggle` flow.
fn sync_advanced_caveman(screen: &mut SettingsScreen) {
    let Some(fields) = screen.fields_per_tab.get(13) else {
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
