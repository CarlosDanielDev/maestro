//! Tests for `App::handle_session_event` desktop-notification dispatch.
//! Extracted to keep `event_handler.rs` under the 400-LOC budget.

#![cfg(test)]

use super::App;
use crate::notifications::desktop::{FakeNotifier, NotifyError};
use crate::session::manager::SessionEvent;
use crate::session::types::{Session, StreamEvent};
use crate::tui::make_test_app;
use std::sync::Arc;

fn session_with_issue(issue: u64) -> Session {
    let mut s = Session::new(
        "fix bug".to_string(),
        "claude-opus-4-5".to_string(),
        "orchestrator".to_string(),
        Some(issue),
    );
    s.issue_title = Some(format!("Issue #{}", issue));
    s
}

fn promote_session(app: &mut App, session: Session) -> uuid::Uuid {
    let id = session.id;
    app.pool.enqueue(session);
    app.pool.try_promote();
    id
}

#[test]
fn app_dispatches_desktop_notification_on_session_completed() {
    let fake = FakeNotifier::new(true);
    let arc_fake: Arc<dyn crate::notifications::desktop::DesktopNotifier> = Arc::new(fake.clone());
    let mut app = make_test_app("notif-test-completed").with_desktop_notifier(arc_fake);

    let session_id = promote_session(&mut app, session_with_issue(42));
    let evt = SessionEvent {
        session_id,
        event: StreamEvent::Completed { cost_usd: 1.50 },
    };

    app.handle_session_event(evt);

    assert_eq!(fake.call_count(), 1);
    let calls = fake.calls();
    assert!(
        calls[0].title.contains("Session complete"),
        "title was: {}",
        calls[0].title
    );
    assert!(
        calls[0].title.contains("42"),
        "title should reference issue #42, got: {}",
        calls[0].title
    );
}

#[test]
fn app_skips_desktop_notification_when_notifier_disabled() {
    let fake = FakeNotifier::new(false);
    let arc_fake: Arc<dyn crate::notifications::desktop::DesktopNotifier> = Arc::new(fake.clone());
    let mut app = make_test_app("notif-test-disabled").with_desktop_notifier(arc_fake);

    let session_id = promote_session(&mut app, session_with_issue(99));
    let evt = SessionEvent {
        session_id,
        event: StreamEvent::Completed { cost_usd: 0.0 },
    };

    app.handle_session_event(evt);

    assert_eq!(fake.call_count(), 0);
}

#[test]
fn app_dispatches_desktop_notification_on_session_error() {
    let fake = FakeNotifier::new(true);
    let arc_fake: Arc<dyn crate::notifications::desktop::DesktopNotifier> = Arc::new(fake.clone());
    let mut app = make_test_app("notif-test-error").with_desktop_notifier(arc_fake);

    let session_id = promote_session(&mut app, session_with_issue(11));
    let evt = SessionEvent {
        session_id,
        event: StreamEvent::Error {
            message: "process exited with code 1".to_string(),
        },
    };

    app.handle_session_event(evt);

    assert_eq!(fake.call_count(), 1);
    let calls = fake.calls();
    assert!(
        calls[0].title.contains("Session errored"),
        "title was: {}",
        calls[0].title
    );
}

#[test]
fn tick_notify_error_logs_permission_denied_warning_then_drains() {
    let fake = FakeNotifier::new(true);
    fake.inject_error(NotifyError::PermissionDenied);
    let arc_fake: Arc<dyn crate::notifications::desktop::DesktopNotifier> = Arc::new(fake);
    let mut app = make_test_app("notif-test-perm-denied").with_desktop_notifier(arc_fake);

    let log_count_before = app.activity_log.entries().len();

    app.tick_notify_error();

    let log_count_after_first = app.activity_log.entries().len();
    assert_eq!(
        log_count_after_first,
        log_count_before + 1,
        "first tick must push exactly one log entry"
    );

    app.tick_notify_error();
    assert_eq!(
        app.activity_log.entries().len(),
        log_count_after_first,
        "second tick must not push another log entry"
    );
}
