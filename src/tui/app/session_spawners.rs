use super::App;
use super::helpers::build_gate_fix_prompt;
use super::types::{CompletionSessionLine, TuiCommand};
use crate::session::types::{Session, SessionStatus};
use crate::tui::activity_log::LogLevel;

impl App {
    pub(super) fn default_model_and_mode(&self) -> (String, String) {
        let model = self
            .config
            .as_ref()
            .map(|c| c.sessions.default_model.clone())
            .unwrap_or_else(|| "opus".to_string());
        let mode = self
            .config
            .as_ref()
            .map(|c| c.sessions.default_mode.clone())
            .unwrap_or_else(|| "orchestrator".to_string());
        (model, mode)
    }

    pub(super) fn spawn_ci_fix_session(
        &mut self,
        pr_number: u64,
        issue_number: u64,
        branch: String,
        attempt: u32,
        failure_log: &str,
    ) {
        use crate::github::ci::build_ci_fix_prompt;
        use crate::session::types::CiFixContext;

        let (model, mode) = self.default_model_and_mode();

        let prompt = build_ci_fix_prompt(pr_number, issue_number, &branch, attempt, failure_log);

        let mut session = Session::new(prompt, model, mode, Some(issue_number));
        let _ = session.transition_to(
            SessionStatus::CiFix,
            crate::session::transition::TransitionReason::CiFixStarted,
        );
        session.issue_title = Some(format!("CI Fix #{} for PR #{}", attempt, pr_number));
        session.ci_fix_context = Some(CiFixContext {
            pr_number,
            issue_number,
            branch,
            attempt,
        });

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

        let prompt = build_gate_fix_prompt(issue_number, &gate_failure_details);

        let mut session = Session::new(prompt, model, mode, Some(issue_number));
        session.issue_title = Some(format!("Gate Fix for #{}", issue_number));

        self.pending_session_launches.push(session);

        self.activity_log.push_simple(
            format!("#{}", issue_number),
            "Launched gate fix session".into(),
            LogLevel::Info,
        );
    }

    /// Spawn a Claude session to resolve merge conflicts for a PR.
    pub fn spawn_conflict_fix_session(&mut self, config: &crate::tui::screens::ConflictFixConfig) {
        use crate::github::merge::build_conflict_fix_prompt;
        use crate::session::types::ConflictFixContext;

        let (model, mode) = self.default_model_and_mode();

        let prompt = build_conflict_fix_prompt(
            config.pr_number,
            config.issue_number,
            &config.branch,
            &config.conflicting_files,
        );

        let mut session = Session::new(prompt, model, mode, Some(config.issue_number));
        let _ = session.transition_to(
            SessionStatus::ConflictFix,
            crate::session::transition::TransitionReason::ConflictFixStarted,
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
