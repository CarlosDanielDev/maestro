use crate::session::types::Session;

pub(super) fn session_label(session: &Session) -> String {
    match session.issue_number {
        Some(n) => format!("#{}", n),
        None => format!("S-{}", &session.id.to_string()[..8]),
    }
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
