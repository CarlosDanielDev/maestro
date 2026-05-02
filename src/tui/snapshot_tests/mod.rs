mod activity_log_dispatch;
mod agent_graph;
mod agent_graph_dispatcher;
mod agent_graph_keybinding_hint;
mod agent_personalities;
mod caveman_row;
mod completion_overlay;
mod copy_keybinding_hint;
mod cost_dashboard;
mod dashboard;
mod detail;
mod fullscreen;
mod issue_browser;
mod landing;
mod milestone;
mod overview;
mod turboquant_dashboard;

use chrono::{TimeZone, Utc};
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use crate::session::types::{ActivityEntry, GateResultEntry, Session, SessionStatus};

pub const TERM_WIDTH: u16 = 120;
pub const TERM_HEIGHT: u16 = 40;

pub fn test_terminal() -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(TERM_WIDTH, TERM_HEIGHT)).unwrap()
}

pub fn fixed_start() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap()
}

/// 2m30s after fixed_start.
pub fn fixed_end() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 1, 1, 10, 2, 30).unwrap()
}

/// Both started_at and finished_at are pinned to avoid drift from
/// `elapsed_display()` calling `Utc::now()` on non-terminal sessions.
pub fn make_session(status: SessionStatus, issue_number: Option<u64>) -> Session {
    let mut s = Session::new(
        "Implement feature X".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        issue_number,
        None,
    );
    s.id = Uuid::nil();
    s.status = status;
    s.started_at = Some(fixed_start());
    s.finished_at = Some(fixed_end());
    s.cost_usd = 0.12;
    s.context_pct = 0.35;
    s.current_activity = "Writing tests".to_string();
    s.last_message = "Analyzing codebase...".to_string();
    s.issue_title = issue_number.map(|_| "Add login flow".to_string());
    s
}

/// Build a Session in `FailedGates` state with two pinned gate failures
/// and a retained worktree path. Used by the completion-overlay
/// snapshot tests (issue #560).
pub fn make_failed_gates_session() -> Session {
    let mut s = Session::new(
        "Implement feature X".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(560),
        None,
    );
    s.id = Uuid::from_u128(0xdead_beef);
    s.status = SessionStatus::FailedGates;
    s.started_at = Some(fixed_start());
    s.finished_at = Some(fixed_end());
    s.cost_usd = 0.42;
    s.context_pct = 0.50;
    s.issue_title = Some("Failed gates recovery test".to_string());
    s.gate_results = vec![
        GateResultEntry::fail(
            "clippy",
            "function 'role_for_subagent_dispatch' is never used (truncated)",
        ),
        GateResultEntry::fail("label_update", "'maestro:in-progress' not found"),
    ];
    s.worktree_path = Some(std::path::PathBuf::from(".maestro/worktrees/issue-560"));
    s
}

pub fn make_activity(offset_secs: i64, message: &str) -> ActivityEntry {
    ActivityEntry {
        timestamp: fixed_start() + chrono::Duration::seconds(offset_secs),
        message: message.to_string(),
    }
}

pub fn make_gh_issue(number: u64, title: &str) -> crate::provider::github::types::GhIssue {
    crate::provider::github::types::GhIssue {
        number,
        title: title.to_string(),
        body: String::new(),
        labels: vec!["maestro:ready".to_string()],
        state: "open".to_string(),
        html_url: format!("https://github.com/owner/repo/issues/{}", number),
        milestone: None,
        assignees: vec![],
    }
}
