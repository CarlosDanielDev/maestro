use super::{FilterMode, FocusPane, IssueBrowserScreen, IssuePromptOverlay, sanitize_for_terminal};
use crate::tui::help::centered_rect;
use crate::tui::icons::{self, IconId};
use crate::tui::markdown::render_markdown;
use crate::tui::marquee::{MarqueeConfig, needs_scroll, visible_slice};
use crate::tui::screens::{ScreenAction, draw_keybinds_bar};
use crate::tui::theme::Theme;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
};

impl IssueBrowserScreen {
    pub(super) fn draw_impl(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        use crate::config::LayoutMode;

        let preview_pct = self.layout.preview_ratio.clamp(10, 90) as u16;

        // Split off keybinds bar at the bottom
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(1)])
            .split(area);

        match self.layout.mode {
            LayoutMode::Vertical => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(100 - preview_pct),
                        Constraint::Percentage(preview_pct),
                    ])
                    .split(outer[0]);
                self.draw_issue_list(f, chunks[0], theme);
                self.draw_preview(f, chunks[1], theme);
            }
            LayoutMode::Horizontal => {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(100 - preview_pct),
                        Constraint::Percentage(preview_pct),
                    ])
                    .split(outer[0]);
                self.draw_issue_list(f, chunks[0], theme);
                self.draw_preview(f, chunks[1], theme);
            }
        }

        draw_keybinds_bar(
            f,
            outer[1],
            &[
                ("Tab", "Switch pane"),
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
                unified_pr: false,
            });
            return ScreenAction::None;
        }

        // For single issue, open the prompt overlay
        if let Some(&idx) = self.filtered_indices.get(self.selected) {
            let issue = &self.issues[idx];
            self.prompt_overlay = Some(IssuePromptOverlay {
                text: String::new(),
                selected_issues: vec![(issue.number, issue.title.clone())],
                unified_pr: false,
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

        let is_list_focused = self.focus == FocusPane::List;
        let block = theme.styled_block(&title, is_list_focused);

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

        // Prefix width: cursor(1) + marker(1) + space(1) + '#'(1) + number(5) + space(1) = 10
        let prefix_width = 10usize;
        let title_max_width = (inner.width as usize).saturating_sub(prefix_width);
        let marquee_cfg = MarqueeConfig::default();

        // Collect row data, noting which is selected for marquee
        let rows: Vec<(usize, usize)> = self
            .filtered_indices
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(visible_height)
            .map(|(display_idx, &issue_idx)| (display_idx, issue_idx))
            .collect();

        // Advance marquee for the selected row if its title overflows
        if let Some(&(_, issue_idx)) = rows
            .iter()
            .find(|(display_idx, _)| *display_idx == self.selected)
        {
            let title = sanitize_for_terminal(&self.issues[issue_idx].title);
            let text_len = title.chars().count();
            let overflow = text_len.saturating_sub(title_max_width);
            self.marquee.advance(overflow, &marquee_cfg);
        }

        let lines: Vec<Line> = rows
            .iter()
            .map(|&(display_idx, issue_idx)| {
                let issue = &self.issues[issue_idx];
                let is_selected = display_idx == self.selected;
                let is_multi = self.selected_set.contains(&issue.number);

                let marker = if is_multi {
                    icons::get(IconId::CheckCircleFill)
                } else {
                    " "
                };
                let cursor = if is_selected {
                    icons::get(IconId::ChevronRight)
                } else {
                    " "
                };

                let style = if is_selected {
                    Style::default()
                        .fg(theme.selection_fg)
                        .bg(theme.selection_bg)
                        .add_modifier(Modifier::BOLD)
                } else if is_multi {
                    Style::default().fg(theme.accent_success)
                } else {
                    Style::default().fg(theme.text_primary)
                };

                let title = sanitize_for_terminal(&issue.title);
                let title_text =
                    if is_selected && needs_scroll(title.chars().count(), title_max_width) {
                        visible_slice(&title, self.marquee.offset, title_max_width)
                    } else if title.chars().count() > title_max_width && title_max_width > 3 {
                        let truncated: String = title.chars().take(title_max_width - 3).collect();
                        format!("{truncated}...")
                    } else {
                        title
                    };

                Line::from(vec![
                    Span::styled(format!("{}{} ", cursor, marker), style),
                    Span::styled(format!("#{:<5} ", issue.number), style),
                    Span::styled(title_text, style),
                ])
            })
            .collect();

        let para = Paragraph::new(lines).block(block);
        f.render_widget(para, area);
    }

    fn draw_preview(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let is_focused = self.focus == FocusPane::Preview;
        let block = theme.styled_block("Preview", is_focused);

        if let Some(&idx) = self.filtered_indices.get(self.selected) {
            let issue = &self.issues[idx];
            let inner = block.inner(area);
            f.render_widget(block, area);

            let header_height = 3u16;
            if inner.height <= header_height {
                return;
            }

            let labels = issue.labels.join(", ");
            let header_lines = vec![
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
                Line::from(vec![Span::styled(
                    "─".repeat(inner.width.saturating_sub(2) as usize),
                    Style::default().fg(theme.border_inactive),
                )]),
            ];

            let header_area = Rect::new(
                inner.x,
                inner.y,
                inner.width,
                header_height.min(inner.height),
            );
            f.render_widget(Paragraph::new(header_lines), header_area);

            let body_height = inner.height.saturating_sub(header_height);
            if body_height > 0 {
                let body_area = Rect::new(
                    inner.x + 1,
                    inner.y + header_height,
                    inner.width.saturating_sub(2),
                    body_height,
                );
                let body = sanitize_for_terminal(&issue.body);
                let rendered = render_markdown(&body, theme, body_area.width);
                let paragraph = Paragraph::new(rendered)
                    .scroll((self.preview_scroll, 0))
                    .wrap(Wrap { trim: false });
                f.render_widget(paragraph, body_area);
            }
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

        let block = theme
            .styled_block(&title, false)
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
                    Constraint::Length(1),                 // unified PR toggle
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
            let issue_block = theme.styled_block_plain(false);
            f.render_widget(Paragraph::new(issue_lines).block(issue_block), chunks[1]);

            crate::tui::widgets::unified_pr_toggle::draw_unified_pr_toggle(
                f,
                chunks[2],
                overlay.unified_pr,
                theme,
            );

            Self::draw_overlay_text_area(f, chunks[3], overlay, theme);

            draw_keybinds_bar(
                f,
                chunks[4],
                &[
                    ("Enter", "Launch all"),
                    ("Ctrl+U", "Unified PR"),
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
            let sanitized = sanitize_for_terminal(&overlay.text);
            let spans = crate::tui::issue_refs::highlight_issue_refs(
                &sanitized,
                theme.accent_identifier,
                theme.text_primary,
            );
            // Convert borrowed spans to owned for lifetime safety
            let owned_spans: Vec<Span<'static>> = spans
                .into_iter()
                .map(|s| Span::styled(s.content.to_string(), s.style))
                .collect();
            Paragraph::new(Line::from(owned_spans)).wrap(Wrap { trim: false })
        };
        let text_block = theme.styled_block_plain(false);
        f.render_widget(text_content.block(text_block), area);
    }
}
