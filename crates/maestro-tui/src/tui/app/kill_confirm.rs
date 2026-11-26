//! The kill dialog (#1261, design §05 "Killing the wrong agent"): type the
//! session's issue number before `⏎` kills it.

use std::path::Path;

use super::App;
use super::helpers::session_label;
use crate::tui::ds::confirm::TypedConfirm;

#[cfg(test)]
#[path = "kill_confirm_tests.rs"]
mod tests;

impl App {
    /// Open the kill dialog for `session_id` with an empty field.
    pub fn open_kill_confirm(&mut self, session_id: uuid::Uuid) {
        self.screen_state.kill_confirm = Some((session_id, self.kill_confirm(session_id)));
        self.navigate_to(super::TuiMode::ConfirmKill(session_id));
    }

    /// The dialog for `session_id`. The token is the issue number; a
    /// session with no issue uses the first 8 characters of its id.
    /// What happens to the worktree comes from [`Self::kill_teardown_rows`].
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
        let mut rows = vec![
            (String::new(), "Stops the agent now.".into()),
            (String::new(), String::new()),
        ];
        rows.extend(self.kill_teardown_rows(session_id));
        rows.extend([
            ("kept".into(), "the transcript".into()),
            ("other".into(), other),
        ]);
        TypedConfirm::new(format!("Kill session {label}?"), token, "kill", rows)
    }

    /// #1456: what Kill does to the session's work, read from the real
    /// teardown. A Killed session is finalized on the next tick, and
    /// `pool.finalize` keeps the worktree only for FailedGates and Errored,
    /// so its worktree (`git worktree remove --force`) and local branch
    /// (force-deleted) go, and its uncommitted files with them.
    pub(crate) fn kill_teardown_rows(&self, session_id: uuid::Uuid) -> Vec<(String, String)> {
        let managed = self.pool.get_active(session_id);
        let Some(path) = managed.and_then(|m| m.worktree_path.as_deref()) else {
            return vec![("lost".into(), "the turn in progress".into())];
        };
        let gone = match managed.and_then(|m| m.branch_name.as_deref()) {
            Some(branch) => format!("the worktree and branch {branch}"),
            None => "the worktree and its branch".to_string(),
        };
        let lost = match uncommitted_files(path) {
            Some(0) => "the turn in progress".to_string(),
            Some(1) => "1 uncommitted file, the turn in progress".to_string(),
            Some(n) => format!("{n} uncommitted files, the turn in progress"),
            None => "any uncommitted files, the turn in progress".to_string(),
        };
        vec![("gone".into(), gone), ("lost".into(), lost)]
    }
}

/// How many files the teardown throws away in `worktree`: one per
/// `git status --porcelain --untracked-files=all` line. `None` when git
/// cannot tell (the dialog then says "any").
fn uncommitted_files(worktree: &Path) -> Option<usize> {
    let out = std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(worktree)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).lines().count())
}
