mod draw;
pub mod types;

pub use types::{LandingTarget, MENU_ITEMS};

use super::{Screen, ScreenAction};
use crate::mascot::MascotState;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{Frame, layout::Rect};

/// Persistent landing screen — replaces the timed splash. Shows mascot +
/// logo + version, with a 5-item menu underneath that routes the user
/// into Dashboard, the wizards, the stats screen, or Quit.
pub struct LandingScreen {
    pub selected: usize,
    pub(super) mascot_state: MascotState,
    pub(super) mascot_frame: usize,
}

impl LandingScreen {
    pub fn new() -> Self {
        Self {
            selected: 0,
            mascot_state: MascotState::Idle,
            mascot_frame: 0,
        }
    }

    pub fn set_mascot(&mut self, state: MascotState, frame: usize) {
        self.mascot_state = state;
        self.mascot_frame = frame;
    }

    fn dispatch_index(&self, idx: usize) -> ScreenAction {
        match MENU_ITEMS[idx].target {
            LandingTarget::Push(mode) => ScreenAction::Push(mode),
        }
    }
}

impl Default for LandingScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapProvider for LandingScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Landing",
            bindings: vec![
                KeyBinding {
                    key: "j/Down",
                    description: "Next entry",
                },
                KeyBinding {
                    key: "k/Up",
                    description: "Previous entry",
                },
                KeyBinding {
                    key: "Enter",
                    description: "Activate selected",
                },
                KeyBinding {
                    key: "d/i/m/s/q",
                    description: "Direct shortcuts",
                },
            ],
        }]
    }
}

impl Screen for LandingScreen {
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

            if let KeyCode::Char(c) = code
                && let Some(idx) = MENU_ITEMS.iter().position(|item| item.shortcut == *c)
            {
                self.selected = idx;
                return self.dispatch_index(idx);
            }

            match code {
                KeyCode::Char('j') | KeyCode::Down if self.selected + 1 < MENU_ITEMS.len() => {
                    self.selected += 1;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.selected = self.selected.saturating_sub(1);
                }
                KeyCode::Enter => return self.dispatch_index(self.selected),
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
    use crate::tui::app::TuiMode;
    use crate::tui::screens::test_helpers::key_event;

    #[test]
    fn new_starts_with_first_entry_selected() {
        let s = LandingScreen::new();
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn j_advances_selection() {
        let mut s = LandingScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert_eq!(s.selected, 1);
    }

    #[test]
    fn down_advances_selection() {
        let mut s = LandingScreen::new();
        s.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        assert_eq!(s.selected, 1);
    }

    #[test]
    fn j_does_not_overflow_past_last() {
        let mut s = LandingScreen::new();
        for _ in 0..MENU_ITEMS.len() + 5 {
            s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        assert_eq!(s.selected, MENU_ITEMS.len() - 1);
    }

    #[test]
    fn k_does_not_underflow_at_zero() {
        let mut s = LandingScreen::new();
        s.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        s.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn enter_on_first_item_pushes_dashboard() {
        let mut s = LandingScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::Dashboard));
    }

    #[test]
    fn enter_on_last_item_pushes_confirm_exit() {
        let mut s = LandingScreen::new();
        for _ in 0..MENU_ITEMS.len() - 1 {
            s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::ConfirmExit));
    }

    #[test]
    fn shortcut_d_pushes_dashboard() {
        let mut s = LandingScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::Dashboard));
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn shortcut_i_pushes_issue_wizard() {
        let mut s = LandingScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Char('i')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::IssueWizard));
    }

    #[test]
    fn shortcut_m_pushes_milestone_wizard() {
        let mut s = LandingScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Char('m')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::MilestoneWizard));
    }

    #[test]
    fn shortcut_s_pushes_project_stats() {
        let mut s = LandingScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Char('s')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::ProjectStats));
    }

    #[test]
    fn shortcut_q_pushes_confirm_exit() {
        let mut s = LandingScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Char('q')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::ConfirmExit));
    }

    #[test]
    fn unknown_letter_returns_none() {
        let mut s = LandingScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Char('z')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn enter_in_insert_mode_is_ignored() {
        let mut s = LandingScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn desired_input_mode_is_normal() {
        let s = LandingScreen::new();
        assert_eq!(s.desired_input_mode(), Some(InputMode::Normal));
    }
}
