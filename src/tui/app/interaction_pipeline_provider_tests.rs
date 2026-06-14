//! Multi-provider tests for the interactive-turn pipeline (#929): per-agent
//! model resolution on the first turn and graceful failure when a provider
//! cannot drive a conversational turn. Split from
//! `interaction_pipeline_tests.rs` (file-size budget); reuses its shared
//! helpers.

#![cfg(test)]

use std::sync::Arc;

use super::interaction_pipeline_tests::{app_with_interaction, pump_session_events, scripted_turn};
use crate::agent_provider::test_fakes::{ScriptedEnd, ScriptedProvider, ScriptedTurn};
use crate::session::interaction::{TurnRole, TurnState};
use crate::session::types::StreamEvent;
use crate::tui::make_test_app;

/// #929: the first interaction turn must use the SELECTED agent's
/// configured model, not the Claude default. Manual QA hit
/// `Model not found: opus/.` because opencode was handed Claude's `opus`.
#[tokio::test]
async fn first_turn_resolves_model_from_selected_agent_not_claude_default() {
    let mut app = make_test_app("ip-model-per-agent");
    app.config = Some(
        toml::from_str(
            r#"
[project]
repo = "owner/repo"
[sessions]
default_model = "opus"
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
[agents]
default = "claude"
[agents.claude]
kind = "claude"
enabled = true
command = "claude"
[agents.opencode]
kind = "opencode"
enabled = true
command = "opencode"
model = "anthropic/claude-sonnet-4"
"#,
        )
        .expect("test config parse"),
    );
    let provider = Arc::new(ScriptedProvider::new(vec![scripted_turn("hi", "conv-1")]));
    app.pool.set_provider(provider.clone());
    app.pool.create_interaction_session(
        942,
        false,
        "opus".to_string(),
        "orchestrator".to_string(),
        Some("opencode".to_string()),
    );
    let managed = app
        .pool
        .interactive_managed(942)
        .expect("interaction just created");
    app.screen_state.interaction_screen =
        Some(crate::tui::screens::InteractionScreen::for_managed(managed));
    app.tui_mode = crate::tui::app::TuiMode::Interaction;

    app.dispatch_interaction_turn(942, "hello".to_string(), "opus".to_string())
        .await;

    let id = app
        .pool
        .interactive_pipeline_session_id(942)
        .expect("session is alive");
    let session = &app.pool.get_active_mut(id).expect("active").session;
    assert_eq!(
        session.model, "anthropic/claude-sonnet-4",
        "first turn must use the selected agent's configured model, not Claude's default"
    );
}

/// #929 AC#5: a provider that fails a conversational turn surfaces a
/// `System` turn explaining it and leaves the session Idle/usable — no
/// panic, no silent hang. Characterizes the unification's failure-settle
/// path (#947) which already implements this.
#[tokio::test]
async fn failed_turn_surfaces_system_explanation_and_stays_usable() {
    let (mut app, _provider) = app_with_interaction(
        "ip-failure-system-turn",
        929,
        vec![ScriptedTurn {
            events: vec![StreamEvent::Error {
                message: "provider has no conversational support".to_string(),
            }],
            end: ScriptedEnd::Ok {
                exit_code: Some(1),
                session_id: None,
            },
        }],
    );

    app.dispatch_interaction_turn(929, "hello".to_string(), "opus".to_string())
        .await;
    pump_session_events(&mut app).await;

    let id = app
        .pool
        .interactive_pipeline_session_id(929)
        .expect("session stays alive after a failed turn");
    let session = &app.pool.get_active_mut(id).expect("active").session;

    assert_eq!(session.turn_state, TurnState::Idle, "input stays usable");
    let system_turn = session
        .turns
        .iter()
        .find(|t| t.role == TurnRole::System)
        .expect("a System turn must explain the failure to the user");
    assert!(
        system_turn.content.contains("no conversational support"),
        "System turn must carry the provider's explanation, got: {}",
        system_turn.content
    );
}
