//! Integration tests that drive the TUI `App` end to end.
//!
//! Moved from the root crate's `src/integration_tests/` with the TUI (#1306).
//! No process spawning. Events are injected via handle_event().

mod adopt_picker_route;
mod budget_prespawn_gate;
mod completion_ticks;
mod fleet_board_actions;
mod fleet_board_route;
mod gate_failure_retention;
mod gates_running_state;
mod interaction_terminator;
mod milestone_health_wizard;
mod wip_backup;

/// Shared test helpers used across integration test modules.
mod helpers {
    use crate::session::types::Session;

    pub fn make_session_with_issue(issue: u64) -> Session {
        Session::new(
            format!("work on issue {}", issue),
            "opus".to_string(),
            "orchestrator".to_string(),
            Some(issue),
            None,
        )
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

    /// Like `make_app_with_gate`, but with `[gates] enabled = false` so
    /// `build_completion_gates` returns an empty `Vec`. Lets tests
    /// exercise the gates-disabled completion path.
    pub fn make_app_without_gates(label: &str) -> crate::tui::app::App {
        let mut app = crate::tui::make_test_app(label);
        let toml = "[project]\nrepo = \"owner/repo\"\n\
                    [sessions]\n\
                    [budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
                    [github]\n[notifications]\n\
                    [gates]\nenabled = false\n";
        app.config = Some(toml::from_str(toml).expect("test config parse"));
        app.with_git_ops(Box::new(crate::git::CliGitOps))
    }
}
