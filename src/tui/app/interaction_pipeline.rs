//! Interactive turns through the normal session pipeline (#947/#948).
//!
//! Phases 2+3 of the unified-interactive-sessions design (spec
//! `docs/superpowers/specs/2026-06-04-unified-interactive-sessions-design.md`
//! §4.1, §4.3, §6): an interaction IS a `SessionMode::Interactive`
//! [`crate::session::types::Session`] in the pool — same prompt path,
//! provider routing, and telemetry funnel (`ManagedSession::handle_event`)
//! as a one-shot turn. The first turn is a normal `spawn` (the issue work,
//! narrated in chat); follow-ups go through `send_followup_turn`
//! (`--resume <agent_session_id>`). When a turn would settle on a one-shot
//! terminal status, the `Session::transition_to` interception keeps the
//! session alive (`SessionStatus::Interactive` + `settled_from`).
//!
//! The Interaction screen stays a [`TurnEvent`] consumer: this module
//! derives chat `TurnEvent`s from the session's `StreamEvent`s and applies
//! them through the existing `TuiDataEvent::InteractionTurnEvent` arm. The
//! transcript persists on `Session::turns`; turn activity lives on
//! `Session::turn_state`.

use chrono::Utc;
use uuid::Uuid;

use super::App;
use crate::session::interaction::{TurnEvent, TurnRecord, TurnRole, TurnState};
use crate::session::types::{SessionMode, SessionStatus, StreamEvent};

impl App {
    /// Dispatch one interaction turn through the normal pipeline. First
    /// turn for the issue (session still `Queued`) → seed the prompt and
    /// `spawn`. Later turns → `send_followup_turn` on the bound
    /// conversation.
    pub(crate) async fn dispatch_interaction_turn(
        &mut self,
        issue_number: u64,
        prompt: String,
        model: String,
    ) {
        let mode = self.interaction_mode_label(issue_number);

        let now = Utc::now();
        let Some(managed) = self.pool.interactive_managed_mut(issue_number) else {
            tracing::warn!("SendInteractionTurn for #{issue_number} with no live interaction");
            return;
        };
        // Mirror the turn into the persisted transcript; the screen keeps
        // its own copy via the TurnStarted event below.
        managed.session.turns.push(TurnRecord {
            role: TurnRole::User,
            content: prompt.clone(),
            started_at: now,
            finished_at: Some(now),
        });
        managed.session.turns.push(TurnRecord {
            role: TurnRole::Agent,
            content: String::new(),
            started_at: now,
            finished_at: None,
        });
        managed.session.turn_state = TurnState::Streaming;

        // First turn = the session has never spawned: it carries the built
        // issue prompt (#946) and re-resolves model/mode now that the issue
        // cache is warm (the launch-time values were defaults, #953).
        let first_turn = managed.session.status == SessionStatus::Queued;
        if first_turn {
            managed.session.prompt = prompt.clone();
            managed.session.model = model;
            managed.session.mode = mode;
        }
        let id = managed.session.id;

        self.apply_interaction_turn_event(
            issue_number,
            TurnEvent::TurnStarted {
                role: TurnRole::Agent,
                at: now,
            },
        );

        let tx = self.pool.event_tx();
        let result = match self.pool.get_active_mut(id) {
            Some(managed) if first_turn => managed.spawn(tx).await,
            Some(managed) => managed.send_followup_turn(prompt, tx),
            None => return,
        };
        if let Err(e) = result {
            self.fail_interaction_turn(issue_number, format!("failed to start turn: {e}"));
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
        let Some(managed) = self.pool.get_active_mut(session_id) else {
            return;
        };
        if managed.session.session_mode != SessionMode::Interactive {
            return;
        }
        let Some(issue_number) = managed.session.issue_number else {
            return;
        };

        let turn_event = match event {
            StreamEvent::AssistantMessage { text } => {
                if let Some(turn) = streaming_agent_turn(&mut managed.session.turns) {
                    turn.content.push_str(text);
                }
                Some(TurnEvent::Chunk(text.clone()))
            }
            StreamEvent::Completed { .. } => {
                let at = Utc::now();
                if let Some(turn) = streaming_agent_turn(&mut managed.session.turns) {
                    turn.finished_at = Some(at);
                }
                managed.session.turn_state = TurnState::Idle;
                // A /pushup terminator queued mid-stream fires now that the
                // streamed output is preserved (#936 contract).
                managed.settle_queued_terminator();
                Some(TurnEvent::TurnFinished { at })
            }
            StreamEvent::Error { message } => {
                let at = Utc::now();
                if let Some(turn) = streaming_agent_turn(&mut managed.session.turns) {
                    turn.finished_at = Some(at);
                }
                managed.session.turns.push(TurnRecord {
                    role: TurnRole::System,
                    content: message.clone(),
                    started_at: at,
                    finished_at: Some(at),
                });
                managed.session.turn_state = TurnState::Idle;
                managed.settle_queued_terminator();
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
        if let Some(managed) = self.pool.interactive_managed_mut(issue_number) {
            if let Some(turn) = streaming_agent_turn(&mut managed.session.turns) {
                turn.finished_at = Some(at);
            }
            managed.session.turns.push(TurnRecord {
                role: TurnRole::System,
                content: message.clone(),
                started_at: at,
                finished_at: Some(at),
            });
            managed.session.turn_state = TurnState::Idle;
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
fn streaming_agent_turn(turns: &mut [TurnRecord]) -> Option<&mut TurnRecord> {
    turns
        .last_mut()
        .filter(|t| t.role == TurnRole::Agent && t.finished_at.is_none())
}
