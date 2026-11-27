//! #1488: the exit save copies the pending PR queue and the pending
//! completions, so an early error in `check_completions` (which skips
//! `sync_state`) cannot drop them on quit.

use super::*;
use crate::state::types::PendingIssueCompletion;

fn completion(issue_number: u64) -> PendingIssueCompletion {
    PendingIssueCompletion {
        issue_number,
        issue_numbers: vec![issue_number],
        success: true,
        cost_usd: 0.0,
        files_touched: vec![],
        worktree_branch: Some(format!("maestro/issue-{issue_number}")),
        worktree_path: None,
        is_ci_fix: false,
    }
}

#[test]
fn save_on_exit_persists_pending_prs() {
    let mut app = make_test_app("exit-save-pending-prs-1488");
    app.pending_prs
        .push(crate::provider::github::types::awaiting_pending_pr(42));

    save_on_exit(&mut app).expect("saved");

    let saved = app.store.load().expect("state saved");
    let issues: Vec<u64> = saved.pending_prs.iter().map(|p| p.issue_number).collect();
    assert_eq!(issues, vec![42], "the exit save must keep the pending PR");
}

#[test]
fn save_on_exit_persists_pending_completions() {
    let mut app = make_test_app("exit-save-pending-completions-1488");
    app.pending_issue_completions.push(completion(7));
    app.completions_in_flight.push(completion(8));

    save_on_exit(&mut app).expect("saved");

    let saved = app.store.load().expect("state saved");
    let mut issues: Vec<u64> = (saved.pending_completions.iter())
        .map(|c| c.issue_number)
        .collect();
    issues.sort_unstable();
    assert_eq!(
        issues,
        vec![7, 8],
        "the exit save must keep queued and in-flight completions"
    );
}
