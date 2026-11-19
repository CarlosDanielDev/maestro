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
pub mod teams;
pub mod theme;
pub mod turboquant;

use crate::config::Config;
use crate::config::schema::{TableSchema, schema_for_config};
use crate::tui::screens::settings::schema_tab::sync::sync_to_config;
use crate::tui::widgets::{Dropdown, WidgetKind};

use super::{CAVEMAN_LABEL, SettingsField, SettingsScreen, widget_by_label};

fn field(widget: WidgetKind) -> SettingsField {
    SettingsField { widget }
}

pub(super) const BYPASS_LABEL: &str =
    "bypass_review_corrections (DANGER: auto-accepts all review fixes)";

/// Label of the Providers-tab Dropdown that mirrors `agents.default`.
/// Lives at index 0 of the Providers tab, above the DynamicMap entries.
pub(super) const DEFAULT_PROVIDER_LABEL: &str = "Default provider";

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
        project::build_fields(config),       // 0
        sessions::build_fields(config),      // 1
        budget::build_fields(config),        // 2
        github::build_fields(config),        // 3
        notifications::build_fields(config), // 4
        gates::build_fields(config),         // 5
        review::build_fields(config),        // 6
        agents::build_fields(config),        // 7
        modes::build_fields(config),         // 8
        teams::build_fields(config),         // 9  (#803)
        theme::build_fields(config),         // 10
        layout::build_fields(config),        // 11
        vec![],                              // 12 (Flags — no widgets)
        turboquant::build_fields(config),    // 13
        advanced::build_fields(config),      // 14
    ]
}

/// Map a settings tab index to the schema table that drives it. `None`
/// means the tab has no widgets (Flags, idx 12) or spans more than one
/// TOML table — Theme (idx 10) and Advanced (idx 14) are handled by
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
        9 => "teams",
        11 => "tui.layout",
        13 => "turboquant",
        _ => return None,
    };
    Some(schema_table(name))
}

pub(super) fn sync_widgets_to_config(screen: &mut SettingsScreen) {
    sync_schema_tabs(screen);
    sync_sessions_bypass_override(screen);
    sync_agents_default_override(screen);
    sync_multi_table(10, &["tui.theme", "tui"], screen);
    sync_theme_screen_local(screen);
    sync_notifications_empty_url_collapse(screen);
    sync_multi_table(14, &["concurrency", "monitoring"], screen);
    sync_advanced_caveman(screen);
}

/// Mirror the Providers-tab Default-provider dropdown back into
/// `config.agents.default`. The dropdown is injected at idx 0 of the
/// tab by `tabs/agents.rs::build_fields` and its options are the
/// currently-configured entry ids.
fn sync_agents_default_override(screen: &mut SettingsScreen) {
    let Some(fields) = screen.fields_per_tab.get(7) else {
        return;
    };
    let Some(WidgetKind::Dropdown(d)) = widget_by_label(fields, DEFAULT_PROVIDER_LABEL) else {
        return;
    };
    screen.config.agents.default = d.selected_value().to_string();
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
/// Toggle ON is NOT staged here (#1239): it stays pending until the
/// bypass warning is confirmed, see `resolve_bypass_request`. Toggle OFF
/// → "default" (only when the current value is the bypass sentinel, so a
/// non-default permission_mode set via TOML hand-edit is preserved on a
/// toggle that's already off).
fn sync_sessions_bypass_override(screen: &mut SettingsScreen) {
    if bypass_toggle_on(screen) == Some(false)
        && screen.config.sessions.permission_mode == "bypassPermissions"
    {
        screen.config.sessions.permission_mode = "default".to_string();
    }
}

fn bypass_toggle_on(screen: &SettingsScreen) -> Option<bool> {
    let fields = screen.fields_per_tab.get(1)?;
    match widget_by_label(fields, BYPASS_LABEL)? {
        WidgetKind::Toggle(w) => Some(w.value),
        _ => None,
    }
}

/// Providers tab (`[agents]`), see `build_fields`.
const AGENTS_TAB: usize = 7;
const BYPASS_MODE: &str = "bypassPermissions";

/// Every agent's Permission Mode dropdown on the Providers tab, by id.
fn agent_mode_dropdowns(screen: &mut SettingsScreen) -> Vec<(String, &mut Dropdown)> {
    let mut out = Vec::new();
    let Some(fields) = screen.fields_per_tab.get_mut(AGENTS_TAB) else {
        return out;
    };
    for field in fields {
        let WidgetKind::DynamicMap(map) = &mut field.widget else {
            continue;
        };
        let section = map.section_path.clone();
        for entry in map.entries_mut() {
            let label = entry.label_for(&section, "permission_mode");
            let id = entry.id.clone();
            let dropdown = (entry.fields.iter_mut()).find_map(|f| match &mut f.widget {
                WidgetKind::Dropdown(d) if d.label == label => Some(d),
                _ => None,
            });
            out.extend(dropdown.map(|d| (id, d)));
        }
    }
    out
}

impl SettingsScreen {
    /// True when a bypass choice is not yet staged: the Sessions toggle
    /// was flipped on (#1239), or an agent's Permission Mode was set to
    /// `bypassPermissions` (#1411), and the typed-CONFIRM warning has not
    /// been answered.
    pub(crate) fn bypass_request_pending(&self) -> bool {
        self.sessions_bypass_pending() || !self.agent_bypass_pending_ids().is_empty()
    }

    fn sessions_bypass_pending(&self) -> bool {
        bypass_toggle_on(self) == Some(true) && self.config.sessions.permission_mode != BYPASS_MODE
    }

    /// Agents whose dropdown says `bypassPermissions` while the config
    /// does not stage it and no typed CONFIRM answered it in this flow.
    fn agent_bypass_pending_ids(&self) -> Vec<String> {
        let Some(fields) = self.fields_per_tab.get(AGENTS_TAB) else {
            return Vec::new();
        };
        let maps = fields.iter().filter_map(|f| match &f.widget {
            WidgetKind::DynamicMap(map) => Some(map),
            _ => None,
        });
        let mut out = Vec::new();
        for map in maps {
            for entry in map.entries() {
                let label = entry.label_for(&map.section_path, "permission_mode");
                let chosen = entry.fields.iter().any(|f| {
                    matches!(&f.widget, WidgetKind::Dropdown(d)
                        if d.label == label && d.selected_value() == BYPASS_MODE)
                });
                // The schema sync copies widgets into `config` as they
                // change, so the baseline is the config as last opened or
                // saved.
                let staged = self.original_config.agent_permission_mode(&entry.id);
                if chosen
                    && staged.as_deref() != Some(BYPASS_MODE)
                    && !self.confirmed_agent_bypass.contains(&entry.id)
                {
                    out.push(entry.id.clone());
                }
            }
        }
        out
    }

    /// Answer a pending bypass request. Confirmed stages the Sessions
    /// `permission_mode = "bypassPermissions"` and marks each pending
    /// agent as confirmed (its dropdown is saved by the schema sync).
    /// Cancelled turns the toggle back off and puts each agent's staged
    /// mode back in its dropdown, so nothing is staged.
    pub(crate) fn resolve_bypass_request(&mut self, confirmed: bool) {
        let sessions = self.sessions_bypass_pending();
        let agents = self.agent_bypass_pending_ids();
        if confirmed {
            if sessions {
                self.config.sessions.permission_mode = BYPASS_MODE.to_string();
            }
            self.confirmed_agent_bypass.extend(agents);
            return;
        }
        let mut old_modes = Vec::new();
        for id in agents {
            let old = (self.original_config.agents.entries.get(&id))
                .and_then(|a| a.permission_mode.clone());
            if let Some(agent) = self.config.agents.entries.get_mut(&id) {
                agent.permission_mode = old.clone();
            }
            // An unset mode shows as the schema default, `default`.
            old_modes.push((id, old.unwrap_or_else(|| "default".to_string())));
        }
        for (id, d) in agent_mode_dropdowns(self) {
            if let Some((_, old)) = old_modes.iter().find(|(pending, _)| *pending == id) {
                d.selected = d.options.iter().position(|o| o == old).unwrap_or(0);
            }
        }
        if !sessions {
            return;
        }
        let Some(fields) = self.fields_per_tab.get_mut(1) else {
            return;
        };
        if let Some(WidgetKind::Toggle(w)) = fields
            .iter_mut()
            .map(|f| &mut f.widget)
            .find(|w| w.label() == BYPASS_LABEL)
        {
            w.value = false;
        }
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
    let Some(fields) = screen.fields_per_tab.get(10) else {
        return;
    };
    if let Some(WidgetKind::Toggle(w)) = fields.first().map(|f| &f.widget) {
        screen.live_preview = w.value;
    }
}

/// Caveman toggle (Advanced tab) is bespoke; route through existing
/// `pending_caveman_toggle` flow.
fn sync_advanced_caveman(screen: &mut SettingsScreen) {
    let Some(fields) = screen.fields_per_tab.get(14) else {
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
