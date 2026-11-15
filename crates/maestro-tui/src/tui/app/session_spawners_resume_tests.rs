//! Tests for `spawn_resume_implement_session` and `create_gate_fix_session`,
//! split out from `session_spawners.rs` to keep that module under the
//! 400-line cap (#695).

use super::create_gate_fix_session;
use crate::session::types::SessionStatus;
use crate::tui::app::types::CompletionSessionLine;

fn make_failed_line(issue: Option<u64>, worktree: Option<&str>) -> CompletionSessionLine {
    CompletionSessionLine {
        session_id: uuid::Uuid::nil(),
        label: "#560".to_string(),
        status: SessionStatus::FailedGates,
        cost_usd: 0.0,
        elapsed: "0s".to_string(),
        pr_link: String::new(),
        error_summary: String::new(),
        gate_failures: vec![],
        worktree_path: worktree.map(std::path::PathBuf::from),
        issue_number: issue,
        model: "claude-opus-4-5".to_string(),
        agent_id: None,
    }
}

#[test]
fn spawn_resume_implement_session_pushes_session_with_continue_prompt() {
    let mut app = crate::tui::make_test_app("issue-560-resume-prompt");
    let line = make_failed_line(Some(560), Some(".maestro/worktrees/issue-560"));
    app.spawn_resume_implement_session(&line);
    assert_eq!(app.pending_session_launches.len(), 1);
    assert!(
        app.pending_session_launches[0]
            .prompt
            .contains("/implement #560 --continue"),
        "resume prompt must use /implement #N --continue, got: {}",
        app.pending_session_launches[0].prompt
    );
}

#[test]
fn spawn_resume_implement_session_sets_worktree_path_on_new_session() {
    let mut app = crate::tui::make_test_app("issue-560-resume-wt");
    let line = make_failed_line(Some(560), Some(".maestro/worktrees/issue-560"));
    app.spawn_resume_implement_session(&line);
    assert_eq!(
        app.pending_session_launches[0].worktree_path,
        Some(std::path::PathBuf::from(".maestro/worktrees/issue-560")),
        "resume session must inherit the failed session's worktree_path so the \
         session manager re-uses the existing worktree"
    );
}

#[test]
fn spawn_resume_implement_session_does_nothing_when_issue_number_is_none() {
    let mut app = crate::tui::make_test_app("issue-560-resume-no-issue");
    let line = make_failed_line(None, Some(".maestro/worktrees/issue-560"));
    app.spawn_resume_implement_session(&line);
    assert!(
        app.pending_session_launches.is_empty(),
        "resume must not spawn a session for an unnamed line"
    );
}

#[test]
fn spawn_resume_implement_session_does_nothing_when_worktree_path_is_none() {
    let mut app = crate::tui::make_test_app("issue-560-resume-no-wt");
    let line = make_failed_line(Some(560), None);
    app.spawn_resume_implement_session(&line);
    assert!(
        app.pending_session_launches.is_empty(),
        "resume must not spawn without a worktree_path — there's nothing to resume to"
    );
}

#[test]
fn create_gate_fix_session_sets_correct_fields() {
    let session = create_gate_fix_session("opus", "orchestrator", 99, "- [clippy]: lint failed");
    assert_eq!(session.issue_number, Some(99));
    assert!(session.issue_title.unwrap().contains("Gate Fix"));
}
