//! Integration tests for the `/pushup` marker → interactive session path
//! (#739 → #949, spec §4.4).
//!
//! Since #949 a marker carrying `issue_number` that matches a live
//! interactive session marks it `pr_linked` and posts a `System` turn —
//! the session STAYS OPEN (no wipe, no navigation) — and still enqueues
//! the existing `PrCreated` command (single read → both events). Legacy
//! markers (no `issue_number`) take only the `PrCreated` path. Mid-stream
//! markers defer the announcement to the turn boundary (#936).

use std::path::{Path, PathBuf};

use crate::session::interaction::{TurnRole, TurnState};
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
        None,
    )
}

fn pr_created_queued(app: &crate::tui::app::App, pr_number: u64) -> bool {
    app.pending_commands
        .iter()
        .any(|c| matches!(c, TuiCommand::PrCreated { pr_number: n, .. } if *n == pr_number))
}

fn pr_announcement_count(app: &mut crate::tui::app::App, id: uuid::Uuid) -> usize {
    app.pool
        .get_active_mut(id)
        .map(|m| {
            m.session
                .turns
                .iter()
                .filter(|t| t.role == TurnRole::System && t.content.contains("PR #7"))
                .count()
        })
        .unwrap_or(0)
}

#[tokio::test]
async fn marker_with_matching_issue_keeps_session_open_and_announces() {
    let home = tempfile::tempdir().unwrap();
    let marker_path = make_marker_dir(home.path());
    write_marker(&marker_path, 7, Some(42));

    let mut app = make_app_with_home(home.path().to_path_buf());
    let id = create_interaction(&mut app, 42);

    app.poll_last_pr_created_marker().await;

    // Marker consumed.
    assert!(!marker_path.exists(), "marker must be deleted");

    // #949: the session STAYS OPEN — no termination, no teardown.
    let live = app
        .pool
        .interactive_managed(42)
        .expect("session must stay live after a PR is linked");
    assert_eq!(live.session.pr_linked, Some(7));
    assert_ne!(live.session.status, SessionStatus::Killed);
    assert_eq!(
        pr_announcement_count(&mut app, id),
        1,
        "one System announcement on the transcript"
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

/// #936 → #949: an announcement deferred while a turn was streaming posts
/// once the turn settles — after the streamed output, never interleaved.
#[tokio::test]
async fn deferred_announcement_posts_after_streaming_turn_settles() {
    let home = tempfile::tempdir().unwrap();
    let marker_path = make_marker_dir(home.path());
    write_marker(&marker_path, 7, Some(42));

    let mut app = make_app_with_home(home.path().to_path_buf());
    let session_id = start_streaming_turn(&mut app, 42);

    // Marker arrives mid-stream: the flag is set, the announcement waits.
    app.poll_last_pr_created_marker().await;
    {
        let managed = app.pool.interactive_managed(42).expect("live");
        assert_eq!(managed.session.pr_linked, Some(7), "flag set immediately");
        assert!(managed.queued_pr_notice.is_some(), "announcement deferred");
    }
    assert_eq!(
        pr_announcement_count(&mut app, session_id),
        0,
        "no announcement while the turn streams"
    );

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

    // Announcement posted AFTER the preserved output; session still open.
    assert_eq!(pr_announcement_count(&mut app, session_id), 1);
    let managed = app.pool.get_active_mut(session_id).expect("registered");
    assert!(managed.queued_pr_notice.is_none(), "queue cleared");
    let turns = &managed.session.turns;
    let reply_idx = turns
        .iter()
        .position(|t| t.content == "streamed reply")
        .expect("streamed reply preserved");
    let announce_idx = turns
        .iter()
        .position(|t| t.content.contains("PR #7"))
        .expect("announcement present");
    assert!(
        announce_idx > reply_idx,
        "announcement must follow the streamed output, never interleave"
    );
    assert!(
        app.pool.interactive_managed(42).is_some(),
        "session stays live"
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

    // The seeded interaction for issue 99 must remain live, unflagged.
    let still_live = app
        .pool
        .interactive_managed(99)
        .expect("interaction 99 must remain live");
    assert_eq!(still_live.session.pr_linked, None);
}
