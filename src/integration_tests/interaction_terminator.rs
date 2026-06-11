//! Integration tests for #739 — InteractionLifecycleEvent + signal_terminator
//! wired into `poll_last_pr_created_marker`.
//!
//! A `/pushup` marker carrying `issue_number` that matches an active
//! interaction session must terminate that session AND still enqueue the
//! existing `PrCreated` command (single read → both events). Legacy markers
//! (no `issue_number`) take only the `PrCreated` path.

use std::path::{Path, PathBuf};

use crate::session::interaction::{CloseReason, InteractionState, TurnRecord, TurnRole};
use crate::session::interaction_lifecycle::InteractionLifecycleEvent;
use crate::tui::app::types::TuiCommand;
use crate::work::pr_marker::PrMarker;

fn make_marker_dir(home: &Path) -> PathBuf {
    let dir = home.join(".maestro");
    std::fs::create_dir_all(&dir).expect("create .maestro dir");
    dir.join("last-pr-created")
}

fn write_marker(path: &Path, pr_number: u64, issue_number: Option<u64>) {
    let marker = PrMarker {
        pr_number,
        owner: "owner".into(),
        repo: "repo".into(),
        issue_number,
        ts: chrono::Utc::now(),
    };
    marker.write_atomic(path).expect("write marker");
}

fn make_app_with_home(home: PathBuf) -> crate::tui::app::App {
    crate::tui::make_test_app("interaction-terminator").with_home_dir(home)
}

fn pr_created_queued(app: &crate::tui::app::App, pr_number: u64) -> bool {
    app.pending_commands
        .iter()
        .any(|c| matches!(c, TuiCommand::PrCreated { pr_number: n, .. } if *n == pr_number))
}

#[tokio::test]
async fn marker_with_matching_issue_terminates_interaction_and_enqueues_pr_created() {
    let home = tempfile::tempdir().unwrap();
    let marker_path = make_marker_dir(home.path());
    write_marker(&marker_path, 7, Some(42));

    let mut app = make_app_with_home(home.path().to_path_buf());
    app.pool.create_interaction_session(42, false);

    app.poll_last_pr_created_marker().await;

    // Marker consumed.
    assert!(!marker_path.exists(), "marker must be deleted");

    // Interaction terminated — no active session remains for issue 42.
    assert!(
        app.pool.find_active_interaction_by_issue(42).is_none(),
        "interaction for issue 42 must be terminated"
    );

    // close_reason is PrCreated { pr_number: 7 }.
    let reason = app
        .pool
        .interaction_close_reason(42)
        .expect("close_reason must be set");
    assert_eq!(*reason, CloseReason::PrCreated { pr_number: 7 });

    // The existing PrCreated path still fires (single read → both events).
    assert!(
        pr_created_queued(&app, 7),
        "TuiCommand::PrCreated must still be enqueued"
    );
}

/// Prepare the live slot as if a turn is mid-stream on the pipeline path
/// (#947): streaming agent record on the interaction + a registered
/// Interactive-mode pipeline session. Returns the pipeline session id.
fn start_streaming_turn(app: &mut crate::tui::app::App, issue: u64) -> uuid::Uuid {
    let id = app
        .pool
        .ensure_interaction_pipeline_session(
            issue,
            "work the issue".to_string(),
            "opus".to_string(),
            "orchestrator".to_string(),
        )
        .expect("pipeline session registered");
    let live = app
        .pool
        .find_active_interaction_by_issue_mut(issue)
        .expect("active");
    let now = chrono::Utc::now();
    live.history.push(TurnRecord {
        role: TurnRole::Agent,
        content: String::new(),
        started_at: now,
        finished_at: None,
    });
    live.state = InteractionState::Streaming;
    id
}

/// #936: a terminator queued while a turn was streaming must fire once that
/// turn settles back to `Idle` and its output is merged. Since #947 the
/// settle happens on the live slot when the pipeline session's `Completed`
/// stream event lands — same contract, no clone/merge dance.
#[tokio::test]
async fn queued_terminator_fires_after_streaming_turn_settles() {
    let home = tempfile::tempdir().unwrap();
    let mut app = make_app_with_home(home.path().to_path_buf());
    app.pool.create_interaction_session(42, false);
    let session_id = start_streaming_turn(&mut app, 42);

    // Marker arrives mid-stream: the live slot is `Streaming`, so
    // `signal_terminator` queues the event instead of firing it.
    {
        let live = app
            .pool
            .find_active_interaction_by_issue_mut(42)
            .expect("active");
        live.signal_terminator(InteractionLifecycleEvent::PrLinkedToIssue {
            pr_number: 7,
            issue_number: 42,
            owner: "owner".into(),
            repo: "repo".into(),
        });
        assert!(
            live.queued_terminator.is_some(),
            "precondition: terminator queued while streaming"
        );
    }

    // The stream delivers the reply, then settles.
    app.handle_session_event(crate::session::manager::SessionEvent {
        session_id,
        event: crate::session::types::StreamEvent::AssistantMessage {
            text: "streamed reply".to_string(),
        },
    });
    app.handle_session_event(crate::session::manager::SessionEvent {
        session_id,
        event: crate::session::types::StreamEvent::Completed { cost_usd: 0.01 },
    });

    // Terminator fired after the settle → no active session remains.
    assert!(
        app.pool.find_active_interaction_by_issue(42).is_none(),
        "queued terminator must fire once the streaming turn settles"
    );

    let closed = app
        .pool
        .interaction_by_issue(42)
        .expect("session still registered (terminated)");
    assert_eq!(closed.state, InteractionState::Terminated);
    assert_eq!(
        closed.close_reason,
        Some(CloseReason::PrCreated { pr_number: 7 })
    );
    assert!(
        closed.closed_at.is_some(),
        "closed_at must be stamped on the deferred fire"
    );
    assert!(
        closed.queued_terminator.is_none(),
        "queued_terminator must be cleared after firing"
    );
    assert!(
        closed.history.iter().any(|t| t.content == "streamed reply"),
        "the completed turn's output must be preserved (fire after settle)"
    );
}

/// #936 idempotency: if the user terminated the session by other means while a
/// turn was in flight, a late `Completed` stream event must NOT resurrect it.
#[tokio::test]
async fn completing_turn_does_not_resurrect_user_quit_session() {
    let home = tempfile::tempdir().unwrap();
    let mut app = make_app_with_home(home.path().to_path_buf());
    app.pool.create_interaction_session(42, false);
    let session_id = start_streaming_turn(&mut app, 42);

    // User quit mid-turn: the live slot is already Terminated.
    {
        let live = app
            .pool
            .find_active_interaction_by_issue_mut(42)
            .expect("active");
        live.state = InteractionState::Terminated;
        live.close_reason = Some(CloseReason::UserQuit);
    }

    // The turn's settle event arrives late.
    app.handle_session_event(crate::session::manager::SessionEvent {
        session_id,
        event: crate::session::types::StreamEvent::Completed { cost_usd: 0.01 },
    });

    let closed = app.pool.interaction_by_issue(42).expect("registered");
    assert_eq!(
        closed.state,
        InteractionState::Terminated,
        "a user-quit session must stay terminated"
    );
    assert_eq!(
        closed.close_reason,
        Some(CloseReason::UserQuit),
        "the original close reason must survive a late settle event"
    );
}

#[tokio::test]
async fn marker_with_no_matching_active_interaction_still_enqueues_pr_created() {
    let home = tempfile::tempdir().unwrap();
    let marker_path = make_marker_dir(home.path());
    write_marker(&marker_path, 7, Some(42));

    let mut app = make_app_with_home(home.path().to_path_buf());
    // No interaction seeded for issue 42.

    app.poll_last_pr_created_marker().await;

    assert!(!marker_path.exists(), "marker must be deleted");
    assert!(
        pr_created_queued(&app, 7),
        "PrCreated must still be enqueued when no session matches"
    );
}

#[tokio::test]
async fn legacy_marker_without_issue_number_enqueues_pr_created_only() {
    let home = tempfile::tempdir().unwrap();
    let marker_path = make_marker_dir(home.path());
    // Legacy marker JSON with no issue_number field.
    std::fs::write(
        &marker_path,
        r#"{"pr_number":3,"owner":"owner","repo":"repo","ts":"2026-01-01T00:00:00Z"}"#,
    )
    .unwrap();

    let mut app = make_app_with_home(home.path().to_path_buf());
    // Seed an unrelated interaction — must not be touched.
    app.pool.create_interaction_session(99, false);

    app.poll_last_pr_created_marker().await;

    assert!(!marker_path.exists(), "marker must be deleted");
    assert!(
        pr_created_queued(&app, 3),
        "PrCreated must be enqueued for legacy marker"
    );

    // The seeded interaction for issue 99 must remain active and Idle.
    let still_active = app
        .pool
        .find_active_interaction_by_issue(99)
        .expect("interaction 99 must remain active");
    assert_eq!(still_active.state, InteractionState::Idle);
}
