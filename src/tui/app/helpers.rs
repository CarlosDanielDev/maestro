use crate::config::Config;
use crate::gates::types::CompletionGate;
use crate::session::types::Session;
use crate::tui::screens::PromptInputScreen;

pub(crate) fn create_prompt_input_screen(
    history: &crate::state::prompt_history::PromptHistoryStore,
    launch_defaults: (bool, bool),
) -> PromptInputScreen {
    let mut screen = PromptInputScreen::new().with_launch_defaults(launch_defaults);
    let prompts: Vec<String> = history.entries().iter().map(|e| e.prompt.clone()).collect();
    screen.set_history(prompts);
    screen
}

pub(crate) fn session_label(session: &Session) -> String {
    match session.issue_number {
        Some(n) => format!("#{}", n),
        None => format!("S-{}", &session.id.to_string()[..8]),
    }
}

/// Build the completion-gate list from a config, used by both the
/// in-pipeline gate run (`completion_pipeline.rs`) and the `[g]` retry
/// path (`gate_retry.rs`). Single source of truth so the two callers
/// can't drift on which gates run.
pub(crate) fn build_completion_gates(config: Option<&Config>) -> Vec<CompletionGate> {
    if let Some(cfg) = config
        && cfg.sessions.completion_gates.enabled
        && !cfg.sessions.completion_gates.commands.is_empty()
    {
        return cfg
            .sessions
            .completion_gates
            .commands
            .iter()
            .map(CompletionGate::from_config_entry)
            .collect();
    }
    if let Some(cfg) = config
        && cfg.gates.enabled
    {
        return vec![CompletionGate::TestsPass {
            command: cfg.gates.test_command.clone(),
        }];
    }
    Vec::new()
}

pub(super) fn build_gate_fix_prompt(issue_number: u64, failure_details: &str) -> String {
    format!(
        "Fix the gate failures for issue #{issue_number}.\n\n\
         GATE FAILURES:\n{failure_details}\n\n\
         IMPORTANT: You are running in unattended mode. \
         Do NOT use AskUserQuestion. \
         Read the failing code, fix the issues, then commit and push. \
         Run the failing gate commands locally to reproduce, then fix and verify. \
         Keep the fix minimal — do NOT refactor unrelated code. Only fix the gate failures."
    )
}

/// #1144: flag sessions that will run completion gates, so the stream
/// handler parks them in `GATES_RUNNING` at stream completion instead
/// of settling `COMPLETED` before the pipeline runs the gates. Single
/// source of truth for the flag, derived from the same config the
/// completion pipeline reads (`build_completion_gates`).
pub(crate) fn mark_gates_pending(session: &mut Session, config: Option<&Config>) {
    session.gates_pending =
        session.issue_number.is_some() && !build_completion_gates(config).is_empty();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gated_config() -> Option<Config> {
        let toml = "[project]\nrepo = \"owner/repo\"\n\
                    [sessions]\n\
                    [budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
                    [github]\n[notifications]\n\
                    [gates]\nenabled = true\ntest_command = \"true\"\n";
        Some(toml::from_str(toml).expect("test config parse"))
    }

    #[test]
    fn marks_gated_issue_sessions() {
        let mut s = Session::new(
            "work".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1144),
            None,
        );
        mark_gates_pending(&mut s, gated_config().as_ref());
        assert!(s.gates_pending, "issue session with gates must be flagged");
    }

    #[test]
    fn does_not_mark_without_gates() {
        let mut s = Session::new(
            "work".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(1144),
            None,
        );
        mark_gates_pending(&mut s, None);
        assert!(
            !s.gates_pending,
            "no config → no gates → session must not be flagged"
        );
    }

    #[test]
    fn does_not_mark_sessions_without_issue() {
        let mut s = Session::new(
            "chat".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        mark_gates_pending(&mut s, gated_config().as_ref());
        assert!(
            !s.gates_pending,
            "sessions without an issue never enter the completion pipeline — must not be flagged"
        );
    }
}
