//! #1282 Preview: the rendered body (`ds::md_block`, so `[TODO: …]` reads as
//! a reversed warn chip) above the create gate (`widgets::armed_bar`).

use super::IssueWizardScreen;
use crate::tui::ds::md_block::md_block;
use crate::tui::theme::Theme;
use crate::tui::widgets::armed_bar::{ARMED_BAR_HEIGHT, ArmedBar};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

impl IssueWizardScreen {
    pub(super) fn draw_preview(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let [body, gate] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(ARMED_BAR_HEIGHT)])
                .areas(area);
        let block = theme.styled_block("Preview", false);
        let inner = block.inner(body);
        f.render_widget(block, body);

        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(
                self.payload().title.clone(),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled("Labels: ", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    self.render_labels().join(", "),
                    Style::default().fg(theme.accent_info),
                ),
            ]),
        ];
        if let Some(m) = self.payload().milestone {
            lines.push(Line::from(vec![
                Span::styled("Milestone: ", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    format!("#{m}"),
                    Style::default().fg(theme.accent_identifier),
                ),
            ]));
        }
        lines.push(Line::from(""));
        lines.extend(md_block(&self.render_body_markdown(), theme, inner.width).lines);
        f.render_widget(Paragraph::new(lines), inner);

        let description = self.create_write_description();
        ArmedBar::new(self.create_gate(), &description, self.now()).render(f, gate, theme);
    }
}
