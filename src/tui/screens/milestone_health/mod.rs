//! Milestone health-check wizard (#500).
//!
//! Read-only DOR + dependency-graph review for an existing milestone, with
//! a deterministic patch proposal screen and a single confirmation gate
//! before any GitHub write.

pub mod diff;
pub mod draw;
pub mod format;
pub mod state;

pub use state::{HealthInput, HealthScreenState, HealthSideEffect, HealthStep};

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect};

use crate::provider::github::types::{GhIssue, GhMilestone};
use crate::tui::app::TuiCommand;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBindingGroup, KeymapProvider};
use crate::tui::screens::{Screen, ScreenAction};
use crate::tui::theme::Theme;

pub struct MilestoneHealthScreen {
    pub state: HealthScreenState,
    pub scroll: u16,
    pending_command: Option<TuiCommand>,
}

impl Default for MilestoneHealthScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl MilestoneHealthScreen {
    pub fn new() -> Self {
        Self {
            state: HealthScreenState::new(),
            scroll: 0,
            pending_command: None,
        }
    }

    fn apply_side_effect(&mut self, eff: HealthSideEffect) -> ScreenAction {
        match eff {
            HealthSideEffect::None => ScreenAction::None,
            HealthSideEffect::Pop => ScreenAction::Pop,
            HealthSideEffect::DispatchFetchMilestones => {
                self.scroll = 0;
                self.pending_command = Some(TuiCommand::FetchMilestones);
                ScreenAction::None
            }
            HealthSideEffect::DispatchFetchIssues { .. } => {
                self.scroll = 0;
                // Pull the cached milestone the reducer just stored on
                // the Loading step. Avoids a redundant `list_milestones`
                // round trip in the background task.
                if let HealthStep::Loading {
                    milestone: Some(m), ..
                } = &self.state.step
                {
                    self.pending_command = Some(TuiCommand::FetchMilestoneHealthIssues {
                        milestone: m.clone(),
                    });
                }
                ScreenAction::None
            }
            HealthSideEffect::DispatchPatch {
                milestone_number,
                description,
            } => {
                self.scroll = 0;
                self.pending_command = Some(TuiCommand::PatchMilestoneDescription {
                    milestone_number,
                    description,
                });
                ScreenAction::None
            }
        }
    }

    /// Take the most recent pending command, if any.
    pub fn take_pending_command(&mut self) -> Option<TuiCommand> {
        self.pending_command.take()
    }

    pub fn apply_milestones_loaded(
        &mut self,
        result: anyhow::Result<Vec<GhMilestone>>,
    ) -> Option<TuiCommand> {
        let _ = self.state.transition(HealthInput::MilestonesLoaded(result));
        self.pending_command.take()
    }

    pub fn apply_issues_fetched(
        &mut self,
        result: anyhow::Result<(GhMilestone, Vec<GhIssue>)>,
    ) -> Option<TuiCommand> {
        let _ = self.state.transition(HealthInput::DataFetched(result));
        self.pending_command.take()
    }

    pub fn apply_patch_result(&mut self, result: anyhow::Result<()>) {
        let _ = self.state.transition(HealthInput::DataPatched(result));
    }
}

impl KeymapProvider for MilestoneHealthScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        Vec::new()
    }
}

impl Screen for MilestoneHealthScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        let key_code = match event {
            Event::Key(KeyEvent { code, .. }) => *code,
            _ => return ScreenAction::None,
        };

        if matches!(
            self.state.step,
            HealthStep::Report { .. } | HealthStep::Patch { .. }
        ) && matches!(key_code, KeyCode::PageUp | KeyCode::PageDown)
        {
            match key_code {
                KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(8),
                KeyCode::PageDown => self.scroll = self.scroll.saturating_add(8),
                _ => {}
            }
            return ScreenAction::None;
        }

        let eff = self.state.transition(HealthInput::Key(key_code));
        self.apply_side_effect(eff)
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        crate::tui::screens::milestone_health::draw::draw(f, area, theme, self, 0);
    }
}
