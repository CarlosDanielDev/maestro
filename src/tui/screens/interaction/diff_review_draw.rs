//! Rendering for the diff reviewer overlay (#918) — split from
//! `diff_review.rs` to keep both under the 400-line guardrail.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use super::diff_review::{DiffLineKind, DiffReview};
use crate::tui::theme::Theme;

impl DiffReview {
    /// Render the overlay over `area` (the full Interaction screen).
    pub(crate) fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let overlay = centered(area, 96, 92);
        f.render_widget(Clear, overlay);
        let block = theme
            .styled_block(" Diff review (read-only) ", true)
            .border_style(Style::default().fg(theme.accent_info));
        let inner = block.inner(overlay);
        f.render_widget(block, overlay);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(inner);
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(28), Constraint::Percentage(72)])
            .split(rows[0]);

        self.draw_file_list(f, panes[0], theme);
        self.draw_diff_pane(f, panes[1], theme);
        self.draw_hint_bar(f, rows[1], theme);
    }

    fn draw_file_list(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block_plain(false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.files.is_empty() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " no changes vs base",
                    Style::default().fg(theme.text_secondary),
                ))),
                inner,
            );
            return;
        }

        // Keep the selected file visible in tall lists.
        let height = inner.height as usize;
        let first = self
            .selected
            .saturating_sub(height.saturating_sub(1) / 2)
            .min(self.files.len().saturating_sub(height));
        let lines: Vec<Line> = self
            .files
            .iter()
            .enumerate()
            .skip(first)
            .take(height)
            .map(|(i, file)| {
                let marker = if i == self.selected { ">" } else { " " };
                let style = if i == self.selected {
                    Style::default()
                        .fg(theme.accent_info)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_primary)
                };
                Line::from(vec![
                    Span::styled(format!("{marker} {}", file.path), style),
                    Span::styled(
                        format!(" +{}", file.adds),
                        Style::default().fg(theme.accent_success),
                    ),
                    Span::styled(
                        format!(" -{}", file.dels),
                        Style::default().fg(theme.accent_error),
                    ),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_diff_pane(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block_plain(false);
        let inner = block.inner(area);
        f.render_widget(block, area);
        self.viewport = inner.height as usize;
        self.clamp();

        let lines: Vec<Line> = self
            .current_lines()
            .iter()
            .skip(self.scroll)
            .take(inner.height as usize)
            .map(|l| {
                let style = match l.kind {
                    DiffLineKind::Add => Style::default().fg(theme.accent_success),
                    DiffLineKind::Del => Style::default().fg(theme.accent_error),
                    DiffLineKind::Hunk => Style::default()
                        .fg(theme.accent_info)
                        .add_modifier(Modifier::BOLD),
                    DiffLineKind::Header => Style::default()
                        .fg(theme.text_secondary)
                        .add_modifier(Modifier::BOLD),
                    DiffLineKind::Context => Style::default().fg(theme.text_primary),
                };
                Line::from(Span::styled(
                    crate::tui::screens::sanitize_for_terminal(&l.text),
                    style,
                ))
            })
            .collect();
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_hint_bar(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let text = match &self.query_input {
            Some(input) => format!(" /{input}_   (Enter search · Esc cancel)"),
            None => {
                " j/k scroll  Ctrl+d/u page  ]/[ hunk  g/G top/bot  Tab file  / search  n/N next  o shell  q close"
                    .to_string()
            }
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text,
                Style::default().fg(theme.text_secondary),
            ))),
            area,
        );
    }
}

fn centered(area: Rect, pct_w: u16, pct_h: u16) -> Rect {
    let w = area.width * pct_w / 100;
    let h = area.height * pct_h / 100;
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}
