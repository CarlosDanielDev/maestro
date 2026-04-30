//! Session spawning service — factory methods for creating specialized sessions.

use super::App;
use super::helpers::build_gate_fix_prompt;
use super::types::{CompletionSessionLine, TuiCommand};
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::tui::activity_log::LogLevel;

/// Resolve default model and mode from config (or built-in defaults).
pub(crate) fn default_model_and_mode(config: Option<&crate::config::Config>) -> (String, String) {
    let model = config
        .map(|c| c.sessions.default_model.clone())
        .unwrap_or_else(|| "opus".to_string());
    let mode = config
        .map(|c| c.sessions.default_mode.clone())
        .unwrap_or_else(|| "orchestrator".to_string());
    (model, mode)
}

/// Create a CI fix session (does not enqueue — caller pushes to pending_session_launches).
pub(crate) fn create_ci_fix_session(
    model: &str,
    mode: &str,
    pr_number: u64,
    issue_number: u64,
    branch: &str,
    attempt: u32,
    failure_log: &str,
) -> Session {
    use crate::provider::github::ci::build_ci_fix_prompt;
    use crate::session::types::CiFixContext;

    let prompt = build_ci_fix_prompt(pr_number, issue_number, branch, attempt, failure_log);
    let mut session = Session::new(
        prompt,
        model.to_string(),
        mode.to_string(),
        Some(issue_number),
        None,
    );
    let _ = session.transition_to(SessionStatus::CiFix, TransitionReason::CiFixStarted);
    session.issue_title = Some(format!("CI Fix #{} for PR #{}", attempt, pr_number));
    session.ci_fix_context = Some(CiFixContext {
        pr_number,
        issue_number,
        branch: branch.to_string(),
        attempt,
    });
    session
}

/// Create a gate fix session (does not enqueue — caller pushes to pending_session_launches).
pub(crate) fn create_gate_fix_session(
    model: &str,
    mode: &str,
    issue_number: u64,
    gate_failure_details: &str,
) -> Session {
    let prompt = build_gate_fix_prompt(issue_number, gate_failure_details);
    let mut session = Session::new(
        prompt,
        model.to_string(),
        mode.to_string(),
        Some(issue_number),
        None,
    );
    session.issue_title = Some(format!("Gate Fix for #{}", issue_number));
    session
}

impl App {
    pub(super) fn default_model_and_mode(&self) -> (String, String) {
        default_model_and_mode(self.config.as_ref())
    }

    pub(super) fn spawn_ci_fix_session(
        &mut self,
        pr_number: u64,
        issue_number: u64,
        branch: String,
        attempt: u32,
        failure_log: &str,
    ) {
        let (model, mode) = self.default_model_and_mode();
        let session = create_ci_fix_session(
            &model,
            &mode,
            pr_number,
            issue_number,
            &branch,
            attempt,
            failure_log,
        );
        self.pending_session_launches.push(session);
    }

    /// Spawn a fix session for gate failures on a NeedsReview session.
    pub fn spawn_gate_fix_session(&mut self, failed_line: &CompletionSessionLine) {
        let issue_number = match failed_line.issue_number {
            Some(n) => n,
            None => return,
        };

        let gate_failure_details: String = failed_line
            .gate_failures
            .iter()
            .map(|gf| format!("- [{}]: {}", gf.gate, gf.message))
            .collect::<Vec<_>>()
            .join("\n")
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(2000)
            .collect();

        let (default_model, mode) = self.default_model_and_mode();
        let model = if failed_line.model.is_empty() {
            default_model
        } else {
            failed_line.model.clone()
        };

        let session = create_gate_fix_session(&model, &mode, issue_number, &gate_failure_details);
        self.pending_session_launches.push(session);

        self.activity_log.push_simple(
            format!("#{}", issue_number),
            "Launched gate fix session".into(),
            LogLevel::Info,
        );
    }

    /// Spawn a Claude session to resolve merge conflicts for a PR.
    pub fn spawn_conflict_fix_session(&mut self, config: &crate::tui::screens::ConflictFixConfig) {
        use crate::provider::github::merge::build_conflict_fix_prompt;
        use crate::session::types::ConflictFixContext;

        let (model, mode) = self.default_model_and_mode();

        let prompt = build_conflict_fix_prompt(
            config.pr_number,
            config.issue_number,
            &config.branch,
            &config.conflicting_files,
        );

        let mut session = Session::new(prompt, model, mode, Some(config.issue_number), None);
        let _ = session.transition_to(
            SessionStatus::ConflictFix,
            TransitionReason::ConflictFixStarted,
        );
        session.issue_title = Some(format!("Conflict Fix for PR #{}", config.pr_number));
        session.conflict_fix_context = Some(ConflictFixContext {
            pr_number: config.pr_number,
            issue_number: config.issue_number,
            branch: config.branch.clone(),
            conflicting_files: config.conflicting_files.clone(),
        });

        self.pending_session_launches.push(session);

        self.activity_log.push_simple(
            format!("PR #{}", config.pr_number),
            "Launched conflict fix session".into(),
            LogLevel::Info,
        );
    }

    /// Advance the queue executor to the next item and queue a session launch for it.
    /// Returns true if a session was queued, false if the executor is finished.
    pub fn advance_queue_and_launch(&mut self) -> bool {
        let issue_num = {
            let exec = match self.queue_executor.as_mut() {
                Some(e) => e,
                None => return false,
            };
            match exec.advance() {
                Some(item) => item.issue_number,
                None => return false,
            }
        };

        if let Some(config) = self
            .queue_launch_configs
            .as_ref()
            .and_then(|cfgs| cfgs.iter().find(|c| c.issue_number == Some(issue_num)))
        {
            self.pending_commands
                .push(TuiCommand::LaunchSession(config.clone()));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_and_mode_without_config_returns_defaults() {
        let (model, mode) = default_model_and_mode(None);
        assert_eq!(model, "opus");
        assert_eq!(mode, "orchestrator");
    }

    #[test]
    fn default_model_and_mode_with_config_uses_config_values() {
        let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
default_model = "haiku"
default_mode = "vibe"
max_concurrent = 1
[budget]
max_cost_per_session = 1.0
max_cost_total = 10.0
[github]
[notifications]
"#;
        let config: crate::config::Config = toml::from_str(toml_str).unwrap();
        let (model, mode) = default_model_and_mode(Some(&config));
        assert_eq!(model, "haiku");
        assert_eq!(mode, "vibe");
    }

    #[test]
    fn create_ci_fix_session_sets_correct_fields() {
        let session = create_ci_fix_session(
            "sonnet",
            "orchestrator",
            42,
            10,
            "feat/fix",
            1,
            "CI error log",
        );
        assert!(session.ci_fix_context.is_some());
        let ctx = session.ci_fix_context.unwrap();
        assert_eq!(ctx.pr_number, 42);
        assert_eq!(ctx.issue_number, 10);
        assert_eq!(ctx.branch, "feat/fix");
        assert_eq!(ctx.attempt, 1);
        assert!(session.issue_title.unwrap().contains("CI Fix"));
        assert_eq!(session.status, SessionStatus::CiFix);
    }

    #[test]
    fn create_gate_fix_session_sets_correct_fields() {
        let session =
            create_gate_fix_session("opus", "orchestrator", 99, "- [clippy]: lint failed");
        assert_eq!(session.issue_number, Some(99));
        assert!(session.issue_title.unwrap().contains("Gate Fix"));
    }
}
