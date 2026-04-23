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
            IssueWizardStep::Dependencies => self.draw_dependencies(f, chunks[1]),
            IssueWizardStep::AiReview => self.draw_ai_review(f, chunks[1]),
            IssueWizardStep::Preview => self.draw_preview(f, chunks[1]),
            IssueWizardStep::Creating => self.draw_creating(f, chunks[1]),
            IssueWizardStep::Complete => self.draw_complete(f, chunks[1]),
            IssueWizardStep::Failed => self.draw_failed(f, chunks[1]),
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
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title("Issue Wizard"),
        );
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

    fn draw_preview(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Preview  (Enter to create on GitHub, Esc to revise)");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            self.payload().title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        let labels = self.render_labels();
        lines.push(Line::from(format!("Labels: {}", labels.join(", "))));
        if let Some(m) = self.payload().milestone {
            lines.push(Line::from(format!("Milestone: #{}", m)));
        }
        lines.push(Line::from(""));
        for raw in self
            .render_body_markdown()
            .lines()
            .take(inner.height.saturating_sub(4) as usize)
        {
            lines.push(Line::from(raw.to_string()));
        }
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_creating(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Creating");
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Creating issue on GitHub…",
                Style::default().add_modifier(Modifier::BOLD),
            )),
        ];
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }

    fn draw_complete(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Complete");
        let inner = block.inner(area);
        f.render_widget(block, area);
        let mut lines: Vec<Line> = Vec::new();
        if let Some(num) = self.created_issue_number() {
            lines.push(Line::from(Span::styled(
                format!("Issue #{} created successfully", num),
                Style::default()
                    .fg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from("Enter: create another  Esc: return to Landing"));
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }

    fn draw_failed(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Failed");
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                self.create_error().unwrap_or("Unknown error").to_string(),
                Style::default().fg(Color::LightRed),
            )),
            Line::from(""),
            Line::from("r: retry  Esc: back to Preview"),
        ];
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }

    fn draw_ai_review(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("AI Review  (r: revise, s: skip, Enter: continue, R: retry on error)");
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

    fn draw_dependencies(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Blocked By  (Space toggles, j/k navigates, Enter advances)");
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.dep_loading() {
            f.render_widget(
                Paragraph::new("  Loading open issues…")
                    .style(Style::default().add_modifier(Modifier::DIM)),
                inner,
            );
            return;
        }

        let Some(issues) = self.dep_issues() else {
            f.render_widget(
                Paragraph::new("  Press Enter to continue without dependencies.")
                    .style(Style::default().add_modifier(Modifier::DIM)),
                inner,
            );
            return;
        };

        if issues.is_empty() {
            f.render_widget(
                Paragraph::new("  No open issues found.")
                    .style(Style::default().add_modifier(Modifier::DIM)),
                inner,
            );
            return;
        }

        let lines: Vec<Line> = issues
            .iter()
            .enumerate()
            .take(inner.height as usize)
            .map(|(i, issue)| {
                let cursor = if i == self.dep_selected() { ">" } else { " " };
                let check = if self.dep_is_checked(issue.number) {
                    "[x]"
                } else {
                    "[ ]"
                };
                let labels = if issue.labels.is_empty() {
                    String::new()
                } else {
                    let names: Vec<&str> =
                        issue.labels.iter().map(|s| s.as_str()).take(3).collect();
                    format!("  ({})", names.join(", "))
                };
                Line::from(vec![
                    Span::raw(format!("{} ", cursor)),
                    Span::styled(check.to_string(), Style::default().fg(Color::LightGreen)),
                    Span::raw(format!(" #{} ", issue.number)),
                    Span::raw(issue.title.clone()),
                    Span::styled(labels, Style::default().add_modifier(Modifier::DIM)),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(lines), inner);
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
            if focused
                && let Some(last) = lines.last_mut()
            {
                last.spans.push(Span::styled(
                    "▏",
                    Style::default().add_modifier(Modifier::SLOW_BLINK),
                ));
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
