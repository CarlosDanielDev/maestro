//! Per-completion git orchestration. Owns the WIP backup commit that
//! runs BEFORE post-completion gates so model edits survive any
//! subsequent failure (gate failure, crash, manual `rm -rf`), and the
//! post-gate amend that replaces the WIP with a clean conventional
//! commit on success.
//!
//! All git work routes through `App::git_ops` (the `GitOps` trait
//! seam) so tests can inject `MockGitOps` via `App::with_git_ops`.

use super::App;
use crate::tui::activity_log::LogLevel;
use crate::util::sanitize::sanitize_log;
use std::path::Path;

#[derive(Debug)]
pub(super) struct CompletionGitOutcome {
    pub committed_clean: bool,
    pub error_message: Option<String>,
}

impl App {
    /// Pre-gate WIP backup. Runs unconditionally when a worktree is
    /// present. If HEAD is already a WIP backup from a prior run
    /// (resume path), this is a no-op — the post-gate amend will
    /// rewrite the existing WIP with a clean message.
    ///
    /// Failures are logged at `Warn` and never propagated: gates must
    /// run regardless of backup success.
    ///
    /// Returns whether HEAD is a WIP backup commit after this call —
    /// the pipeline threads the answer into `amend_or_commit_and_push`
    /// so the post-gate step doesn't re-run detection.
    pub(super) fn backup_wip_before_gates(
        &mut self,
        issue_number: u64,
        worktree_path: &Path,
    ) -> bool {
        let head_is_wip = match self.git_ops.head_is_wip_backup(worktree_path) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    issue = issue_number,
                    error = %e,
                    "WIP detection failed; proceeding to backup",
                );
                false
            }
        };

        if head_is_wip {
            tracing::info!(
                issue = issue_number,
                "HEAD is WIP backup from prior run; skipping new WIP commit"
            );
            self.activity_log.push_simple(
                format!("#{}", issue_number),
                "Resuming with existing WIP backup commit".into(),
                LogLevel::Info,
            );
            return true;
        }

        match self.git_ops.backup_wip(worktree_path, issue_number) {
            Ok(()) => {
                tracing::info!(issue = issue_number, "WIP backup commit created");
                self.activity_log.push_simple(
                    format!("#{}", issue_number),
                    "WIP backup commit created before gates".into(),
                    LogLevel::Info,
                );
                true
            }
            Err(e) => {
                tracing::warn!(issue = issue_number, error = %e, "WIP backup failed");
                self.activity_log.push_simple(
                    format!("#{}", issue_number),
                    format!(
                        "WIP backup commit failed: {} (gates will run anyway)",
                        sanitize_log(&e.to_string())
                    ),
                    LogLevel::Warn,
                );
                false
            }
        }
    }

    /// Post-gate-success git step. When `head_is_wip` is true, replaces
    /// the WIP commit at HEAD with a clean conventional-commit message
    /// and pushes. When false (because `backup_wip` failed earlier),
    /// falls back to the legacy `commit_and_push` path so we never
    /// silently drop the model's work.
    ///
    /// `head_is_wip` comes from the return of
    /// `backup_wip_before_gates` — passing it in avoids a redundant
    /// `head_is_wip_backup` subprocess pair.
    pub(super) fn amend_or_commit_and_push(
        &self,
        worktree_path: &Path,
        branch: &str,
        message: &str,
        head_is_wip: bool,
    ) -> CompletionGitOutcome {
        let result = if head_is_wip {
            self.git_ops
                .amend_clean_and_push(worktree_path, branch, message)
        } else {
            self.git_ops.commit_and_push(worktree_path, branch, message)
        };

        match result {
            Ok(()) => CompletionGitOutcome {
                committed_clean: true,
                error_message: None,
            },
            Err(e) => CompletionGitOutcome {
                committed_clean: false,
                error_message: Some(sanitize_log(&e.to_string())),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::git::MockGitOps;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn backup_wip_before_gates_calls_backup_wip_when_head_is_not_wip() {
        let ops = MockGitOps::new();
        let backup_calls = Arc::clone(&ops.backup_wip_calls);
        let mut app = crate::tui::make_test_app("cg-test-1").with_git_ops(Box::new(ops));

        let head_is_wip = app.backup_wip_before_gates(562, &PathBuf::from("/tmp/fake-wt-562-1"));

        assert!(head_is_wip, "fresh WIP creation must report WIP at HEAD");
        let calls = backup_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, 562u64);
    }

    #[test]
    fn backup_wip_before_gates_skips_backup_when_head_already_wip() {
        let ops = MockGitOps::new().with_head_wip(true);
        let backup_calls = Arc::clone(&ops.backup_wip_calls);
        let mut app = crate::tui::make_test_app("cg-test-2").with_git_ops(Box::new(ops));

        let head_is_wip = app.backup_wip_before_gates(562, &PathBuf::from("/tmp/fake-wt-562-2"));

        assert!(head_is_wip, "resume path must report WIP at HEAD");
        assert!(
            backup_calls.lock().unwrap().is_empty(),
            "no WIP stacking — backup_wip must NOT be called when HEAD is already a WIP commit"
        );
    }

    #[test]
    fn backup_wip_before_gates_returns_false_on_failure() {
        // Failure must be swallowed (gates still run) and surface as
        // `head_is_wip = false` so the post-gate path falls back to
        // legacy commit_and_push instead of trying to amend nothing.
        let ops = MockGitOps::new().with_failure();
        let mut app = crate::tui::make_test_app("cg-test-3").with_git_ops(Box::new(ops));

        let head_is_wip = app.backup_wip_before_gates(562, &PathBuf::from("/tmp/fake-wt-562-3"));

        assert!(!head_is_wip);
    }

    #[test]
    fn amend_or_commit_and_push_calls_amend_when_head_is_wip() {
        let ops = MockGitOps::new();
        let amend_calls = Arc::clone(&ops.amend_calls);
        let commit_and_push_calls = Arc::clone(&ops.commit_and_push_calls);
        let app = crate::tui::make_test_app("cg-test-4").with_git_ops(Box::new(ops));

        app.amend_or_commit_and_push(
            &PathBuf::from("/tmp/fake-wt-562-4"),
            "feat/issue-562",
            "feat: implement changes for issue #562",
            true,
        );

        let amend = amend_calls.lock().unwrap();
        assert_eq!(amend.len(), 1);
        assert_eq!(amend[0].1, "feat/issue-562");
        assert_eq!(amend[0].2, "feat: implement changes for issue #562");
        assert!(commit_and_push_calls.lock().unwrap().is_empty());
    }

    #[test]
    fn amend_or_commit_and_push_falls_back_when_head_is_not_wip() {
        let ops = MockGitOps::new();
        let amend_calls = Arc::clone(&ops.amend_calls);
        let commit_and_push_calls = Arc::clone(&ops.commit_and_push_calls);
        let app = crate::tui::make_test_app("cg-test-5").with_git_ops(Box::new(ops));

        app.amend_or_commit_and_push(
            &PathBuf::from("/tmp/fake-wt-562-5"),
            "feat/issue-562",
            "feat: implement changes for issue #562",
            false,
        );

        let cap = commit_and_push_calls.lock().unwrap();
        assert_eq!(cap.len(), 1);
        assert_eq!(cap[0].1, "feat/issue-562");
        assert!(amend_calls.lock().unwrap().is_empty());
    }
}
