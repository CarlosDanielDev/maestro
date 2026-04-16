mod draw;
pub mod types;

pub use types::*;

use crate::github::types::GhPullRequest;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{Frame, layout::Rect};

use super::{Screen, ScreenAction};

pub struct PrReviewScreen {
    pub step: PrReviewStep,
    pub prs: Vec<GhPullRequest>,
    pub selected: usize,
    pub scroll_offset: u16,
    pub current_pr: Option<GhPullRequest>,
    pub form: ReviewForm,
    pub error: Option<String>,
    pub spinner_tick: usize,
}

impl PrReviewScreen {
    pub fn new() -> Self {
        Self {
            step: PrReviewStep::Loading,
            prs: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            current_pr: None,
            form: ReviewForm::default(),
            error: None,
            spinner_tick: 0,
        }
    }

    pub fn tick(&mut self) {
        self.spinner_tick = self.spinner_tick.wrapping_add(1);
    }

    pub fn set_prs(&mut self, prs: Vec<GhPullRequest>) {
        self.prs = prs;
        self.selected = 0;
        self.error = None;
        self.step = PrReviewStep::PrList;
    }

    pub fn find_pr(&self, number: u64) -> Option<GhPullRequest> {
        self.prs.iter().find(|p| p.number == number).cloned()
    }

    pub fn set_pr_detail(&mut self, pr: GhPullRequest) {
        self.current_pr = Some(pr);
        self.scroll_offset = 0;
        self.step = PrReviewStep::PrDetail;
    }

    pub fn open_submit_form(&mut self) {
        self.form = ReviewForm::default();
        self.step = PrReviewStep::SubmitReview;
    }

    pub fn set_done(&mut self) {
        self.step = PrReviewStep::Done;
    }

    pub fn set_error(&mut self, msg: &str) {
        self.error = Some(msg.to_string());
    }

    pub fn set_loading_error(&mut self, msg: &str) {
        self.error = Some(msg.to_string());
    }

    #[allow(dead_code)] // Reason: will be used by data_handler for error recovery
    pub fn clear_error(&mut self) {
        self.error = None;
    }

    fn handle_pr_list_input(&mut self, code: KeyCode) -> ScreenAction {
        match code {
            KeyCode::Char('j') | KeyCode::Down
                if !self.prs.is_empty() && self.selected < self.prs.len() - 1 =>
            {
                self.selected += 1;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                if self.prs.is_empty() {
                    return ScreenAction::None;
                }
                let pr_number = self.prs[self.selected].number;
                return ScreenAction::FetchPrDetail(pr_number);
            }
            KeyCode::Esc => return ScreenAction::Pop,
            _ => {}
        }
        ScreenAction::None
    }

    fn handle_pr_detail_input(&mut self, code: KeyCode) -> ScreenAction {
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
            KeyCode::Char('r') => {
                self.open_submit_form();
            }
            KeyCode::Esc => {
                self.step = PrReviewStep::PrList;
            }
            _ => {}
        }
        ScreenAction::None
    }

    fn handle_submit_review_input(&mut self, code: KeyCode) -> ScreenAction {
        match code {
            KeyCode::Tab => {
                self.form.event = self.form.event.next();
            }
            KeyCode::BackTab => {
                self.form.event = self.form.event.prev();
            }
            KeyCode::Backspace => {
                self.form.body.pop();
            }
            KeyCode::Enter => {
                if let Some(ref pr) = self.current_pr {
                    return ScreenAction::SubmitPrReview {
                        pr_number: pr.number,
                        event: self.form.event,
                        body: self.form.body.clone(),
                    };
                }
            }
            KeyCode::Esc => {
                self.step = PrReviewStep::PrDetail;
            }
            KeyCode::Char(c) => {
                self.form.body.push(c);
            }
            _ => {}
        }
        ScreenAction::None
    }
}

impl KeymapProvider for PrReviewScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        match self.step {
            PrReviewStep::Loading => vec![KeyBindingGroup {
                title: "Actions",
                bindings: vec![KeyBinding {
                    key: "Esc",
                    description: "Cancel",
                }],
            }],
            PrReviewStep::PrList => vec![
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
                    ],
                },
                KeyBindingGroup {
                    title: "Actions",
                    bindings: vec![
                        KeyBinding {
                            key: "Enter",
                            description: "View PR",
                        },
                        KeyBinding {
                            key: "Esc",
                            description: "Back",
                        },
                    ],
                },
            ],
            PrReviewStep::PrDetail => vec![
                KeyBindingGroup {
                    title: "Navigation",
                    bindings: vec![KeyBinding {
                        key: "j/k",
                        description: "Scroll",
                    }],
                },
                KeyBindingGroup {
                    title: "Actions",
                    bindings: vec![
                        KeyBinding {
                            key: "r",
                            description: "Review",
                        },
                        KeyBinding {
                            key: "Esc",
                            description: "Back",
                        },
                    ],
                },
            ],
            PrReviewStep::SubmitReview => vec![KeyBindingGroup {
                title: "Form",
                bindings: vec![
                    KeyBinding {
                        key: "Tab",
                        description: "Cycle review type",
                    },
                    KeyBinding {
                        key: "Enter",
                        description: "Submit",
                    },
                    KeyBinding {
                        key: "Esc",
                        description: "Cancel",
                    },
                ],
            }],
            PrReviewStep::Done => vec![KeyBindingGroup {
                title: "Actions",
                bindings: vec![KeyBinding {
                    key: "Esc/Enter",
                    description: "Back",
                }],
            }],
        }
    }
}

impl Screen for PrReviewScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match self.step {
                PrReviewStep::Loading => {
                    if *code == KeyCode::Esc {
                        return ScreenAction::Pop;
                    }
                }
                PrReviewStep::PrList => return self.handle_pr_list_input(*code),
                PrReviewStep::PrDetail => return self.handle_pr_detail_input(*code),
                PrReviewStep::SubmitReview => return self.handle_submit_review_input(*code),
                PrReviewStep::Done => {
                    if matches!(code, KeyCode::Esc | KeyCode::Enter) {
                        return ScreenAction::Pop;
                    }
                }
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        draw::draw_pr_review_screen(self, f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        match self.step {
            PrReviewStep::SubmitReview => Some(InputMode::Insert),
            _ => Some(InputMode::Normal),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::types::PrReviewEvent;
    use crate::tui::screens::test_helpers::key_event;
    use crossterm::event::KeyCode;

    fn make_pr(number: u64) -> GhPullRequest {
        GhPullRequest {
            number,
            title: format!("PR #{}: Fix something", number),
            body: format!("## Summary\n\nFixes issue #{}", number),
            state: "open".to_string(),
            html_url: format!("https://github.com/owner/repo/pull/{}", number),
            head_branch: format!("maestro/issue-{}", number),
            base_branch: "main".to_string(),
            author: "bot".to_string(),
            labels: vec![],
            draft: false,
            mergeable: true,
            additions: 10,
            deletions: 5,
            changed_files: 3,
        }
    }

    fn make_three_prs() -> Vec<GhPullRequest> {
        vec![make_pr(1), make_pr(2), make_pr(3)]
    }

    fn screen_at_pr_list() -> PrReviewScreen {
        let mut screen = PrReviewScreen::new();
        screen.set_prs(make_three_prs());
        screen
    }

    fn screen_at_pr_detail() -> PrReviewScreen {
        let mut screen = screen_at_pr_list();
        screen.set_pr_detail(make_pr(1));
        screen
    }

    fn screen_at_submit_review() -> PrReviewScreen {
        let mut screen = screen_at_pr_detail();
        screen.open_submit_form();
        screen
    }

    // --- Group 4: initial state ---

    #[test]
    fn new_starts_in_loading_state() {
        let screen = PrReviewScreen::new();
        assert_eq!(screen.step, PrReviewStep::Loading);
    }

    #[test]
    fn new_has_no_prs_initially() {
        let screen = PrReviewScreen::new();
        assert!(screen.prs.is_empty());
    }

    #[test]
    fn new_selected_index_is_zero() {
        let screen = PrReviewScreen::new();
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn new_scroll_offset_is_zero() {
        let screen = PrReviewScreen::new();
        assert_eq!(screen.scroll_offset, 0);
    }

    #[test]
    fn new_has_no_error() {
        let screen = PrReviewScreen::new();
        assert!(screen.error.is_none());
    }

    // --- Group 5: state transitions ---

    #[test]
    fn set_prs_transitions_loading_to_pr_list() {
        let mut screen = PrReviewScreen::new();
        screen.set_prs(vec![make_pr(1), make_pr(2)]);
        assert_eq!(screen.step, PrReviewStep::PrList);
        assert_eq!(screen.prs.len(), 2);
    }

    #[test]
    fn set_prs_with_empty_list_transitions_to_pr_list_with_empty_state() {
        let mut screen = PrReviewScreen::new();
        screen.set_prs(vec![]);
        assert_eq!(screen.step, PrReviewStep::PrList);
        assert!(screen.prs.is_empty());
    }

    #[test]
    fn set_pr_detail_transitions_to_pr_detail() {
        let mut screen = screen_at_pr_list();
        screen.set_pr_detail(make_pr(5));
        assert_eq!(screen.step, PrReviewStep::PrDetail);
        assert_eq!(screen.current_pr.as_ref().unwrap().number, 5);
    }

    #[test]
    fn open_submit_form_transitions_to_submit_review() {
        let mut screen = screen_at_pr_detail();
        screen.open_submit_form();
        assert_eq!(screen.step, PrReviewStep::SubmitReview);
    }

    #[test]
    fn set_error_stores_message_and_does_not_change_step() {
        let mut screen = screen_at_pr_list();
        screen.set_error("network failure");
        assert_eq!(screen.error.as_deref(), Some("network failure"));
        assert_eq!(screen.step, PrReviewStep::PrList);
    }

    #[test]
    fn set_done_transitions_to_done() {
        let mut screen = screen_at_submit_review();
        screen.set_done();
        assert_eq!(screen.step, PrReviewStep::Done);
    }

    #[test]
    fn clear_error_removes_stored_error() {
        let mut screen = PrReviewScreen::new();
        screen.set_error("bad");
        screen.clear_error();
        assert!(screen.error.is_none());
    }

    // --- Group 6: PrList keyboard ---

    #[test]
    fn pr_list_key_j_advances_cursor() {
        let mut screen = screen_at_pr_list();
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn pr_list_key_down_advances_cursor() {
        let mut screen = screen_at_pr_list();
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn pr_list_key_k_moves_cursor_up() {
        let mut screen = screen_at_pr_list();
        screen.selected = 2;
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn pr_list_key_up_moves_cursor_up() {
        let mut screen = screen_at_pr_list();
        screen.selected = 2;
        screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn pr_list_cursor_does_not_underflow_at_zero() {
        let mut screen = screen_at_pr_list();
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn pr_list_cursor_does_not_overflow_past_last() {
        let mut screen = screen_at_pr_list();
        for _ in 0..10 {
            screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        assert_eq!(screen.selected, 2);
    }

    #[test]
    fn pr_list_enter_on_empty_list_returns_none() {
        let mut screen = PrReviewScreen::new();
        screen.set_prs(vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn pr_list_enter_returns_fetch_pr_detail_action() {
        let mut screen = screen_at_pr_list();
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::FetchPrDetail(1));
    }

    #[test]
    fn pr_list_esc_returns_pop() {
        let mut screen = screen_at_pr_list();
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    // --- Group 7: PrDetail keyboard ---

    #[test]
    fn pr_detail_key_j_scrolls_down() {
        let mut screen = screen_at_pr_detail();
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.scroll_offset, 1);
    }

    #[test]
    fn pr_detail_key_down_scrolls_down() {
        let mut screen = screen_at_pr_detail();
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        assert_eq!(screen.scroll_offset, 1);
    }

    #[test]
    fn pr_detail_key_k_scrolls_up() {
        let mut screen = screen_at_pr_detail();
        screen.scroll_offset = 3;
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.scroll_offset, 2);
    }

    #[test]
    fn pr_detail_scroll_does_not_underflow() {
        let mut screen = screen_at_pr_detail();
        for _ in 0..5 {
            screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        }
        assert_eq!(screen.scroll_offset, 0);
    }

    #[test]
    fn pr_detail_key_r_opens_submit_form() {
        let mut screen = screen_at_pr_detail();
        screen.handle_input(&key_event(KeyCode::Char('r')), InputMode::Normal);
        assert_eq!(screen.step, PrReviewStep::SubmitReview);
    }

    #[test]
    fn pr_detail_esc_returns_to_pr_list() {
        let mut screen = screen_at_pr_detail();
        screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(screen.step, PrReviewStep::PrList);
    }

    // --- Group 8: SubmitReview form ---

    #[test]
    fn submit_review_default_event_is_comment() {
        let screen = screen_at_submit_review();
        assert_eq!(screen.form.event, PrReviewEvent::Comment);
    }

    #[test]
    fn submit_review_tab_cycles_event_type_forward() {
        let mut screen = screen_at_submit_review();
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        assert_eq!(screen.form.event, PrReviewEvent::Approve);
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        assert_eq!(screen.form.event, PrReviewEvent::RequestChanges);
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Insert);
        assert_eq!(screen.form.event, PrReviewEvent::Comment);
    }

    #[test]
    fn submit_review_backtab_cycles_event_type_backward() {
        let mut screen = screen_at_submit_review();
        screen.handle_input(&key_event(KeyCode::BackTab), InputMode::Insert);
        assert_eq!(screen.form.event, PrReviewEvent::RequestChanges);
    }

    #[test]
    fn submit_review_char_input_appends_to_body() {
        let mut screen = screen_at_submit_review();
        screen.handle_input(&key_event(KeyCode::Char('L')), InputMode::Insert);
        screen.handle_input(&key_event(KeyCode::Char('G')), InputMode::Insert);
        assert_eq!(screen.form.body, "LG");
    }

    #[test]
    fn submit_review_backspace_removes_last_char_from_body() {
        let mut screen = screen_at_submit_review();
        screen.form.body = "LG".to_string();
        screen.handle_input(&key_event(KeyCode::Backspace), InputMode::Insert);
        assert_eq!(screen.form.body, "L");
    }

    #[test]
    fn submit_review_backspace_on_empty_body_does_not_panic() {
        let mut screen = screen_at_submit_review();
        screen.handle_input(&key_event(KeyCode::Backspace), InputMode::Insert);
        assert!(screen.form.body.is_empty());
    }

    #[test]
    fn submit_review_enter_returns_submit_action() {
        let mut screen = screen_at_submit_review();
        screen.form.event = PrReviewEvent::Approve;
        screen.form.body = "LGTM".to_string();
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(
            action,
            ScreenAction::SubmitPrReview {
                pr_number: 1,
                event: PrReviewEvent::Approve,
                body: "LGTM".to_string(),
            }
        );
    }

    #[test]
    fn submit_review_enter_with_empty_body_still_returns_submit_action() {
        let mut screen = screen_at_submit_review();
        screen.form.event = PrReviewEvent::Approve;
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert);
        assert_eq!(
            action,
            ScreenAction::SubmitPrReview {
                pr_number: 1,
                event: PrReviewEvent::Approve,
                body: String::new(),
            }
        );
    }

    #[test]
    fn submit_review_esc_returns_to_pr_detail() {
        let mut screen = screen_at_submit_review();
        screen.handle_input(&key_event(KeyCode::Esc), InputMode::Insert);
        assert_eq!(screen.step, PrReviewStep::PrDetail);
    }

    // --- Group 9: Done step ---

    #[test]
    fn done_esc_returns_pop() {
        let mut screen = PrReviewScreen::new();
        screen.step = PrReviewStep::Done;
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn done_enter_returns_pop() {
        let mut screen = PrReviewScreen::new();
        screen.step = PrReviewStep::Done;
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn done_any_other_key_returns_none() {
        let mut screen = PrReviewScreen::new();
        screen.step = PrReviewStep::Done;
        let action = screen.handle_input(&key_event(KeyCode::Char('x')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    // --- Group 10: Error states ---

    #[test]
    fn error_is_cleared_when_new_prs_arrive() {
        let mut screen = screen_at_pr_list();
        screen.set_error("timeout");
        screen.set_prs(vec![make_pr(1)]);
        assert!(screen.error.is_none());
    }

    #[test]
    fn error_displayed_in_loading_state_after_fetch_fails() {
        let mut screen = PrReviewScreen::new();
        screen.set_loading_error("network timeout");
        assert_eq!(screen.error.as_deref(), Some("network timeout"));
        assert_eq!(screen.step, PrReviewStep::Loading);
    }

    #[test]
    fn loading_state_ignores_navigation_keys() {
        let mut screen = PrReviewScreen::new();
        let action1 = screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        let action2 = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action1, ScreenAction::None);
        assert_eq!(action2, ScreenAction::None);
        assert_eq!(screen.selected, 0);
    }

    // --- Tick ---

    #[test]
    fn tick_increments_spinner() {
        let mut screen = PrReviewScreen::new();
        assert_eq!(screen.spinner_tick, 0);
        screen.tick();
        assert_eq!(screen.spinner_tick, 1);
    }

    // --- desired_input_mode ---

    #[test]
    fn desired_input_mode_is_insert_for_submit_review() {
        let screen = screen_at_submit_review();
        assert_eq!(screen.desired_input_mode(), Some(InputMode::Insert));
    }

    #[test]
    fn desired_input_mode_is_normal_for_pr_list() {
        let screen = screen_at_pr_list();
        assert_eq!(screen.desired_input_mode(), Some(InputMode::Normal));
    }
}
