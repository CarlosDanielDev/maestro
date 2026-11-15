//! Guard + edge-case tests for the interactive-turn pipeline (#947): the
//! Interactive-mode exemptions on the one-shot completion machinery,
//! dispatch failure paths, and transcript sanitation. Split from
//! `interaction_pipeline_tests.rs` (file-size budget), which keeps the
//! happy-path dispatch/resume/telemetry tests and the shared helpers.

#![cfg(test)]

use std::sync::Arc;

use super::interaction_pipeline_tests::{app_with_interaction, pump_session_events, scripted_turn};
use crate::agent_provider::test_fakes::{ScriptedEnd, ScriptedProvider, ScriptedTurn};
use crate::session::interaction::{TurnRole, TurnState};
use crate::session::types::StreamEvent;
use crate::tui::make_test_app;

#[tokio::test]
async fn interactive_completion_skips_one_shot_completion_machinery() {
    let fake = crate::notifications::desktop::FakeNotifier::new(true);
    let arc_fake: Arc<dyn crate::notifications::desktop::DesktopNotifier> = Arc::new(fake.clone());
    let (mut app, _provider) = app_with_interaction(
        "ip-no-completion-pipeline",
        947,
        vec![scripted_turn("done", "conv-947")],
    );
    app = app.with_desktop_notifier(arc_fake);

    app.dispatch_interaction_turn(947, "go".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;

    // No gates/PR/teardown pipeline, no per-turn desktop noise.
    assert!(
        app.pending_issue_completions.is_empty(),
        "interactive settle must not feed the one-shot completion pipeline"
    );
    assert_eq!(fake.call_count(), 0);
}

#[tokio::test]
async fn interactive_assistant_pr_url_does_not_trigger_auto_review() {
    let (mut app, _provider) = app_with_interaction(
        "ip-no-pr-autodetect",
        947,
        vec![ScriptedTurn {
            events: vec![
                StreamEvent::AssistantMessage {
                    text: "opened https://github.com/owner/repo/pull/123".to_string(),
                },
                StreamEvent::Completed { cost_usd: 0.01 },
            ],
            end: ScriptedEnd::Ok {
                exit_code: Some(0),
                session_id: Some("conv-947"),
            },
        }],
    );

    app.dispatch_interaction_turn(947, "push it".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;

    assert!(
        !app.pending_commands
            .iter()
            .any(|c| matches!(c, crate::tui::app::TuiCommand::PrCreated { .. })),
        "PR auto-detect (#327) must stay off for interactive sessions — \
         PR handling is the #739 marker path, reworked in Phase 4"
    );
}

#[tokio::test]
async fn provider_error_unlocks_the_turn_with_a_system_note() {
    let (mut app, _provider) = app_with_interaction(
        "ip-error-unlocks",
        947,
        vec![ScriptedTurn {
            events: vec![],
            end: ScriptedEnd::FailedStatus {
                status: "1",
                stderr: "boom",
            },
        }],
    );

    app.dispatch_interaction_turn(947, "go".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;

    let session = app
        .pool
        .interactive_managed(947)
        .map(|m| m.session.clone())
        .expect("interaction alive");
    assert_eq!(
        session.turn_state,
        TurnState::Idle,
        "a failed turn must settle back to Idle so the input unlocks"
    );
    assert!(
        session.turns.iter().any(|t| t.role == TurnRole::System),
        "failure surfaces as a System turn"
    );
}

#[tokio::test]
async fn unknown_stream_lines_never_reach_the_transcript() {
    // Security informational (PR #991 review): `StreamEvent::Unknown.raw`
    // is unparsed provider output — potentially hostile terminal escapes.
    // It must never enter the chat transcript, the persisted history, or
    // the call log.
    let hostile = "\u{1b}]0;evil\u{7}\u{1b}[2J$(rm -rf /)";
    let (mut app, _provider) = app_with_interaction(
        "ip-unknown-sanitized",
        947,
        vec![ScriptedTurn {
            events: vec![
                StreamEvent::Unknown {
                    raw: hostile.to_string(),
                },
                StreamEvent::AssistantMessage {
                    text: "ok".to_string(),
                },
                StreamEvent::Completed { cost_usd: 0.01 },
            ],
            end: ScriptedEnd::Ok {
                exit_code: Some(0),
                session_id: Some("conv-947"),
            },
        }],
    );

    app.dispatch_interaction_turn(947, "go".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;

    let session = app
        .pool
        .interactive_managed(947)
        .map(|m| m.session.clone())
        .expect("interaction alive");

    // #950: the screen renders a projection of the live session.
    let view = crate::tui::screens::InteractionView::from_session(&session);
    assert_eq!(view.turns.last().map(|t| t.content.as_str()), Some("ok"));
    assert!(
        !session.turns.iter().any(|t| t.content.contains("evil")),
        "raw unparsed lines must never enter the persisted history"
    );

    let id = app
        .pool
        .interactive_pipeline_session_id(947)
        .expect("pipeline session");
    let session = &app.pool.get_active_mut(id).expect("active").session;
    assert!(
        !session
            .call_log
            .iter()
            .any(|e| e.payload_json.contains("evil")),
        "Unknown events are dropped from the call log (#868 contract)"
    );
}

#[tokio::test]
async fn dispatch_without_interaction_is_a_noop() {
    let mut app = make_test_app("ip-no-interaction");
    app.pool
        .set_provider(Arc::new(ScriptedProvider::new(vec![])));

    app.dispatch_interaction_turn(947, "go".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;

    assert!(app.pool.interactive_pipeline_session_id(947).is_none());
}
