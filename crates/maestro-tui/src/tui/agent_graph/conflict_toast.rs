//! The file-conflict toast (§02.20 "Conflict toast", #1292). The first
//! time `process_file_claim_event` records a conflict for a (file, agent)
//! pair it shows the file, who holds the claim and the policy, as a sticky
//! error `ds::toast`. While that toast is the visible one on a top-level
//! tab screen (no modal, no text field), `⌃P` pauses the agent that does
//! NOT hold the claim (`⌃Z` resumes it) and `⌃O` shows the file in
//! Dashboard › Deps › files. The keys are scoped to the tab screens and
//! shown in their footer only there ([`footer`]); the toast text names no
//! key, because it is drawn on every screen (`⌃P` is Interaction's pushup).
//! Under the `pause` and `kill` policies maestro already acted, so only
//! `⌃O` is offered.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::navigation::keymap::InlineHint;
use uuid::Uuid;

use super::model::agent_label;
use crate::config::ConflictPolicy;
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::toast::{Severity, Toast};
use crate::tui::screens::dashboard::usage::View;

/// Widest path in the toast, so its keys stay inside the 72-cell toast.
const PATH_W: usize = 22;
/// Prompts kept: the visible toast and the errors `ds::toast` queues.
const MAX_PROMPTS: usize = 6;

/// A raised conflict toast: its text (to match the visible toast) and
/// what its keys act on.
#[derive(Debug, Clone)]
pub(crate) struct ConflictPrompt {
    message: String,
    file: String,
    /// The agent without the claim; `None` when `⌃P` is not offered.
    pause: Option<Uuid>,
}

impl App {
    fn agent_name(&self, id: Uuid) -> String {
        self.pool
            .get_session(id)
            .map_or_else(|| format!("S-{}", &id.to_string()[..4]), agent_label)
    }

    /// Show the toast for a conflict on `file` (repo-relative).
    pub(crate) fn raise_conflict_toast(
        &mut self,
        file: &str,
        holder: Uuid,
        other: Uuid,
        policy: ConflictPolicy,
    ) {
        let (who, name) = (self.agent_name(holder), self.agent_name(other));
        let keys = match policy {
            ConflictPolicy::Warn => format!("{name} edits it too"),
            ConflictPolicy::Pause => format!("{name} paused (policy)"),
            ConflictPolicy::Kill => format!("{name} killed (policy)"),
        };
        let message = format!(
            "{} held by {who} · {keys}",
            super::ladder::mid(file, PATH_W)
        );
        if self.conflict_prompts.len() >= MAX_PROMPTS {
            self.conflict_prompts.remove(0);
        }
        self.conflict_prompts.push(ConflictPrompt {
            message: message.clone(),
            file: file.to_string(),
            pause: (policy == ConflictPolicy::Warn).then_some(other),
        });
        self.toasts
            .push(Toast::new(Severity::Error, message), Instant::now());
    }

    /// The prompt whose keys act now: its toast is the visible one, the
    /// screen is a top-level tab (no modal or other screen owns the keys)
    /// and no text field has focus (`typing`).
    fn acting_conflict_prompt(&self, typing: bool, now: Instant) -> Option<usize> {
        let tab_screen = crate::tui::ds::tab_bar::tab_index(self.tui_mode).is_some();
        if typing || !tab_screen || self.copy_toast.is_some() {
            return None;
        }
        let visible = self.toasts.visible(now)?;
        (self.conflict_prompts.iter()).position(|p| p.message == visible.message)
    }

    /// `Esc` dismissed the toast showing `message`: forget its prompt.
    pub(crate) fn forget_conflict_prompt(&mut self, message: &str) {
        self.conflict_prompts.retain(|p| p.message != message);
    }

    /// `⌃P` / `⌃O` on the visible conflict toast. `false` (the key is not
    /// used) when it is neither, no conflict toast is visible, the screen
    /// is not a top-level tab (a modal or another screen owns the keys),
    /// or `typing`: a text field has focus (`is_text_input_mode`).
    pub(crate) fn conflict_toast_key(&mut self, key: &KeyEvent, typing: bool) -> bool {
        if key.modifiers != KeyModifiers::CONTROL {
            return false;
        }
        let now = Instant::now();
        let Some(at) = self.acting_conflict_prompt(typing, now) else {
            return false;
        };
        match key.code {
            KeyCode::Char('p') => {
                let Some(id) = self.conflict_prompts[at].pause else {
                    return false;
                };
                self.conflict_prompts.remove(at);
                let _ = self.toasts.dismiss_error(now);
                let name = self.agent_name(id);
                let toast = match self.pause_session(id) {
                    Ok(()) => Toast::undoable(
                        format!("paused {name}"),
                        Box::new(move |app: &mut App| app.resume_session(id)) as _,
                    ),
                    Err(why) => Toast::new(Severity::Warn, format!("{name} not paused: {why}")),
                };
                self.toasts.push(toast, now);
            }
            KeyCode::Char('o') => {
                let file = self.conflict_prompts.remove(at).file;
                let _ = self.toasts.dismiss_error(now);
                self.navigate_to(TuiMode::Dashboard);
                let dash = &mut self.screen_state.dashboard;
                dash.view = View::Deps;
                dash.deps.show_file(&file);
            }
            _ => return false,
        }
        true
    }
}

/// The toast's keys, first in the footer, only where they act now.
pub(crate) fn footer(app: &App, hints: &mut Vec<InlineHint>) {
    let typing = crate::tui::input_handler::is_text_input_mode(app);
    let Some(at) = app.acting_conflict_prompt(typing, Instant::now()) else {
        return;
    };
    let mut keys = Vec::new();
    if app.conflict_prompts[at].pause.is_some() {
        keys.push(("⌃P", "pause other agent"));
    }
    keys.push(("⌃O", "show conflict file"));
    let keys = keys.into_iter().map(|(key, action)| InlineHint {
        key,
        action,
        priority: 0,
    });
    hints.splice(0..0, keys);
}
