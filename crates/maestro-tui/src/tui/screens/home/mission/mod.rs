//! Home: Mission Control (`docs/design-system/index.html` §02, #1264).
//!
//! Replaces the Release Console as the Home screen (`TuiMode::Landing`):
//! Needs you (focused on open), the Fleet table, Actions grouped
//! RUN · PLAN · INSIGHT, Budget & quota and the Milestone card, laid out
//! for Wide, Regular and Compact. The Activity pane under it is the shell's
//! (`ui::draw`). Everything drawn comes from a plain [`MissionView`], built
//! by `App::mission_view`; keys go through [`MissionControl::handle_key`].

pub mod dispatch;
mod draw;

pub use draw::draw;

use chrono::{DateTime, Utc};
use crossterm::event::KeyCode;

use crate::tui::app::TuiMode;
use crate::tui::ds::attention::AttentionItem;
use crate::tui::ds::fleet_table::FleetRow;
use crate::tui::screens::landing::{LandingTarget, MENU_ITEMS};

/// The Actions groups, by shortcut. Labels and targets come from the
/// landing `MENU_ITEMS`, so the letters stay the same; Quit is not here
/// (it is a global key, not a peer of navigation).
pub const ACTION_GROUPS: &[(&str, &[char])] = &[
    ("RUN", &['d', 'f', 'a']),
    ("PLAN", &['i', 'm', 'r', 'p', 'h']),
    ("INSIGHT", &['s', 't']),
];

/// One action as drawn and run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeAction {
    pub key: char,
    pub label: &'static str,
    pub mode: TuiMode,
}

/// Every action in drawn order (groups flattened).
pub fn actions() -> Vec<HomeAction> {
    ACTION_GROUPS
        .iter()
        .flat_map(|(_, keys)| keys.iter())
        .filter_map(|key| {
            let item = MENU_ITEMS.iter().find(|i| i.shortcut == *key)?;
            let LandingTarget::Push(mode) = item.target;
            Some(HomeAction {
                key: *key,
                label: item.label,
                mode,
            })
        })
        .collect()
}

/// What decides whether an action can run now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionGate {
    pub gh_auth_ok: bool,
    /// `Some(false)` once project stats loaded with no open milestone;
    /// `None` while they load.
    pub has_milestone: Option<bool>,
}

impl Default for ActionGate {
    fn default() -> Self {
        Self {
            gh_auth_ok: true,
            has_milestone: None,
        }
    }
}

/// Why `key` cannot run now, or `None` when it can.
pub fn disabled_reason(key: char, gate: &ActionGate) -> Option<&'static str> {
    match key {
        'i' | 'm' | 'h' if !gate.gh_auth_ok => Some("gh signed out"),
        'h' if gate.has_milestone == Some(false) => Some("no milestones yet"),
        _ => None,
    }
}

/// One gauge row of the Budget & quota panel.
#[derive(Debug, Clone, PartialEq)]
pub struct QuotaRow {
    pub label: String,
    /// Used / limit; `None` draws no bar (no cap set).
    pub ratio: Option<f64>,
    pub value: String,
}

/// The Milestone card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MilestoneCard {
    Loading,
    NoMilestone,
    Active {
        title: String,
        closed: u32,
        total: u32,
    },
}

/// Everything Home draws, as plain data.
#[derive(Debug, Clone)]
pub struct MissionView {
    pub attention: Vec<AttentionItem>,
    pub fleet: Vec<FleetRow>,
    /// Some agent is still working (picks the Needs-you empty copy).
    pub fleet_busy: bool,
    /// Session spend first, then each quota window.
    pub budget: Vec<QuotaRow>,
    pub milestone: MilestoneCard,
    pub gate: ActionGate,
    pub now: DateTime<Utc>,
}

/// The panel that takes `↑↓` and `⏎`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HomeFocus {
    #[default]
    NeedsYou,
    Actions,
}

/// What a key asks the dispatcher to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionCommand {
    None,
    Push(TuiMode),
    /// Run the `⏎` action of this Needs-you row.
    OpenAttention(usize),
}

/// Home's own state: focus and the two selections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionControl {
    pub focus: HomeFocus,
    pub attention_selected: usize,
    pub action_selected: usize,
    /// Set on every draw: Compact has no Actions panel, so `Tab` stays on
    /// Needs you there (the letters still run actions).
    pub actions_visible: bool,
}

impl Default for MissionControl {
    fn default() -> Self {
        Self {
            focus: HomeFocus::NeedsYou,
            attention_selected: 0,
            action_selected: 0,
            actions_visible: true,
        }
    }
}

impl MissionControl {
    pub fn handle_key(
        &mut self,
        code: KeyCode,
        attention_len: usize,
        gate: &ActionGate,
    ) -> MissionCommand {
        let actions = actions();
        match code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    HomeFocus::NeedsYou if self.actions_visible => HomeFocus::Actions,
                    _ => HomeFocus::NeedsYou,
                };
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let (sel, len) = self.selection(attention_len, actions.len());
                if *sel + 1 < len {
                    *sel += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let (sel, _) = self.selection(attention_len, actions.len());
                *sel = sel.saturating_sub(1);
            }
            KeyCode::Enter => match self.focus {
                HomeFocus::NeedsYou if self.attention_selected < attention_len => {
                    return MissionCommand::OpenAttention(self.attention_selected);
                }
                HomeFocus::NeedsYou => {}
                HomeFocus::Actions => {
                    if let Some(a) = actions.get(self.action_selected) {
                        return run(a, gate);
                    }
                }
            },
            KeyCode::Char('n') => return MissionCommand::Push(TuiMode::ReleaseNotes),
            KeyCode::Char(c) => {
                if let Some(a) = actions.iter().find(|a| a.key == c) {
                    return run(a, gate);
                }
            }
            _ => {}
        }
        MissionCommand::None
    }

    /// The focused list's selection and its length.
    fn selection(&mut self, attention_len: usize, actions_len: usize) -> (&mut usize, usize) {
        match self.focus {
            HomeFocus::NeedsYou => (&mut self.attention_selected, attention_len),
            HomeFocus::Actions => (&mut self.action_selected, actions_len),
        }
    }
}

/// A disabled action does nothing; it already says why on screen.
fn run(action: &HomeAction, gate: &ActionGate) -> MissionCommand {
    if disabled_reason(action.key, gate).is_some() {
        MissionCommand::None
    } else {
        MissionCommand::Push(action.mode)
    }
}

#[cfg(test)]
mod tests;
