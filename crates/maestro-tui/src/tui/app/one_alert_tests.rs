//! #1470: one failure raises exactly one alert. While the chat for an
//! interaction is open, its error shows as the screen's "turn failed" WARN
//! (plus a System turn); with no chat open for that issue (Esc'd, in the
//! background, or another issue on screen) it keeps the session ERR. Never
//! two, never zero.

use crate::agent_provider::test_fakes::{ScriptedEnd, ScriptedProvider, ScriptedTurn};
use crate::session::interaction::TurnRole;
use crate::session::types::{Session, SessionStatus};
use crate::tui::activity_log::{LogEntry, LogLevel};
use crate::tui::app::interaction_pipeline_tests::{
    app_with_interaction, pump_session_events, scripted_turn,
};
use crate::tui::app::App;
use crate::tui::input_handler::handle_key;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const SPAWN_FAIL: &str = "scripted spawn failure";

fn spawn_fail() -> ScriptedTurn {
    ScriptedTurn {
        events: vec![],
        end: ScriptedEnd::SpawnFail,
    }
}

/// Alerts about the failure only: the launch also raises an unrelated
/// bypass-mode notice.
fn failure_alerts(app: &mut App, needle: &str) -> Vec<LogEntry> {
    let alerts = app.activity_log.take_alerts();
    (alerts.into_iter())
        .filter(|e| e.message.contains(needle))
        .collect()
}

/// Leave the chat the way a user does: `Esc` through `handle_key` on a
/// drawn terminal (≥ 80x24, so the size guard does not eat the key).
async fn leave_chat(app: &mut App) {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    handle_key(app, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).await;
    assert!(
        app.screen_state.interaction_screen.is_none(),
        "setup: Esc closes the chat; mode {:?}",
        app.tui_mode
    );
}

/// Chat open for the session: one alert (the WARN "turn failed", which
/// names the cause) and one System turn, not an extra ERR.
#[tokio::test]
async fn failed_first_turn_raises_one_alert() {
    let (mut app, _provider) = app_with_interaction("one-alert-1470", 1470, vec![spawn_fail()]);

    app.dispatch_interaction_turn(1470, "do the work".into(), "opus".into())
        .await;
    pump_session_events(&mut app).await;

    let failure = failure_alerts(&mut app, SPAWN_FAIL);
    assert_eq!(failure.len(), 1, "one failure, one alert: {failure:?}");
    assert!(
        failure[0].message.contains("#1470 turn failed"),
        "the one alert must say what failed: {failure:?}"
    );
    let managed = app.pool.interactive_managed(1470).expect("interaction");
    let system_turns = (managed.session.turns.iter())
        .filter(|t| t.role == TurnRole::System)
        .count();
    assert_eq!(system_turns, 1, "one System turn in the chat");
}

/// The first-turn spawn handshake is non-blocking (#803): the failure can
/// land after the user left the chat. It must still raise one alert.
#[tokio::test]
async fn background_first_turn_failure_raises_one_alert() {
    let (mut app, _provider) = app_with_interaction("bg-first-1470", 1470, vec![spawn_fail()]);

    app.dispatch_interaction_turn(1470, "do the work".into(), "opus".into())
        .await;
    leave_chat(&mut app).await;
    pump_session_events(&mut app).await;

    let failure = failure_alerts(&mut app, SPAWN_FAIL);
    assert_eq!(failure.len(), 1, "never silent, never two: {failure:?}");
    assert_eq!(failure[0].level, LogLevel::Error, "{failure:?}");
    assert_eq!(failure[0].session_label, "#1470", "{failure:?}");
}

/// A follow-up turn that fails after the user left the chat raises one
/// alert.
#[tokio::test]
async fn background_followup_failure_raises_one_alert() {
    let followup_fail = ScriptedTurn {
        events: vec![],
        end: ScriptedEnd::FailedStatus {
            status: "1",
            stderr: "scripted follow-up failure",
        },
    };
    let (mut app, _provider) = app_with_interaction(
        "bg-followup-1470",
        1470,
        vec![scripted_turn("hi", "conv-1470"), followup_fail],
    );
    app.dispatch_interaction_turn(1470, "do the work".into(), "opus".into())
        .await;
    pump_session_events(&mut app).await;
    let _ = app.activity_log.take_alerts();

    app.dispatch_interaction_turn(1470, "and more".into(), "opus".into())
        .await;
    leave_chat(&mut app).await;
    pump_session_events(&mut app).await;

    let failure = failure_alerts(&mut app, "scripted follow-up failure");
    assert_eq!(failure.len(), 1, "never silent, never two: {failure:?}");
    assert_eq!(failure[0].level, LogLevel::Error, "{failure:?}");
}

/// Chat open for #1470, #1471 fails in the background: one alert, and it
/// names #1471. The #1470 screen does not take #1471's event.
#[tokio::test]
async fn other_issue_failure_names_its_own_issue() {
    let (mut app, _provider) = app_with_interaction("other-1470", 1470, vec![spawn_fail()]);
    app.pool.create_interaction_session(
        1471,
        false,
        "opus".to_string(),
        "orchestrator".to_string(),
        None,
    );

    app.dispatch_interaction_turn(1471, "do the work".into(), "opus".into())
        .await;
    pump_session_events(&mut app).await;

    let failure = failure_alerts(&mut app, SPAWN_FAIL);
    assert_eq!(failure.len(), 1, "one failure, one alert: {failure:?}");
    let text = format!("{} {}", failure[0].session_label, failure[0].message);
    assert!(
        text.contains("#1471") && !text.contains("#1470"),
        "the alert must name #1471: {failure:?}"
    );
    let screen = app.screen_state.interaction_screen.as_ref().unwrap();
    assert!(screen.is_for_issue(1470), "setup: #1470 stays on screen");
}

/// A one-shot session whose provider fails to start keeps its ERR alert,
/// exactly one.
#[tokio::test]
async fn non_interaction_spawn_failure_still_alerts() {
    let mut app = crate::tui::make_test_app("one-shot-fail-1470");
    let provider = ScriptedProvider::new(vec![spawn_fail()]);
    app.pool.set_provider(std::sync::Arc::new(provider));
    let session = Session::new(
        "probe".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(1470),
        None,
    );
    let id = session.id;

    app.add_session(session).await.unwrap();
    pump_session_events(&mut app).await;

    let status = app.pool.get_session(id).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Errored), "setup: spawn failed");
    let failure = failure_alerts(&mut app, SPAWN_FAIL);
    assert_eq!(failure.len(), 1, "one failure, one alert: {failure:?}");
    assert_eq!(failure[0].level, LogLevel::Error, "{failure:?}");
}
