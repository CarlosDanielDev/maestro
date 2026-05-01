use crate::config::Config;
use crate::gates::types::CompletionGate;
use crate::session::types::Session;
use crate::tui::screens::PromptInputScreen;

pub(crate) fn create_prompt_input_screen(
    history: &crate::state::prompt_history::PromptHistoryStore,
) -> PromptInputScreen {
    let mut screen = PromptInputScreen::new();
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
