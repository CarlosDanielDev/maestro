use crate::session::types::GateResultEntry;
use crate::tui::app::*;

fn make_app() -> crate::tui::app::App {
    crate::tui::make_test_app("maestro-tui-app-test")
}

#[test]
fn build_completion_summary_gate_failure_message_truncated() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(60),
        None,
    );
    session.status = crate::session::types::SessionStatus::NeedsReview;
    session.gate_results = vec![GateResultEntry::fail("tests", "x".repeat(300))];
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    let msg = &summary.sessions[0].gate_failures[0].message;
    assert!(
        msg.len() <= 104,
        "gate failure message must be truncated, got {} chars",
        msg.len()
    );
}

#[test]
fn completion_summary_dismissed_defaults_to_false() {
    let app = make_app();
    assert!(
        !app.completion_summary_dismissed,
        "completion_summary_dismissed must default to false"
    );
}

#[test]
fn transition_to_dashboard_sets_dismissed_flag() {
    let mut app = make_app();
    app.tui_mode = TuiMode::CompletionSummary;
    app.completion_summary = Some(CompletionSummaryData {
        sessions: vec![],
        total_cost_usd: 0.0,
        session_count: 0,
        suggestions: vec![],
        selected_suggestion: 0,
    });
    app.transition_to_dashboard();
    assert!(
        app.completion_summary_dismissed,
        "transition_to_dashboard must set completion_summary_dismissed = true"
    );
    assert!(
        app.completion_summary.is_none(),
        "completion_summary must be cleared"
    );
    assert!(
        matches!(app.tui_mode, TuiMode::Dashboard),
        "tui_mode must be Dashboard"
    );
}

#[test]
fn transition_to_dashboard_clears_issue_browser_screen() {
    let mut app = make_app();
    app.screen_state.issue_browser_screen =
        Some(crate::tui::screens::IssueBrowserScreen::new(vec![]));
    app.transition_to_dashboard();
    assert!(
        app.screen_state.issue_browser_screen.is_none(),
        "transition_to_dashboard must clear stale issue_browser_screen"
    );
}

#[test]
fn conflict_suggestion_stores_all_fields() {
    let sg = ConflictSuggestion {
        pr_number: 42,
        issue_number: 10,
        branch: "feat/auth".to_string(),
        conflicting_files: vec!["src/a.rs".to_string()],
        message: "has merge conflicts".to_string(),
    };
    assert_eq!(sg.pr_number, 42);
    assert_eq!(sg.issue_number, 10);
    assert_eq!(sg.branch, "feat/auth");
    assert_eq!(sg.conflicting_files.len(), 1);
    assert!(!sg.message.is_empty());
}

#[test]
fn completion_summary_data_suggestions_defaults_to_empty() {
    let data = CompletionSummaryData {
        session_count: 0,
        total_cost_usd: 0.0,
        sessions: vec![],
        suggestions: vec![],
        selected_suggestion: 0,
    };
    assert!(data.suggestions.is_empty());
}

#[test]
fn completion_summary_data_selected_suggestion_defaults_to_zero() {
    let data = CompletionSummaryData {
        session_count: 0,
        total_cost_usd: 0.0,
        sessions: vec![],
        suggestions: vec![],
        selected_suggestion: 0,
    };
    assert_eq!(data.selected_suggestion, 0);
}

#[test]
fn completion_summary_data_holds_multiple_suggestions() {
    let data = CompletionSummaryData {
        session_count: 0,
        total_cost_usd: 0.0,
        sessions: vec![],
        suggestions: vec![
            ConflictSuggestion {
                pr_number: 1,
                issue_number: 1,
                branch: "a".to_string(),
                conflicting_files: vec![],
                message: "conflict".to_string(),
            },
            ConflictSuggestion {
                pr_number: 2,
                issue_number: 2,
                branch: "b".to_string(),
                conflicting_files: vec![],
                message: "conflict".to_string(),
            },
        ],
        selected_suggestion: 0,
    };
    assert_eq!(data.suggestions.len(), 2);
}

#[test]
fn selected_suggestion_can_be_advanced() {
    let mut data = CompletionSummaryData {
        session_count: 0,
        total_cost_usd: 0.0,
        sessions: vec![],
        suggestions: vec![
            ConflictSuggestion {
                pr_number: 1,
                issue_number: 1,
                branch: "a".to_string(),
                conflicting_files: vec![],
                message: "conflict".to_string(),
            },
            ConflictSuggestion {
                pr_number: 2,
                issue_number: 2,
                branch: "b".to_string(),
                conflicting_files: vec![],
                message: "conflict".to_string(),
            },
        ],
        selected_suggestion: 0,
    };
    data.selected_suggestion = 1;
    assert_eq!(data.selected_suggestion, 1);
}

#[test]
fn has_conflict_suggestions_returns_false_when_empty() {
    let data = CompletionSummaryData {
        session_count: 0,
        total_cost_usd: 0.0,
        sessions: vec![],
        suggestions: vec![],
        selected_suggestion: 0,
    };
    assert!(!data.has_conflict_suggestions());
}

#[test]
fn has_conflict_suggestions_returns_true_when_populated() {
    let data = CompletionSummaryData {
        session_count: 0,
        total_cost_usd: 0.0,
        sessions: vec![],
        suggestions: vec![ConflictSuggestion {
            pr_number: 1,
            issue_number: 1,
            branch: "a".to_string(),
            conflicting_files: vec![],
            message: "conflict".to_string(),
        }],
        selected_suggestion: 0,
    };
    assert!(data.has_conflict_suggestions());
}

#[test]
fn build_completion_summary_suggestions_default_to_empty() {
    let app = make_app();
    let summary = app.build_completion_summary();
    assert!(summary.suggestions.is_empty());
    assert_eq!(summary.selected_suggestion, 0);
}

#[test]
fn build_completion_summary_populates_gate_failures_for_failed_gates_session() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(560),
        None,
    );
    session.status = crate::session::types::SessionStatus::FailedGates;
    session.gate_results = vec![
        GateResultEntry::fail("clippy", "function 'x' is never used (truncated)"),
        GateResultEntry::fail("label_update", "'maestro:in-progress' not found"),
    ];
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert_eq!(
        summary.sessions[0].gate_failures.len(),
        2,
        "FailedGates sessions must surface their gate_results as gate_failures \
     on the CompletionSessionLine — currently only NeedsReview does."
    );
    assert_eq!(summary.sessions[0].gate_failures[0].gate, "clippy");
    assert_eq!(summary.sessions[0].gate_failures[1].gate, "label_update");
}

#[test]
fn build_completion_summary_includes_worktree_path_on_failed_gates_line() {
    let mut app = make_app();
    let mut session = crate::session::types::Session::new(
        "task".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(560),
        None,
    );
    session.status = crate::session::types::SessionStatus::FailedGates;
    session.worktree_path = Some(std::path::PathBuf::from(".maestro/worktrees/issue-560"));
    app.pool.enqueue(session);

    let summary = app.build_completion_summary();
    assert_eq!(
        summary.sessions[0].worktree_path,
        Some(std::path::PathBuf::from(".maestro/worktrees/issue-560")),
        "FailedGates session's worktree_path must surface on the line so [s] can reach it"
    );
}
