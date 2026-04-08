use super::{Screen, ScreenAction, SessionConfig, draw_keybinds_bar, sanitize_for_terminal};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crate::work::conflicts::{ConflictReport, IssueWithFiles, predict_conflicts};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// An entry in the confirmation list, combining queue position with display data.
#[derive(Debug, Clone)]
struct ConfirmationEntry {
    issue_number: u64,
    title: String,
    files_to_modify: Option<Vec<String>>,
}

pub struct QueueConfirmationScreen {
    entries: Vec<ConfirmationEntry>,
    conflict_report: ConflictReport,
    pub(crate) selected: usize,
    scroll_offset: usize,
}

impl QueueConfirmationScreen {
    /// Create a new queue confirmation screen from issue data and a conflict report.
    ///
    /// `issues` provides display data (title) and file scope for re-validation on removal.
    pub fn new(
        issues: Vec<IssueWithFiles>,
        titles: &std::collections::HashMap<u64, String>,
        conflict_report: ConflictReport,
    ) -> Self {
        let entries: Vec<ConfirmationEntry> = issues
            .into_iter()
            .map(|iwf| ConfirmationEntry {
                issue_number: iwf.issue_number,
                title: titles
                    .get(&iwf.issue_number)
                    .cloned()
                    .unwrap_or_else(|| format!("Issue #{}", iwf.issue_number)),
                files_to_modify: iwf.files_to_modify,
            })
            .collect();

        Self {
            entries,
            conflict_report,
            selected: 0,
            scroll_offset: 0,
        }
    }

    /// Returns the issue entries in queue order.
    pub fn items(&self) -> Vec<IssueWithFiles> {
        self.entries
            .iter()
            .map(|e| IssueWithFiles {
                issue_number: e.issue_number,
                files_to_modify: e.files_to_modify.clone(),
            })
            .collect()
    }

    /// Returns the current conflict report.
    pub fn conflict_report(&self) -> &ConflictReport {
        &self.conflict_report
    }

    /// Re-validate conflicts after removing an entry.
    fn revalidate_conflicts(&mut self) {
        let issues_with_files: Vec<IssueWithFiles> = self.items();
        self.conflict_report = predict_conflicts(&issues_with_files);
    }

    fn sync_scroll(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + visible_height {
            self.scroll_offset = self.selected - visible_height + 1;
        }
    }

    fn draw_issue_list(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_inactive))
            .title(Span::styled(
                format!(" Queue ({} issues) ", self.entries.len()),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        f.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        if self.entries.is_empty() {
            let empty = Paragraph::new(Line::from(Span::styled(
                "Queue is empty",
                Style::default().fg(theme.text_muted),
            )));
            f.render_widget(empty, inner);
            return;
        }

        let visible_height = inner.height as usize;
        let mut lines: Vec<Line> = Vec::new();

        for (i, entry) in self.entries.iter().enumerate().skip(self.scroll_offset) {
            if lines.len() >= visible_height {
                break;
            }
            let is_selected = i == self.selected;

            // Check if this issue is involved in any conflict
            let has_conflict = self
                .conflict_report
                .conflicts
                .iter()
                .any(|c| c.issue_numbers.contains(&entry.issue_number));
            let is_unknown = self
                .conflict_report
                .unknown_scope_issues
                .contains(&entry.issue_number);

            let badge = if has_conflict {
                Span::styled(" [WARN]", Style::default().fg(theme.accent_warning))
            } else if is_unknown {
                Span::styled(" [??]", Style::default().fg(theme.accent_warning))
            } else {
                Span::styled(" [OK]", Style::default().fg(theme.accent_success))
            };

            let title = sanitize_for_terminal(&entry.title);
            let num_label = format!(" {}. #{} ", i + 1, entry.issue_number);

            let style = if is_selected {
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(theme.text_primary)
            };

            lines.push(Line::from(vec![
                Span::styled(num_label, style),
                Span::styled(title, style),
                badge,
            ]));
        }

        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_conflict_panel(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let (border_color, title) = if self.conflict_report.is_safe {
            (theme.accent_success, " No Conflicts ")
        } else {
            (theme.accent_error, " Conflicts ")
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(border_color)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        f.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        if self.conflict_report.is_safe {
            let msg = Paragraph::new(Line::from(Span::styled(
                " No file conflicts detected",
                Style::default().fg(theme.accent_success),
            )));
            f.render_widget(msg, inner);
            return;
        }

        let mut lines: Vec<Line> = Vec::new();

        for conflict in &self.conflict_report.conflicts {
            let nums: Vec<String> = conflict
                .issue_numbers
                .iter()
                .map(|n| format!("#{}", n))
                .collect();
            lines.push(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(&conflict.file_path, Style::default().fg(theme.accent_error)),
                Span::styled(
                    format!(" — shared by {}", nums.join(", ")),
                    Style::default().fg(theme.text_secondary),
                ),
            ]));
        }

        if !self.conflict_report.unknown_scope_issues.is_empty() {
            let nums: Vec<String> = self
                .conflict_report
                .unknown_scope_issues
                .iter()
                .map(|n| format!("#{}", n))
                .collect();
            lines.push(Line::from(Span::styled(
                format!(" Unknown scope: {}", nums.join(", ")),
                Style::default().fg(theme.accent_warning),
            )));
        }

        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }
}

impl Screen for QueueConfirmationScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match code {
                KeyCode::Char('j') | KeyCode::Down => {
                    if !self.entries.is_empty() && self.selected < self.entries.len() - 1 {
                        self.selected += 1;
                        self.sync_scroll(10);
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.selected = self.selected.saturating_sub(1);
                    self.sync_scroll(10);
                }
                KeyCode::Char('d') | KeyCode::Delete => {
                    if !self.entries.is_empty() {
                        self.entries.remove(self.selected);
                        if self.selected >= self.entries.len() && self.selected > 0 {
                            self.selected -= 1;
                        }
                        self.revalidate_conflicts();
                        if self.entries.is_empty() {
                            return ScreenAction::Pop;
                        }
                    }
                }
                KeyCode::Enter => {
                    if !self.entries.is_empty() {
                        let configs: Vec<SessionConfig> = self
                            .entries
                            .iter()
                            .map(|e| SessionConfig {
                                issue_number: Some(e.issue_number),
                                title: e.title.clone(),
                                custom_prompt: None,
                            })
                            .collect();
                        return ScreenAction::LaunchSessions(configs);
                    }
                }
                KeyCode::Esc => return ScreenAction::Pop,
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let has_warnings = !self.conflict_report.is_safe;
        let conflict_height = if has_warnings { 8 } else { 3 };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(6),
                Constraint::Length(conflict_height),
                Constraint::Length(1),
            ])
            .split(area);

        self.draw_issue_list(f, chunks[0], theme);
        self.draw_conflict_panel(f, chunks[1], theme);
        draw_keybinds_bar(
            f,
            chunks[2],
            &[("Enter", "Confirm"), ("d", "Remove"), ("Esc", "Cancel")],
            theme,
        );
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        Some(InputMode::Normal)
    }
}

impl KeymapProvider for QueueConfirmationScreen {
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
                ],
            },
            KeyBindingGroup {
                title: "Actions",
                bindings: vec![
                    KeyBinding {
                        key: "Enter",
                        description: "Confirm and launch queue",
                    },
                    KeyBinding {
                        key: "d/Delete",
                        description: "Remove selected issue",
                    },
                    KeyBinding {
                        key: "Esc",
                        description: "Cancel and go back",
                    },
                ],
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::key_event;
    use crate::work::conflicts::{ConflictReport, FileConflict, IssueWithFiles};
    use std::collections::HashMap;

    fn safe_report() -> ConflictReport {
        ConflictReport {
            conflicts: vec![],
            unknown_scope_issues: vec![],
            is_safe: true,
        }
    }

    fn make_issues(numbers: &[u64]) -> (Vec<IssueWithFiles>, HashMap<u64, String>) {
        let issues: Vec<IssueWithFiles> = numbers
            .iter()
            .map(|&n| IssueWithFiles {
                issue_number: n,
                files_to_modify: Some(vec![format!("src/{}.rs", n)]),
            })
            .collect();
        let titles: HashMap<u64, String> = numbers
            .iter()
            .map(|&n| (n, format!("Issue #{}", n)))
            .collect();
        (issues, titles)
    }

    fn make_screen(numbers: &[u64]) -> QueueConfirmationScreen {
        let (issues, titles) = make_issues(numbers);
        QueueConfirmationScreen::new(issues, &titles, safe_report())
    }

    #[test]
    fn new_sets_cursor_to_zero() {
        let screen = make_screen(&[10, 20, 30]);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn j_moves_cursor_down() {
        let mut screen = make_screen(&[10, 20]);
        let action = screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected, 1);
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn j_does_not_overflow_past_last_item() {
        let mut screen = make_screen(&[10]);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn k_moves_cursor_up() {
        let mut screen = make_screen(&[10, 20]);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn k_saturates_at_zero() {
        let mut screen = make_screen(&[10, 20]);
        screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn esc_returns_pop() {
        let mut screen = make_screen(&[10]);
        let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn enter_launches_sessions() {
        let mut screen = make_screen(&[10, 20]);
        let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
        match action {
            ScreenAction::LaunchSessions(configs) => {
                assert_eq!(configs.len(), 2);
                assert_eq!(configs[0].issue_number, Some(10));
                assert_eq!(configs[1].issue_number, Some(20));
            }
            other => panic!("Expected LaunchSessions, got {:?}", other),
        }
    }

    #[test]
    fn d_removes_item_and_revalidates() {
        let (issues, titles) = make_issues(&[10, 20]);
        let report = ConflictReport {
            conflicts: vec![FileConflict {
                file_path: "src/10.rs".to_string(),
                issue_numbers: vec![10, 20],
            }],
            unknown_scope_issues: vec![],
            is_safe: false,
        };
        let mut screen = QueueConfirmationScreen::new(issues, &titles, report);
        assert!(!screen.conflict_report().is_safe);

        // Remove issue 10 (selected = 0)
        screen.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
        let items = screen.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].issue_number, 20);
        // After removing #10, no more conflict since only #20 remains
        assert!(screen.conflict_report().is_safe);
    }

    #[test]
    fn d_on_last_item_clamps_cursor() {
        let mut screen = make_screen(&[10, 20]);
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
        assert_eq!(screen.selected, 1);
        screen.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
        assert_eq!(screen.selected, 0);
        assert_eq!(screen.items().len(), 1);
    }

    #[test]
    fn d_on_all_items_pops() {
        let mut screen = make_screen(&[10]);
        let action = screen.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
        assert_eq!(action, ScreenAction::Pop);
    }

    #[test]
    fn conflict_report_reflects_unknown_scope() {
        let issues = vec![IssueWithFiles {
            issue_number: 5,
            files_to_modify: None,
        }];
        let titles = HashMap::from([(5, "Test".to_string())]);
        let report = ConflictReport {
            conflicts: vec![],
            unknown_scope_issues: vec![5],
            is_safe: false,
        };
        let screen = QueueConfirmationScreen::new(issues, &titles, report);
        assert!(!screen.conflict_report().is_safe);
        assert_eq!(screen.conflict_report().unknown_scope_issues, vec![5]);
    }
}
