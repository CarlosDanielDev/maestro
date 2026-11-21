mod draw;
use super::{Screen, ScreenAction, SessionConfig, sanitize_for_terminal};
use crate::provider::types::Issue;
use crate::tui::marquee::MarqueeState;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crate::tui::widgets::launch_dialog::{
    self, LaunchAction, LaunchBudget, LaunchDialog, LaunchMode,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{Frame, layout::Rect};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusPane {
    List,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    None,
    Label,
    Milestone,
}

pub struct IssueBrowserScreen {
    pub(crate) issues: Vec<Issue>,
    pub(crate) filtered_indices: Vec<usize>,
    pub(crate) selected: usize,
    scroll_offset: usize,
    pub(crate) selected_set: HashSet<u64>,
    pub(crate) filter_mode: FilterMode,
    pub(crate) filter_text: String,
    milestone_filter: Option<u64>,
    pub(crate) loading: bool,
    /// Last known visible height from draw, used for scroll sync.
    last_visible_height: usize,
    /// The shared launch dialog (#1262), open before launching session(s).
    pub(crate) launch_dialog: Option<LaunchDialog>,
    /// Spend the dialog states, synced from App every frame.
    pub(crate) launch_budget: LaunchBudget,
    /// Layout configuration for panel arrangement.
    pub(crate) layout: crate::config::LayoutConfig,
    /// Marquee animation state for the currently selected issue title.
    marquee: MarqueeState,
    /// Which panel currently has focus.
    pub(crate) focus: FocusPane,
    /// Vertical scroll offset for the preview panel markdown content.
    pub(crate) preview_scroll: u16,
    /// Launch defaults `(produce_pr, interaction)` from `[behavior.launch]`
    /// (or `(true, false)`); they pick the dialog's initial mode.
    pub(crate) launch_defaults: (bool, bool),
    /// Animation tick for the braille loading spinner (#738 QA polish).
    pub(crate) spinner_tick: usize,
}

impl IssueBrowserScreen {
    pub fn new(issues: Vec<Issue>) -> Self {
        let filtered_indices: Vec<usize> = (0..issues.len()).collect();
        Self {
            issues,
            filtered_indices,
            selected: 0,
            scroll_offset: 0,
            selected_set: HashSet::new(),
            filter_mode: FilterMode::None,
            filter_text: String::new(),
            milestone_filter: None,
            loading: false,
            last_visible_height: 20,
            launch_dialog: None,
            launch_budget: LaunchBudget::default(),
            layout: crate::config::LayoutConfig::default(),
            marquee: MarqueeState::new(),
            focus: FocusPane::List,
            preview_scroll: 0,
            launch_defaults: (true, false),
            spinner_tick: 0,
        }
    }

    /// Inject the global animation tick so the loading spinner advances each
    /// frame. Mirrors `set_spinner_context` on the milestone/wizard screens.
    pub fn set_spinner_context(&mut self, spinner_tick: usize) {
        self.spinner_tick = spinner_tick;
    }

    pub fn with_layout(mut self, layout: crate::config::LayoutConfig) -> Self {
        self.layout = layout;
        self
    }

    /// Seed the launch defaults `(produce_pr, interaction)`.
    pub fn with_launch_defaults(mut self, defaults: (bool, bool)) -> Self {
        self.launch_defaults = defaults;
        self
    }

    /// The spend the launch dialog states (model, caps, spent so far).
    /// `ui.rs` calls it every frame so an open dialog stays current.
    pub fn set_launch_budget(&mut self, budget: LaunchBudget) {
        if let Some(dialog) = self.launch_dialog.as_mut() {
            dialog.set_budget(budget.clone());
        }
        self.launch_budget = budget;
    }

    /// Open the launch dialog for `issues`, seeded with the defaults.
    fn open_dialog(&mut self, issues: Vec<(u64, String)>) {
        let mode = LaunchMode::from_defaults(self.launch_defaults);
        let mut dialog = LaunchDialog::new(issues, mode);
        dialog.set_budget(self.launch_budget.clone());
        self.launch_dialog = Some(dialog);
    }

    pub fn set_issues(&mut self, issues: Vec<Issue>) {
        self.issues = issues;
        self.selected = 0;
        self.scroll_offset = 0;
        self.loading = false;
        self.marquee.reset();
        self.focus = FocusPane::List;
        self.preview_scroll = 0;
        self.reapply_filters();
    }
}

impl KeymapProvider for IssueBrowserScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![
            KeyBindingGroup {
                title: "Navigation",
                bindings: vec![
                    KeyBinding {
                        key: "j/Down",
                        description: "Move down / Scroll preview",
                    },
                    KeyBinding {
                        key: "k/Up",
                        description: "Move up / Scroll preview",
                    },
                    KeyBinding {
                        key: "Tab",
                        description: "Switch focus pane",
                    },
                    KeyBinding {
                        key: "Space",
                        description: "Toggle multi-select",
                    },
                ],
            },
            KeyBindingGroup {
                title: "Actions",
                bindings: vec![
                    KeyBinding {
                        key: "Enter",
                        description: "Run selected issue(s)",
                    },
                    KeyBinding {
                        key: "/",
                        description: "Filter by label/title",
                    },
                    KeyBinding {
                        key: "m",
                        description: "Filter by milestone",
                    },
                    KeyBinding {
                        key: "Esc",
                        description: "Back / Cancel filter",
                    },
                ],
            },
        ]
    }
}

impl Screen for IssueBrowserScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        // While the launch dialog is open it takes every key.
        if let Some(dialog) = self.launch_dialog.as_mut() {
            return match dialog.handle_input(event) {
                LaunchAction::Cancel => {
                    self.launch_dialog = None;
                    ScreenAction::None
                }
                LaunchAction::Confirm(custom_prompt) => self
                    .launch_dialog
                    .take()
                    .map_or(ScreenAction::None, |d| launch_action(d, custom_prompt)),
                LaunchAction::None => ScreenAction::None,
            };
        }

        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            // In filter mode, handle text input (acts like Insert mode)
            if self.filter_mode != FilterMode::None {
                return self.handle_filter_input(*code);
            }

            match code {
                KeyCode::Esc => {
                    if self.focus == FocusPane::Preview {
                        self.focus = FocusPane::List;
                    } else {
                        return ScreenAction::Pop;
                    }
                }
                KeyCode::Tab | KeyCode::BackTab => {
                    self.focus = match self.focus {
                        FocusPane::List => FocusPane::Preview,
                        FocusPane::Preview => FocusPane::List,
                    };
                }
                KeyCode::Char('j') | KeyCode::Down => match self.focus {
                    FocusPane::List => {
                        if !self.filtered_indices.is_empty()
                            && self.selected < self.filtered_indices.len() - 1
                        {
                            self.selected += 1;
                            self.preview_scroll = 0;
                            self.sync_scroll();
                            self.marquee.reset();
                        }
                    }
                    FocusPane::Preview => {
                        self.preview_scroll = self.preview_scroll.saturating_add(1);
                    }
                },
                KeyCode::Char('k') | KeyCode::Up => match self.focus {
                    FocusPane::List => {
                        self.selected = self.selected.saturating_sub(1);
                        self.preview_scroll = 0;
                        self.sync_scroll();
                        self.marquee.reset();
                    }
                    FocusPane::Preview => {
                        self.preview_scroll = self.preview_scroll.saturating_sub(1);
                    }
                },
                KeyCode::Char(' ') => {
                    if let Some(&idx) = self.filtered_indices.get(self.selected) {
                        let number = self.issues[idx].number;
                        if !self.selected_set.remove(&number) {
                            self.selected_set.insert(number);
                        }
                    }
                }
                KeyCode::Char('/') => {
                    self.filter_mode = FilterMode::Label;
                }
                KeyCode::Char('m') => {
                    self.filter_mode = FilterMode::Milestone;
                }
                KeyCode::Char('t') => {
                    if let Some(&idx) = self.filtered_indices.get(self.selected) {
                        let issue = &self.issues[idx];
                        return ScreenAction::PushTeamWizard {
                            mode: crate::tui::screens::team_wizard::TeamWizardMode::Launch,
                            preselect: Some(
                                crate::tui::screens::team_wizard::TeamLaunchInput::Issue {
                                    number: issue.number,
                                    title: issue.title.clone(),
                                },
                            ),
                        };
                    }
                }
                KeyCode::Enter => {
                    return self.handle_enter();
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.draw_impl(f, area, theme);
        if let Some(dialog) = self.launch_dialog.as_ref() {
            launch_dialog::draw(f, area, dialog, theme);
        }
    }

    fn footer_hints(&self) -> Vec<(&'static str, &'static str)> {
        self.footer_hints_impl()
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        if self.launch_dialog.is_some() || self.filter_mode != FilterMode::None {
            Some(InputMode::Insert)
        } else {
            Some(InputMode::Normal)
        }
    }
}

/// The launch a confirmed dialog asks for: one session, one unified
/// session, or one per issue. The mode sets `produce_pr` / `interaction`.
fn launch_action(dialog: LaunchDialog, custom_prompt: Option<String>) -> ScreenAction {
    let (produce_pr, interaction) = dialog.mode.flags();
    let unified = dialog.unified_pr;
    let mut issues = dialog.selected_issues;
    if issues.len() == 1 {
        let (number, title) = issues.remove(0);
        return ScreenAction::LaunchSession(SessionConfig {
            issue_number: Some(number),
            title,
            custom_prompt,
            agent_id: None,
            produce_pr,
            interaction,
        });
    }
    if unified {
        return ScreenAction::LaunchUnifiedSession(super::UnifiedSessionConfig {
            issues,
            custom_prompt,
            agent_id: None,
            produce_pr,
            interaction,
        });
    }
    let configs = issues
        .into_iter()
        .map(|(number, title)| SessionConfig {
            issue_number: Some(number),
            title,
            custom_prompt: custom_prompt.clone(),
            agent_id: None,
            produce_pr,
            interaction,
        })
        .collect();
    ScreenAction::LaunchSessions(configs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::{key_event, key_event_with_modifiers};
    use crossterm::event::{KeyCode, KeyModifiers};

    fn make_issue(number: u64, title: &str) -> Issue {
        Issue {
            number,
            title: title.to_string(),
            body: String::new(),
            labels: vec!["maestro:ready".to_string()],
            state: "open".to_string(),
            html_url: format!("https://github.com/owner/repo/issues/{}", number),
            milestone: None,
            assignees: vec![],
        }
    }

    fn make_issue_with_milestone(number: u64, milestone_number: u64) -> Issue {
        Issue {
            number,
            title: format!("Issue #{}", number),
            body: String::new(),
            labels: vec![],
            state: "open".to_string(),
            html_url: format!("https://github.com/owner/repo/issues/{}", number),
            milestone: Some(milestone_number),
            assignees: vec![],
        }
    }

    fn make_three_issues() -> Vec<Issue> {
        vec![
            make_issue(1, "Add login"),
            make_issue(2, "Fix crash"),
            make_issue(3, "Add logout"),
        ]
    }

    /// Build an IssueBrowserScreen with issues #1 and #3 already in selected_set.
    fn screen_with_two_selected() -> IssueBrowserScreen {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal); // select #1
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal); // move to #2
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal); // move to #3
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal); // select #3
        screen
    }

    // ---- initial state ----

    #[test]
    fn issue_browser_initial_state_has_all_issues_visible() {
        let screen = IssueBrowserScreen::new(make_three_issues());
        assert_eq!(screen.filtered_indices.len(), 3);
    }

    #[test]
    fn issue_browser_initial_selected_is_zero() {
        let screen = IssueBrowserScreen::new(make_three_issues());
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn set_spinner_context_stores_tick_for_braille_loading() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.set_spinner_context(7);
        assert_eq!(screen.spinner_tick, 7);
        // The render path feeds this tick to spinner::graph_node_frame, which
        // yields a braille glyph (non-ASCII) under Nerd Font.
        assert!(!crate::tui::spinner::graph_node_frame(7, true).is_ascii());
    }

    #[test]
    fn issue_browser_loading_flag_initially_false() {
        let screen = IssueBrowserScreen::new(make_three_issues());
        assert!(!screen.loading);
    }

    // ---- navigation ----

    #[test]
    fn issue_browser_key_j_advances_cursor() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn issue_browser_key_down_advances_cursor() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn issue_browser_key_k_moves_cursor_up() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn issue_browser_key_up_moves_cursor_up() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn issue_browser_cursor_does_not_underflow() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn issue_browser_cursor_does_not_overflow() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        for _ in 0..10 {
            screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        assert_eq!(screen.selected, 2);
    }

    // ---- screen actions ----

    #[test]
    fn issue_browser_esc_from_list_returns_pop() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        assert_eq!(screen.focus, FocusPane::List);
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn issue_browser_enter_on_single_issue_requests_resume_or_launch() {
        // #738: single-issue Enter defers to the dispatch (which sees the
        // session pool) so an active interaction session can be resumed
        // without the launch dialog. The overlay is opened by the dispatch
        // fallback, not by the browser directly.
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal); // move to issue 2 (number=2)
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(
            action,
            ScreenAction::ResumeOrLaunchIssue { issue_number: 2 }
        );
        assert!(
            screen.launch_dialog.is_none(),
            "browser must not open the overlay itself"
        );
    }

    #[test]
    fn open_launch_overlay_for_selected_opens_dialog_for_current_issue() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.open_launch_overlay_for_selected();
        let overlay = screen
            .launch_dialog
            .as_ref()
            .expect("overlay should be open");
        assert_eq!(overlay.selected_issues.len(), 1);
        assert_eq!(overlay.selected_issues[0].0, 2);
        assert_eq!(overlay.selected_issues[0].1, "Fix crash");
    }

    #[test]
    fn issue_browser_enter_with_multi_select_returns_launch_sessions() {
        let mut screen = screen_with_two_selected();

        // First Enter: opens overlay, does not launch
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::None, "first Enter must open overlay");
        assert!(screen.launch_dialog.is_some());

        // Second Enter (empty prompt): dispatches LaunchSessions
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::LaunchSessions(configs) => {
                assert_eq!(configs.len(), 2);
            }
            other => panic!("Expected LaunchSessions, got {:?}", other),
        }
    }

    #[test]
    fn issue_browser_empty_issue_list_enter_returns_none() {
        let mut screen = IssueBrowserScreen::new(vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    // ---- multi-select ----

    #[test]
    fn issue_browser_space_adds_issue_to_selected_set() {
        let issues = make_three_issues();
        let issue_number = issues[0].number;
        let mut screen = IssueBrowserScreen::new(issues);
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(screen.selected_set.contains(&issue_number));
    }

    #[test]
    fn issue_browser_space_removes_issue_from_selected_set_if_already_selected() {
        let issues = make_three_issues();
        let issue_number = issues[0].number;
        let mut screen = IssueBrowserScreen::new(issues);
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
        assert!(!screen.selected_set.contains(&issue_number));
    }

    // ---- label filter ----

    #[test]
    fn issue_browser_slash_enters_filter_mode() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('/')), InputMode::Normal);
        assert_eq!(screen.filter_mode, FilterMode::Label);
    }

    #[test]
    fn issue_browser_filter_text_updates_filtered_indices() {
        let issues = vec![
            make_issue(1, "Add login"),
            make_issue(2, "Fix crash"),
            make_issue(3, "Add logout"),
        ];
        let mut screen = IssueBrowserScreen::new(issues);
        screen.handle_input(&key_event(KeyCode::Char('/')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('A')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
        assert_eq!(screen.filtered_indices.len(), 2);
    }

    #[test]
    fn issue_browser_filter_text_is_case_insensitive() {
        let issues = vec![make_issue(1, "Implement Feature")];
        let mut screen = IssueBrowserScreen::new(issues);
        screen.handle_input(&key_event(KeyCode::Char('/')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('i')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('m')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('p')), InputMode::Normal);
        assert_eq!(screen.filtered_indices.len(), 1);
    }

    #[test]
    fn issue_browser_esc_in_filter_mode_clears_filter_and_exits() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('/')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('F')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('i')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert!(screen.filter_text.is_empty());
        assert_eq!(screen.filter_mode, FilterMode::None);
        assert_eq!(screen.filtered_indices.len(), 3);
    }

    #[test]
    fn issue_browser_backspace_in_filter_mode_deletes_last_char() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('/')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('a')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('b')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Backspace), InputMode::Normal);
        assert_eq!(screen.filter_text, "a");
    }

    #[test]
    fn issue_browser_filter_no_match_results_in_empty_filtered_indices() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('/')), InputMode::Normal);
        for c in "zzznomatch".chars() {
            screen.handle_input(&key_event(KeyCode::Char(c)), InputMode::Normal);
        }
        assert_eq!(screen.filtered_indices.len(), 0);
    }

    // ---- milestone filter ----

    #[test]
    fn issue_browser_key_m_enters_milestone_filter_mode() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('m')), InputMode::Normal);
        assert_eq!(screen.filter_mode, FilterMode::Milestone);
    }

    #[test]
    fn issue_browser_milestone_filter_shows_only_matching_issues() {
        let issues = vec![
            make_issue_with_milestone(1, 10),
            make_issue_with_milestone(2, 10),
            make_issue_with_milestone(3, 99),
        ];
        let mut screen = IssueBrowserScreen::new(issues);
        screen.set_milestone_filter(Some(10));
        assert_eq!(screen.filtered_indices.len(), 2);
    }

    #[test]
    fn issue_browser_clear_milestone_filter_restores_all_issues() {
        let issues = vec![
            make_issue_with_milestone(1, 10),
            make_issue_with_milestone(2, 10),
            make_issue_with_milestone(3, 99),
        ];
        let mut screen = IssueBrowserScreen::new(issues);
        screen.set_milestone_filter(Some(10));
        assert_eq!(screen.filtered_indices.len(), 2);
        screen.set_milestone_filter(None);
        assert_eq!(screen.filtered_indices.len(), 3);
    }

    // ---- cursor clamping after filter ----

    #[test]
    fn issue_browser_cursor_clamps_when_filter_reduces_list() {
        let issues = vec![
            make_issue(1, "Alpha one"),
            make_issue(2, "Alpha two"),
            make_issue(3, "Beta one"),
            make_issue(4, "Beta two"),
            make_issue(5, "Beta three"),
        ];
        let mut screen = IssueBrowserScreen::new(issues);
        for _ in 0..4 {
            screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        assert_eq!(screen.selected, 4);
        screen.handle_input(&key_event(KeyCode::Char('/')), InputMode::Normal);
        for c in "Alpha".chars() {
            screen.handle_input(&key_event(KeyCode::Char(c)), InputMode::Normal);
        }
        assert!(screen.selected <= 1);
    }

    // ---- set_issues with milestone filter (issue #117) ----

    #[test]
    fn set_issues_with_active_milestone_filter_respects_filter() {
        let mut screen = IssueBrowserScreen::new(vec![]);
        screen.set_milestone_filter(Some(10));

        let issues = vec![
            make_issue_with_milestone(1, 10),
            make_issue_with_milestone(2, 10),
            make_issue_with_milestone(3, 99),
        ];
        screen.set_issues(issues);

        assert_eq!(
            screen.filtered_indices.len(),
            2,
            "set_issues must reapply active milestone filter"
        );
    }

    #[test]
    fn set_issues_without_active_milestone_filter_shows_all() {
        let mut screen = IssueBrowserScreen::new(vec![]);

        let issues = vec![
            make_issue_with_milestone(1, 10),
            make_issue_with_milestone(2, 10),
            make_issue_with_milestone(3, 99),
        ];
        screen.set_issues(issues);

        assert_eq!(
            screen.filtered_indices.len(),
            3,
            "set_issues with no filter must show all issues"
        );
    }

    // ---- milestone text filter (typed input via 'm' key) ----

    #[test]
    fn milestone_filter_mode_typed_number_matches_by_milestone_not_title() {
        // All issues share the same title so a title-match bug would return all 3.
        let issues = vec![
            Issue {
                number: 1,
                title: "Same title".to_string(),
                body: String::new(),
                labels: vec![],
                state: "open".to_string(),
                html_url: String::new(),
                milestone: Some(42),
                assignees: vec![],
            },
            Issue {
                number: 2,
                title: "Same title".to_string(),
                body: String::new(),
                labels: vec![],
                state: "open".to_string(),
                html_url: String::new(),
                milestone: Some(42),
                assignees: vec![],
            },
            Issue {
                number: 3,
                title: "Same title".to_string(),
                body: String::new(),
                labels: vec![],
                state: "open".to_string(),
                html_url: String::new(),
                milestone: None,
                assignees: vec![],
            },
        ];
        let mut screen = IssueBrowserScreen::new(issues);
        screen.handle_input(&key_event(KeyCode::Char('m')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('4')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('2')), InputMode::Normal);
        assert_eq!(
            screen.filtered_indices.len(),
            2,
            "Milestone filter must match by milestone number, not title"
        );
    }

    #[test]
    fn milestone_filter_mode_invalid_text_shows_no_results() {
        let issues = vec![
            make_issue_with_milestone(1, 5),
            make_issue_with_milestone(2, 5),
        ];
        let mut screen = IssueBrowserScreen::new(issues);
        screen.handle_input(&key_event(KeyCode::Char('m')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('a')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('b')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('c')), InputMode::Normal);
        assert_eq!(
            screen.filtered_indices.len(),
            0,
            "Non-numeric milestone filter text must match no issues"
        );
    }

    #[test]
    fn milestone_filter_mode_esc_clears_filter_and_restores_all_issues() {
        let issues = vec![
            make_issue_with_milestone(1, 7),
            make_issue_with_milestone(2, 7),
            make_issue_with_milestone(3, 99),
        ];
        let mut screen = IssueBrowserScreen::new(issues);
        screen.handle_input(&key_event(KeyCode::Char('m')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('7')), InputMode::Normal);
        assert_eq!(screen.filtered_indices.len(), 2);
        screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(
            screen.filtered_indices.len(),
            3,
            "Esc must restore all issues"
        );
        assert_eq!(screen.filter_mode, FilterMode::None);
        assert!(screen.filter_text.is_empty());
    }

    #[test]
    fn label_filter_mode_still_matches_by_title_regression() {
        let issues = vec![
            make_issue(1, "Add feature"),
            make_issue(2, "Fix bug"),
            make_issue(3, "Add config"),
        ];
        let mut screen = IssueBrowserScreen::new(issues);
        screen.handle_input(&key_event(KeyCode::Char('/')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('A')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
        assert_eq!(
            screen.filtered_indices.len(),
            2,
            "Label filter must still match by title substring"
        );
    }

    // ---- set_issues ----

    #[test]
    fn issue_browser_set_issues_replaces_and_resets() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected, 2);

        screen.loading = true;
        let new_issues = vec![make_issue(10, "New issue"), make_issue(11, "Another")];
        screen.set_issues(new_issues);

        assert_eq!(screen.issues.len(), 2);
        assert_eq!(screen.filtered_indices.len(), 2);
        assert_eq!(screen.selected, 0);
        assert!(!screen.loading);
    }

    #[test]
    fn issue_browser_set_issues_with_empty_list() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.set_issues(vec![]);
        assert!(screen.issues.is_empty());
        assert!(screen.filtered_indices.is_empty());
        assert_eq!(screen.selected, 0);
    }

    // ---- Issue #99: IssueBrowserScreen overlay integration ----

    #[test]
    fn issue_browser_overlay_cancel_dismisses_overlay() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.open_launch_overlay_for_selected();
        assert!(screen.launch_dialog.is_some());
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert!(screen.launch_dialog.is_none());
    }

    #[test]
    fn issue_browser_overlay_confirm_with_text_returns_launch_with_custom_prompt() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.open_launch_overlay_for_selected();
        for c in "focus on errors".chars() {
            screen.handle_input(&key_event(KeyCode::Char(c)), InputMode::Normal);
        }
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::LaunchSession(config) => {
                assert_eq!(config.issue_number, Some(1));
                assert_eq!(config.custom_prompt, Some("focus on errors".to_string()));
            }
            other => panic!("Expected LaunchSession, got {:?}", other),
        }
    }

    #[test]
    fn issue_browser_overlay_confirm_empty_returns_launch_custom_prompt_none() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.open_launch_overlay_for_selected();
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal); // confirm empty
        match action {
            ScreenAction::LaunchSession(config) => {
                assert_eq!(config.custom_prompt, None);
            }
            other => panic!("Expected LaunchSession, got {:?}", other),
        }
    }

    #[test]
    fn issue_browser_enter_with_multi_select_opens_overlay() {
        let mut screen = screen_with_two_selected();
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(
            action,
            ScreenAction::None,
            "multi-select Enter must open overlay, not dispatch LaunchSessions"
        );
        assert!(screen.launch_dialog.is_some());
    }

    // ---- Issue #130: multi-select prompt overlay ----

    #[test]
    fn multi_select_overlay_has_correct_selected_issues() {
        let mut screen = screen_with_two_selected();
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        let overlay = screen.launch_dialog.as_ref().unwrap();
        let mut numbers: Vec<u64> = overlay.selected_issues.iter().map(|(n, _)| *n).collect();
        numbers.sort();
        assert_eq!(numbers, vec![1, 3]);
        let titles: std::collections::HashMap<u64, &str> = overlay
            .selected_issues
            .iter()
            .map(|(n, t)| (*n, t.as_str()))
            .collect();
        assert_eq!(titles[&1], "Add login");
        assert_eq!(titles[&3], "Add logout");
    }

    #[test]
    fn multi_select_overlay_confirm_with_text_returns_launch_sessions_with_prompt() {
        let mut screen = screen_with_two_selected();
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        for c in "focus on auth".chars() {
            screen.handle_input(&key_event(KeyCode::Char(c)), InputMode::Normal);
        }
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::LaunchSessions(configs) => {
                assert_eq!(configs.len(), 2);
                for config in &configs {
                    assert_eq!(config.custom_prompt, Some("focus on auth".to_string()));
                }
            }
            other => panic!("Expected LaunchSessions, got {:?}", other),
        }
        assert!(screen.launch_dialog.is_none());
    }

    #[test]
    fn multi_select_overlay_confirm_empty_returns_launch_sessions_with_none() {
        let mut screen = screen_with_two_selected();
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::LaunchSessions(configs) => {
                assert_eq!(configs.len(), 2);
                for config in &configs {
                    assert_eq!(config.custom_prompt, None);
                }
            }
            other => panic!("Expected LaunchSessions, got {:?}", other),
        }
    }

    #[test]
    fn multi_select_overlay_cancel_preserves_selection() {
        let mut screen = screen_with_two_selected();
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert!(screen.launch_dialog.is_none());
        assert!(screen.selected_set.contains(&1));
        assert!(screen.selected_set.contains(&3));
    }

    #[test]
    fn single_issue_overlay_still_returns_launch_session() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.open_launch_overlay_for_selected();
        for c in "hint".chars() {
            screen.handle_input(&key_event(KeyCode::Char(c)), InputMode::Normal);
        }
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::LaunchSession(config) => {
                assert_eq!(config.issue_number, Some(1));
                assert_eq!(config.custom_prompt, Some("hint".to_string()));
            }
            other => panic!("Expected LaunchSession, got {:?}", other),
        }
    }

    #[test]
    fn multi_select_overlay_desired_mode_is_insert() {
        let mut screen = screen_with_two_selected();
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(screen.desired_input_mode(), Some(InputMode::Insert));
    }

    #[test]
    fn issue_browser_overlay_captures_input_before_list_navigation() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        let initial_selected = screen.selected;
        screen.open_launch_overlay_for_selected();
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal); // should type into overlay
        assert_eq!(
            screen.selected, initial_selected,
            "cursor must not move while overlay is open"
        );
        assert_eq!(
            screen.launch_dialog.as_ref().unwrap().editor_text(),
            "j",
            "char must be typed into overlay text"
        );
    }

    #[test]
    fn issue_browser_overlay_active_desired_mode_is_insert() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.open_launch_overlay_for_selected();
        assert_eq!(screen.desired_input_mode(), Some(InputMode::Insert));
    }

    // ---- focus pane and preview scroll (issue #289) ----

    #[test]
    fn focus_pane_default_is_list() {
        let screen = IssueBrowserScreen::new(make_three_issues());
        assert_eq!(screen.focus, FocusPane::List);
    }

    #[test]
    fn tab_switches_focus_from_list_to_preview() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        assert_eq!(screen.focus, FocusPane::Preview);
    }

    #[test]
    fn backtab_switches_focus_from_preview_to_list() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::BackTab), InputMode::Normal);
        assert_eq!(screen.focus, FocusPane::List);
    }

    #[test]
    fn esc_from_preview_focus_returns_to_list_and_returns_none() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert_eq!(screen.focus, FocusPane::List);
    }

    #[test]
    fn esc_from_list_focus_returns_pop() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn j_in_list_focus_advances_selected() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn j_in_preview_focus_increments_preview_scroll() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.preview_scroll, 1);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn down_in_preview_focus_increments_preview_scroll() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        assert_eq!(screen.preview_scroll, 1);
    }

    #[test]
    fn k_in_preview_focus_decrements_preview_scroll() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.preview_scroll, 1);
    }

    #[test]
    fn k_in_preview_focus_at_zero_stays_at_zero() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.preview_scroll, 0);
    }

    #[test]
    fn navigating_list_resets_preview_scroll() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.preview_scroll, 2);
        screen.handle_input(&key_event(KeyCode::BackTab), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.preview_scroll, 0);
    }

    #[test]
    fn set_issues_resets_preview_scroll_and_focus() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.preview_scroll, 2);
        assert_eq!(screen.focus, FocusPane::Preview);
        screen.set_issues(make_three_issues());
        assert_eq!(screen.preview_scroll, 0);
        assert_eq!(screen.focus, FocusPane::List);
    }

    // --- Issue #302: Unified PR toggle in overlay ---

    #[test]
    fn overlay_starts_with_unified_pr_false() {
        let mut screen = screen_with_two_selected();
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        let overlay = screen.launch_dialog.as_ref().unwrap();
        assert!(!overlay.unified_pr);
    }

    #[test]
    fn overlay_ctrl_u_toggles_unified_pr_with_multi_select() {
        let mut screen = screen_with_two_selected();
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        // Toggle on
        screen.handle_input(
            &key_event_with_modifiers(KeyCode::Char('u'), KeyModifiers::CONTROL),
            InputMode::Normal,
        );
        assert!(screen.launch_dialog.as_ref().unwrap().unified_pr);
        // Toggle off
        screen.handle_input(
            &key_event_with_modifiers(KeyCode::Char('u'), KeyModifiers::CONTROL),
            InputMode::Normal,
        );
        assert!(!screen.launch_dialog.as_ref().unwrap().unified_pr);
    }

    #[test]
    fn overlay_ctrl_u_no_toggle_with_single_issue() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        // Open overlay for single issue (no multi-select)
        screen.open_launch_overlay_for_selected();
        let overlay = screen.launch_dialog.as_ref().unwrap();
        assert_eq!(overlay.selected_issues.len(), 1);
        // Press Ctrl+U — should not toggle (only 1 issue)
        screen.handle_input(
            &key_event_with_modifiers(KeyCode::Char('u'), KeyModifiers::CONTROL),
            InputMode::Normal,
        );
        assert!(!screen.launch_dialog.as_ref().unwrap().unified_pr);
    }

    #[test]
    fn overlay_confirm_unified_returns_launch_unified_session() {
        let mut screen = screen_with_two_selected();
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        // Toggle unified on
        screen.handle_input(
            &key_event_with_modifiers(KeyCode::Char('u'), KeyModifiers::CONTROL),
            InputMode::Normal,
        );
        // Confirm
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::LaunchUnifiedSession(config) => {
                assert_eq!(config.issues.len(), 2);
                assert!(config.custom_prompt.is_none());
            }
            other => panic!("Expected LaunchUnifiedSession, got {:?}", other),
        }
    }

    #[test]
    fn overlay_confirm_non_unified_returns_launch_sessions() {
        let mut screen = screen_with_two_selected();
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        // Don't toggle — default is false
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::LaunchSessions(configs) => {
                assert_eq!(configs.len(), 2);
            }
            other => panic!("Expected LaunchSessions, got {:?}", other),
        }
    }

    // ---- #1262: the Mode radio replaces the two checkboxes ----

    fn launch_single_with(keys: &[KeyCode]) -> SessionConfig {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.open_launch_overlay_for_selected();
        for code in keys {
            screen.handle_input(&key_event(*code), InputMode::Insert);
        }
        match screen.handle_input(&key_event(KeyCode::Enter), InputMode::Insert) {
            ScreenAction::LaunchSession(config) => config,
            other => panic!("Expected LaunchSession, got {:?}", other),
        }
    }

    #[test]
    fn launch_default_mode_is_autonomous() {
        let config = launch_single_with(&[]);
        assert_eq!(config.issue_number, Some(1));
        assert!(config.produce_pr);
        assert!(!config.interaction);
        assert_eq!(config.custom_prompt, None);
    }

    #[test]
    fn launch_chat_until_pr_sets_both_flags() {
        let config = launch_single_with(&[KeyCode::Tab, KeyCode::Down]);
        assert!(config.produce_pr);
        assert!(config.interaction);
    }

    #[test]
    fn launch_chat_open_sets_interaction_only() {
        let config = launch_single_with(&[KeyCode::Tab, KeyCode::Up]);
        assert!(!config.produce_pr);
        assert!(config.interaction);
    }

    #[test]
    fn launch_carries_instructions_with_mode() {
        let mut keys: Vec<KeyCode> = "focus on auth".chars().map(KeyCode::Char).collect();
        keys.extend([KeyCode::Tab, KeyCode::Down]);
        let config = launch_single_with(&keys);
        assert_eq!(config.custom_prompt, Some("focus on auth".to_string()));
        assert!(config.interaction);
    }

    #[test]
    fn launch_defaults_pick_the_initial_mode() {
        let mut screen =
            IssueBrowserScreen::new(make_three_issues()).with_launch_defaults((true, true));
        screen.open_launch_overlay_for_selected();
        let dialog = screen.launch_dialog.as_ref().unwrap();
        assert_eq!(dialog.mode, LaunchMode::ChatUntilPr);
        assert_eq!(
            dialog.focus,
            crate::tui::widgets::launch_dialog::LaunchFocus::Instructions
        );
    }
}

#[cfg(test)]
mod multi_overlay_launch_options_tests {
    //! #919 / #1262: the chosen mode flows into every emitted config
    //! (per-session AND unified).

    use super::*;
    use crate::tui::screens::test_helpers::key_event;
    use crossterm::event::KeyCode;

    fn screen_with_multi_dialog(unified: bool, mode: LaunchMode) -> IssueBrowserScreen {
        let mut screen = IssueBrowserScreen::new(Vec::new());
        let mut dialog = LaunchDialog::new(vec![(1, "One".into()), (2, "Two".into())], mode);
        dialog.unified_pr = unified;
        screen.launch_dialog = Some(dialog);
        screen
    }

    #[test]
    fn unified_submit_carries_launch_mode() {
        let mut screen = screen_with_multi_dialog(true, LaunchMode::ChatOpen);
        let action = screen.handle_input(
            &key_event(KeyCode::Enter),
            crate::tui::navigation::InputMode::Insert,
        );
        match action {
            ScreenAction::LaunchUnifiedSession(cfg) => {
                assert!(!cfg.produce_pr);
                assert!(cfg.interaction);
                assert_eq!(cfg.issues.len(), 2);
            }
            other => panic!("expected LaunchUnifiedSession, got {other:?}"),
        }
    }

    #[test]
    fn per_session_multi_submit_carries_launch_mode() {
        let mut screen = screen_with_multi_dialog(false, LaunchMode::ChatOpen);
        let action = screen.handle_input(
            &key_event(KeyCode::Enter),
            crate::tui::navigation::InputMode::Insert,
        );
        match action {
            ScreenAction::LaunchSessions(configs) => {
                assert_eq!(configs.len(), 2);
                for config in configs {
                    assert!(!config.produce_pr, "all sessions inherit the choice");
                    assert!(config.interaction);
                }
            }
            other => panic!("expected LaunchSessions, got {other:?}"),
        }
    }
}
