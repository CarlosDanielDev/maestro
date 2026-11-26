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
    layout::{Alignment, Constraint, Layout, Rect},
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
                    "r: retry (paid AI call)    Esc: back to review",
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
            let list = self.section_list_lines(candidate, theme);
            let list_h = u16::try_from(list.len() + 1).unwrap_or(u16::MAX);
            let [list_area, diff_area] =
                Layout::vertical([Constraint::Length(list_h), Constraint::Fill(1)]).areas(inner);
            f.render_widget(Paragraph::new(list), list_area);
            // The focused section, wrapped and scrolled with ↑/↓, so every
            // line of what `a` accepts can be read. A cut says so.
            let diff = section_diff_lines(self.payload(), candidate, self.section_focus());
            self.diff_view.set((diff_area.width, diff_area.height));
            let rows = super::improve_sections::wrapped_rows(&diff, diff_area.width)
                .saturating_sub(usize::from(self.diff_scroll()));
            let more = rows.saturating_sub(usize::from(diff_area.height));
            let mut para = Paragraph::new(diff)
                .wrap(Wrap { trim: false })
                .scroll((self.diff_scroll(), 0));
            if more > 0 || self.diff_scroll() > 0 {
                let hint = if more > 0 {
                    format!(" ↓ {more} more rows · ↑↓ scroll ")
                } else {
                    " ↑ scroll up ".to_string()
                };
                para = para.block(
                    ratatui::widgets::Block::default().title_bottom(
                        Line::from(Span::styled(
                            hint,
                            Style::default()
                                .fg(theme.accent_warning)
                                .add_modifier(Modifier::BOLD),
                        ))
                        .right_aligned(),
                    ),
                );
            }
            f.render_widget(para, diff_area);
            return;
        }

        // Keys are in the footer (`footer_hints`), not in the title.
        let block = theme.styled_block("AI review", false);
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
                    "r retries (a paid AI call) · s skips to Preview · Esc goes back · your draft is unchanged",
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
            // #1454: idle — say what the review does and what it costs;
            // nothing is sent until `r`.
            None => {
                let muted = Style::default().fg(theme.text_secondary);
                vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "Claude reads this draft and lists unclear acceptance criteria, missed edge cases, scope concerns and testability gaps.",
                        Style::default().fg(theme.text_primary),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            "r runs the review",
                            Style::default()
                                .fg(theme.text_primary)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" · a paid AI call (claude --print)", muted),
                    ]),
                    Line::from(Span::styled(
                        "Enter or s goes to Preview without it · nothing is sent until you press r",
                        muted,
                    )),
                ]
            }
        };
        f.render_widget(Paragraph::new(body).wrap(Wrap { trim: false }), inner);
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
