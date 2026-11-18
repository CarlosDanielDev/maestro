//! The kill dialog (#1261, design §05 "Killing the wrong agent"): type the
//! session's issue number before `⏎` kills it.

use super::App;
use super::helpers::session_label;
use crate::tui::ds::confirm::TypedConfirm;

impl App {
    /// Open the kill dialog for `session_id` with an empty field.
    pub fn open_kill_confirm(&mut self, session_id: uuid::Uuid) {
        self.screen_state.kill_confirm = Some((session_id, self.kill_confirm(session_id)));
        self.navigate_to(super::TuiMode::ConfirmKill(session_id));
    }

    /// The dialog for `session_id`. The token is the issue number; a
    /// session with no issue uses the first 8 characters of its id.
    /// `pool.kill_session` only stops the process, so the worktree, branch
    /// and transcript are listed as kept.
    pub fn kill_confirm(&self, session_id: uuid::Uuid) -> TypedConfirm {
        let short = session_id.to_string()[..8].to_string();
        let session = self.pool.get_session(session_id);
        let label = session
            .map(session_label)
            .unwrap_or_else(|| format!("S-{short}"));
        let token = session
            .and_then(|s| s.issue_number)
            .map_or(short, |n| n.to_string());
        let others = self
            .pool
            .all_sessions()
            .iter()
            .filter(|s| s.id != session_id && !s.status.is_terminal())
            .count();
        let other = match others {
            0 => "no other session is running".to_string(),
            1 => "1 other session keeps running".to_string(),
            n => format!("{n} other sessions keep running"),
        };
        TypedConfirm::new(
            format!("Kill session {label}?"),
            token,
            "kill",
            vec![
                (String::new(), "Stops the agent now.".into()),
                (String::new(), String::new()),
                (
                    "kept".into(),
                    "the worktree, its branch and the transcript".into(),
                ),
                ("lost".into(), "the turn in progress".into()),
                ("other".into(), other),
            ],
        )
    }
}
