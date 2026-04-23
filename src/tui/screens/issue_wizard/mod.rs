mod draw;
pub mod types;

pub use types::{IssueCreationPayload, IssueWizardStep};
#[allow(unused_imports)]
pub use types::IssueType;

use super::{Screen, ScreenAction};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{Frame, layout::Rect};

/// State for the Issue Wizard. #291 owns the scaffold (step machine,
/// payload, dispatch); per-step UIs land in #293/#295/#296/#298.
pub struct IssueWizardScreen {
    step: IssueWizardStep,
    payload: IssueCreationPayload,
}

impl IssueWizardScreen {
    pub fn new() -> Self {
        Self {
            step: IssueWizardStep::default(),
            payload: IssueCreationPayload::new(),
        }
    }

    pub fn step(&self) -> IssueWizardStep {
        self.step
    }

    #[allow(dead_code)] // Reason: payload mutated by step handlers in #293+
    pub fn payload(&self) -> &IssueCreationPayload {
        &self.payload
    }

    #[allow(dead_code)] // Reason: payload mutated by step handlers in #293+
    pub fn payload_mut(&mut self) -> &mut IssueCreationPayload {
        &mut self.payload
    }

    /// Advance to the next step. Returns true if the step changed.
    pub fn advance(&mut self) -> bool {
        if let Some(next) = self.step.next() {
            self.step = next;
            true
        } else {
            false
        }
    }

    /// Move back one step. Returns true if the step changed; false if we
    /// were already at the first step (caller should pop the screen).
    pub fn retreat(&mut self) -> bool {
        if let Some(prev) = self.step.previous() {
            self.step = prev;
            true
        } else {
            false
        }
    }
}

impl Default for IssueWizardScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapProvider for IssueWizardScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Issue Wizard",
            bindings: vec![
                KeyBinding {
                    key: "Enter",
                    description: "Next step",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Previous step (or close)",
                },
            ],
        }]
    }
}

impl Screen for IssueWizardScreen {
    fn handle_input(&mut self, event: &Event, mode: InputMode) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            if mode == InputMode::Insert {
                return ScreenAction::None;
            }

            match code {
                KeyCode::Enter => {
                    self.advance();
                }
                KeyCode::Esc => {
                    if self.step.is_first() {
                        return ScreenAction::Pop;
                    }
                    self.retreat();
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.draw_impl(f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        Some(InputMode::Normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::key_event;

    #[test]
    fn new_starts_at_context_step() {
        let s = IssueWizardScreen::new();
        assert_eq!(s.step(), IssueWizardStep::Context);
    }

    #[test]
    fn step_index_is_one_based() {
        assert_eq!(IssueWizardStep::Context.index(), 1);
        assert_eq!(IssueWizardStep::TypeSelect.index(), 2);
        assert_eq!(IssueWizardStep::Failed.index(), 10);
    }

    #[test]
    fn step_total_is_ten() {
        assert_eq!(IssueWizardStep::total(), 10);
    }

    #[test]
    fn next_from_context_returns_type_select() {
        assert_eq!(
            IssueWizardStep::Context.next(),
            Some(IssueWizardStep::TypeSelect)
        );
    }

    #[test]
    fn next_from_failed_returns_none() {
        assert_eq!(IssueWizardStep::Failed.next(), None);
    }

    #[test]
    fn previous_from_context_returns_none() {
        assert_eq!(IssueWizardStep::Context.previous(), None);
    }

    #[test]
    fn previous_from_type_select_returns_context() {
        assert_eq!(
            IssueWizardStep::TypeSelect.previous(),
            Some(IssueWizardStep::Context)
        );
    }

    #[test]
    fn enter_advances_step() {
        let mut s = IssueWizardScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert_eq!(s.step(), IssueWizardStep::TypeSelect);
    }

    #[test]
    fn esc_on_first_step_returns_pop() {
        let mut s = IssueWizardScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
        assert_eq!(s.step(), IssueWizardStep::Context);
    }

    #[test]
    fn esc_on_later_step_retreats() {
        let mut s = IssueWizardScreen::new();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(s.step(), IssueWizardStep::BasicInfo);
        let action = s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert_eq!(s.step(), IssueWizardStep::TypeSelect);
    }

    #[test]
    fn enter_at_last_step_does_not_overflow() {
        let mut s = IssueWizardScreen::new();
        for _ in 0..IssueWizardStep::total() + 5 {
            s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        }
        assert_eq!(s.step(), IssueWizardStep::Failed);
    }

    #[test]
    fn payload_default_has_feature_type() {
        let p = IssueCreationPayload::default();
        assert_eq!(p.issue_type, IssueType::Feature);
        assert!(p.title.is_empty());
        assert!(p.blocked_by.is_empty());
    }

    #[test]
    fn issue_type_labels() {
        assert_eq!(IssueType::Feature.label(), "Feature");
        assert_eq!(IssueType::Bug.label(), "Bug");
    }

    #[test]
    fn enter_in_insert_mode_is_ignored() {
        let mut s = IssueWizardScreen::new();
        s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(s.step(), IssueWizardStep::Context);
    }

    #[test]
    fn desired_input_mode_is_normal() {
        let s = IssueWizardScreen::new();
        assert_eq!(s.desired_input_mode(), Some(InputMode::Normal));
    }
}
