//! Rendering for the `AiReview` step. Owns the review-text view plus
//! the three improve sub-states (loading, error, diff) landed in #450.
//! Kept in its own file so `draw.rs` stays under the 400-LOC guardrail.

use super::IssueWizardScreen;
use super::draw_diff::section_diff_lines;
use super::{Decision, FieldId, IssueCreationPayload, lint};
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
            let block = theme.styled_block("AI review · proposed changes, per section", false);
            let inner = block.inner(area);
            f.render_widget(block, area);
            let mut lines: Vec<Line> = self.section_list_lines(candidate, theme);
            lines.push(Line::from(""));
            lines.extend(section_diff_lines(
                self.payload(),
                candidate,
                self.section_focus(),
            ));
            f.render_widget(Paragraph::new(lines), inner);
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

    /// One row per changed section: focus marker, decision and how many
    /// `[TODO:` the proposal has there, so placeholders show before `a`.
    fn section_list_lines(
        &self,
        candidate: &IssueCreationPayload,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        self.changed_sections()
            .into_iter()
            .map(|i| {
                let field = FieldId::ALL[i];
                let focused = i == self.section_focus();
                let (glyph, word, color) = match self.decisions()[i] {
                    Decision::Accepted => ("✓", "accepted", theme.accent_success),
                    Decision::KeptMine => ("✗", "kept mine", theme.text_secondary),
                    Decision::Pending => ("○", "undecided · keeps mine", theme.text_muted),
                };
                let label_style = if focused {
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_primary)
                };
                let mut spans = vec![
                    Span::raw(if focused { " ▸ " } else { "   " }),
                    Span::styled(format!("{glyph} "), Style::default().fg(color)),
                    Span::styled(field.label(), label_style),
                    Span::styled(format!("  {word}"), Style::default().fg(color)),
                ];
                let todos = lint::count(field.payload_ref(candidate));
                if todos > 0 {
                    spans.push(Span::styled(
                        format!("  ! {todos} [TODO:"),
                        Style::default()
                            .fg(theme.accent_warning)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                Line::from(spans)
            })
            .collect()
    }
}
