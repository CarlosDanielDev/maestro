pub(crate) mod draw;
pub mod types;

pub use types::{LandingTarget, MENU_ITEMS};

use super::{Screen, ScreenAction};
use crate::mascot::{MascotState, MascotStyle};
use crate::tui::app::TuiMode;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{Frame, layout::Rect};

const NETWORK_MEASURE_TICKS: usize = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NetworkMeasureState {
    Standby,
    Measuring { tick: usize },
    Last { tick: usize },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct NetworkRates {
    pub down_kib_s: f64,
    pub up_bytes_s: usize,
}

/// Persistent landing screen — replaces the timed splash. Shows mascot +
/// logo + version, with a 5-item menu underneath that routes the user
/// into Dashboard, the wizards, the stats screen, or Quit.
pub struct LandingScreen {
    pub selected: usize,
    pub(super) mascot_state: MascotState,
    pub(super) mascot_frame: usize,
    pub(super) mascot_style: MascotStyle,
    pub(super) animation_tick: usize,
    pub(super) network_measure_started: Option<usize>,
    pub(super) last_network_measure_tick: Option<usize>,
    pub(super) network_peak_rates: Option<NetworkRates>,
}

impl LandingScreen {
    pub fn new() -> Self {
        Self {
            selected: 0,
            mascot_state: MascotState::Idle,
            mascot_frame: 0,
            mascot_style: MascotStyle::default(),
            animation_tick: 0,
            network_measure_started: None,
            last_network_measure_tick: None,
            network_peak_rates: None,
        }
    }

    pub fn set_mascot(&mut self, state: MascotState, frame: usize, style: MascotStyle) {
        self.mascot_state = state;
        self.mascot_frame = frame;
        self.mascot_style = style;
    }

    pub fn set_animation_context(&mut self, tick: usize) {
        self.animation_tick = tick;
        if let Some(started) = self.network_measure_started {
            let elapsed = self.animation_tick.saturating_sub(started);
            if elapsed < NETWORK_MEASURE_TICKS {
                self.record_network_range(elapsed);
                self.last_network_measure_tick = Some(elapsed);
            } else {
                self.network_measure_started = None;
            }
        }
    }

    pub fn trigger_network_measurement(&mut self) {
        self.network_measure_started = Some(self.animation_tick);
        self.last_network_measure_tick = Some(0);
        self.network_peak_rates = Some(network_rates(0));
    }

    pub(super) fn network_measure_state(&self) -> NetworkMeasureState {
        if let Some(started) = self.network_measure_started {
            let elapsed = self.animation_tick.saturating_sub(started);
            if elapsed < NETWORK_MEASURE_TICKS {
                return NetworkMeasureState::Measuring { tick: elapsed };
            }
        }
        if let Some(tick) = self.last_network_measure_tick {
            return NetworkMeasureState::Last { tick };
        }
        NetworkMeasureState::Standby
    }

    pub(super) fn network_peak_rates(&self) -> Option<NetworkRates> {
        self.network_peak_rates
    }

    fn record_network_rates(&mut self, rates: NetworkRates) {
        self.network_peak_rates = Some(match self.network_peak_rates {
            Some(peaks) => NetworkRates {
                down_kib_s: peaks.down_kib_s.max(rates.down_kib_s),
                up_bytes_s: peaks.up_bytes_s.max(rates.up_bytes_s),
            },
            None => rates,
        });
    }

    fn record_network_range(&mut self, elapsed: usize) {
        let Some(last_tick) = self.last_network_measure_tick else {
            self.record_network_rates(network_rates(elapsed));
            return;
        };

        if elapsed <= last_tick {
            self.record_network_rates(network_rates(elapsed));
            return;
        }

        for tick in last_tick + 1..=elapsed {
            self.record_network_rates(network_rates(tick));
        }
    }

    fn dispatch_index(&self, idx: usize) -> ScreenAction {
        match MENU_ITEMS[idx].target {
            LandingTarget::Push(mode) => ScreenAction::Push(mode),
        }
    }
}

pub(super) fn network_rates(tick: usize) -> NetworkRates {
    NetworkRates {
        down_kib_s: 5.25 + (tick % 9) as f64 * 0.19,
        up_bytes_s: 560 + (tick % 7) * 17,
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
                    key: "d/i/m/s/p/r/h/q",
                    description: "Direct shortcuts",
                },
                KeyBinding {
                    key: "n",
                    description: "Release notes",
                },
                KeyBinding {
                    key: "w",
                    description: "Measure network",
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

            if matches!(code, KeyCode::Char('n')) {
                return ScreenAction::Push(TuiMode::ReleaseNotes);
            }

            if matches!(code, KeyCode::Char('w') | KeyCode::Char('W')) {
                self.trigger_network_measurement();
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
    fn shortcut_n_pushes_release_notes() {
        let mut s = LandingScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Char('n')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::ReleaseNotes));
    }

    #[test]
    fn shortcut_w_starts_network_measurement() {
        let mut s = LandingScreen::new();
        s.set_animation_context(42);
        let action = s.handle_input(&key_event(KeyCode::Char('w')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert_eq!(
            s.network_measure_state(),
            NetworkMeasureState::Measuring { tick: 0 }
        );
        s.set_animation_context(45);
        assert_eq!(
            s.network_measure_state(),
            NetworkMeasureState::Measuring { tick: 3 }
        );
        assert_eq!(s.network_peak_rates(), Some(network_rates(3)));
    }

    #[test]
    fn shortcut_uppercase_w_starts_network_measurement() {
        let mut s = LandingScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Char('W')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert_eq!(
            s.network_measure_state(),
            NetworkMeasureState::Measuring { tick: 0 }
        );
    }

    #[test]
    fn network_measurement_expiry_keeps_last_sample() {
        let mut s = LandingScreen::new();
        s.set_animation_context(10);
        s.trigger_network_measurement();
        s.set_animation_context(10 + NETWORK_MEASURE_TICKS - 1);
        assert_eq!(
            s.network_measure_state(),
            NetworkMeasureState::Measuring {
                tick: NETWORK_MEASURE_TICKS - 1
            }
        );
        s.set_animation_context(10 + NETWORK_MEASURE_TICKS);
        assert_eq!(
            s.network_measure_state(),
            NetworkMeasureState::Last {
                tick: NETWORK_MEASURE_TICKS - 1
            }
        );
        assert_eq!(
            s.network_peak_rates(),
            Some(NetworkRates {
                down_kib_s: network_rates(8).down_kib_s,
                up_bytes_s: network_rates(6).up_bytes_s,
            })
        );
    }

    #[test]
    fn network_measurement_restart_resets_peak_rates() {
        let mut s = LandingScreen::new();
        s.set_animation_context(0);
        s.trigger_network_measurement();
        s.set_animation_context(8);
        assert_eq!(
            s.network_peak_rates(),
            Some(NetworkRates {
                down_kib_s: network_rates(8).down_kib_s,
                up_bytes_s: network_rates(6).up_bytes_s,
            })
        );

        s.set_animation_context(20);
        s.trigger_network_measurement();
        assert_eq!(s.network_peak_rates(), Some(network_rates(0)));
    }

    // I-1: milestone health entry point (#500).
    #[test]
    fn shortcut_h_pushes_milestone_health() {
        let mut s = LandingScreen::new();
        let action = s.handle_input(&key_event(KeyCode::Char('h')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::MilestoneHealth));
    }

    // I-2: menu contains the milestone-health entry (#500).
    #[test]
    fn landing_menu_contains_milestone_health_entry() {
        let item = MENU_ITEMS.iter().find(|m| m.shortcut == 'h');
        assert!(item.is_some());
        let item = item.unwrap();
        assert_eq!(item.label, "Milestone Review");
        assert_eq!(item.target, LandingTarget::Push(TuiMode::MilestoneHealth));
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

    #[test]
    fn landing_screen_set_mascot_propagates_style() {
        let mut s = LandingScreen::new();
        s.set_mascot(MascotState::Idle, 0, MascotStyle::Sprite);
        assert_eq!(s.mascot_style, MascotStyle::Sprite);
        s.set_mascot(MascotState::Idle, 0, MascotStyle::Ascii);
        assert_eq!(s.mascot_style, MascotStyle::Ascii);
    }
}
