use super::{Screen, ScreenAction, SessionConfig, draw_keybinds_bar, sanitize_for_terminal};
use crate::github::types::GhIssue;
use crate::tui::help::centered_rect;
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::collections::HashSet;

/// Action returned by the prompt overlay's input handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OverlayAction {
    /// No navigation change.
    None,
    /// User cancelled the overlay.
    Cancel,
    /// User confirmed — `None` means empty/whitespace-only prompt.
    Confirm(Option<String>),
}

/// Inline prompt overlay shown before launching an issue session.
#[derive(Debug, Clone)]
pub(crate) struct IssuePromptOverlay {
    pub text: String,
    pub issue_number: u64,
    pub issue_title: String,
}

impl IssuePromptOverlay {
    pub fn handle_input(&mut self, event: &Event) -> OverlayAction {
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match code {
                KeyCode::Esc => return OverlayAction::Cancel,
                KeyCode::Enter => {
                    if modifiers.contains(KeyModifiers::SHIFT) {
                        self.text.push('\n');
                        return OverlayAction::None;
                    }
                    // Enter or Ctrl+Enter confirms
                    let trimmed = self.text.trim().to_string();
                    return if trimmed.is_empty() {
                        OverlayAction::Confirm(None)
                    } else {
                        OverlayAction::Confirm(Some(trimmed))
                    };
                }
                KeyCode::Backspace => {
                    self.text.pop();
                }
                KeyCode::Char(c) => {
                    if self.text.len() < 2048 {
                        self.text.push(*c);
                    }
                }
                _ => {}
            }
        }
        OverlayAction::None
    }
}

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
    /// Prompt overlay shown before launching a single-issue session.
    pub(crate) prompt_overlay: Option<IssuePromptOverlay>,
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
            prompt_overlay: None,
        }
    }

    pub fn set_issues(&mut self, issues: Vec<GhIssue>) {
        self.issues = issues;
        self.selected = 0;
        self.scroll_offset = 0;
        self.loading = false;
        self.reapply_filters();
    }

    fn draw_impl(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(6),    // issue list
                Constraint::Length(8), // preview pane
                Constraint::Length(1), // keybinds bar
            ])
            .split(area);

        self.draw_issue_list(f, chunks[0], theme);
        self.draw_preview(f, chunks[1], theme);
        draw_keybinds_bar(
            f,
            chunks[2],
            &[
                ("Enter", "Run"),
                ("Space", "Select"),
                ("/", "Filter"),
                ("Esc", "Back"),
            ],
            theme,
        );
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

    fn handle_enter(&mut self) -> ScreenAction {
        if self.filtered_indices.is_empty() {
            return ScreenAction::None;
        }

        // If multi-select is active, launch all selected (skip overlay)
        if !self.selected_set.is_empty() {
            let configs: Vec<SessionConfig> = self
                .issues
                .iter()
                .filter(|i| self.selected_set.contains(&i.number))
                .map(|i| SessionConfig {
                    issue_number: Some(i.number),
                    title: i.title.clone(),
                    custom_prompt: None,
                })
                .collect();
            return ScreenAction::LaunchSessions(configs);
        }

        // For single issue, open the prompt overlay
        if let Some(&idx) = self.filtered_indices.get(self.selected) {
            let issue = &self.issues[idx];
            self.prompt_overlay = Some(IssuePromptOverlay {
                text: String::new(),
                issue_number: issue.number,
                issue_title: issue.title.clone(),
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

    fn draw_issue_list(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let title = if self.filter_mode != FilterMode::None {
            format!(" Issues — Filter: {} ", self.filter_text)
        } else {
            " Issues ".to_string()
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent_info));

        if self.loading {
            let para = Paragraph::new("  Loading...")
                .style(Style::default().fg(theme.accent_warning))
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
                .style(Style::default().fg(theme.text_secondary))
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
                        .fg(theme.branding_fg)
                        .bg(theme.accent_info)
                        .add_modifier(Modifier::BOLD)
                } else if is_multi {
                    Style::default().fg(theme.accent_success)
                } else {
                    Style::default().fg(theme.text_primary)
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

    fn draw_preview(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(" Preview ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_inactive));

        if let Some(&idx) = self.filtered_indices.get(self.selected) {
            let issue = &self.issues[idx];
            let labels = issue.labels.join(", ");
            let lines = vec![
                Line::from(vec![
                    Span::styled("Title: ", Style::default().fg(theme.text_secondary)),
                    Span::styled(
                        sanitize_for_terminal(&issue.title),
                        Style::default().fg(theme.text_primary),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("State: ", Style::default().fg(theme.text_secondary)),
                    Span::styled(&issue.state, Style::default().fg(theme.accent_success)),
                    Span::raw("  |  "),
                    Span::styled("Labels: ", Style::default().fg(theme.text_secondary)),
                    Span::styled(labels, Style::default().fg(theme.accent_warning)),
                ]),
                Line::raw(""),
                Line::from(Span::styled(
                    sanitize_for_terminal(
                        &issue.body.lines().take(3).collect::<Vec<_>>().join("\n"),
                    ),
                    Style::default().fg(theme.text_muted),
                )),
            ];
            let para = Paragraph::new(lines).block(block);
            f.render_widget(para, area);
        } else {
            let para = Paragraph::new("  Select an issue to preview")
                .style(Style::default().fg(theme.text_secondary))
                .block(block);
            f.render_widget(para, area);
        }
    }

    fn draw_prompt_overlay(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let overlay = match &self.prompt_overlay {
            Some(o) => o,
            None => return,
        };

        let overlay_area = centered_rect(65, 55, area);
        f.render_widget(Clear, overlay_area);

        let title = format!(" #{} — {} ", overlay.issue_number, overlay.issue_title);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent_info));

        let inner = block.inner(overlay_area);
        f.render_widget(block, overlay_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // hint
                Constraint::Min(3),    // text area
                Constraint::Length(1), // keybinds
            ])
            .split(inner);

        // Hint line
        let hint = Paragraph::new(Line::from(Span::styled(
            "Additional instructions (optional):",
            Style::default().fg(theme.text_secondary),
        )));
        f.render_widget(hint, chunks[0]);

        // Text area
        let text_content = if overlay.text.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                "Type your prompt here...",
                Style::default().fg(theme.text_muted),
            )))
        } else {
            Paragraph::new(sanitize_for_terminal(&overlay.text))
                .style(Style::default().fg(theme.text_primary))
                .wrap(Wrap { trim: false })
        };
        let text_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_inactive));
        f.render_widget(text_content.block(text_block), chunks[1]);

        // Keybinds bar
        draw_keybinds_bar(
            f,
            chunks[2],
            &[
                ("Enter", "Launch"),
                ("Shift+Enter", "New line"),
                ("Esc", "Cancel"),
            ],
            theme,
        );
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
                        description: "Move down",
                    },
                    KeyBinding {
                        key: "k/Up",
                        description: "Move up",
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
        // When overlay is active, route all input to it
        if let Some(ref mut overlay) = self.prompt_overlay {
            match overlay.handle_input(event) {
                OverlayAction::Cancel => {
                    self.prompt_overlay = None;
                    return ScreenAction::None;
                }
                OverlayAction::Confirm(custom_prompt) => {
                    let issue_number = overlay.issue_number;
                    let title = overlay.issue_title.clone();
                    self.prompt_overlay = None;
                    return ScreenAction::LaunchSession(SessionConfig {
                        issue_number: Some(issue_number),
                        title,
                        custom_prompt,
                    });
                }
                OverlayAction::None => return ScreenAction::None,
            }
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

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.draw_impl(f, area, theme);
        if self.prompt_overlay.is_some() {
            self.draw_prompt_overlay(f, area, theme);
        }
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        if self.prompt_overlay.is_some() || self.filter_mode != FilterMode::None {
            Some(InputMode::Insert)
        } else {
            Some(InputMode::Normal)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::{key_event, key_event_with_modifiers};
    use crossterm::event::{KeyCode, KeyModifiers};

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
    fn issue_browser_esc_returns_pop() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn issue_browser_enter_on_single_issue_opens_overlay() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal); // move to issue 2 (number=2)
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::None, "should not launch directly");
        let overlay = screen
            .prompt_overlay
            .as_ref()
            .expect("overlay should be open");
        assert_eq!(overlay.issue_number, 2);
        assert_eq!(overlay.issue_title, "Fix crash");
    }

    #[test]
    fn issue_browser_enter_with_multi_select_returns_launch_sessions() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal); // select issue #1
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal); // select issue #3
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

    // ---- Issue #99: IssuePromptOverlay state machine ----

    fn make_overlay(number: u64, title: &str) -> IssuePromptOverlay {
        IssuePromptOverlay {
            text: String::new(),
            issue_number: number,
            issue_title: title.to_string(),
        }
    }

    fn overlay_with_text(number: u64, title: &str, text: &str) -> IssuePromptOverlay {
        IssuePromptOverlay {
            text: text.to_string(),
            issue_number: number,
            issue_title: title.to_string(),
        }
    }

    #[test]
    fn overlay_initial_state_text_is_empty() {
        let overlay = make_overlay(42, "Fix crash");
        assert!(overlay.text.is_empty());
    }

    #[test]
    fn overlay_typing_appends_characters() {
        let mut overlay = make_overlay(42, "Fix crash");
        overlay.handle_input(&key_event(KeyCode::Char('a')));
        overlay.handle_input(&key_event(KeyCode::Char('b')));
        assert_eq!(overlay.text, "ab");
    }

    #[test]
    fn overlay_backspace_removes_last_character() {
        let mut overlay = overlay_with_text(42, "T", "hello");
        overlay.handle_input(&key_event(KeyCode::Backspace));
        assert_eq!(overlay.text, "hell");
    }

    #[test]
    fn overlay_backspace_on_empty_is_noop() {
        let mut overlay = make_overlay(42, "T");
        let action = overlay.handle_input(&key_event(KeyCode::Backspace));
        assert_eq!(action, OverlayAction::None);
        assert_eq!(overlay.text, "");
    }

    #[test]
    fn overlay_escape_returns_cancel() {
        let mut overlay = make_overlay(42, "T");
        let action = overlay.handle_input(&key_event(KeyCode::Esc));
        assert_eq!(action, OverlayAction::Cancel);
    }

    #[test]
    fn overlay_enter_with_text_returns_confirm_some() {
        let mut overlay = overlay_with_text(42, "T", "focus on error handling");
        let action = overlay.handle_input(&key_event(KeyCode::Enter));
        assert_eq!(
            action,
            OverlayAction::Confirm(Some("focus on error handling".to_string()))
        );
    }

    #[test]
    fn overlay_enter_with_empty_text_returns_confirm_none() {
        let mut overlay = make_overlay(42, "T");
        let action = overlay.handle_input(&key_event(KeyCode::Enter));
        assert_eq!(action, OverlayAction::Confirm(None));
    }

    #[test]
    fn overlay_enter_with_whitespace_only_returns_confirm_none() {
        let mut overlay = overlay_with_text(42, "T", "   \n  ");
        let action = overlay.handle_input(&key_event(KeyCode::Enter));
        assert_eq!(action, OverlayAction::Confirm(None));
    }

    #[test]
    fn overlay_shift_enter_inserts_newline() {
        let mut overlay = overlay_with_text(42, "T", "line one");
        let action = overlay.handle_input(&key_event_with_modifiers(
            KeyCode::Enter,
            KeyModifiers::SHIFT,
        ));
        assert_eq!(overlay.text, "line one\n");
        assert_eq!(action, OverlayAction::None);
    }

    #[test]
    fn overlay_ctrl_enter_also_confirms() {
        let mut overlay = overlay_with_text(42, "T", "hint");
        let action = overlay.handle_input(&key_event_with_modifiers(
            KeyCode::Enter,
            KeyModifiers::CONTROL,
        ));
        assert_eq!(action, OverlayAction::Confirm(Some("hint".to_string())));
    }

    #[test]
    fn overlay_stores_issue_number_and_title() {
        let overlay = make_overlay(99, "Custom feature");
        assert_eq!(overlay.issue_number, 99);
        assert_eq!(overlay.issue_title, "Custom feature");
    }

    #[test]
    fn overlay_confirm_text_is_trimmed() {
        let mut overlay = overlay_with_text(42, "T", "  trimmed  ");
        let action = overlay.handle_input(&key_event(KeyCode::Enter));
        assert_eq!(action, OverlayAction::Confirm(Some("trimmed".to_string())));
    }

    // ---- Issue #99: IssueBrowserScreen overlay integration ----

    #[test]
    fn issue_browser_overlay_cancel_dismisses_overlay() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal); // open overlay
        assert!(screen.prompt_overlay.is_some());
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert!(screen.prompt_overlay.is_none());
    }

    #[test]
    fn issue_browser_overlay_confirm_with_text_returns_launch_with_custom_prompt() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal); // open overlay
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
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal); // open overlay
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal); // confirm empty
        match action {
            ScreenAction::LaunchSession(config) => {
                assert_eq!(config.custom_prompt, None);
            }
            other => panic!("Expected LaunchSession, got {:?}", other),
        }
    }

    #[test]
    fn issue_browser_enter_with_multi_select_skips_overlay() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal); // select #1
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal); // select #2
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::LaunchSessions(_) => {}
            other => panic!("Expected LaunchSessions without overlay, got {:?}", other),
        }
        assert!(screen.prompt_overlay.is_none());
    }

    #[test]
    fn issue_browser_overlay_captures_input_before_list_navigation() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        let initial_selected = screen.selected;
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal); // open overlay
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal); // should type into overlay
        assert_eq!(
            screen.selected, initial_selected,
            "cursor must not move while overlay is open"
        );
        assert_eq!(
            screen.prompt_overlay.as_ref().unwrap().text,
            "j",
            "char must be typed into overlay text"
        );
    }

    #[test]
    fn issue_browser_overlay_active_desired_mode_is_insert() {
        let mut screen = IssueBrowserScreen::new(make_three_issues());
        screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal); // open overlay
        assert_eq!(screen.desired_input_mode(), Some(InputMode::Insert));
    }
}
