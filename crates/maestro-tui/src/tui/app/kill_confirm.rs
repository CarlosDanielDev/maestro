//! The kill dialog (#1261, design §05 "Killing the wrong agent"): type the
//! session's issue number before `⏎` kills it.

use std::path::Path;

use super::App;
use super::helpers::session_label;
use crate::session::pool::session_slug;
use crate::session::types::SessionStatus;
use crate::tui::ds::confirm::TypedConfirm;
use crate::tui::ds::toast::{Severity, Toast};

#[cfg(test)]
#[path = "kill_confirm_tests.rs"]
mod tests;

impl App {
    /// Open the kill dialog for `session_id` with an empty field. The
    /// uncommitted files are counted here, once, never per frame.
    /// A session Kill cannot end (Errored, GatesRunning, NeedsPr: no
    /// `Killed` transition) gets a toast instead: Kill would do nothing.
    pub fn open_kill_confirm(&mut self, session_id: uuid::Uuid) {
        if let Some(status) = self.unkillable_status(session_id) {
            let label = (self.pool.get_session(session_id))
                .map_or_else(|| "The session".to_string(), session_label);
            let text = format!(
                "{label} is {}: Kill cannot stop it, nothing changes",
                status.label().to_lowercase()
            );
            let toast = Toast::new(Severity::Info, text);
            self.push_toast(toast, std::time::Instant::now());
            return;
        }
        let uncommitted = self.uncommitted_files(session_id);
        let confirm = self.kill_confirm_with(session_id, uncommitted);
        self.screen_state.kill_confirm = Some((session_id, confirm));
        self.navigate_to(super::TuiMode::ConfirmKill(session_id));
    }

    /// The dialog for `session_id` without running git (the draw fallback):
    /// the uncommitted count reads as unknown, and still warns.
    pub fn kill_confirm(&self, session_id: uuid::Uuid) -> TypedConfirm {
        self.kill_confirm_with(session_id, None)
    }

    /// The dialog for `session_id`. The token is the issue number; a
    /// session with no issue uses the first 8 characters of its id.
    /// What happens to the worktree comes from [`Self::kill_teardown_rows`].
    fn kill_confirm_with(
        &self,
        session_id: uuid::Uuid,
        uncommitted: Option<usize>,
    ) -> TypedConfirm {
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
        rows.extend(self.kill_teardown_rows(session_id, uncommitted));
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
    /// `uncommitted` is [`Self::uncommitted_files`]; `None` reads "count
    /// unknown" and never "nothing lost".
    pub(crate) fn kill_teardown_rows(
        &self,
        session_id: uuid::Uuid,
        uncommitted: Option<usize>,
    ) -> Vec<(String, String)> {
        if let Some(status) = self.unkillable_status(session_id) {
            let text = format!(
                "it is {}: Kill leaves it as it is",
                status.label().to_lowercase()
            );
            return vec![("skips".into(), text)];
        }
        let managed = self.pool.get_active(session_id);
        if self.torn_down_worktree(session_id).is_none() {
            return vec![("lost".into(), "the turn in progress".into())];
        }
        let gone = match managed.and_then(|m| m.branch_name.as_deref()) {
            Some(branch) => format!("the worktree and branch {branch}"),
            None => "the worktree and its branch".to_string(),
        };
        let lost = match uncommitted {
            Some(0) => "the turn in progress".to_string(),
            Some(1) => "1 uncommitted file, the turn in progress".to_string(),
            Some(n) => format!("{n} uncommitted files, the turn in progress"),
            None => "uncommitted files (count unknown), turn in progress".to_string(),
        };
        vec![("gone".into(), gone), ("lost".into(), lost)]
    }

    /// How many uncommitted files Kill throws away in `session_id`'s
    /// worktree. Runs git: call it once, when a dialog opens.
    pub(crate) fn uncommitted_files(&self, session_id: uuid::Uuid) -> Option<usize> {
        count_uncommitted(self.torn_down_worktree(session_id)?)
    }

    /// The session's worktree when it is the one teardown removes:
    /// `wipe_worktree` acts on `.maestro/worktrees/<session_slug>` only. A
    /// session in cwd (`None`, or `.` for an interaction whose worktree
    /// failed: the main checkout) loses no worktree and no files to Kill.
    fn torn_down_worktree(&self, session_id: uuid::Uuid) -> Option<&Path> {
        let managed = self.pool.get_active(session_id)?;
        let path = managed.worktree_path.as_deref()?;
        let slug = session_slug(&managed.session);
        path.ends_with(Path::new(".maestro").join("worktrees").join(slug))
            .then_some(path)
    }

    /// The status of a session Kill cannot move to `Killed`, if it is one.
    fn unkillable_status(&self, session_id: uuid::Uuid) -> Option<SessionStatus> {
        let status = self.pool.get_session(session_id)?.status;
        (!status.can_transition_to(SessionStatus::Killed)).then_some(status)
    }
}

/// One per `git status --porcelain --untracked-files=all` line. `None` when
/// git cannot tell (the path is gone, not a repo): the dialog says unknown.
fn count_uncommitted(worktree: &Path) -> Option<usize> {
    let out = std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(worktree)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).lines().count())
}
