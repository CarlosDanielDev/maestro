//! Rendering for the `AiReview` step. Owns the review-text view plus
//! the three improve sub-states (loading, error, diff) landed in #450.
//! Kept in its own file so `draw.rs` stays under the 400-LOC guardrail.

use super::IssueWizardScreen;
use super::draw_diff::build_diff_lines;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

impl IssueWizardScreen {
    pub(super) fn draw_ai_review(&self, f: &mut Frame, area: Rect) {
        // Improve sub-state takes precedence over the default review
        // view — loading, error, and diff are exclusive with each other
        // and with the underlying review text.
        if self.improve_loading() {
            let block = Block::default()
                .borders(Borders::ALL)
                .title("AI Review · Improving…");
            let inner = block.inner(area);
            f.render_widget(block, area);
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "AI is rewriting your issue using its own feedback…",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
            ];
            f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
            return;
        }

        if let Some(err) = self.improve_error() {
            let block = Block::default()
                .borders(Borders::ALL)
                .title("AI Review · Improve failed");
            let inner = block.inner(area);
            f.render_widget(block, area);
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "AI improve failed:",
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(err.to_string()),
                Line::from(""),
                Line::from("r: retry    Esc: back to review"),
            ];
            f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
            return;
        }

        if let Some(candidate) = self.improve_candidate() {
            let block = Block::default().borders(Borders::ALL).title(
                "AI Review · Proposed changes  (a: accept, d: discard, r: retry, j/k: scroll)",
            );
            let inner = block.inner(area);
            f.render_widget(block, area);
            let lines = build_diff_lines(self.payload(), candidate);
            let para = Paragraph::new(lines).scroll((self.diff_scroll(), 0));
            f.render_widget(para, inner);
            return;
        }

        let block = Block::default().borders(Borders::ALL).title(
            "AI Review  (r: revise, s: skip, i: improve with AI, Enter: continue, R: retry on error)",
        );
        let inner = block.inner(area);
        f.render_widget(block, area);

        if let Some(err) = self.review_error() {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "AI review failed:",
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(err.to_string()),
                Line::from(""),
                Line::from("Press R to retry, s to skip, Esc to go back."),
            ];
            f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
            return;
        }

        if self.review_loading() {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "AI is reviewing your issue…",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
            ];
            f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
            return;
        }

        let body: Vec<Line> = match self.review_text() {
            Some(text) => text.lines().map(Line::from).collect(),
            None => vec![Line::from("Press Enter to continue (no review run yet).")],
        };
        f.render_widget(Paragraph::new(body), inner);
    }
}
