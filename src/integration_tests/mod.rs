//! Integration tests for the full session lifecycle.
//!
//! No process spawning. Events are injected via handle_event().
//! All external dependencies are replaced with in-process mocks.

#[cfg(test)]
mod adapt_pipeline;
#[cfg(test)]
mod completion_pipeline;
#[cfg(test)]
mod concurrent_sessions;
#[cfg(test)]
mod gate_failure_retention;
#[cfg(test)]
mod init;
#[cfg(test)]
mod milestone_health_wizard;
#[cfg(test)]
mod session_lifecycle;
#[cfg(test)]
mod stream_parsing;
#[cfg(test)]
mod upgrade;
#[cfg(test)]
mod wip_backup;
#[cfg(test)]
mod worktree_lifecycle;

/// Shared test helpers used across integration test modules.
#[cfg(test)]
mod helpers {
    use tokio::sync::mpsc;

    use crate::provider::github::types::GhIssue;
    use crate::session::pool::SessionPool;
    use crate::session::types::Session;
    use crate::session::worktree::MockWorktreeManager;

    pub fn make_pool(max: usize) -> SessionPool {
        let (tx, _rx) = mpsc::unbounded_channel();
        SessionPool::new(max, Box::new(MockWorktreeManager::new()), tx)
    }

    pub fn make_pool_with_worktree(max: usize, wt: MockWorktreeManager) -> SessionPool {
        let (tx, _rx) = mpsc::unbounded_channel();
        SessionPool::new(max, Box::new(wt), tx)
    }

    pub fn make_session(prompt: &str) -> Session {
        Session::new(
            prompt.to_string(),
            "opus".to_string(),
            "orchestrator".to_string(),
            None,
            None,
        )
    }

    /// Create a session pre-set to Running status (for event handling tests).
    pub fn make_running_session(prompt: &str) -> Session {
        let mut s = make_session(prompt);
        s.status = crate::session::types::SessionStatus::Running;
        s.started_at = Some(chrono::Utc::now());
        s
    }

    pub fn make_session_with_issue(issue: u64) -> Session {
        Session::new(
            format!("work on issue {}", issue),
            "opus".to_string(),
            "orchestrator".to_string(),
            Some(issue),
            None,
        )
    }

    /// Create a session with an issue number pre-set to Running status.
    pub fn make_running_session_with_issue(issue: u64) -> Session {
        let mut s = make_session_with_issue(issue);
        s.status = crate::session::types::SessionStatus::Running;
        s.started_at = Some(chrono::Utc::now());
        s
    }

    pub fn make_gh_issue(number: u64) -> GhIssue {
        GhIssue {
            number,
            title: format!("Implement feature #{}", number),
            body: String::new(),
            labels: vec!["maestro:in-progress".to_string()],
            state: "open".to_string(),
            html_url: format!("https://github.com/owner/repo/issues/{}", number),
            milestone: None,
            assignees: vec![],
        }
    }

    /// Real-git helper: run `git <args>` synchronously in `dir`,
    /// asserting success. Used by integration tests that need a real
    /// repo on disk rather than the `MockWorktreeManager`.
    pub fn run_git(dir: &std::path::Path, args: &[&str]) {
        let s = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git must be on PATH");
        assert!(s.success(), "git {:?} failed in {:?}", args, dir);
    }

    /// Real-git helper: initialize `dir` as a git repo with a single
    /// `init` commit on `main`, identity pre-configured.
    pub fn init_git_repo(dir: &std::path::Path) {
        run_git(dir, &["init", "-q", "-b", "main"]);
        run_git(dir, &["config", "user.email", "test@example.com"]);
        run_git(dir, &["config", "user.name", "Test"]);
        std::fs::write(dir.join("README.md"), "init").expect("write README");
        run_git(dir, &["add", "README.md"]);
        run_git(dir, &["commit", "-q", "-m", "init"]);
    }

    /// Read the subject of the HEAD commit in `dir`.
    pub fn git_head_subject(dir: &std::path::Path) -> String {
        let out = std::process::Command::new("git")
            .args(["log", "-1", "--pretty=%s"])
            .current_dir(dir)
            .output()
            .expect("git log");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Build a maestro `App` wired to the canonical test toml with the
    /// gate `test_command` set to `test_command` (`"true"` to force a
    /// passing gate, `"false"` to force a failing one). The gate
    /// runner is real, so the test exercises the actual gate plumbing.
    /// Real `CliGitOps` is wired so subprocess expectations land on
    /// the worktree at `worktree_path` for the surrounding test.
    pub fn make_app_with_gate(label: &str, test_command: &str) -> crate::tui::app::App {
        let mut app = crate::tui::make_test_app(label);
        let toml = format!(
            "[project]\nrepo = \"owner/repo\"\n\
             [sessions]\n\
             [budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
             [github]\n[notifications]\n\
             [gates]\nenabled = true\ntest_command = \"{test_command}\"\n",
        );
        app.config = Some(toml::from_str(&toml).expect("test config parse"));
        app.with_git_ops(Box::new(crate::git::CliGitOps))
    }
}
