//! Rendering for the `AiReview` step. Owns the review-text view plus
//! the three improve sub-states (loading, error, diff) landed in #450.
//! Kept in its own file so `draw.rs` stays under the 400-LOC guardrail.

use super::IssueWizardScreen;
use super::draw_diff::build_diff_lines;
use crate::tui::theme::Theme;
use crate::tui::widgets::BrailleSpinner;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

impl IssueWizardScreen {
    pub(super) fn draw_ai_review(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        // Improve sub-state takes precedence over the default review
        // view — loading, error, and diff are exclusive with each other
        // and with the underlying review text.
        if self.improve_loading() {
            let block = theme.styled_block("AI Review - Improving", false);
            let inner = block.inner(area);
            f.render_widget(block, area);
            let lines = vec![
                Line::from(""),
                BrailleSpinner::render(
                    self.spinner_tick(),
                    "AI is rewriting your issue using its own feedback…",
                    self.use_nerd_font(),
                    theme,
                ),
            ];
            f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
            return;
        }

        if let Some(err) = self.improve_error() {
            let block = theme.styled_block("AI Review - Improve failed", false);
            let inner = block.inner(area);
            f.render_widget(block, area);
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "AI improve failed:",
                    Style::default()
                        .fg(theme.accent_error)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    err.to_string(),
                    Style::default().fg(theme.text_primary),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "r: retry    Esc: back to review",
                    Style::default().fg(theme.text_secondary),
                )),
            ];
            f.render_widget(
                Paragraph::new(lines)
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: false }),
                inner,
            );
            return;
        }

        if let Some(candidate) = self.improve_candidate() {
            let block = theme.styled_block(
                "AI Review - Proposed changes  (a: accept, d: discard, r: retry, j/k: scroll)",
                false,
            );
            let inner = block.inner(area);
            f.render_widget(block, area);
            let lines = build_diff_lines(self.payload(), candidate);
            let para = Paragraph::new(lines).scroll((self.diff_scroll(), 0));
            f.render_widget(para, inner);
            return;
        }

        let block = theme.styled_block(
            "AI Review  (r: revise, s: skip, i: improve with AI, Enter: continue, R: retry on error)",
            false,
        );
        let inner = block.inner(area);
        f.render_widget(block, area);

        if let Some(err) = self.review_error() {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "AI review failed:",
                    Style::default()
                        .fg(theme.accent_error)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    err.to_string(),
                    Style::default().fg(theme.text_primary),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press R to retry, s to skip, Esc to go back.",
                    Style::default().fg(theme.text_secondary),
                )),
            ];
            f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
            return;
        }

        if self.review_loading() {
            let lines = vec![
                Line::from(""),
                BrailleSpinner::render(
                    self.spinner_tick(),
                    "AI is reviewing your issue…",
                    self.use_nerd_font(),
                    theme,
                ),
            ];
            f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
            return;
        }

        let body: Vec<Line> = match self.review_text() {
            Some(text) => text
                .lines()
                .map(|line| {
                    Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(theme.text_primary),
                    ))
                })
                .collect(),
            None => vec![Line::from(Span::styled(
                "Press Enter to continue (no review run yet).",
                Style::default().fg(theme.text_secondary),
            ))],
        };
        f.render_widget(Paragraph::new(body), inner);
    }
}
