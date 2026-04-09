use super::{FilterMode, IssueBrowserScreen, IssuePromptOverlay, sanitize_for_terminal};
use crate::tui::help::centered_rect;
use crate::tui::screens::{ScreenAction, draw_keybinds_bar};
use crate::tui::theme::Theme;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

impl IssueBrowserScreen {
    pub(super) fn draw_impl(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
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

    pub(super) fn sync_scroll(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + self.last_visible_height {
            self.scroll_offset = self.selected - self.last_visible_height + 1;
        }
    }

    pub(super) fn handle_filter_input(&mut self, code: KeyCode) -> ScreenAction {
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

    pub(super) fn handle_enter(&mut self) -> ScreenAction {
        if self.filtered_indices.is_empty() {
            return ScreenAction::None;
        }

        // If multi-select is active, open overlay with all selected issues
        if !self.selected_set.is_empty() {
            let selected_issues: Vec<(u64, String)> = self
                .issues
                .iter()
                .filter(|i| self.selected_set.contains(&i.number))
                .map(|i| (i.number, i.title.clone()))
                .collect();
            self.prompt_overlay = Some(IssuePromptOverlay {
                text: String::new(),
                selected_issues,
            });
            return ScreenAction::None;
        }

        // For single issue, open the prompt overlay
        if let Some(&idx) = self.filtered_indices.get(self.selected) {
            let issue = &self.issues[idx];
            self.prompt_overlay = Some(IssuePromptOverlay {
                text: String::new(),
                selected_issues: vec![(issue.number, issue.title.clone())],
            });
        }

        ScreenAction::None
    }

    pub(super) fn reapply_filters(&mut self) {
        let filter_lower = self.filter_text.to_lowercase();

        // When in milestone filter mode, parse typed text as milestone number
        let typed_milestone: Option<u64> =
            if self.filter_mode == FilterMode::Milestone && !self.filter_text.is_empty() {
                self.filter_text.trim().parse::<u64>().ok()
            } else {
                None
            };

        self.filtered_indices = self
            .issues
            .iter()
            .enumerate()
            .filter(|(_, issue)| {
                // Programmatic milestone filter (set via set_milestone_filter)
                if let Some(ms) = self.milestone_filter
                    && issue.milestone != Some(ms)
                {
                    return false;
                }
                // Typed milestone filter (user pressed 'm' and typed a number)
                if self.filter_mode == FilterMode::Milestone && !self.filter_text.is_empty() {
                    return match typed_milestone {
                        Some(ms) => issue.milestone == Some(ms),
                        None => false, // non-numeric text matches nothing
                    };
                }
                // Text filter (applies to title in label filter mode)
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

    pub(super) fn draw_prompt_overlay(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let overlay = match &self.prompt_overlay {
            Some(o) => o,
            None => return,
        };

        let is_multi = overlay.is_multi();
        let height_pct = if is_multi { 65 } else { 55 };
        let overlay_area = centered_rect(65, height_pct, area);
        f.render_widget(Clear, overlay_area);

        let title = if is_multi {
            format!(" {} issues selected ", overlay.selected_issues.len())
        } else {
            let (number, ref title) = overlay.selected_issues[0];
            format!(" #{} — {} ", number, title)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent_info));

        let inner = block.inner(overlay_area);
        f.render_widget(block, overlay_area);

        if is_multi {
            let issue_list_height = overlay.selected_issues.len().min(8) as u16 + 2;
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),                 // hint
                    Constraint::Length(issue_list_height), // issue list
                    Constraint::Min(3),                    // text area
                    Constraint::Length(1),                 // keybinds
                ])
                .split(inner);

            let hint = Paragraph::new(Line::from(Span::styled(
                "Shared prompt for all sessions (optional):",
                Style::default().fg(theme.text_secondary),
            )));
            f.render_widget(hint, chunks[0]);

            let issue_lines: Vec<Line> = overlay
                .selected_issues
                .iter()
                .take(8)
                .map(|(num, title)| {
                    Line::from(vec![
                        Span::styled(
                            format!("  #{:<5} ", num),
                            Style::default().fg(theme.accent_info),
                        ),
                        Span::styled(
                            sanitize_for_terminal(title),
                            Style::default().fg(theme.text_primary),
                        ),
                    ])
                })
                .collect();
            let issue_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_inactive));
            f.render_widget(Paragraph::new(issue_lines).block(issue_block), chunks[1]);

            Self::draw_overlay_text_area(f, chunks[2], overlay, theme);

            draw_keybinds_bar(
                f,
                chunks[3],
                &[
                    ("Enter", "Launch all"),
                    ("Shift+Enter", "New line"),
                    ("Esc", "Cancel"),
                ],
                theme,
            );
        } else {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // hint
                    Constraint::Min(3),    // text area
                    Constraint::Length(1), // keybinds
                ])
                .split(inner);

            let hint = Paragraph::new(Line::from(Span::styled(
                "Additional instructions (optional):",
                Style::default().fg(theme.text_secondary),
            )));
            f.render_widget(hint, chunks[0]);

            Self::draw_overlay_text_area(f, chunks[1], overlay, theme);

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

    fn draw_overlay_text_area(
        f: &mut Frame,
        area: Rect,
        overlay: &IssuePromptOverlay,
        theme: &Theme,
    ) {
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
        f.render_widget(text_content.block(text_block), area);
    }
}
