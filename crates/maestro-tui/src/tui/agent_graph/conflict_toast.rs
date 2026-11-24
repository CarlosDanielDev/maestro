//! The file-conflict toast (§02.20 "Conflict toast", #1292). When
//! `process_file_claim_event` records a conflict it shows the file, who
//! holds the claim and the policy, as a sticky error `ds::toast`. While
//! that toast is the visible one, `p` pauses the agent that does NOT
//! hold the claim (`⌃Z` resumes it) and `⏎` shows the file in Dashboard
//! › Deps › files. Under the `pause` and `kill` policies maestro already
//! acted, so the toast offers only `⏎`.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};
use uuid::Uuid;

use super::model::agent_label;
use crate::config::ConflictPolicy;
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::toast::{Severity, Toast};
use crate::tui::screens::dashboard::usage::View;

/// Widest path in the toast, so its keys stay inside the 72-cell toast.
const PATH_W: usize = 22;

/// A raised conflict toast: its text (to match the visible toast) and
/// what its keys act on.
#[derive(Debug, Clone)]
pub(crate) struct ConflictPrompt {
    message: String,
    file: String,
    /// The agent without the claim; `None` when `p` is not offered.
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
            ConflictPolicy::Warn => format!("p pause {name} · ⏎ show file"),
            ConflictPolicy::Pause => format!("{name} paused (policy) · ⏎ show file"),
            ConflictPolicy::Kill => format!("{name} killed (policy) · ⏎ show file"),
        };
        let message = format!(
            "{} · {who} holds it · {keys}",
            super::ladder::mid(file, PATH_W)
        );
        self.conflict_prompts.push(ConflictPrompt {
            message: message.clone(),
            file: file.to_string(),
            pause: (policy == ConflictPolicy::Warn).then_some(other),
        });
        self.toasts
            .push(Toast::new(Severity::Error, message), Instant::now());
    }

    /// `p` / `⏎` on the visible conflict toast. `false` when the key is
    /// not one of them or no conflict toast is visible. The caller skips
    /// it while a text field has focus.
    pub(crate) fn conflict_toast_key(&mut self, key: &KeyEvent) -> bool {
        if !key.modifiers.is_empty() || self.copy_toast.is_some() {
            return false;
        }
        let now = Instant::now();
        let Some(visible) = self.toasts.visible(now) else {
            return false;
        };
        let Some(at) = (self.conflict_prompts.iter()).position(|p| p.message == visible.message)
        else {
            return false;
        };
        let prompt = &self.conflict_prompts[at];
        match key.code {
            KeyCode::Char('p') if prompt.pause.is_some() => {
                let Some(id) = prompt.pause else {
                    return false;
                };
                self.conflict_prompts.remove(at);
                let _ = self.toasts.dismiss_error(now);
                let name = self.agent_name(id);
                let toast = if self.pause_session(id) {
                    Toast::undoable(
                        format!("paused {name}"),
                        Box::new(move |app: &mut App| app.resume_session(id)) as _,
                    )
                } else {
                    Toast::new(Severity::Warn, format!("{name} has no process to pause"))
                };
                self.toasts.push(toast, now);
            }
            KeyCode::Enter => {
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
