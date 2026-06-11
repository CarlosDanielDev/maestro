//! Interactive turns through the normal session pipeline (#947).
//!
//! Phase 2 of the unified-interactive-sessions design (spec
//! `docs/superpowers/specs/2026-06-04-unified-interactive-sessions-design.md`
//! §4.3, §6): an interaction's turns run on a real `SessionMode::Interactive`
//! [`crate::session::types::Session`] in the pool — same prompt path,
//! provider routing, and telemetry funnel (`ManagedSession::handle_event`)
//! as a one-shot turn. The first turn is a normal `spawn`; follow-ups go
//! through `send_followup_turn` (`--resume <agent_session_id>`).
//!
//! The Interaction screen stays a [`TurnEvent`] consumer: this module
//! derives chat `TurnEvent`s from the session's `StreamEvent`s and applies
//! them through the existing `TuiDataEvent::InteractionTurnEvent` arm, so
//! the screen and the persisted `InteractionSession` view-model keep the
//! exact transcript semantics the retired `interaction_turn` loop had.

use chrono::Utc;
use uuid::Uuid;

use super::App;
use crate::session::interaction::{InteractionState, TurnEvent, TurnRecord, TurnRole};
use crate::session::types::{SessionMode, StreamEvent};

impl App {
    /// Dispatch one interaction turn through the normal pipeline (#947).
    /// First turn for the issue → register the pipeline session and
    /// `spawn` it (the issue work, narrated in chat). Later turns →
    /// `send_followup_turn` on the bound conversation.
    pub(crate) async fn dispatch_interaction_turn(
        &mut self,
        issue_number: u64,
        prompt: String,
        model: String,
    ) {
        let mode = self.interaction_mode_label(issue_number);

        let now = Utc::now();
        let Some(interaction) = self.pool.find_active_interaction_by_issue_mut(issue_number) else {
            tracing::warn!("SendInteractionTurn for #{issue_number} with no active interaction");
            return;
        };
        // Mirror the turn into the persisted view-model; the screen keeps
        // its own copy via the TurnStarted event below.
        interaction.history.push(TurnRecord {
            role: TurnRole::User,
            content: prompt.clone(),
            started_at: now,
            finished_at: Some(now),
        });
        interaction.history.push(TurnRecord {
            role: TurnRole::Agent,
            content: String::new(),
            started_at: now,
            finished_at: None,
        });
        interaction.state = InteractionState::Streaming;

        self.apply_interaction_turn_event(
            issue_number,
            TurnEvent::TurnStarted {
                role: TurnRole::Agent,
                at: now,
            },
        );

        let tx = self.pool.event_tx();
        if let Some(id) = self.pool.interactive_pipeline_session_id(issue_number) {
            let result = self
                .pool
                .get_active_mut(id)
                .map(|managed| managed.send_followup_turn(prompt, tx));
            if let Some(Err(e)) = result {
                self.fail_interaction_turn(
                    issue_number,
                    format!("failed to start follow-up turn: {e}"),
                );
            }
        } else {
            let Some(id) =
                self.pool
                    .ensure_interaction_pipeline_session(issue_number, prompt, model, mode)
            else {
                self.fail_interaction_turn(
                    issue_number,
                    "no interaction registered for this issue".to_string(),
                );
                return;
            };
            let result = match self.pool.get_active_mut(id) {
                Some(managed) => managed.spawn(tx).await,
                None => return,
            };
            if let Err(e) = result {
                self.fail_interaction_turn(issue_number, format!("failed to spawn turn: {e}"));
            }
        }
    }

    /// Derive chat [`TurnEvent`]s from an Interactive-mode session's stream
    /// (#947). Called from `handle_session_event` after the standard
    /// per-session machinery ran; a no-op for one-shot sessions.
    pub(crate) fn forward_interactive_stream_event(
        &mut self,
        session_id: Uuid,
        event: &StreamEvent,
    ) {
        let Some((session_mode, issue_number)) = self
            .pool
            .get_active_mut(session_id)
            .map(|m| (m.session.session_mode, m.session.issue_number))
        else {
            return;
        };
        if session_mode != SessionMode::Interactive {
            return;
        }
        let Some(issue_number) = issue_number else {
            return;
        };

        let turn_event = match event {
            StreamEvent::AssistantMessage { text } => {
                if let Some(interaction) =
                    self.pool.find_active_interaction_by_issue_mut(issue_number)
                    && let Some(turn) = streaming_agent_turn(interaction)
                {
                    turn.content.push_str(text);
                }
                Some(TurnEvent::Chunk(text.clone()))
            }
            StreamEvent::Completed { .. } => {
                let at = Utc::now();
                if let Some(interaction) =
                    self.pool.find_active_interaction_by_issue_mut(issue_number)
                {
                    if let Some(turn) = streaming_agent_turn(interaction) {
                        turn.finished_at = Some(at);
                    }
                    interaction.state = InteractionState::Idle;
                    // A /pushup terminator queued mid-stream fires now that
                    // the streamed output is preserved (#936 contract).
                    interaction.settle_queued_terminator();
                }
                Some(TurnEvent::TurnFinished { at })
            }
            StreamEvent::Error { message } => {
                let at = Utc::now();
                if let Some(interaction) =
                    self.pool.find_active_interaction_by_issue_mut(issue_number)
                {
                    if let Some(turn) = streaming_agent_turn(interaction) {
                        turn.finished_at = Some(at);
                    }
                    interaction.history.push(TurnRecord {
                        role: TurnRole::System,
                        content: message.clone(),
                        started_at: at,
                        finished_at: Some(at),
                    });
                    interaction.state = InteractionState::Idle;
                    interaction.settle_queued_terminator();
                }
                Some(TurnEvent::Error(message.clone()))
            }
            _ => None,
        };

        if let Some(event) = turn_event {
            self.apply_interaction_turn_event(issue_number, event);
        }
    }

    /// Settle a turn that never started (dispatch failure): close the
    /// streaming records, surface a `System` note, unlock the input.
    fn fail_interaction_turn(&mut self, issue_number: u64, message: String) {
        let at = Utc::now();
        if let Some(interaction) = self.pool.find_active_interaction_by_issue_mut(issue_number) {
            if let Some(turn) = streaming_agent_turn(interaction) {
                turn.finished_at = Some(at);
            }
            interaction.history.push(TurnRecord {
                role: TurnRole::System,
                content: message.clone(),
                started_at: at,
                finished_at: Some(at),
            });
            interaction.state = InteractionState::Idle;
        }
        self.apply_interaction_turn_event(issue_number, TurnEvent::Error(message));
    }

    /// Apply a derived [`TurnEvent`] through the existing data-event arm so
    /// screen application + activity logging + deferred-teardown handling
    /// stay in one place.
    fn apply_interaction_turn_event(&mut self, issue_number: u64, event: TurnEvent) {
        self.handle_data_event(crate::tui::app::TuiDataEvent::InteractionTurnEvent {
            issue_number,
            event,
        });
    }

    /// Agent-mode label for the pipeline session: resolved from the cached
    /// issue's labels like a one-shot launch, falling back to the
    /// configured default when the cache is cold.
    fn interaction_mode_label(&self, issue_number: u64) -> String {
        self.state
            .issue_cache
            .get(&issue_number)
            .map(|issue| issue.labels.clone())
            .map(|labels| {
                self.resolve_model_and_mode(&labels, Some(&self.selected_agent_id()))
                    .1
            })
            .unwrap_or_else(|| self.session_config.default_mode.clone())
    }
}

/// The in-flight agent record (last `Agent` turn not yet finished).
fn streaming_agent_turn(
    interaction: &mut crate::session::interaction::InteractionSession,
) -> Option<&mut TurnRecord> {
    interaction
        .history
        .last_mut()
        .filter(|t| t.role == TurnRole::Agent && t.finished_at.is_none())
}
