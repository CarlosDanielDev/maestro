//! #1412: `maestro run --once` must exit when the last attempt of a session
//! ends `Errored`, and keep waiting while that error can still be retried.

use super::*;
use crate::session::manager::SessionEvent;
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus, StreamEvent};

/// Retries allowed, with a cooldown long enough that no test ever spawns one.
const SESSIONS_TOML: &str = "[project]\nrepo = \"owner/repo\"\n\
     [sessions]\nmax_retries = 2\nretry_cooldown_secs = 3600\n\
     [budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
     [github]\n[notifications]\n[gates]\nenabled = false\n";

fn once_app(label: &str, with_config: bool) -> App {
    let mut app = make_test_app(label);
    app.once_mode = true;
    if with_config {
        app.config = Some(toml::from_str(SESSIONS_TOML).expect("test config parse"));
    }
    app
}

/// Puts one session in `pool.active` at `Spawning`, as a real launch does.
fn spawn_one(app: &mut App) -> uuid::Uuid {
    let session = Session::new(
        "prompt".into(),
        "opus".into(),
        "orchestrator".into(),
        None,
        None,
    );
    let id = session.id;
    app.pool.enqueue(session);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(id).expect("promoted");
    assert!(
        managed
            .session
            .transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .is_ok()
    );
    id
}

/// Drives `id` to `Errored` through the same event a failed spawn sends.
fn error_out(app: &mut App, id: uuid::Uuid, message: &str) {
    let event = StreamEvent::Error {
        message: message.to_string(),
    };
    app.handle_session_event(SessionEvent {
        session_id: id,
        event,
    });
    let status = app.pool.get_session(id).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Errored), "setup: errored");
}

#[tokio::test]
async fn once_counts_non_retryable_errored_session_as_done() {
    use crate::agent_provider::claude::HEADLESS_NO_PERMISSION;
    let refusal = format!("Error: invalid agent provider config: {HEADLESS_NO_PERMISSION}");
    // (case, retry_count, error message)
    let cases = [
        ("out of retries", 2, "boom".to_string()),
        ("permission refusal", 0, refusal),
    ];
    for (case, retry_count, message) in cases {
        let mut app = once_app("once-errored-done", true);
        let id = spawn_one(&mut app);
        app.pool.get_active_mut(id).unwrap().session.retry_count = retry_count;
        error_out(&mut app, id, &message);
        app.pool
            .get_active_mut(id)
            .unwrap()
            .session
            .log_activity(message.clone());

        app.check_completions().await.expect("tick");

        assert!(app.all_done(), "{case}: errored session must count as done");
        assert!(completion_ready(&app, false), "{case}: --once must exit");
    }
}

#[tokio::test]
async fn once_waits_for_retryable_errored_session() {
    let mut app = once_app("once-errored-retryable", true);
    let id = spawn_one(&mut app);
    error_out(&mut app, id, "transient");
    // An earlier retry just happened: the cooldown holds this one back.
    app.pool.get_active_mut(id).unwrap().session.last_retry_at = Some(chrono::Utc::now());

    app.check_completions().await.expect("tick");

    assert!(
        app.pool.get_active_mut(id).is_some(),
        "a retryable session stays active"
    );
    assert!(!app.all_done());
    assert!(!completion_ready(&app, false), "--once must keep waiting");
}

#[tokio::test]
async fn once_loop_returns_after_spawning_to_errored() {
    let mut app = once_app("once-loop-errored", false);
    let id = spawn_one(&mut app);
    // The failed spawn reaches the loop as a stream event, like production.
    let tx = app.pool.event_tx();
    tx.send(SessionEvent {
        session_id: id,
        event: StreamEvent::Error {
            message: "agent binary not found".into(),
        },
    })
    .expect("send");

    let mut terminal = None;
    let result =
        tokio::time::timeout(Duration::from_secs(10), event_loop(&mut terminal, &mut app)).await;

    assert!(result.is_ok(), "--once loop hung on an errored session");
    let status = app.pool.all_sessions().first().map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Errored));
}

/// #1266: headless `--once` cannot raise the budget cap, so the cap pauses
/// the agent, ends the loop, and exits non-zero naming the paused count.
#[tokio::test]
async fn once_loop_returns_at_the_budget_cap() {
    let mut app = once_app("once-budget-cap", false);
    app.budget_enforcer = Some(crate::budget::BudgetEnforcer::new(100.0, 1.0, 80));
    let id = spawn_one(&mut app);
    let managed = app.pool.get_active_mut(id).expect("active");
    assert!(
        managed
            .session
            .transition_to(SessionStatus::Running, TransitionReason::Spawned)
            .is_ok()
    );
    let tx = app.pool.event_tx();
    for _ in 0..2 {
        let event = StreamEvent::CostUpdate { cost_usd: 2.0 };
        tx.send(SessionEvent {
            session_id: id,
            event,
        })
        .expect("send");
    }

    let mut terminal = None;
    let result =
        tokio::time::timeout(Duration::from_secs(10), event_loop(&mut terminal, &mut app)).await;

    assert!(result.is_ok(), "--once loop hung at the budget cap");
    let status = app.pool.get_session(id).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Paused));
    let err = once_exit_status(&app).expect_err("a paused run is not a success");
    assert!(err.to_string().contains("1 session(s) paused"), "{err}");
}
