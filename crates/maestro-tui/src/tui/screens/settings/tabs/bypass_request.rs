//! Pending bypass requests in Settings. A bypass choice is not staged
//! until the typed-CONFIRM warning is answered: the Sessions toggle
//! (#1239) and any agent's or mode's Permission Mode dropdown set to
//! `bypassPermissions` (#1411).

use crate::config::Config;
use crate::tui::screens::bypass_warning::BypassWarningState;
use crate::tui::screens::settings::SettingsScreen;
use crate::tui::widgets::{Dropdown, WidgetKind};

use super::{BYPASS_LABEL, bypass_toggle_on};

const BYPASS_MODE: &str = "bypassPermissions";
/// The Sessions toggle's target; the others are `<section>.<id>`.
const SESSIONS: &str = "sessions";

/// Tabs whose dynamic-map entries carry a `permission_mode`, by section:
/// Providers (`[agents.<id>]`) and Modes (`[modes.<id>]`).
const MODE_TABS: [(usize, &str); 2] = [(7, "agents"), (8, "modes")];

/// The entry's own `permission_mode` in `config`.
fn own_mode(config: &Config, section: &str, id: &str) -> Option<String> {
    match section {
        "agents" => config.agent_permission_mode(id),
        _ => (config.modes.get(id)).and_then(|m| m.permission_mode.clone()),
    }
}

fn set_own_mode(config: &mut Config, section: &str, id: &str, mode: Option<String>) {
    match section {
        "agents" => {
            if let Some(agent) = config.agents.entries.get_mut(id) {
                agent.permission_mode = mode;
            }
        }
        _ => {
            if let Some(m) = config.modes.get_mut(id) {
                m.permission_mode = mode;
            }
        }
    }
}

/// Every entry's Permission Mode dropdown on the mode tabs, as
/// `(section, id, value)`.
fn mode_values(screen: &SettingsScreen) -> Vec<(&'static str, String, String)> {
    let mut out = Vec::new();
    for (tab, section) in MODE_TABS {
        let Some(fields) = screen.fields_per_tab.get(tab) else {
            continue;
        };
        for field in fields {
            let WidgetKind::DynamicMap(map) = &field.widget else {
                continue;
            };
            for entry in map.entries() {
                let label = entry.label_for(&map.section_path, "permission_mode");
                let value = entry.fields.iter().find_map(|f| match &f.widget {
                    WidgetKind::Dropdown(d) if d.label == label => Some(d.selected_value()),
                    _ => None,
                });
                if let Some(value) = value {
                    out.push((section, entry.id.clone(), value.to_string()));
                }
            }
        }
    }
    out
}

/// The dropdown of entry `id` on the `section` tab.
fn mode_dropdown<'a>(
    screen: &'a mut SettingsScreen,
    section: &str,
    id: &str,
) -> Option<&'a mut Dropdown> {
    let (tab, _) = MODE_TABS.into_iter().find(|(_, s)| *s == section)?;
    for field in screen.fields_per_tab.get_mut(tab)? {
        let WidgetKind::DynamicMap(map) = &mut field.widget else {
            continue;
        };
        let path = map.section_path.clone();
        let entry = map.entries_mut().iter_mut().find(|e| e.id == id)?;
        let label = entry.label_for(&path, "permission_mode");
        return entry.fields.iter_mut().find_map(|f| match &mut f.widget {
            WidgetKind::Dropdown(d) if d.label == label => Some(d),
            _ => None,
        });
    }
    None
}

impl SettingsScreen {
    /// True when a bypass choice is not yet staged and the typed-CONFIRM
    /// warning has not answered it.
    pub(crate) fn bypass_request_pending(&self) -> bool {
        !self.pending_bypass_targets().is_empty()
    }

    fn sessions_bypass_pending(&self) -> bool {
        bypass_toggle_on(self) == Some(true) && self.config.sessions.permission_mode != BYPASS_MODE
    }

    /// What a pending request would write: `sessions`, `agents.<id>` or
    /// `modes.<id>`. An entry is pending when its dropdown says bypass, the
    /// config as last opened or saved does not (the schema sync copies
    /// widgets into `config` as they change), and no typed CONFIRM in this
    /// flow answered it.
    pub(crate) fn pending_bypass_targets(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.sessions_bypass_pending() {
            out.push(SESSIONS.to_string());
        }
        for (section, id, value) in mode_values(self) {
            let target = format!("{section}.{id}");
            if value == BYPASS_MODE
                && own_mode(&self.original_config, section, &id).as_deref() != Some(BYPASS_MODE)
                && !self.confirmed_bypass.contains(&target)
            {
                out.push(target);
            }
        }
        out
    }

    /// True when `target` is what new sessions of the pool get: the
    /// Sessions toggle or the default agent. A mode or another agent only
    /// reaches its own sessions.
    pub(crate) fn bypass_target_is_pool(&self, target: &str) -> bool {
        target == SESSIONS || target == format!("agents.{}", self.config.agents.default)
    }

    /// Where a staged, non-pool target really applies (#1411). A mode's
    /// `permission_mode` is read when its sessions spawn; another agent's
    /// only when `maestro run --agent <id>` starts.
    pub(crate) fn bypass_target_effect(target: &str) -> String {
        match target.split_once('.') {
            Some(("modes", id)) => format!("sessions in mode {id} after Ctrl+S"),
            Some(("agents", id)) => format!("`maestro run --agent {id}` started after Ctrl+S"),
            _ => "every future session after Ctrl+S".to_string(),
        }
    }

    /// The Settings warning for the pending request. Unlike ⌃B it names
    /// the key Ctrl+S writes to the committed `maestro.toml`.
    pub(crate) fn bypass_warning(&self) -> BypassWarningState {
        let targets = self.pending_bypass_targets();
        let pool = targets.iter().any(|t| self.bypass_target_is_pool(t));
        let keys: Vec<String> = (targets.iter())
            .map(|t| format!("{t}.permission_mode"))
            .collect();
        let (title, scope) = if pool {
            (
                "DANGER: enable bypass for every future session?".to_string(),
                "every future session after Ctrl+S; this run now".to_string(),
            )
        } else {
            let effects: Vec<String> = (targets.iter())
                .map(|t| Self::bypass_target_effect(t))
                .collect();
            (
                format!("DANGER: enable bypass for {}?", targets.join(", ")),
                effects.join("; "),
            )
        };
        let row = |label: &str, text: &str| (label.to_string(), text.to_string());
        BypassWarningState::with_text(
            title,
            vec![
                row("", "Bypass auto-accepts EVERY review concern."),
                row("", ""),
                row("does", "edits files, commits and pushes, no asking"),
                row("writes", &keys.join(", ")),
                row("to", "maestro.toml (committed)"),
                row("scope", &scope),
                row("or", "Ctrl+B turns it on for this run only"),
            ],
        )
    }

    /// Answer a pending bypass request. Confirmed stages the Sessions
    /// value and marks each pending entry as confirmed (its dropdown is
    /// saved by the schema sync). Cancelled turns the toggle back off and
    /// puts each entry's last saved mode back, so nothing is staged.
    pub(crate) fn resolve_bypass_request(&mut self, confirmed: bool) {
        let targets = self.pending_bypass_targets();
        let sessions = targets.iter().any(|t| t == SESSIONS);
        if confirmed {
            if sessions {
                self.config.sessions.permission_mode = BYPASS_MODE.to_string();
            }
            self.confirmed_bypass.extend(targets);
            return;
        }
        for target in targets.iter().filter(|t| *t != SESSIONS) {
            let Some((section, id)) = target.split_once('.') else {
                continue;
            };
            let old = own_mode(&self.original_config, section, id);
            set_own_mode(&mut self.config, section, id, old.clone());
            // An unset mode shows as the schema default, `default`.
            let old = old.unwrap_or_else(|| "default".to_string());
            if let Some(d) = mode_dropdown(self, section, id) {
                d.selected = d.options.iter().position(|o| *o == old).unwrap_or(0);
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
