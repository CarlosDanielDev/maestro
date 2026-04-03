use super::{ScreenAction, SessionConfig, draw_keybinds_bar, sanitize_for_terminal};
use crate::github::types::GhIssue;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    None,
    Label,
    Milestone,
}

pub struct IssueBrowserScreen {
    pub(crate) issues: Vec<GhIssue>,
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
}

impl IssueBrowserScreen {
    pub fn new(issues: Vec<GhIssue>) -> Self {
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
        }
    }

    pub fn handle_input(&mut self, event: &Event) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            // In filter mode, handle text input
            if self.filter_mode != FilterMode::None {
                return self.handle_filter_input(*code);
            }

            match code {
                KeyCode::Esc => return ScreenAction::Pop,
                KeyCode::Char('j') | KeyCode::Down => {
                    if !self.filtered_indices.is_empty()
                        && self.selected < self.filtered_indices.len() - 1
                    {
                        self.selected += 1;
                        self.sync_scroll();
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.selected = self.selected.saturating_sub(1);
                    self.sync_scroll();
                }
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
                KeyCode::Enter => {
                    return self.handle_enter();
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(6),    // issue list
                Constraint::Length(8), // preview pane
                Constraint::Length(1), // keybinds bar
            ])
            .split(area);

        self.draw_issue_list(f, chunks[0]);
        self.draw_preview(f, chunks[1]);
        draw_keybinds_bar(
            f,
            chunks[2],
            &[
                ("Enter", "Run"),
                ("Space", "Select"),
                ("/", "Filter"),
                ("Esc", "Back"),
            ],
        );
    }

    pub fn tick(&mut self) {
        // No-op; async data fetching would drain channel here
    }

    pub fn set_milestone_filter(&mut self, milestone: Option<u64>) {
        self.milestone_filter = milestone;
        self.reapply_filters();
    }

    fn sync_scroll(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + self.last_visible_height {
            self.scroll_offset = self.selected - self.last_visible_height + 1;
        }
    }

    fn handle_filter_input(&mut self, code: KeyCode) -> ScreenAction {
        match code {
            KeyCode::Esc => {
                self.filter_text.clear();
                self.filter_mode = FilterMode::None;
                self.reapply_filters();
            }
            KeyCode::Backspace => {
                self.filter_text.pop();
                self.reapply_filters();
            }
            KeyCode::Char(c) => {
                if self.filter_text.len() < 256 {
                    self.filter_text.push(c);
                    self.reapply_filters();
                }
            }
            KeyCode::Enter => {
                self.filter_mode = FilterMode::None;
            }
            _ => {}
        }
        ScreenAction::None
    }

    fn handle_enter(&self) -> ScreenAction {
        if self.filtered_indices.is_empty() {
            return ScreenAction::None;
        }

        // If multi-select is active, launch all selected
        if !self.selected_set.is_empty() {
            let configs: Vec<SessionConfig> = self
                .issues
                .iter()
                .filter(|i| self.selected_set.contains(&i.number))
                .map(|i| SessionConfig {
                    issue_number: Some(i.number),
                    title: i.title.clone(),
                })
                .collect();
            return ScreenAction::LaunchSessions(configs);
        }

        // Otherwise launch the single selected issue
        if let Some(&idx) = self.filtered_indices.get(self.selected) {
            let issue = &self.issues[idx];
            return ScreenAction::LaunchSession(SessionConfig {
                issue_number: Some(issue.number),
                title: issue.title.clone(),
            });
        }

        ScreenAction::None
    }

    fn reapply_filters(&mut self) {
        let filter_lower = self.filter_text.to_lowercase();

        self.filtered_indices = self
            .issues
            .iter()
            .enumerate()
            .filter(|(_, issue)| {
                // Milestone filter
                if let Some(ms) = self.milestone_filter
                    && issue.milestone != Some(ms)
                {
                    return false;
                }
                // Text filter (applies to title in label filter mode, or always if text exists)
                if !filter_lower.is_empty() {
                    let title_lower = issue.title.to_lowercase();
                    if !title_lower.contains(&filter_lower) {
                        return false;
                    }
                }
                true
            })
            .map(|(idx, _)| idx)
            .collect();

        // Clamp cursor
        if self.filtered_indices.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.filtered_indices.len() {
            self.selected = self.filtered_indices.len() - 1;
        }
    }

    fn draw_issue_list(&mut self, f: &mut Frame, area: Rect) {
        let title = if self.filter_mode != FilterMode::None {
            format!(" Issues — Filter: {} ", self.filter_text)
        } else {
            " Issues ".to_string()
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue));

        if self.loading {
            let para = Paragraph::new("  Loading...")
                .style(Style::default().fg(Color::Yellow))
                .block(block);
            f.render_widget(para, area);
            return;
        }

        if self.filtered_indices.is_empty() {
            let msg = if self.issues.is_empty() {
                "  No issues found"
            } else {
                "  No issues match the filter"
            };
            let para = Paragraph::new(msg)
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            f.render_widget(para, area);
            return;
        }

        let inner = block.inner(area);
        self.last_visible_height = inner.height as usize;
        let visible_height = inner.height as usize;

        let lines: Vec<Line> = self
            .filtered_indices
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(visible_height)
            .map(|(display_idx, &issue_idx)| {
                let issue = &self.issues[issue_idx];
                let is_selected = display_idx == self.selected;
                let is_multi = self.selected_set.contains(&issue.number);

                let marker = if is_multi { "◉" } else { " " };
                let cursor = if is_selected { "▸" } else { " " };

                let style = if is_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if is_multi {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::White)
                };

                Line::from(vec![
                    Span::styled(format!("{}{} ", cursor, marker), style),
                    Span::styled(format!("#{:<5} ", issue.number), style),
                    Span::styled(sanitize_for_terminal(&issue.title), style),
                ])
            })
            .collect();

        let para = Paragraph::new(lines).block(block);
        f.render_widget(para, area);
    }

    fn draw_preview(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Preview ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        if let Some(&idx) = self.filtered_indices.get(self.selected) {
            let issue = &self.issues[idx];
            let labels = issue.labels.join(", ");
            let lines = vec![
                Line::from(vec![
                    Span::styled("Title: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        sanitize_for_terminal(&issue.title),
                        Style::default().fg(Color::White),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("State: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&issue.state, Style::default().fg(Color::Green)),
                    Span::raw("  |  "),
                    Span::styled("Labels: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(labels, Style::default().fg(Color::Yellow)),
                ]),
                Line::raw(""),
                Line::from(Span::styled(
                    sanitize_for_terminal(
                        &issue.body.lines().take(3).collect::<Vec<_>>().join("\n"),
                    ),
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            let para = Paragraph::new(lines).block(block);
            f.render_widget(para, area);
        } else {
            let para = Paragraph::new("  Select an issue to preview")
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            f.render_widget(para, area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::key_event;
    use crossterm::event::KeyCode;

    fn make_issue(number: u64, title: &str) -> GhIssue {
        GhIssue {
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

    fn make_issue_with_milestone(number: u64, milestone_number: u64) -> GhIssue {
        GhIssue {
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

    fn make_three_issues() -> Vec<GhIssue> {
        vec![
            make_issue(1, "Add login"),
            make_issue(2, "Fix crash"),
            make_issue(3, "Add logout"),
        ]
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
    fn issue_browser_loading_flag_initially_false() {
        let screen = IssueBrowserScreen::new(make_three_issues());
        assert!(!screen.loading);
    }

    // ---- navigation ----

    #[test]
    fn issue_browser_key_j_advances_cursor() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('j')));
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn issue_browser_key_down_advances_cursor() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Down));
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn issue_browser_key_k_moves_cursor_up() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('j')));
        screen.handle_input(&key_event(KeyCode::Char('j')));
        screen.handle_input(&key_event(KeyCode::Char('k')));
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn issue_browser_key_up_moves_cursor_up() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Down));
        screen.handle_input(&key_event(KeyCode::Up));
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn issue_browser_cursor_does_not_underflow() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('k')));
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn issue_browser_cursor_does_not_overflow() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        for _ in 0..10 {
            screen.handle_input(&key_event(KeyCode::Char('j')));
        }
        assert_eq!(screen.selected, 2);
    }

    // ---- screen actions ----

    #[test]
    fn issue_browser_esc_returns_pop() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        let action = screen.handle_input(&key_event(KeyCode::Esc));
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn issue_browser_enter_on_single_issue_returns_launch_session() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('j'))); // move to issue 2 (number=2)
        let action = screen.handle_input(&key_event(KeyCode::Enter));
        match action {
            ScreenAction::LaunchSession(config) => {
                assert_eq!(config.issue_number, Some(2));
            }
            other => panic!("Expected LaunchSession, got {:?}", other),
        }
    }

    #[test]
    fn issue_browser_enter_with_multi_select_returns_launch_sessions() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char(' '))); // select issue #1
        screen.handle_input(&key_event(KeyCode::Char('j')));
        screen.handle_input(&key_event(KeyCode::Char('j')));
        screen.handle_input(&key_event(KeyCode::Char(' '))); // select issue #3
        let action = screen.handle_input(&key_event(KeyCode::Enter));
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
        let action = screen.handle_input(&key_event(KeyCode::Enter));
        assert_eq!(action, ScreenAction::None);
    }

    // ---- multi-select ----

    #[test]
    fn issue_browser_space_adds_issue_to_selected_set() {
        let issues = make_three_issues();
        let issue_number = issues[0].number;
        let mut screen = IssueBrowserScreen::new(issues);
        screen.handle_input(&key_event(KeyCode::Char(' ')));
        assert!(screen.selected_set.contains(&issue_number));
    }

    #[test]
    fn issue_browser_space_removes_issue_from_selected_set_if_already_selected() {
        let issues = make_three_issues();
        let issue_number = issues[0].number;
        let mut screen = IssueBrowserScreen::new(issues);
        screen.handle_input(&key_event(KeyCode::Char(' ')));
        screen.handle_input(&key_event(KeyCode::Char(' ')));
        assert!(!screen.selected_set.contains(&issue_number));
    }

    // ---- label filter ----

    #[test]
    fn issue_browser_slash_enters_filter_mode() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('/')));
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
        screen.handle_input(&key_event(KeyCode::Char('/')));
        screen.handle_input(&key_event(KeyCode::Char('A')));
        screen.handle_input(&key_event(KeyCode::Char('d')));
        screen.handle_input(&key_event(KeyCode::Char('d')));
        assert_eq!(screen.filtered_indices.len(), 2);
    }

    #[test]
    fn issue_browser_filter_text_is_case_insensitive() {
        let issues = vec![make_issue(1, "Implement Feature")];
        let mut screen = IssueBrowserScreen::new(issues);
        screen.handle_input(&key_event(KeyCode::Char('/')));
        screen.handle_input(&key_event(KeyCode::Char('i')));
        screen.handle_input(&key_event(KeyCode::Char('m')));
        screen.handle_input(&key_event(KeyCode::Char('p')));
        assert_eq!(screen.filtered_indices.len(), 1);
    }

    #[test]
    fn issue_browser_esc_in_filter_mode_clears_filter_and_exits() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('/')));
        screen.handle_input(&key_event(KeyCode::Char('F')));
        screen.handle_input(&key_event(KeyCode::Char('i')));
        screen.handle_input(&key_event(KeyCode::Esc));
        assert!(screen.filter_text.is_empty());
        assert_eq!(screen.filter_mode, FilterMode::None);
        assert_eq!(screen.filtered_indices.len(), 3);
    }

    #[test]
    fn issue_browser_backspace_in_filter_mode_deletes_last_char() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('/')));
        screen.handle_input(&key_event(KeyCode::Char('a')));
        screen.handle_input(&key_event(KeyCode::Char('b')));
        screen.handle_input(&key_event(KeyCode::Backspace));
        assert_eq!(screen.filter_text, "a");
    }

    #[test]
    fn issue_browser_filter_no_match_results_in_empty_filtered_indices() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('/')));
        for c in "zzznomatch".chars() {
            screen.handle_input(&key_event(KeyCode::Char(c)));
        }
        assert_eq!(screen.filtered_indices.len(), 0);
    }

    // ---- milestone filter ----

    #[test]
    fn issue_browser_key_m_enters_milestone_filter_mode() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('m')));
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
            screen.handle_input(&key_event(KeyCode::Char('j')));
        }
        assert_eq!(screen.selected, 4);
        screen.handle_input(&key_event(KeyCode::Char('/')));
        for c in "Alpha".chars() {
            screen.handle_input(&key_event(KeyCode::Char(c)));
        }
        assert!(screen.selected <= 1);
    }
}
