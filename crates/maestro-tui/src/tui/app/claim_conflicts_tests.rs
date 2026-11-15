//! Tests for #1156: repo-relative file claims across worktrees, and the
//! activity-log line for PRs from the same run that change the same files.

use super::super::App;
use crate::provider::github::client::mock::MockGitHubClient;
use crate::provider::types::Issue;
use crate::session::types::StreamEvent;
use crate::session::worktree::MockWorktreeManager;
use crate::state::store::StateStore;
use crate::tui::activity_log::{ActivityLog, LogEntry};
use crate::tui::snapshot_tests::fixed_start;
use crate::tui::theme::Theme;
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};
use std::path::PathBuf;

fn make_app() -> App {
    let tmp = std::env::temp_dir().join(format!(
        "maestro-claim-test-1156-{}.json",
        uuid::Uuid::new_v4()
    ));
    App::new(
        StateStore::new(tmp),
        3,
        Box::new(MockWorktreeManager::new()),
        "bypassPermissions".into(),
        vec![],
    )
}

fn edit(path: &str) -> StreamEvent {
    StreamEvent::ToolUse {
        tool: "Edit".into(),
        file_path: Some(path.into()),
        command_preview: None,
        subagent_name: None,
    }
}

fn issue(number: u64) -> Issue {
    Issue {
        number,
        title: format!("Issue {number}"),
        body: String::new(),
        labels: vec![],
        state: "open".into(),
        html_url: String::new(),
        milestone: None,
        assignees: vec![],
    }
}

#[test]
fn edits_to_same_repo_file_in_two_worktrees_log_conflict() {
    let mut app = make_app();
    let a = app
        .pool
        .create_interaction_session(1, false, "m".into(), "orchestrator".into(), None);
    let b = app
        .pool
        .create_interaction_session(2, false, "m".into(), "orchestrator".into(), None);

    app.process_file_claim_event(a, &edit("/tmp/mock-worktrees/issue-1/tests/test_todo.py"));
    app.process_file_claim_event(b, &edit("/tmp/mock-worktrees/issue-2/tests/test_todo.py"));

    let conflicts: Vec<&str> = app
        .activity_log
        .entries()
        .iter()
        .map(|e| e.message.as_str())
        .filter(|m| m.starts_with("CONFLICT: tests/test_todo.py"))
        .collect();
    assert_eq!(conflicts.len(), 1, "entries: {conflicts:?}");
}

#[tokio::test]
async fn activity_log_lists_prs_sharing_files() {
    let mock = MockGitHubClient::new();
    let handle = mock.clone();
    let git = crate::git::MockGitOps::new()
        .with_commits_ahead(true)
        .with_changed_files(vec!["tests/test_todo.py".into()]);
    let mut app = make_app().with_git_ops(Box::new(git));
    app.gh_auth_ok = true;
    app.config = Some(
        toml::from_str(
            "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\nper_session_usd = 5.0\n\
             total_usd = 50.0\nalert_threshold_pct = 80\n[github]\nauto_pr = true\n[notifications]\n",
        )
        .expect("test config"),
    );
    app.github_client = Some(std::sync::Arc::new(mock));

    for (num, pr) in [(1u64, 101u64), (2, 102)] {
        app.state.issue_cache.insert(num, issue(num));
        handle.set_create_pr_response(pr);
        app.on_issue_session_completed(
            num,
            vec![num],
            true,
            0.0,
            Some(format!("maestro/issue-{num}")),
            Some(PathBuf::from(format!("/tmp/wt-1156-{num}"))),
            false,
        )
        .await;
    }

    let shared: Vec<&LogEntry> = app
        .activity_log
        .entries()
        .iter()
        .filter(|e| e.message.contains("both change"))
        .collect();
    assert_eq!(shared.len(), 1, "exactly one overlap line expected");

    let mut log = ActivityLog::new(10);
    log.push(LogEntry {
        timestamp: fixed_start(),
        ..shared[0].clone()
    });
    let mut terminal = Terminal::new(TestBackend::new(80, 4)).expect("terminal");
    terminal
        .draw(|f| log.draw_in_zone(f, f.area(), &Theme::dark(), false, &chrono::Utc))
        .expect("draw");
    assert_snapshot!(terminal.backend());
}
