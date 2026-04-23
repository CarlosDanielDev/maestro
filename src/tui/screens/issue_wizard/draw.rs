use super::IssueWizardScreen;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

impl IssueWizardScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, _theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(area);

        let step = self.step();
        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("Step {}/{}: ", step.index(), super::IssueWizardStep::total()),
                Style::default().add_modifier(Modifier::DIM),
            ),
            Span::styled(step.label(), Style::default().add_modifier(Modifier::BOLD)),
        ]))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM).title("Issue Wizard"));
        f.render_widget(header, chunks[0]);

        let body = Paragraph::new(vec![
            Line::from(""),
            Line::from(format!("Stub for step `{}`.", step.label())),
            Line::from(""),
            Line::from("Press Enter to advance, Esc to go back."),
        ])
        .alignment(Alignment::Center);
        f.render_widget(body, chunks[1]);
    }
}
