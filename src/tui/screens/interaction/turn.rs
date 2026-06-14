//! Turn send + telemetry for the Interaction screen (#738).
//!
//! Splits the send path and per-turn counters out of `mod.rs` (file-size
//! budget). Since #950 the transcript lives on the live `Session` (written by
//! the pipeline): `begin_turn` only returns the send action and bumps the
//! counters, and `log_turn_event` folds streaming `TurnEvent`s into the
//! activity-log line without touching turns.

use super::InteractionScreen;
use crate::session::interaction::TurnEvent;
use crate::session::interaction::TurnRole;
use crate::tui::screens::ScreenAction;
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

    /// Reset the per-turn chunk/timer counters and return the
    /// `SendInteractionTurn` action the dispatch turns into a spawned turn.
    /// The User turn + `Streaming` flip now happen on the live session in the
    /// pipeline (`dispatch_interaction_turn`, #947/#950) — the screen reads
    /// them back through the injected view, so it no longer pushes the turn.
    pub(super) fn begin_turn(&mut self, content: String) -> ScreenAction {
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
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // RC4 guard: quitting wipes the worktree AND deletes the
                // branch. When a PR was intended (`produce_pr`) but none was
                // ever linked, the work lives only on this local branch
                // (gates failed → no PR → never pushed). Require a second
                // `[y]` so that work is never discarded silently. Work
                // already captured in a linked PR quits on the first `[y]`.
                if self.produce_pr && self.view.pr_linked.is_none() && !self.quit_loss_acknowledged
                {
                    self.quit_loss_acknowledged = true;
                    // Keep the modal open so the render layer can surface the
                    // stronger "unpushed work will be lost" warning.
                    return ScreenAction::None;
                }
                self.quit_modal_open = false;
                self.quit_loss_acknowledged = false;
                ScreenAction::QuitInteraction {
                    issue_number: self.issue_number,
                }
            }
            _ => {
                self.quit_modal_open = false;
                self.quit_loss_acknowledged = false;
                ScreenAction::None
            }
        }
    }

    /// Fold one streaming [`TurnEvent`] into the screen's per-turn telemetry
    /// counters and emit the activity-log line (#950). The transcript itself
    /// lives on the live session (written by the pipeline) and is read back
    /// through the injected view — this method never mutates turns or
    /// lifecycle. `TurnFinished`/`Error` return the pinned activity-log line
    /// (#742); other events return [`ScreenAction::None`].
    pub fn log_turn_event(&mut self, event: &TurnEvent) -> ScreenAction {
        match event {
            TurnEvent::TurnStarted {
                role: TurnRole::Agent,
                at,
            } => {
                self.stream_started_at = Some(*at);
                self.stream_chunks = 0;
                ScreenAction::None
            }
            TurnEvent::TurnStarted { .. } => ScreenAction::None,
            TurnEvent::Chunk(_) => {
                self.stream_chunks += 1;
                ScreenAction::None
            }
            TurnEvent::TurnFinished { at } => {
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
                // Every transition leaves exactly one log line (#742). The
                // System turn for the error is pushed onto the session by the
                // pipeline (`fail_interaction_turn`), not here.
                super::activity_action(&crate::work::activity::InteractionActivity::TurnFailed {
                    issue: self.issue_number,
                    detail: msg.clone(),
                })
            }
        }
    }
}
