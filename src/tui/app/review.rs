use super::App;
use crate::tui::activity_log::LogLevel;

impl App {
    pub(super) fn dispatch_review(&mut self, pr_number: u64, branch: &str, issue_number: u64) {
        let Some(config) = &self.config else { return };
        let review_cfg = &config.review;
        if !review_cfg.enabled {
            return;
        }

        // Workspace trust gate (#1045): review commands come from the repo
        // config; they never execute while the workspace is untrusted.
        if !self.workspace_trusted {
            self.activity_log.push_simple(
                format!("#{}", issue_number),
                "Review skipped: workspace config is not trusted (run `maestro trust`)".into(),
                LogLevel::Warn,
            );
            return;
        }

        if !review_cfg.reviewers.is_empty() {
            let reviewers: Vec<crate::review::council::ReviewerConfig> = review_cfg
                .reviewers
                .iter()
                .map(|r| crate::review::council::ReviewerConfig {
                    name: r.name.clone(),
                    command: r.command.clone(),
                    required: r.required,
                })
                .collect();
            match crate::review::council::ReviewCouncil::convene(pr_number, branch, &reviewers) {
                Ok(council_result) => {
                    let status_label = match &council_result.status {
                        crate::review::council::ReviewStatus::Approved { .. } => "Council approved",
                        crate::review::council::ReviewStatus::Rejected { .. } => "Council rejected",
                        crate::review::council::ReviewStatus::Partial { .. } => "Council partial",
                    };
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("PR #{}: {}", pr_number, status_label),
                        LogLevel::Info,
                    );
                    let comment =
                        crate::review::council::ReviewCouncil::format_comment(&council_result);
                    let _ = crate::review::dispatch::ReviewDispatcher::post_comment(
                        pr_number, &comment,
                    );
                }
                Err(e) => {
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Council review failed: {}", e),
                        LogLevel::Error,
                    );
                }
            }
        } else {
            let review_config = crate::review::ReviewConfig {
                enabled: review_cfg.enabled,
                command: review_cfg.command.clone(),
            };
            let dispatcher = crate::review::ReviewDispatcher::new(review_config);
            match dispatcher.dispatch(pr_number, branch) {
                Ok(result) => {
                    let status = if result.success {
                        "Review passed"
                    } else {
                        "Review failed"
                    };
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("PR #{}: {}", pr_number, status),
                        LogLevel::Info,
                    );
                    let comment_body = format!(
                        "**Maestro Review**\n\nStatus: {}\n\n```\n{}\n```",
                        status, result.output
                    );
                    let _ = crate::review::dispatch::ReviewDispatcher::post_comment(
                        pr_number,
                        &comment_body,
                    );
                }
                Err(e) => {
                    self.activity_log.push_simple(
                        format!("#{}", issue_number),
                        format!("Review dispatch failed: {}", e),
                        LogLevel::Error,
                    );
                }
            }
        }
    }
}

/// Workspace trust gate tests (#1045) — unix-only: marker probe is a script.
#[cfg(all(test, unix))]
mod workspace_trust_tests {
    use super::*;
    use crate::commands::setup::setup_app_from_config_with_trust;
    use crate::config::{Config, LoadedConfig};
    use crate::session::worktree::MockWorktreeManager;
    use crate::state::store::StateStore;

    fn review_app(
        tag: &str,
        workspace_trusted: bool,
    ) -> (tempfile::TempDir, App, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join(format!("reviewer-{tag}.sh"));
        let marker = dir.path().join(format!("marker-{tag}.txt"));
        std::fs::write(
            &script,
            format!("#!/bin/sh\ntouch '{}'\necho reviewed\n", marker.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let toml_body = format!(
            r#"[project]
repo = "owner/repo"
base_branch = "main"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[notifications]
[review]
enabled = true
command = "{} {{pr_number}}"
"#,
            script.display()
        );
        let config: Config = toml::from_str(&toml_body).unwrap();
        let app = setup_app_from_config_with_trust(
            LoadedConfig {
                config,
                path: dir.path().join("maestro.toml"),
            },
            StateStore::new(dir.path().join(format!("state-{tag}.json"))),
            Box::new(MockWorktreeManager::new()),
            None,
            false,
            workspace_trusted,
        );
        (dir, app, marker)
    }

    #[tokio::test]
    async fn workspace_trust_untrusted_review_dispatch_is_skipped() {
        let (dir, mut app, marker) = review_app("untrusted", false);

        app.dispatch_review(42, "main", 7);

        assert!(
            !marker.exists(),
            "untrusted review command must not execute"
        );
        assert!(
            app.activity_log
                .entries()
                .iter()
                .any(|e| e.message.contains("not trusted")),
            "activity log must record the skip"
        );
        let _ = dir;
    }

    #[test]
    fn workspace_trust_trusted_review_command_executes() {
        // The trusted path is the pre-existing dispatch flow; assert the
        // execution primitive it uses — WITHOUT reaching `gh pr comment`,
        // which would post to a real repository from tests.
        let (dir, _app, marker) = review_app("trusted", true);
        let script = dir.path().join("reviewer-trusted.sh");
        let command = format!("{} {{pr_number}}", script.display());

        let result = crate::review::dispatch::ReviewDispatcher::run_review_command(
            &command, 42, "main", None,
        )
        .unwrap();

        assert!(result.success, "output: {}", result.output);
        assert!(marker.exists(), "trusted review command must execute");
    }
}
