use super::{FieldId, IssueType, IssueWizardScreen, IssueWizardStep};
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

impl IssueWizardScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(area);

        self.draw_header(f, chunks[0]);
        match self.step() {
            IssueWizardStep::TypeSelect => self.draw_type_select(f, chunks[1], theme),
            IssueWizardStep::BasicInfo => self.draw_basic_info(f, chunks[1]),
            IssueWizardStep::DorFields => self.draw_dor_fields(f, chunks[1]),
            _ => self.draw_stub(f, chunks[1]),
        }
        self.draw_footer(f, chunks[2]);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let step = self.step();
        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("Step {}/{}: ", step.index(), IssueWizardStep::total()),
                Style::default().add_modifier(Modifier::DIM),
            ),
            Span::styled(step.label(), Style::default().add_modifier(Modifier::BOLD)),
        ]))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM).title("Issue Wizard"));
        f.render_widget(header, area);
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect) {
        let mut spans: Vec<Span> = Vec::new();
        if let Some(err) = self.validation_error() {
            spans.push(Span::styled(
                err,
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                "Enter: next  Tab: cycle  Shift+Enter: newline  Esc: back",
                Style::default().add_modifier(Modifier::DIM),
            ));
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
            area,
        );
    }

    fn draw_stub(&self, f: &mut Frame, area: Rect) {
        let body = Paragraph::new(vec![
            Line::from(""),
            Line::from(format!("Stub for step `{}`.", self.step().label())),
            Line::from(""),
            Line::from("Press Enter to advance, Esc to go back."),
        ])
        .alignment(Alignment::Center);
        f.render_widget(body, area);
    }

    fn draw_type_select(&self, f: &mut Frame, area: Rect, _theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(centered_rect(area, 60, 7));

        let pick = |label: &str, selected: bool| -> Paragraph<'static> {
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().add_modifier(Modifier::DIM)
            };
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(label.to_string(), style)),
                Line::from(""),
            ])
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL))
        };

        f.render_widget(
            pick("Feature", self.payload().issue_type == IssueType::Feature),
            chunks[0],
        );
        f.render_widget(
            pick("Bug", self.payload().issue_type == IssueType::Bug),
            chunks[1],
        );
    }

    fn draw_basic_info(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3)])
            .split(area);
        self.draw_field(f, chunks[0], FieldId::Title);
        self.draw_field(f, chunks[1], FieldId::Overview);
    }

    fn draw_dor_fields(&self, f: &mut Frame, area: Rect) {
        let fields = self.step_fields();
        if fields.is_empty() {
            return;
        }
        let constraints: Vec<Constraint> = fields
            .iter()
            .map(|f| {
                if f.is_multiline() {
                    Constraint::Min(3)
                } else {
                    Constraint::Length(3)
                }
            })
            .collect();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);
        for (i, field) in fields.iter().enumerate() {
            self.draw_field(f, chunks[i], *field);
        }
    }

    fn draw_field(&self, f: &mut Frame, area: Rect, field: FieldId) {
        let focused = self.focused_field() == Some(field);
        let border_style = if focused {
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::DIM)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(field.label());
        let inner = block.inner(area);
        f.render_widget(block, area);

        let content = self.field_value(field);
        let display: Vec<Line> = if content.is_empty() && !focused {
            vec![Line::from(Span::styled(
                "(empty)",
                Style::default().add_modifier(Modifier::DIM),
            ))]
        } else {
            let mut lines: Vec<Line> = content.split('\n').map(Line::from).collect();
            if focused {
                if let Some(last) = lines.last_mut() {
                    last.spans.push(Span::styled(
                        "▏",
                        Style::default().add_modifier(Modifier::SLOW_BLINK),
                    ));
                }
            }
            lines
        };
        f.render_widget(Paragraph::new(display), inner);
    }
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
