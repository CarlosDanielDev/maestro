//! Integration tests for #739 — InteractionLifecycleEvent + signal_terminator
//! wired into `poll_last_pr_created_marker`, ported to the unified
//! interactive session (#948).
//!
//! A `/pushup` marker carrying `issue_number` that matches a live
//! interactive session must terminate that session AND still enqueue the
//! existing `PrCreated` command (single read → both events). Legacy markers
//! (no `issue_number`) take only the `PrCreated` path.

use std::path::{Path, PathBuf};

use crate::session::interaction::{TurnRole, TurnState};
use crate::session::interaction_lifecycle::InteractionLifecycleEvent;
use crate::session::transition::TransitionReason;
use crate::session::types::SessionStatus;
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

fn create_interaction(app: &mut crate::tui::app::App, issue: u64) -> uuid::Uuid {
    app.pool.create_interaction_session(
        issue,
        false,
        "opus".to_string(),
        "orchestrator".to_string(),
    )
}

fn pr_created_queued(app: &crate::tui::app::App, pr_number: u64) -> bool {
    app.pending_commands
        .iter()
        .any(|c| matches!(c, TuiCommand::PrCreated { pr_number: n, .. } if *n == pr_number))
}

/// Last transition recorded on the session behind `id`.
fn last_transition_reason(
    app: &mut crate::tui::app::App,
    id: uuid::Uuid,
) -> Option<TransitionReason> {
    app.pool
        .get_active_mut(id)
        .and_then(|m| m.session.transition_history.last().map(|t| t.reason))
}

#[tokio::test]
async fn marker_with_matching_issue_terminates_interaction_and_enqueues_pr_created() {
    let home = tempfile::tempdir().unwrap();
    let marker_path = make_marker_dir(home.path());
    write_marker(&marker_path, 7, Some(42));

    let mut app = make_app_with_home(home.path().to_path_buf());
    let id = create_interaction(&mut app, 42);

    app.poll_last_pr_created_marker().await;

    // Marker consumed.
    assert!(!marker_path.exists(), "marker must be deleted");

    // Interaction terminated — no live session remains for issue 42.
    assert!(
        app.pool.interactive_managed(42).is_none(),
        "interaction for issue 42 must be terminated"
    );

    // The termination is audited as PrLinked on the unified session.
    let managed = app.pool.get_active_mut(id).expect("session registered");
    assert_eq!(managed.session.status, SessionStatus::Killed);
    assert_eq!(
        last_transition_reason(&mut app, id),
        Some(TransitionReason::PrLinked)
    );

    // The existing PrCreated path still fires (single read → both events).
    assert!(
        pr_created_queued(&app, 7),
        "TuiCommand::PrCreated must still be enqueued"
    );
}

/// Prepare the live session as if a turn is mid-stream on the pipeline
/// path (#947): streaming agent record + `TurnState::Streaming`.
fn start_streaming_turn(app: &mut crate::tui::app::App, issue: u64) -> uuid::Uuid {
    let id = create_interaction(app, issue);
    let managed = app
        .pool
        .interactive_managed_mut(issue)
        .expect("live interaction");
    let now = chrono::Utc::now();
    managed
        .session
        .turns
        .push(crate::session::interaction::TurnRecord {
            role: TurnRole::Agent,
            content: String::new(),
            started_at: now,
            finished_at: None,
        });
    managed.session.turn_state = TurnState::Streaming;
    id
}

/// #936: a terminator queued while a turn was streaming must fire once that
/// turn settles back to idle and its output is merged. Since #948 the
/// settle happens on the live session when its `Completed` stream event
/// lands — same contract, no clone/merge dance.
#[tokio::test]
async fn queued_terminator_fires_after_streaming_turn_settles() {
    let home = tempfile::tempdir().unwrap();
    let mut app = make_app_with_home(home.path().to_path_buf());
    let session_id = start_streaming_turn(&mut app, 42);

    // Marker arrives mid-stream: the turn is streaming, so
    // `signal_terminator` queues the event instead of firing it.
    {
        let managed = app
            .pool
            .interactive_managed_mut(42)
            .expect("live interaction");
        managed.signal_terminator(InteractionLifecycleEvent::PrLinkedToIssue {
            pr_number: 7,
            issue_number: 42,
            owner: "owner".into(),
            repo: "repo".into(),
        });
        assert!(
            managed.queued_terminator.is_some(),
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

    // Terminator fired after the settle → no live session remains.
    assert!(
        app.pool.interactive_managed(42).is_none(),
        "queued terminator must fire once the streaming turn settles"
    );

    let managed = app.pool.get_active_mut(session_id).expect("registered");
    assert_eq!(managed.session.status, SessionStatus::Killed);
    assert!(
        managed.queued_terminator.is_none(),
        "queued_terminator must be cleared after firing"
    );
    assert!(
        managed
            .session
            .turns
            .iter()
            .any(|t| t.content == "streamed reply"),
        "the completed turn's output must be preserved (fire after settle)"
    );
    assert_eq!(
        last_transition_reason(&mut app, session_id),
        Some(TransitionReason::PrLinked)
    );
}

/// #936 idempotency: if the user terminated the session by other means while
/// a turn was in flight, a late settle must NOT fire the queued terminator
/// twice or resurrect the session.
#[tokio::test]
async fn completing_turn_does_not_resurrect_user_quit_session() {
    let home = tempfile::tempdir().unwrap();
    let mut app = make_app_with_home(home.path().to_path_buf());
    let session_id = start_streaming_turn(&mut app, 42);

    // User quit mid-turn: the session is already terminal.
    {
        let managed = app
            .pool
            .interactive_managed_mut(42)
            .expect("live interaction");
        managed
            .session
            .transition_to(SessionStatus::Killed, TransitionReason::UserKill)
            .unwrap();
    }

    // The turn's settle event arrives late.
    app.handle_session_event(crate::session::manager::SessionEvent {
        session_id,
        event: crate::session::types::StreamEvent::Completed { cost_usd: 0.01 },
    });

    let managed = app.pool.get_active_mut(session_id).expect("registered");
    assert_eq!(
        managed.session.status,
        SessionStatus::Killed,
        "a user-quit session must stay terminated"
    );
    assert_eq!(
        last_transition_reason(&mut app, session_id),
        Some(TransitionReason::UserKill),
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
    create_interaction(&mut app, 99);

    app.poll_last_pr_created_marker().await;

    assert!(!marker_path.exists(), "marker must be deleted");
    assert!(
        pr_created_queued(&app, 3),
        "PrCreated must be enqueued for legacy marker"
    );

    // The seeded interaction for issue 99 must remain live and idle.
    let still_live = app
        .pool
        .interactive_managed(99)
        .expect("interaction 99 must remain live");
    assert_eq!(still_live.session.turn_state, TurnState::Idle);
}
