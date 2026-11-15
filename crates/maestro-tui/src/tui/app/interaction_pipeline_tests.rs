//! Tests for the interactive-turn pipeline dispatch (#947): follow-up
//! turns through the normal resumed-turn path, telemetry parity, and the
//! Interactive-mode guards on the one-shot completion machinery.

#![cfg(test)]

use std::sync::Arc;

use super::App;
use crate::agent_provider::test_fakes::{ScriptedEnd, ScriptedProvider, ScriptedTurn};
use crate::session::interaction::{TurnRole, TurnState};
use crate::session::types::{SessionMode, SessionStatus, StreamEvent};
use crate::tui::make_test_app;

/// Shared with `interaction_pipeline_guard_tests`. App with an open interaction for `issue` and a scripted pool provider.
pub(super) fn app_with_interaction(
    name: &str,
    issue: u64,
    turns: Vec<ScriptedTurn>,
) -> (App, Arc<ScriptedProvider>) {
    let mut app = make_test_app(name);
    let provider = Arc::new(ScriptedProvider::new(turns));
    app.pool.set_provider(provider.clone());
    app.pool.create_interaction_session(
        issue,
        false,
        "opus".to_string(),
        "orchestrator".to_string(),
        None,
    );
    let managed = app
        .pool
        .interactive_managed(issue)
        .expect("interaction just created");
    app.screen_state.interaction_screen =
        Some(crate::tui::screens::InteractionScreen::for_managed(managed));
    app.tui_mode = crate::tui::app::TuiMode::Interaction;
    (app, provider)
}

/// Pump `SessionEvent`s through the app until the channel goes quiet.
pub(super) async fn pump_session_events(app: &mut App) {
    while let Ok(Some(evt)) =
        tokio::time::timeout(std::time::Duration::from_millis(250), app.event_rx.recv()).await
    {
        app.handle_session_event(evt);
    }
}

pub(super) fn scripted_turn(text: &str, session_id: &'static str) -> ScriptedTurn {
    ScriptedTurn {
        events: vec![
            StreamEvent::AssistantMessage {
                text: text.to_string(),
            },
            StreamEvent::Completed { cost_usd: 0.05 },
        ],
        end: ScriptedEnd::Ok {
            exit_code: Some(0),
            session_id: Some(session_id),
        },
    }
}

#[tokio::test]
async fn first_turn_runs_through_pipeline_and_binds_resume_id() {
    let (mut app, provider) = app_with_interaction(
        "ip-first-turn",
        947,
        vec![scripted_turn("hello from the flow", "conv-947")],
    );

    app.dispatch_interaction_turn(947, "do the work".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;

    // A real Interactive-mode pipeline session exists and settled.
    let id = app
        .pool
        .interactive_pipeline_session_id(947)
        .expect("pipeline session registered");
    let managed = app.pool.get_active_mut(id).expect("active");
    assert_eq!(managed.session.session_mode, SessionMode::Interactive);
    // #948: the settle is intercepted — the session stays alive with the
    // one-shot outcome recorded in settled_from.
    assert_eq!(managed.session.status, SessionStatus::Interactive);
    assert_eq!(managed.session.settled_from, Some(SessionStatus::Completed));
    assert_eq!(
        managed.session.agent_session_id.as_deref(),
        Some("conv-947")
    );
    // Telemetry landed on the Session like any one-shot turn.
    assert!(!managed.session.call_log.is_empty());
    assert!((managed.session.cost_usd - 0.05).abs() < f64::EPSILON);

    // First request is a fresh conversation.
    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].resume_session_id.is_none());
    assert_eq!(requests[0].prompt, "do the work");

    // The persisted transcript lives on the Session (#948).
    let session = &app.pool.get_active_mut(id).expect("active").session;
    assert_eq!(session.turn_state, TurnState::Idle);
    let roles: Vec<TurnRole> = session.turns.iter().map(|t| t.role).collect();
    assert_eq!(roles, vec![TurnRole::User, TurnRole::Agent]);
    assert_eq!(session.turns[1].content, "hello from the flow");
    assert!(session.turns[1].finished_at.is_some());

    // #950: the screen is a pure view — projecting the live session yields the
    // same transcript it renders.
    let view = crate::tui::screens::InteractionView::from_session(session);
    assert_eq!(
        view.turns.last().map(|t| t.content.as_str()),
        Some("hello from the flow")
    );
}

#[tokio::test]
async fn followup_turn_resumes_the_bound_conversation() {
    let (mut app, provider) = app_with_interaction(
        "ip-followup",
        947,
        vec![
            scripted_turn("first answer", "conv-947"),
            scripted_turn("second answer", "conv-947"),
        ],
    );

    app.dispatch_interaction_turn(947, "first".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;
    app.dispatch_interaction_turn(947, "second".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;

    // Same pipeline session, resumed — never a second registration.
    let interactive: Vec<_> = app
        .pool
        .all_sessions()
        .into_iter()
        .filter(|s| s.session_mode == SessionMode::Interactive)
        .collect();
    assert_eq!(interactive.len(), 1);
    // #948: kept alive across both turns; the follow-up re-settles.
    assert_eq!(interactive[0].status, SessionStatus::Interactive);
    assert_eq!(interactive[0].settled_from, Some(SessionStatus::Completed));

    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].resume_session_id.is_none());
    assert_eq!(requests[1].resume_session_id.as_deref(), Some("conv-947"));
    assert_eq!(requests[1].prompt, "second");

    let session = app
        .pool
        .interactive_managed(947)
        .map(|m| m.session.clone())
        .expect("interaction alive");
    let roles: Vec<TurnRole> = session.turns.iter().map(|t| t.role).collect();
    assert_eq!(
        roles,
        vec![
            TurnRole::User,
            TurnRole::Agent,
            TurnRole::User,
            TurnRole::Agent
        ]
    );
    assert_eq!(session.turns[3].content, "second answer");
}

/// Spec §9 (#948): settling from a failure lands in the kept-alive state
/// and a follow-up retry resumes the SAME conversation.
#[tokio::test]
async fn failure_settles_interactive_and_followup_retries_on_same_resume_id() {
    let (mut app, provider) = app_with_interaction(
        "ip-failure-stays-alive",
        947,
        vec![
            ScriptedTurn {
                events: vec![StreamEvent::Error {
                    message: "clippy failed".to_string(),
                }],
                end: ScriptedEnd::Ok {
                    exit_code: Some(0),
                    session_id: Some("conv-947"),
                },
            },
            scripted_turn("fixed it", "conv-947"),
        ],
    );

    app.dispatch_interaction_turn(947, "run the gates".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;

    // Failure is not terminal for interactive sessions (spec §4.3).
    let id = app
        .pool
        .interactive_pipeline_session_id(947)
        .expect("session stays alive after failure");
    {
        let session = &app.pool.get_active_mut(id).expect("active").session;
        assert_eq!(session.status, SessionStatus::Interactive);
        assert_eq!(session.settled_from, Some(SessionStatus::Errored));
        assert_eq!(session.turn_state, TurnState::Idle, "input unlocked");
    }

    // Discuss + retry: the follow-up resumes the same conversation.
    app.dispatch_interaction_turn(947, "fix the clippy error".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;

    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].resume_session_id.as_deref(), Some("conv-947"));

    let session = &app.pool.get_active_mut(id).expect("active").session;
    assert_eq!(session.settled_from, Some(SessionStatus::Completed));
}

/// The #947 acceptance criterion: a follow-up turn emits the same
/// call-log/cost/token records as a one-shot turn fed the same stream.
#[tokio::test]
async fn followup_telemetry_matches_a_one_shot_turn() {
    use crate::session::types::{CallLogKind, TokenUsage};

    fn parity_events() -> Vec<StreamEvent> {
        vec![
            StreamEvent::AssistantMessage {
                text: "working on it".to_string(),
            },
            StreamEvent::ToolUse {
                tool: "Bash".to_string(),
                file_path: None,
                command_preview: Some("cargo test".to_string()),
                subagent_name: None,
            },
            StreamEvent::ToolResult {
                tool: "Bash".to_string(),
                is_error: false,
            },
            StreamEvent::TokenUpdate {
                usage: TokenUsage {
                    input_tokens: 1200,
                    output_tokens: 340,
                    cache_read_tokens: 800,
                    cache_creation_tokens: 0,
                },
            },
            StreamEvent::Completed { cost_usd: 0.07 },
        ]
    }

    // Control: a one-shot session fed the parity stream.
    let mut one_shot_app = make_test_app("ip-parity-oneshot");
    one_shot_app
        .pool
        .set_provider(Arc::new(ScriptedProvider::new(vec![ScriptedTurn {
            events: parity_events(),
            end: ScriptedEnd::Ok {
                exit_code: Some(0),
                session_id: Some("conv-os"),
            },
        }])));
    let session = crate::session::types::Session::new(
        "one shot".to_string(),
        "opus".to_string(),
        "orchestrator".to_string(),
        Some(942),
        None,
    );
    let one_shot_id = session.id;
    one_shot_app
        .add_session(session)
        .await
        .expect("one-shot enqueued");
    pump_session_events(&mut one_shot_app).await;
    let one_shot = one_shot_app
        .pool
        .get_active_mut(one_shot_id)
        .expect("one-shot session")
        .session
        .clone();

    // Interactive: first turn settles, then the follow-up gets the SAME
    // parity stream. Its incremental records must match the one-shot's.
    let (mut app, _provider) = app_with_interaction(
        "ip-parity-interactive",
        947,
        vec![
            scripted_turn("first", "conv-947"),
            ScriptedTurn {
                events: parity_events(),
                end: ScriptedEnd::Ok {
                    exit_code: Some(0),
                    session_id: Some("conv-947"),
                },
            },
        ],
    );
    app.dispatch_interaction_turn(947, "go".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;

    let id = app
        .pool
        .interactive_pipeline_session_id(947)
        .expect("pipeline session");
    let before_followup = app
        .pool
        .get_active_mut(id)
        .expect("session")
        .session
        .call_log
        .len();

    app.dispatch_interaction_turn(947, "follow up".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;

    let interactive = app
        .pool
        .get_active_mut(id)
        .expect("session")
        .session
        .clone();

    let one_shot_kinds: Vec<CallLogKind> = one_shot.call_log.iter().map(|e| e.kind).collect();
    let followup_kinds: Vec<CallLogKind> = interactive.call_log[before_followup..]
        .iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(
        followup_kinds, one_shot_kinds,
        "a follow-up turn must append the same call-log records as a one-shot turn"
    );
    assert!(
        (interactive.cost_usd - one_shot.cost_usd).abs() < f64::EPSILON,
        "cost accounting must match: follow-up {} vs one-shot {}",
        interactive.cost_usd,
        one_shot.cost_usd
    );
    assert_eq!(
        interactive.token_usage, one_shot.token_usage,
        "token accounting must match"
    );
}
