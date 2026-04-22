use super::{Screen, ScreenAction, SessionConfig, draw_keybinds_bar, sanitize_for_terminal};
use crate::provider::github::types::{GhIssue, GhMilestone};
use crate::tui::app::TuiMode;
use crate::tui::icons::{self, IconId};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::panels::compact_gauge_bar_counts;
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MilestoneEntry {
    pub number: u64,
    pub title: String,
    pub description: String,
    pub state: String,
    pub open_issues: u32,
    pub closed_issues: u32,
    pub issues: Vec<GhIssue>,
}

impl From<(GhMilestone, Vec<GhIssue>)> for MilestoneEntry {
    fn from((ms, issues): (GhMilestone, Vec<GhIssue>)) -> Self {
        Self {
            number: ms.number,
            title: ms.title,
            description: ms.description,
            state: ms.state,
            open_issues: ms.open_issues,
            closed_issues: ms.closed_issues,
            issues,
        }
    }
}

impl MilestoneEntry {
    pub fn progress_ratio(&self) -> f64 {
        let total = self.open_issues as f64 + self.closed_issues as f64;
        if total == 0.0 {
            return 0.0;
        }
        self.closed_issues as f64 / total
    }

    pub fn total_issues(&self) -> u32 {
        self.open_issues + self.closed_issues
    }
}

pub struct MilestoneScreen {
    pub(crate) milestones: Vec<MilestoneEntry>,
    pub(crate) selected: usize,
    scroll_offset: usize,
    pub(crate) loading: bool,
    /// Last known visible slots from draw, used for scroll sync.
    last_visible_slots: usize,
}

impl MilestoneScreen {
    pub fn new(milestones: Vec<MilestoneEntry>) -> Self {
        Self {
            milestones,
            selected: 0,
            scroll_offset: 0,
            loading: false,
            last_visible_slots: 6,
        }
    }

    fn draw_impl(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(6),    // milestone list
                Constraint::Length(8), // detail pane
                Constraint::Length(1), // keybinds
            ])
            .split(area);

        self.draw_milestone_list(f, chunks[0], theme);
        self.draw_detail(f, chunks[1], theme);
        draw_keybinds_bar(
            f,
            chunks[2],
            &[
                ("Enter", "View Issues"),
                ("r", "Run All Open"),
                ("Esc", "Back"),
            ],
            theme,
        );
    }

    #[allow(dead_code)]
    #[allow(clippy::needless_pass_by_ref_mut)] // Reason: &mut reserved for future tick-driven state mutations
    pub fn tick(&mut self) {}

    pub fn selected_milestone(&self) -> Option<&MilestoneEntry> {
        self.milestones.get(self.selected)
    }

    fn sync_scroll(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + self.last_visible_slots {
            self.scroll_offset = self.selected - self.last_visible_slots + 1;
        }
    }

    fn handle_run_all(&self) -> ScreenAction {
        if let Some(entry) = self.milestones.get(self.selected) {
            if entry.issues.is_empty() {
                return ScreenAction::None;
            }
            let configs: Vec<SessionConfig> = entry
                .issues
                .iter()
                .filter(|i| i.state == "open")
                .map(|i| SessionConfig {
                    issue_number: Some(i.number),
                    title: i.title.clone(),
                    custom_prompt: None,
                })
                .collect();
            if configs.is_empty() {
                return ScreenAction::None;
            }
            return ScreenAction::LaunchSessions(configs);
        }
        ScreenAction::None
    }

    fn draw_milestone_list(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let title = format!("{} Milestones", icons::get(IconId::Milestone));
        let block = theme
            .styled_block(&title, false)
            .border_style(Style::default().fg(theme.border_active));

        if self.loading {
            let para = Paragraph::new("  Loading...")
                .style(Style::default().fg(theme.accent_warning))
                .block(block);
            f.render_widget(para, area);
            return;
        }

        if self.milestones.is_empty() {
            let para = Paragraph::new("  No milestones found")
                .style(Style::default().fg(theme.text_secondary))
                .block(block);
            f.render_widget(para, area);
            return;
        }

        let inner = block.inner(area);
        f.render_widget(block, area);

        let visible_slots = (inner.height as usize) / 3;
        self.last_visible_slots = visible_slots.max(1);
        let milestones_to_show: Vec<(usize, &MilestoneEntry)> = self
            .milestones
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(visible_slots)
            .collect();

        for (display_idx, (idx, entry)) in milestones_to_show.iter().enumerate() {
            let y = inner.y + (display_idx * 3) as u16;
            if y + 2 >= inner.y + inner.height {
                break;
            }

            let is_selected = *idx == self.selected;
            let cursor = if is_selected {
                format!("{} ", icons::get(IconId::ChevronRight))
            } else {
                "  ".to_string()
            };

            let title_style = if is_selected {
                Style::default()
                    .fg(theme.selection_fg)
                    .bg(theme.selection_bg)
                    .add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK)
            } else {
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD)
            };

            let title_line = Line::from(vec![
                Span::styled(cursor, title_style),
                Span::styled(format!("{} ", icons::get(IconId::Milestone)), title_style),
                Span::styled(sanitize_for_terminal(&entry.title), title_style),
            ]);
            let title_area = Rect::new(inner.x, y, inner.width, 1);
            f.render_widget(Paragraph::new(title_line), title_area);

            let ratio = entry.progress_ratio();
            let pct = ratio * 100.0;
            let gauge_area = Rect::new(inner.x + 2, y + 1, inner.width.saturating_sub(4), 1);
            let bar_width = gauge_area.width.saturating_sub(20) as usize;
            let (filled, empty) = compact_gauge_bar_counts(pct, bar_width);
            let gauge_color = theme.milestone_gauge_color(pct);
            let gauge_line = Line::from(vec![
                Span::styled("[", Style::default().fg(gauge_color)),
                Span::styled(
                    icons::get(IconId::GaugeFilled).repeat(filled),
                    Style::default().fg(gauge_color),
                ),
                Span::styled(
                    icons::get(IconId::GaugeEmpty).repeat(empty),
                    Style::default().fg(theme.gauge_background),
                ),
                Span::styled(
                    format!(
                        "] {}/{} issues ({:.0}%)",
                        entry.closed_issues,
                        entry.total_issues(),
                        pct
                    ),
                    Style::default().fg(gauge_color),
                ),
            ]);
            f.render_widget(Paragraph::new(gauge_line), gauge_area);

            let status_line = Line::from(vec![
                Span::styled(
                    format!("  {} ", icons::get(IconId::IssueClosed)),
                    Style::default().fg(theme.accent_success),
                ),
                Span::styled(
                    entry.closed_issues.to_string(),
                    Style::default()
                        .fg(theme.accent_success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("    "),
                Span::styled(
                    format!("{} ", icons::get(IconId::IssueOpened)),
                    Style::default().fg(theme.accent_warning),
                ),
                Span::styled(
                    entry.open_issues.to_string(),
                    Style::default().fg(theme.accent_warning),
                ),
            ]);
            let status_area = Rect::new(inner.x, y + 2, inner.width, 1);
            f.render_widget(Paragraph::new(status_line), status_area);
        }
    }

    fn draw_detail(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Issues", false);

        if let Some(entry) = self.milestones.get(self.selected) {
            if entry.issues.is_empty() {
                let para = Paragraph::new(format!("  {} — no issues loaded", entry.title))
                    .style(Style::default().fg(theme.text_secondary))
                    .block(block);
                f.render_widget(para, area);
                return;
            }

            let lines: Vec<Line> = entry
                .issues
                .iter()
                .take(5)
                .map(|i| {
                    let (symbol, symbol_color) = if i.state == "closed" {
                        (icons::get(IconId::IssueClosed), theme.accent_success)
                    } else {
                        (icons::get(IconId::IssueOpened), theme.accent_warning)
                    };
                    Line::from(vec![
                        Span::styled(format!("  {} ", symbol), Style::default().fg(symbol_color)),
                        Span::styled(
                            format!("#{} ", i.number),
                            Style::default()
                                .fg(theme.accent_identifier)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            sanitize_for_terminal(&i.title),
                            Style::default().fg(theme.text_secondary),
                        ),
                    ])
                })
                .collect();

            let para = Paragraph::new(lines).block(block);
            f.render_widget(para, area);
        } else {
            let para = Paragraph::new("  Select a milestone")
                .style(Style::default().fg(theme.text_secondary))
                .block(block);
            f.render_widget(para, area);
        }
    }
}

impl KeymapProvider for MilestoneScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![KeyBindingGroup {
            title: "Milestones",
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
                    key: "Enter",
                    description: "View issues",
                },
                KeyBinding {
                    key: "r",
                    description: "Run all open issues",
                },
                KeyBinding {
                    key: "Esc",
                    description: "Back",
                },
            ],
        }]
    }
}

impl Screen for MilestoneScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match code {
                KeyCode::Esc => return ScreenAction::Pop,
                KeyCode::Char('j') | KeyCode::Down
                    if !self.milestones.is_empty() && self.selected < self.milestones.len() - 1 =>
                {
                    self.selected += 1;
                    self.sync_scroll();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.selected = self.selected.saturating_sub(1);
                    self.sync_scroll();
                }
                KeyCode::Enter => {
                    if self.milestones.is_empty() {
                        return ScreenAction::None;
                    }
                    return ScreenAction::Push(TuiMode::IssueBrowser);
                }
                KeyCode::Char('r') => {
                    return self.handle_run_all();
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
    use crossterm::event::KeyCode;

    fn make_issue(number: u64) -> GhIssue {
        GhIssue {
            number,
            title: format!("Issue #{}", number),
            body: String::new(),
            labels: vec![],
            state: "open".to_string(),
            html_url: format!("https://github.com/owner/repo/issues/{}", number),
            milestone: None,
            assignees: vec![],
        }
    }

    fn make_entry(number: u64, open: u32, closed: u32) -> MilestoneEntry {
        MilestoneEntry {
            number,
            title: format!("Milestone v{}", number),
            description: String::new(),
            state: "open".to_string(),
            open_issues: open,
            closed_issues: closed,
            issues: vec![],
        }
    }

    fn make_entry_with_issues(number: u64, issues: Vec<GhIssue>) -> MilestoneEntry {
        let open = issues.len() as u32;
        MilestoneEntry {
            number,
            title: format!("Milestone v{}", number),
            description: String::new(),
            state: "open".to_string(),
            open_issues: open,
            closed_issues: 0,
            issues,
        }
    }

    // ---- initial state ----

    #[test]
    fn milestone_screen_initial_selected_is_zero() {
        let screen = MilestoneScreen::new(vec![make_entry(1, 3, 7), make_entry(2, 1, 2)]);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn milestone_screen_loading_flag_initially_false() {
        let screen = MilestoneScreen::new(vec![make_entry(1, 0, 5)]);
        assert!(!screen.loading);
    }

    // ---- navigation ----

    #[test]
    fn milestone_screen_key_j_advances_cursor() {
        let mut screen = MilestoneScreen::new(vec![
            make_entry(1, 0, 0),
            make_entry(2, 0, 0),
            make_entry(3, 0, 0),
        ]);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn milestone_screen_key_down_advances_cursor() {
        let mut screen = MilestoneScreen::new(vec![make_entry(1, 0, 0), make_entry(2, 0, 0)]);
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn milestone_screen_key_k_moves_cursor_up() {
        let mut screen = MilestoneScreen::new(vec![
            make_entry(1, 0, 0),
            make_entry(2, 0, 0),
            make_entry(3, 0, 0),
        ]);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn milestone_screen_key_up_moves_cursor_up() {
        let mut screen = MilestoneScreen::new(vec![make_entry(1, 0, 0), make_entry(2, 0, 0)]);
        screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn milestone_screen_cursor_does_not_underflow() {
        let mut screen = MilestoneScreen::new(vec![make_entry(1, 0, 0), make_entry(2, 0, 0)]);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn milestone_screen_cursor_does_not_overflow() {
        let mut screen = MilestoneScreen::new(vec![
            make_entry(1, 0, 0),
            make_entry(2, 0, 0),
            make_entry(3, 0, 0),
        ]);
        for _ in 0..10 {
            screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        }
        assert_eq!(screen.selected, 2);
    }

    // ---- screen actions ----

    #[test]
    fn milestone_screen_esc_returns_pop() {
        let mut screen = MilestoneScreen::new(vec![make_entry(1, 0, 0)]);
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn milestone_screen_enter_returns_push_issue_browser_with_milestone_number() {
        let mut screen = MilestoneScreen::new(vec![make_entry(7, 3, 0), make_entry(12, 1, 5)]);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::Push(TuiMode::IssueBrowser) => {}
            other => panic!("Expected Push(IssueBrowser), got {:?}", other),
        }
    }

    #[test]
    fn milestone_screen_empty_list_enter_returns_none() {
        let mut screen = MilestoneScreen::new(vec![]);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn milestone_screen_key_r_on_milestone_returns_launch_sessions_for_all_open_issues() {
        let issues = vec![make_issue(10), make_issue(11)];
        let mut screen = MilestoneScreen::new(vec![make_entry_with_issues(1, issues)]);
        let action = screen.handle_input(&key_event(KeyCode::Char('r')), InputMode::Normal);
        match action {
            ScreenAction::LaunchSessions(configs) => {
                assert_eq!(configs.len(), 2);
            }
            other => panic!("Expected LaunchSessions, got {:?}", other),
        }
    }

    #[test]
    fn milestone_screen_key_r_on_empty_milestone_returns_none() {
        let mut screen = MilestoneScreen::new(vec![make_entry(1, 0, 5)]);
        let action = screen.handle_input(&key_event(KeyCode::Char('r')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
    }

    // ---- MilestoneEntry::progress_ratio ----

    #[test]
    fn milestone_entry_progress_ratio_computed_correctly() {
        let entry = make_entry(1, 3, 7);
        let ratio = entry.progress_ratio();
        assert!((ratio - 0.7_f64).abs() < f64::EPSILON * 10.0);
    }

    #[test]
    fn milestone_entry_progress_ratio_zero_when_all_open() {
        let entry = make_entry(1, 5, 0);
        assert_eq!(entry.progress_ratio(), 0.0);
    }

    #[test]
    fn milestone_entry_progress_ratio_one_when_all_closed() {
        let entry = make_entry(1, 0, 5);
        assert_eq!(entry.progress_ratio(), 1.0);
    }

    #[test]
    fn milestone_entry_progress_ratio_zero_when_no_issues() {
        let entry = make_entry(1, 0, 0);
        assert_eq!(entry.progress_ratio(), 0.0);
    }

    // ---- tick ----

    #[test]
    fn milestone_screen_tick_does_not_panic() {
        let mut screen = MilestoneScreen::new(vec![make_entry(1, 2, 3)]);
        screen.tick();
        screen.tick();
        screen.tick();
    }
}
