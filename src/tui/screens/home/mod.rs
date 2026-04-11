mod draw;
pub mod types;

pub use types::*;

#[allow(dead_code)]
use super::{Screen, ScreenAction};
use crate::tui::app::TuiMode;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::focus::{FocusId, FocusRing};
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{Frame, layout::Rect};

const QUICK_ACTIONS: &[(&str, char)] = &[
    ("Browse Issues", 'i'),
    ("Browse Milestones", 'm'),
    ("Run Prompt", 'r'),
    ("Adapt Project", 'a'),
    ("Review PRs", 'p'),
    ("Status", 's'),
    ("Cost Report", 'c'),
    ("Token Report", 't'),
    ("Settings", 'S'),
    ("Update Maestro", 'u'),
    ("Quit", 'q'),
];

const LOGO: &str = r#"
 ███╗   ███╗ █████╗ ███████╗███████╗████████╗██████╗  ██████╗
 ████╗ ████║██╔══██╗██╔════╝██╔════╝╚══██╔══╝██╔══██╗██╔═══██╗
 ██╔████╔██║███████║█████╗  ███████╗  ██║   ██████╔╝██║   ██║
 ██║╚██╔╝██║██╔══██║██╔══╝  ╚════██║  ██║   ██╔══██╗██║   ██║
 ██║ ╚═╝ ██║██║  ██║███████╗███████║  ██║   ██║  ██║╚██████╔╝
 ╚═╝     ╚═╝╚═╝  ╚═╝╚══════╝╚══════╝  ╚═╝   ╚═╝  ╚═╝ ╚═════╝
"#;

pub struct HomeScreen {
    pub selected_action: usize,
    pub recent_sessions: Vec<SessionSummary>,
    pub project_info: ProjectInfo,
    pub warnings: Vec<String>,
    pub suggestions: Vec<Suggestion>,
    pub selected_suggestion: usize,
    pub loading_suggestions: bool,
    pub focus_ring: FocusRing,
}

impl HomeScreen {
    pub const NUM_ACTIONS: usize = QUICK_ACTIONS.len();
    #[allow(dead_code)] // Reason: quit action index for keyboard shortcut
    pub const QUIT_ACTION_INDEX: usize = 10;
    pub const QUICK_ACTIONS_PANE: FocusId = FocusId("home:quick_actions");
    pub const SUGGESTIONS_PANE: FocusId = FocusId("home:suggestions");

    pub fn new(
        project_info: ProjectInfo,
        recent_sessions: Vec<SessionSummary>,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            selected_action: 0,
            recent_sessions,
            project_info,
            warnings,
            suggestions: Vec::new(),
            selected_suggestion: 0,
            loading_suggestions: false,
            focus_ring: FocusRing::new(vec![Self::QUICK_ACTIONS_PANE, Self::SUGGESTIONS_PANE]),
        }
    }

    fn is_quick_actions_focused(&self) -> bool {
        self.focus_ring.is_focused(Self::QUICK_ACTIONS_PANE)
    }

    fn is_suggestions_focused(&self) -> bool {
        self.focus_ring.is_focused(Self::SUGGESTIONS_PANE)
    }
}

impl KeymapProvider for HomeScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![
            KeyBindingGroup {
                title: "Navigation",
                bindings: vec![
                    KeyBinding {
                        key: "j/Down",
                        description: "Move down",
                    },
                    KeyBinding {
                        key: "k/Up",
                        description: "Move up",
                    },
                    KeyBinding {
                        key: "Tab",
                        description: "Cycle focus between panes",
                    },
                ],
            },
            KeyBindingGroup {
                title: "Actions",
                bindings: vec![
                    KeyBinding {
                        key: "Enter",
                        description: "Execute selected action",
                    },
                    KeyBinding {
                        key: "i",
                        description: "Browse Issues",
                    },
                    KeyBinding {
                        key: "m",
                        description: "Browse Milestones",
                    },
                    KeyBinding {
                        key: "r",
                        description: "Run Prompt",
                    },
                    KeyBinding {
                        key: "a",
                        description: "Adapt Project",
                    },
                    KeyBinding {
                        key: "p",
                        description: "Review PRs",
                    },
                    KeyBinding {
                        key: "n",
                        description: "Release Notes",
                    },
                    KeyBinding {
                        key: "R",
                        description: "Refresh Suggestions",
                    },
                    KeyBinding {
                        key: "q",
                        description: "Quit",
                    },
                ],
            },
        ]
    }
}

impl Screen for HomeScreen {
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
                KeyCode::Char('i') => return ScreenAction::Push(TuiMode::IssueBrowser),
                KeyCode::Char('m') => return ScreenAction::Push(TuiMode::MilestoneView),
                KeyCode::Char('r') => return ScreenAction::Push(TuiMode::PromptInput),
                KeyCode::Char('a') => return ScreenAction::Push(TuiMode::AdaptWizard),
                KeyCode::Char('n') => return ScreenAction::Push(TuiMode::ReleaseNotes),
                KeyCode::Char('p') => return ScreenAction::Push(TuiMode::PrReview),
                KeyCode::Char('R') => return ScreenAction::RefreshSuggestions,
                KeyCode::Char('s') => return ScreenAction::Push(TuiMode::Overview),
                KeyCode::Char('c') => return ScreenAction::Push(TuiMode::CostDashboard),
                KeyCode::Char('t') => return ScreenAction::Push(TuiMode::TokenDashboard),
                KeyCode::Char('S') => return ScreenAction::Push(TuiMode::Settings),
                KeyCode::Char('u') => return ScreenAction::CheckForUpdate,
                KeyCode::Char('q') => return ScreenAction::Quit,
                KeyCode::Tab => {
                    self.focus_ring.next();
                }
                KeyCode::BackTab => {
                    self.focus_ring.previous();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    if self.is_quick_actions_focused() {
                        if self.selected_action < Self::NUM_ACTIONS - 1 {
                            self.selected_action += 1;
                        }
                    } else if self.is_suggestions_focused()
                        && !self.suggestions.is_empty()
                        && self.selected_suggestion < self.suggestions.len() - 1
                    {
                        self.selected_suggestion += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if self.is_quick_actions_focused() {
                        self.selected_action = self.selected_action.saturating_sub(1);
                    } else if self.is_suggestions_focused() {
                        self.selected_suggestion = self.selected_suggestion.saturating_sub(1);
                    }
                }
                KeyCode::Enter => {
                    if self.is_quick_actions_focused() {
                        return self.execute_selected_action();
                    } else if self.is_suggestions_focused() {
                        if let Some(suggestion) = self.suggestions.get(self.selected_suggestion) {
                            return ScreenAction::Push(suggestion.action);
                        }
                        return ScreenAction::None;
                    }
                }
                KeyCode::Esc => return ScreenAction::None,
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
    use crossterm::event::KeyCode;

    fn make_project_info() -> ProjectInfo {
        ProjectInfo {
            repo: "owner/repo".to_string(),
            branch: "main".to_string(),
            username: None,
        }
    }

    fn make_project_info_with_user(name: &str) -> ProjectInfo {
        ProjectInfo {
            repo: "owner/repo".to_string(),
            branch: "main".to_string(),
            username: Some(name.to_string()),
        }
    }

    fn make_session_summary(id: u64) -> SessionSummary {
        SessionSummary {
            issue_number: id,
            title: format!("Issue #{}", id),
            status: "completed".to_string(),
            cost_usd: 0.05,
        }
    }

    #[test]
    fn home_initial_selected_action_is_zero() {
        let screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        assert_eq!(screen.selected_action, 0);
    }

    #[test]
    fn home_key_j_moves_selection_down() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert_eq!(screen.selected_action, 1);
    }

    #[test]
    fn home_key_down_moves_selection_down() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        assert_eq!(screen.selected_action, 1);
    }

    #[test]
    fn home_key_k_moves_selection_up() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_action, 2);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected_action, 1);
    }

    #[test]
    fn home_key_k_does_not_underflow_at_zero() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected_action, 0);
    }

    #[test]
    fn home_key_j_does_not_overflow_past_last_action() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let num_actions = HomeScreen::NUM_ACTIONS;
        for _ in 0..num_actions + 5 {
            screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        assert_eq!(screen.selected_action, num_actions - 1);
    }

    #[test]
    fn home_key_up_moves_selection_up() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
        assert_eq!(screen.selected_action, 0);
    }

    #[test]
    fn home_key_i_returns_push_issue_browser() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('i')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::IssueBrowser));
    }

    #[test]
    fn home_key_m_returns_push_milestone_view() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('m')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::MilestoneView));
    }

    #[test]
    fn home_key_q_returns_quit() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('q')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Quit);
    }

    #[test]
    fn home_enter_on_issues_action_returns_push_issue_browser() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::IssueBrowser));
    }

    #[test]
    fn home_enter_on_milestones_action_returns_push_milestone_view() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::MilestoneView));
    }

    #[test]
    fn home_enter_on_quit_action_returns_quit() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        for _ in 0..HomeScreen::QUIT_ACTION_INDEX {
            screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        }
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::Quit);
    }

    #[test]
    fn home_esc_returns_none() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn home_tick_does_not_panic() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.tick();
        screen.tick();
        screen.tick();
    }

    #[test]
    fn home_recent_sessions_stored() {
        let sessions = vec![make_session_summary(10), make_session_summary(11)];
        let screen = HomeScreen::new(make_project_info(), sessions, vec![]);
        assert_eq!(screen.recent_sessions.len(), 2);
        assert_eq!(screen.recent_sessions[0].issue_number, 10);
        assert_eq!(screen.recent_sessions[1].issue_number, 11);
    }

    #[test]
    fn home_unknown_key_returns_none() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('x')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    // --- Tests for ProjectInfo.username field (Issue #34) ---

    #[test]
    fn project_info_with_user_stores_username() {
        let info = make_project_info_with_user("carlos");
        assert_eq!(info.username, Some("carlos".to_string()));
    }

    #[test]
    fn project_info_without_user_is_none() {
        let info = make_project_info();
        assert!(info.username.is_none());
    }

    #[test]
    fn home_screen_stores_project_info_with_user() {
        let info = make_project_info_with_user("testuser");
        let screen = HomeScreen::new(info, vec![], vec![]);
        assert_eq!(screen.project_info.username, Some("testuser".to_string()));
    }

    #[test]
    fn home_screen_stores_project_info_without_user() {
        let info = make_project_info();
        let screen = HomeScreen::new(info, vec![], vec![]);
        assert!(screen.project_info.username.is_none());
    }

    // --- Tests for Work Suggestions (Issue #35) ---

    fn make_home_with_suggestions(suggestions: Vec<Suggestion>) -> HomeScreen {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.suggestions = suggestions;
        screen
    }

    fn focus_suggestions(screen: &mut HomeScreen) {
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    }

    // -- Suggestion::build_suggestions (pure logic) --

    #[test]
    fn build_suggestions_with_ready_issues_emits_ready_issues_suggestion() {
        let result = Suggestion::build_suggestions(3, 0, &[], 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, SuggestionKind::ReadyIssues { count: 3 });

        assert_eq!(result[0].action, TuiMode::IssueBrowser);
    }

    #[test]
    fn build_suggestions_with_zero_ready_issues_emits_no_ready_suggestion() {
        let result = Suggestion::build_suggestions(0, 0, &[], 1);
        assert!(result.is_empty());
    }

    #[test]
    fn build_suggestions_with_failed_issues_emits_failed_issues_suggestion() {
        let result = Suggestion::build_suggestions(0, 2, &[], 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, SuggestionKind::FailedIssues { count: 2 });

        assert_eq!(result[0].action, TuiMode::IssueBrowser);
    }

    #[test]
    fn build_suggestions_with_milestone_emits_milestone_progress_suggestion() {
        let result = Suggestion::build_suggestions(0, 0, &[("v1.0".to_string(), 3, 10)], 1);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].kind,
            SuggestionKind::MilestoneProgress {
                title: "v1.0".to_string(),
                closed: 3,
                total: 10,
            }
        );

        assert_eq!(result[0].action, TuiMode::MilestoneView);
    }

    #[test]
    fn build_suggestions_milestone_with_zero_total_is_skipped() {
        let result = Suggestion::build_suggestions(0, 0, &[("empty".to_string(), 0, 0)], 1);
        assert!(result.is_empty());
    }

    #[test]
    fn build_suggestions_multiple_milestones_emits_one_per_nonzero() {
        let milestones = vec![
            ("v1".to_string(), 1u32, 5u32),
            ("v2".to_string(), 0u32, 0u32),
        ];
        let result = Suggestion::build_suggestions(0, 0, &milestones, 1);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].kind,
            SuggestionKind::MilestoneProgress {
                title: "v1".to_string(),
                closed: 1,
                total: 5,
            }
        );
    }

    #[test]
    fn build_suggestions_with_no_active_sessions_emits_idle_sessions_suggestion() {
        let result = Suggestion::build_suggestions(0, 0, &[], 0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, SuggestionKind::IdleSessions);

        assert_eq!(result[0].action, TuiMode::Overview);
    }

    #[test]
    fn build_suggestions_with_active_sessions_does_not_emit_idle() {
        let result = Suggestion::build_suggestions(0, 0, &[], 2);
        assert!(
            result
                .iter()
                .all(|s| s.kind != SuggestionKind::IdleSessions)
        );
    }

    #[test]
    fn build_suggestions_all_zeros_with_active_sessions_returns_empty() {
        let result = Suggestion::build_suggestions(0, 0, &[], 1);
        assert!(result.is_empty());
    }

    #[test]
    fn build_suggestions_message_contains_count_for_ready_issues() {
        let result = Suggestion::build_suggestions(5, 0, &[], 1);
        assert!(result[0].message.contains("5"));
    }

    #[test]
    fn build_suggestions_message_contains_percentage_for_milestone() {
        let result = Suggestion::build_suggestions(0, 0, &[("v2".to_string(), 5, 10)], 1);
        assert!(result[0].message.contains("50"));
    }

    #[test]
    fn build_suggestions_order_is_ready_then_milestone_then_failed_then_idle() {
        let milestones = vec![("v1".to_string(), 1u32, 2u32)];
        let result = Suggestion::build_suggestions(1, 1, &milestones, 0);
        assert_eq!(result.len(), 4);
        assert!(matches!(result[0].kind, SuggestionKind::ReadyIssues { .. }));
        assert!(matches!(
            result[1].kind,
            SuggestionKind::MilestoneProgress { .. }
        ));
        assert!(matches!(
            result[2].kind,
            SuggestionKind::FailedIssues { .. }
        ));
        assert_eq!(result[3].kind, SuggestionKind::IdleSessions);
    }

    // -- FocusRing focus and Tab toggle --

    #[test]
    fn home_initial_focus_is_quick_actions() {
        let screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        assert!(screen.focus_ring.is_focused(HomeScreen::QUICK_ACTIONS_PANE));
    }

    #[test]
    fn home_tab_toggles_focus_from_quick_actions_to_suggestions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        assert!(screen.focus_ring.is_focused(HomeScreen::SUGGESTIONS_PANE));
    }

    #[test]
    fn home_tab_toggles_focus_back_to_quick_actions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        assert!(screen.focus_ring.is_focused(HomeScreen::QUICK_ACTIONS_PANE));
    }

    #[test]
    fn home_shift_tab_cycles_focus_in_reverse() {
        use crate::tui::screens::test_helpers::key_event_with_modifiers;
        use crossterm::event::KeyModifiers;
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let shift_tab = key_event_with_modifiers(KeyCode::BackTab, KeyModifiers::SHIFT);
        screen.handle_input(&shift_tab, InputMode::Normal);
        assert!(screen.focus_ring.is_focused(HomeScreen::SUGGESTIONS_PANE));
    }

    // -- Screen trait tests --

    #[test]
    fn home_desired_input_mode_is_normal() {
        let screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        assert_eq!(screen.desired_input_mode(), Some(InputMode::Normal));
    }

    #[test]
    fn home_keybindings_returns_at_least_one_group() {
        let screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        assert!(!screen.keybindings().is_empty());
    }

    #[test]
    fn home_handle_input_navigation_keys_ignored_in_insert_mode() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('q')), InputMode::Insert);
        assert_ne!(action, ScreenAction::Quit);
    }

    // -- Suggestion list navigation --

    #[test]
    fn home_suggestions_initial_selected_index_is_zero() {
        let screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        assert_eq!(screen.selected_suggestion, 0);
    }

    #[test]
    fn home_j_navigates_suggestions_when_focus_is_suggestions() {
        let sug = Suggestion::build_suggestions(1, 0, &[("v1".to_string(), 1, 2)], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_suggestion, 1);
    }

    #[test]
    fn home_down_navigates_suggestions_when_focus_is_suggestions() {
        let sug = Suggestion::build_suggestions(1, 0, &[("v1".to_string(), 1, 2)], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        assert_eq!(screen.selected_suggestion, 1);
    }

    #[test]
    fn home_k_navigates_suggestions_up_when_focus_is_suggestions() {
        let sug = Suggestion::build_suggestions(1, 0, &[("v1".to_string(), 1, 2)], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_suggestion, 1);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected_suggestion, 0);
    }

    #[test]
    fn home_suggestion_navigation_does_not_underflow() {
        let sug = Suggestion::build_suggestions(1, 0, &[], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected_suggestion, 0);
    }

    #[test]
    fn home_suggestion_navigation_does_not_overflow() {
        let sug = Suggestion::build_suggestions(1, 0, &[("v1".to_string(), 1, 2)], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        for _ in 0..10 {
            screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        assert_eq!(screen.selected_suggestion, 1);
    }

    #[test]
    fn home_j_navigates_quick_actions_when_focus_is_quick_actions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_action, 1);
        assert_eq!(screen.selected_suggestion, 0);
    }

    // -- Enter on a suggestion --

    #[test]
    fn home_enter_on_suggestion_returns_push_with_suggestion_action() {
        let sug = Suggestion::build_suggestions(3, 0, &[], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::IssueBrowser));
    }

    #[test]
    fn home_enter_on_milestone_suggestion_returns_push_milestone_view() {
        let sug = Suggestion::build_suggestions(0, 0, &[("v1".to_string(), 1, 5)], 1);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::MilestoneView));
    }

    #[test]
    fn home_enter_on_idle_suggestion_returns_push_overview() {
        let sug = Suggestion::build_suggestions(0, 0, &[], 0);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::Overview));
    }

    #[test]
    fn home_enter_when_suggestions_empty_and_focused_returns_none() {
        let mut screen = make_home_with_suggestions(vec![]);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    // -- Shortcut keys always active regardless of focus --

    #[test]
    fn home_char_i_returns_issue_browser_when_focused_on_suggestions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Char('i')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::IssueBrowser));
    }

    #[test]
    fn home_char_m_returns_milestone_view_when_focused_on_suggestions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Char('m')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::MilestoneView));
    }

    #[test]
    fn home_char_q_returns_quit_when_focused_on_suggestions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Char('q')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Quit);
    }

    // -- Edge cases --

    #[test]
    fn build_suggestions_singular_message_for_one_ready_issue() {
        let result = Suggestion::build_suggestions(1, 0, &[], 1);
        assert!(result[0].message.contains("1 issue labeled"));
        assert!(!result[0].message.contains("issues"));
    }

    #[test]
    fn build_suggestions_plural_message_for_multiple_ready_issues() {
        let result = Suggestion::build_suggestions(3, 0, &[], 1);
        assert!(result[0].message.contains("3 issues"));
    }

    #[test]
    fn build_suggestions_singular_message_for_one_failed_issue() {
        let result = Suggestion::build_suggestions(0, 1, &[], 1);
        assert!(result[0].message.contains("1 issue labeled"));
        assert!(!result[0].message.contains("issues"));
    }

    #[test]
    fn build_suggestions_milestone_closed_exceeds_total_clamps_to_100() {
        let result = Suggestion::build_suggestions(0, 0, &[("v1".to_string(), 15, 10)], 1);
        assert!(result[0].message.contains("100%"));
    }

    #[test]
    fn build_suggestions_milestone_fully_complete_shows_100() {
        let result = Suggestion::build_suggestions(0, 0, &[("v1".to_string(), 10, 10)], 1);
        assert!(result[0].message.contains("100%"));
    }

    #[test]
    fn build_suggestions_milestone_zero_closed_shows_0() {
        let result = Suggestion::build_suggestions(0, 0, &[("v1".to_string(), 0, 5)], 1);
        assert!(result[0].message.contains("0%"));
    }

    #[test]
    fn build_suggestions_multiple_nonzero_milestones_all_emitted() {
        let milestones = vec![
            ("v1".to_string(), 1u32, 5u32),
            ("v2".to_string(), 3u32, 10u32),
            ("v3".to_string(), 7u32, 7u32),
        ];
        let result = Suggestion::build_suggestions(0, 0, &milestones, 1);
        assert_eq!(result.len(), 3);
        for (i, (title, _, _)) in milestones.iter().enumerate() {
            assert!(result[i].message.contains(title.as_str()));
        }
    }

    #[test]
    fn home_j_on_empty_suggestions_when_focused_does_not_panic() {
        let mut screen = make_home_with_suggestions(vec![]);
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_suggestion, 0);
    }

    #[test]
    fn home_k_on_empty_suggestions_when_focused_does_not_panic() {
        let mut screen = make_home_with_suggestions(vec![]);
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected_suggestion, 0);
    }

    #[test]
    fn set_suggestions_resets_selected_index() {
        let sug = Suggestion::build_suggestions(1, 1, &[("v1".to_string(), 1, 2)], 0);
        let mut screen = make_home_with_suggestions(sug);
        focus_suggestions(&mut screen);
        // Navigate to index 2
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_suggestion, 2);
        // Replace with fewer suggestions
        let new_sug = Suggestion::build_suggestions(1, 0, &[], 1);
        screen.set_suggestions(new_sug);
        assert_eq!(screen.selected_suggestion, 0);
    }

    #[test]
    fn home_k_in_suggestions_does_not_move_quick_actions_selection() {
        let sug = Suggestion::build_suggestions(1, 0, &[("v1".to_string(), 1, 2)], 1);
        let mut screen = make_home_with_suggestions(sug);
        // Move quick actions selection to 2
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected_action, 2);
        // Switch to suggestions and navigate
        focus_suggestions(&mut screen);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        // Quick actions selection must be unchanged
        assert_eq!(screen.selected_action, 2);
        assert_eq!(screen.selected_suggestion, 0);
    }

    // --- Issue #86: suggestion refresh keybinding and loading state ---

    #[test]
    fn home_shift_r_returns_refresh_suggestions() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('R')), InputMode::Normal);
        assert_eq!(action, ScreenAction::RefreshSuggestions);
    }

    #[test]
    fn home_shift_r_from_suggestions_pane_returns_refresh_suggestions() {
        let mut screen = make_home_with_suggestions(vec![]);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Char('R')), InputMode::Normal);
        assert_eq!(action, ScreenAction::RefreshSuggestions);
    }

    #[test]
    fn home_loading_suggestions_defaults_to_false() {
        let screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        assert!(!screen.loading_suggestions);
    }

    #[test]
    fn set_suggestions_clears_loading_flag() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        screen.loading_suggestions = true;
        screen.set_suggestions(vec![]);
        assert!(!screen.loading_suggestions);
    }

    // --- Issue #238: What's New / Release Notes keybinding ---

    #[test]
    fn home_key_n_returns_push_release_notes() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Char('n')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::ReleaseNotes));
    }

    #[test]
    fn home_key_n_works_from_suggestions_pane() {
        let mut screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        focus_suggestions(&mut screen);
        let action = screen.handle_input(&key_event(KeyCode::Char('n')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Push(TuiMode::ReleaseNotes));
    }

    #[test]
    fn home_keybindings_contains_release_notes() {
        let screen = HomeScreen::new(make_project_info(), vec![], vec![]);
        let groups = screen.keybindings();
        let all_bindings: Vec<&str> = groups
            .iter()
            .flat_map(|g| g.bindings.iter().map(|b| b.key))
            .collect();
        assert!(all_bindings.contains(&"n"));
    }
}
