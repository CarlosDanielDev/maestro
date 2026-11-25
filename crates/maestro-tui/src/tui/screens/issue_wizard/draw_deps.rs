//! #1282 Blocked-by picker drawing: the `/` filter row, the selected chips
//! (always every checked issue, also the ones the filter hides) and the
//! filtered list with labels as chips.

use super::IssueWizardScreen;
use super::draw::selection_style;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::BrailleSpinner;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

impl IssueWizardScreen {
    pub(super) fn draw_dependencies(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Blocked by", false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.dep_loading() {
            f.render_widget(
                Paragraph::new(BrailleSpinner::render(
                    self.spinner_tick(),
                    "Loading open issues…",
                    self.use_nerd_font(),
                    theme,
                )),
                inner,
            );
            return;
        }

        let Some(issues) = self.dep_issues() else {
            f.render_widget(
                Paragraph::new(Span::styled(
                    "  Press Enter to continue without dependencies.",
                    Style::default().fg(theme.text_secondary),
                )),
                inner,
            );
            return;
        };

        let [filter_row, chips_row, list] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(inner);
        self.draw_dep_filter(f, filter_row, theme);
        f.render_widget(Paragraph::new(self.dep_chips_line(theme)), chips_row);

        let muted = Style::default().fg(theme.text_secondary);
        if issues.is_empty() {
            f.render_widget(
                Paragraph::new(Span::styled("  No open issues found.", muted)),
                list,
            );
            return;
        }
        let visible = self.dep_visible();
        if visible.is_empty() {
            let msg = format!(
                "  No open issue matches \"{}\" · Esc clears the filter, the selection stays",
                sanitize_for_terminal(&self.dep_filter_text())
            );
            f.render_widget(Paragraph::new(Span::styled(msg, muted)), list);
            return;
        }

        let lines: Vec<Line> = visible
            .iter()
            .filter_map(|&i| issues.get(i))
            .enumerate()
            .take(list.height as usize)
            .map(|(row, issue)| {
                let selected = row == self.dep_selected();
                let style = selection_style(theme, selected, theme.text_primary);
                let tint = |c| if selected { style } else { style.fg(c) };
                let check = if self.dep_is_checked(issue.number) {
                    "[x]"
                } else {
                    "[ ]"
                };
                let mut spans = vec![
                    Span::styled("  ", style),
                    Span::styled(check, tint(theme.accent_success)),
                    Span::styled(
                        format!(" #{} ", issue.number),
                        tint(theme.accent_identifier),
                    ),
                    Span::styled(sanitize_for_terminal(&issue.title), style),
                ];
                for label in &issue.labels {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        format!(" {} ", sanitize_for_terminal(label)),
                        Style::default()
                            .fg(theme.text_secondary)
                            .add_modifier(Modifier::REVERSED),
                    ));
                }
                Line::from(spans)
            })
            .collect();
        f.render_widget(Paragraph::new(lines), list);
    }

    /// `/ filter  text` — the live textarea while editing, else the hint.
    fn draw_dep_filter(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let label = Span::styled(" / filter  ", Style::default().fg(theme.text_muted));
        let label_w = label.width() as u16;
        f.render_widget(Paragraph::new(Line::from(label)), area);
        let rest = Rect {
            x: area.x + label_w,
            width: area.width.saturating_sub(label_w),
            ..area
        };
        if self.dep_filter_editing() || !self.dep_filter_text().is_empty() {
            f.render_widget(self.dep_filter.area(), rest);
        } else {
            f.render_widget(
                Paragraph::new(Span::styled(
                    "number, title or label",
                    Style::default().fg(theme.text_muted),
                )),
                rest,
            );
        }
    }

    /// `Selected 2  #12  #975`: every checked issue, also the ones the
    /// filter hides, so the chips say what `## Blocked By` will hold.
    pub(super) fn dep_chips_line(&self, theme: &Theme) -> Line<'static> {
        let checked = self.dep_checked_numbers();
        let muted = Style::default().fg(theme.text_muted);
        if checked.is_empty() {
            return Line::from(Span::styled(" Selected 0 · Space toggles a row", muted));
        }
        let mut spans = vec![Span::styled(
            format!(" Selected {}  ", checked.len()),
            muted,
        )];
        for n in checked {
            spans.push(Span::styled(
                format!(" #{n} "),
                Style::default()
                    .fg(theme.accent_identifier)
                    .add_modifier(Modifier::REVERSED | Modifier::BOLD),
            ));
            spans.push(Span::raw(" "));
        }
        Line::from(spans)
    }
}
