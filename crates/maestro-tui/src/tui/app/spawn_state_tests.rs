//! #1453: `ManagedSession::spawn` honors the session state machine. Each
//! production spawn caller still spawns from its legal state (`Queued`),
//! through its real path; a refused spawn is logged and shown, never
//! silent.

use crate::config::{Config, LoadedConfig};
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::session::worktree::MockWorktreeManager;
use crate::setup::setup_app_from_config_with_trust;
use crate::state::store::StateStore;
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::toast::Severity;
use crate::tui::input_handler::handle_key;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tempfile::TempDir;

/// A production-built, trusted App. The agent command does not exist, so
/// a spawn starts no real process: the session sits in `Spawning`.
fn app_with(sessions: &str, total_usd: f64) -> (App, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let toml_body = format!(
        r#"[project]
repo = "owner/repo"
base_branch = "main"
[sessions]
{sessions}
[budget]
per_session_usd = 5.0
total_usd = {total_usd}
alert_threshold_pct = 80
[notifications]
[agents]
default = "q"
[agents.q]
kind = "qwen"
command = "maestro-missing-agent-1453"
"#
    );
    let config: Config = toml::from_str(&toml_body).unwrap();
    let app = setup_app_from_config_with_trust(
        LoadedConfig {
            config,
            path: dir.path().join("maestro.toml"),
        },
        StateStore::new(dir.path().join("state-1453.json")),
        Box::new(MockWorktreeManager::new()),
        None,
        false,
        true,
    );
    (app, dir)
}

fn fresh_session() -> Session {
    Session::new(
        "probe".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(1453),
        None,
    )
}

fn logged(app: &App, needle: &str) -> bool {
    (app.activity_log.entries().iter()).any(|e| e.message.contains(needle))
}

fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    format!("{}", terminal.backend())
}

/// `app/session_lifecycle.rs`: `add_session` promotes a Queued session
/// and spawns it.
#[tokio::test]
async fn add_session_spawns_from_queued() {
    let (mut app, _dir) = app_with("", 50.0);
    let session = fresh_session();
    let id = session.id;

    app.add_session(session).await.unwrap();

    let status = app.pool.get_session(id).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Spawning));
    assert!(logged(&app, "Session started"));
    assert!(!logged(&app, "Spawn failed"));
}

/// `app/completion_pipeline.rs` retry: a Stalled session is retried; the
/// retry waits in the queue for the slot, and the tick promotion spawns
/// it from Queued once the original is finalized.
#[tokio::test]
async fn retry_spawns_from_queued_on_tick_promotion() {
    let (mut app, _dir) = app_with(
        "max_concurrent = 1\nmax_retries = 2\nretry_cooldown_secs = 0",
        50.0,
    );
    let original = fresh_session();
    let original_id = original.id;
    app.pool.enqueue(original);
    let _ = app.pool.try_promote();
    let managed = app.pool.get_active_mut(original_id).unwrap();
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
        (SessionStatus::Stalled, TransitionReason::HealthStall),
    ] {
        managed.session.transition_to(to, why).unwrap();
    }

    app.check_completions().await.unwrap();

    let retry = (app.pool.all_sessions().into_iter())
        .find(|s| s.id != original_id && s.retry_count == 1)
        .expect("the stalled session must be retried");
    assert_eq!(retry.status, SessionStatus::Spawning, "spawned from Queued");
    assert!(logged(&app, "Retrying (attempt 1/2)"));
    assert!(!logged(&app, "Spawn failed"));
}

/// `app/budget.rs`: `S` on a Warn gate spawns the parked (Queued) session.
#[tokio::test]
async fn budget_gate_s_spawns_from_queued() {
    let (mut app, _dir) = app_with("", 1.0);
    let spender = fresh_session();
    let spender_id = spender.id;
    app.pool.enqueue(spender);
    let _ = app.pool.try_promote();
    app.pool
        .get_active_mut(spender_id)
        .unwrap()
        .session
        .cost_usd = 0.35;
    app.sync_state();
    let session = fresh_session();
    let id = session.id;
    app.add_session(session).await.unwrap();
    assert!(
        matches!(app.tui_mode, TuiMode::BudgetPreSpawn { session_id } if session_id == id),
        "setup: Warn opens the gate; got {:?}",
        app.tui_mode
    );

    handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('S'), KeyModifiers::NONE),
    )
    .await;

    let status = app.pool.get_session(id).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Spawning));
    assert!(logged(&app, "post budget gate"));
    assert!(!logged(&app, "Spawn failed"));
}

/// `app/interaction_pipeline.rs`: the first turn spawns the interaction
/// session from Queued and runs the provider once.
#[tokio::test]
async fn interaction_first_turn_spawns_from_queued() {
    use crate::tui::app::interaction_pipeline_tests::{
        app_with_interaction, pump_session_events, scripted_turn,
    };
    let (mut app, provider) =
        app_with_interaction("spawn-1453", 1453, vec![scripted_turn("hi", "conv-1453")]);

    app.dispatch_interaction_turn(1453, "do the work".into(), "opus".into())
        .await;

    let status = (app.pool.interactive_managed(1453)).map(|m| m.session.status);
    assert_eq!(status, Some(SessionStatus::Spawning));
    pump_session_events(&mut app).await;
    assert_eq!(provider.requests.lock().unwrap().len(), 1, "one run");
    assert!(!logged(&app, "Spawn failed"));
}

/// A spawn the state machine refuses (a session already Running) starts
/// nothing, is logged as an error, and off the log screens shows as an
/// error toast.
#[tokio::test]
async fn refused_spawn_is_visible() {
    use crate::tui::screens::ScreenAction;
    let (mut app, _dir) = app_with("", 50.0);
    crate::tui::screen_dispatch::handle_screen_action(
        &mut app,
        ScreenAction::Push(TuiMode::Settings),
    );
    let mut live = fresh_session();
    for (to, why) in [
        (SessionStatus::Spawning, TransitionReason::Promoted),
        (SessionStatus::Running, TransitionReason::Spawned),
    ] {
        live.transition_to(to, why).unwrap();
    }
    let id = live.id;

    app.add_session(live).await.unwrap();

    let status = app.pool.get_session(id).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Running), "no second spawn");
    assert!(
        logged(&app, "Spawn failed: illegal transition RUNNING -> SPAWNING"),
        "the refusal must be logged"
    );
    assert!(!logged(&app, "Session started"));
    let out = draw(&mut app);
    let toast = app.toasts.visible(std::time::Instant::now());
    assert!(
        toast.is_some_and(|t| t.severity == Severity::Error && t.message.contains("Spawn failed")),
        "the refusal must raise an error toast"
    );
    assert!(
        out.contains("Spawn failed"),
        "the toast must be drawn:\n{out}"
    );
}

/// Judge round: a first turn that fails to start gets no extra spawn
/// report from this PR (only the turn's own failure surfaces it), and the
/// chat shows a System turn. #1470 trims that path to one alert; see
/// `failed_first_turn_raises_one_alert`.
#[tokio::test]
async fn failed_first_turn_adds_no_spawn_report() {
    use crate::agent_provider::test_fakes::{ScriptedEnd, ScriptedTurn};
    use crate::session::interaction::TurnRole;
    use crate::tui::app::interaction_pipeline_tests::{app_with_interaction, pump_session_events};
    let failing = ScriptedTurn {
        events: vec![],
        end: ScriptedEnd::SpawnFail,
    };
    let (mut app, _provider) = app_with_interaction("spawn-fail-1453", 1453, vec![failing]);

    app.dispatch_interaction_turn(1453, "do the work".into(), "opus".into())
        .await;
    pump_session_events(&mut app).await;

    let alerts = app.activity_log.take_alerts();
    assert!(
        !alerts.iter().any(|e| e.message.contains("Spawn failed")),
        "no spawn report on the first-turn path: {alerts:?}"
    );
    assert!(
        alerts.iter().any(|e| e.message.contains("turn failed")),
        "the turn failure must still surface: {alerts:?}"
    );
    let managed = app.pool.interactive_managed(1453).expect("interaction");
    assert!(
        (managed.session.turns.iter()).any(|t| t.role == TurnRole::System),
        "the failure must show as a System turn in the chat"
    );
}

/// #1470: a first turn whose provider fails to start raises one alert
/// (the WARN "turn failed", which names the cause) and one System turn,
/// not an extra ERR for the same failure.
#[tokio::test]
async fn failed_first_turn_raises_one_alert() {
    use crate::agent_provider::test_fakes::{ScriptedEnd, ScriptedTurn};
    use crate::session::interaction::TurnRole;
    use crate::tui::app::interaction_pipeline_tests::{app_with_interaction, pump_session_events};
    let failing = ScriptedTurn {
        events: vec![],
        end: ScriptedEnd::SpawnFail,
    };
    let (mut app, _provider) = app_with_interaction("one-alert-1470", 1470, vec![failing]);

    app.dispatch_interaction_turn(1470, "do the work".into(), "opus".into())
        .await;
    pump_session_events(&mut app).await;

    // Other alerts (e.g. the bypass-mode notice at launch) are not about
    // this failure; count only the ones that are.
    let alerts = app.activity_log.take_alerts();
    let failure: Vec<_> = (alerts.iter())
        .filter(|e| e.message.contains("scripted spawn failure"))
        .collect();
    assert_eq!(failure.len(), 1, "one failure, one alert: {alerts:?}");
    assert!(
        failure[0].message.contains("turn failed"),
        "the one alert must say what failed: {alerts:?}"
    );
    let managed = app.pool.interactive_managed(1470).expect("interaction");
    let system_turns = (managed.session.turns.iter())
        .filter(|t| t.role == TurnRole::System)
        .count();
    assert_eq!(system_turns, 1, "one System turn in the chat");
}

/// #1470: a one-shot session whose provider fails to start still raises
/// its ERR alert.
#[tokio::test]
async fn non_interaction_spawn_failure_still_alerts() {
    use crate::agent_provider::test_fakes::{ScriptedEnd, ScriptedProvider, ScriptedTurn};
    use crate::tui::activity_log::LogLevel;
    use crate::tui::app::interaction_pipeline_tests::pump_session_events;
    let mut app = crate::tui::make_test_app("one-shot-fail-1470");
    let failing = ScriptedTurn {
        events: vec![],
        end: ScriptedEnd::SpawnFail,
    };
    app.pool
        .set_provider(std::sync::Arc::new(ScriptedProvider::new(vec![failing])));
    let session = fresh_session();
    let id = session.id;

    app.add_session(session).await.unwrap();
    pump_session_events(&mut app).await;

    let status = app.pool.get_session(id).map(|s| s.status);
    assert_eq!(status, Some(SessionStatus::Errored), "setup: the spawn failed");
    let alerts = app.activity_log.take_alerts();
    assert!(
        (alerts.iter()).any(|e| e.level == LogLevel::Error
            && e.message.contains("scripted spawn failure")),
        "the one-shot spawn failure must still alert: {alerts:?}"
    );
}
