//! Issue #560 — snapshot tests for the completion overlay at 80×24.
//!
//! Two variants:
//!   1. Success modal: title "Session Complete", success border,
//!      success-keys hint bar.
//!   2. Failed-gates modal: title "Session Failed Gates", warning border,
//!      recovery-keys hint bar (s/g/r/v/q), worktree-path line.

use super::*;
use crate::session::types::SessionStatus;
use crate::tui::app::types::{CompletionSessionLine, CompletionSummaryData, GateFailureInfo};
use crate::tui::theme::Theme;
use insta::assert_snapshot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn term_80x24() -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(80, 24)).unwrap()
}

fn line_for(session: &Session) -> CompletionSessionLine {
    CompletionSessionLine {
        session_id: session.id,
        label: format!("#{}", session.issue_number.unwrap_or(0)),
        status: session.status,
        cost_usd: session.cost_usd,
        elapsed: session.elapsed_display(),
        pr_link: String::new(),
        error_summary: String::new(),
        gate_failures: session
            .gate_results
            .iter()
            .filter(|r| !r.passed)
            .map(|r| GateFailureInfo {
                gate: r.gate.clone(),
                message: r.message.clone(),
            })
            .collect(),
        worktree_path: session.worktree_path.clone(),
        issue_number: session.issue_number,
        model: session.model.clone(),
    }
}

#[test]
fn snapshot_completion_overlay_success_modal_80x24() {
    let mut term = term_80x24();
    let theme = Theme::dark();
    let session = make_session(SessionStatus::Completed, Some(42));
    let summary = CompletionSummaryData {
        sessions: vec![line_for(&session)],
        total_cost_usd: session.cost_usd,
        session_count: 1,
        suggestions: vec![],
        selected_suggestion: 0,
    };

    term.draw(|f| {
        crate::tui::ui::draw_completion_overlay(f, &summary, f.area(), &theme);
    })
    .unwrap();

    assert_snapshot!(term.backend());
}

#[test]
fn snapshot_completion_overlay_failed_gates_modal_80x24() {
    let mut term = term_80x24();
    let theme = Theme::dark();
    let session = make_failed_gates_session();
    let summary = CompletionSummaryData {
        sessions: vec![line_for(&session)],
        total_cost_usd: session.cost_usd,
        session_count: 1,
        suggestions: vec![],
        selected_suggestion: 0,
    };

    term.draw(|f| {
        crate::tui::ui::draw_completion_overlay(f, &summary, f.area(), &theme);
    })
    .unwrap();

    assert_snapshot!(term.backend());
}
