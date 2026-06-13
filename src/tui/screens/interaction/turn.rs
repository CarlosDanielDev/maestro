//! Turn lifecycle for the Interaction screen (#738).
//!
//! Splits the send/quit/stream state transitions out of `mod.rs` (file-size
//! budget). These methods own the screen's view of history: `begin_turn`
//! pushes the user turn on send, and `apply_turn_event` folds the streaming
//! `TurnEvent`s from `send_turn` into the live transcript.

use super::InteractionScreen;
use super::view_state::InteractionState;
use crate::session::interaction::TurnEvent;
use crate::session::interaction::{TurnRecord, TurnRole};
use crate::tui::screens::ScreenAction;
use chrono::Utc;
use crossterm::event::KeyCode;
use ratatui::style::Style;
use tui_textarea::TextArea;

impl InteractionScreen {
    /// Inject the global animation tick so the "agent responding" spinner
    /// advances each frame while streaming (#738 QA).
    pub fn set_spinner_context(&mut self, spinner_tick: usize) {
        self.spinner_tick = spinner_tick;
    }

    /// Record which provider/CLI and model the turns run against, shown in
    /// the screen header so the user knows who they are talking to (#738 QA).
    pub fn set_provider_context(
        &mut self,
        agent_label: impl Into<String>,
        model: impl Into<String>,
    ) {
        self.agent_label = agent_label.into();
        self.model = model.into();
    }

    /// Record the issue title shown in the header (#738 QA). Sanitized at this
    /// boundary so every downstream renderer (header + starter hint) is safe
    /// from terminal-escape injection in external GitHub titles.
    pub fn set_issue_title(&mut self, title: impl Into<String>) {
        self.issue_title = crate::tui::screens::sanitize_for_terminal(&title.into());
    }

    /// Seed the first turn from the launch-dialog prompt: push it as a `User`
    /// turn and flip to `Streaming` so the chat starts on the user's
    /// instruction. The dispatch enqueues the matching turn command (#738).
    pub fn seed_turn(&mut self, prompt: String) {
        let _ = self.begin_turn(prompt);
    }

    /// Reset the editor to an empty buffer (history untouched).
    pub(super) fn clear_editor(&mut self) {
        let mut editor = TextArea::default();
        editor.set_cursor_line_style(Style::default());
        self.editor = editor;
    }

    /// Append a finished `User` turn for `content` and flip into `Streaming`,
    /// resetting the per-turn chunk/timer counters. Returns the
    /// `SendInteractionTurn` action the dispatch turns into a spawned turn.
    pub(super) fn begin_turn(&mut self, content: String) -> ScreenAction {
        let now = Utc::now();
        self.history.push(TurnRecord {
            role: TurnRole::User,
            content: content.clone(),
            started_at: now,
            finished_at: Some(now),
        });
        self.state = InteractionState::Streaming;
        self.stream_started_at = None;
        self.stream_chunks = 0;
        self.turn_count += 1;
        ScreenAction::SendInteractionTurn {
            issue_number: self.issue_number,
            prompt: content,
        }
    }

    /// Resolve a key while the quit-confirm modal is open: `y`/`Y` quits —
    /// the app terminates the session and starts the worktree wipe (#949);
    /// the screen terminates when the teardown result lands. Anything else
    /// cancels the modal.
    pub(super) fn handle_quit_modal(&mut self, code: KeyCode) -> ScreenAction {
        self.quit_modal_open = false;
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => ScreenAction::QuitInteraction {
                issue_number: self.issue_number,
            },
            _ => ScreenAction::None,
        }
    }

    /// Apply one streaming [`TurnEvent`] to the screen's view of history.
    /// `TurnFinished` returns the per-turn activity-log line; other events
    /// return [`ScreenAction::None`]. The pool's session is updated in
    /// parallel by `send_turn`; this keeps the rendered transcript live.
    pub fn apply_turn_event(&mut self, event: &TurnEvent) -> ScreenAction {
        match event {
            TurnEvent::TurnStarted {
                role: TurnRole::Agent,
                at,
            } => {
                self.stream_started_at = Some(*at);
                self.stream_chunks = 0;
                self.state = InteractionState::Streaming;
                self.history.push(TurnRecord {
                    role: TurnRole::Agent,
                    content: String::new(),
                    started_at: *at,
                    finished_at: None,
                });
                ScreenAction::None
            }
            TurnEvent::TurnStarted { .. } => ScreenAction::None,
            TurnEvent::Chunk(text) => {
                if let Some(turn) = self.streaming_agent_turn() {
                    turn.content.push_str(text);
                }
                self.stream_chunks += 1;
                ScreenAction::None
            }
            TurnEvent::TurnFinished { at } => {
                if let Some(turn) = self.streaming_agent_turn() {
                    turn.finished_at = Some(*at);
                }
                self.state = InteractionState::Idle;
                let ms = self
                    .stream_started_at
                    .map(|start| (*at - start).num_milliseconds().max(0))
                    .unwrap_or(0);
                super::activity_action(&crate::work::activity::InteractionActivity::TurnComplete {
                    issue: self.issue_number,
                    turn_index: self.turn_count,
                    chunk_count: self.stream_chunks,
                    duration_ms: ms,
                })
            }
            TurnEvent::Error(msg) => {
                let now = Utc::now();
                self.history.push(TurnRecord {
                    role: TurnRole::System,
                    content: msg.clone(),
                    started_at: now,
                    finished_at: Some(now),
                });
                self.state = InteractionState::Idle;
                // Every transition leaves exactly one log line (#742).
                super::activity_action(&crate::work::activity::InteractionActivity::TurnFailed {
                    issue: self.issue_number,
                    detail: msg.clone(),
                })
            }
        }
    }

    /// The in-flight agent turn (last `Agent` turn that has not finished).
    fn streaming_agent_turn(&mut self) -> Option<&mut TurnRecord> {
        self.history
            .last_mut()
            .filter(|t| t.role == TurnRole::Agent && t.finished_at.is_none())
    }

    /// Content of the last agent turn — test seam for the pipeline-turn
    /// integration (#947, `interaction_pipeline_tests`).
    #[cfg(test)]
    pub(crate) fn last_agent_content(&self) -> String {
        self.history
            .iter()
            .rev()
            .find(|t| t.role == TurnRole::Agent)
            .map(|t| t.content.clone())
            .unwrap_or_default()
    }
}
