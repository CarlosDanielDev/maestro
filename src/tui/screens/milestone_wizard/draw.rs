use super::{MilestoneWizardScreen, MilestoneWizardStep};
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

const GOAL_QUESTIONS: &[&str] = &[
    "What is the main objective of this milestone?",
    "What problem does it solve?",
    "Who benefits when this ships?",
];

const NON_GOAL_PROMPT: &str =
    "What should this milestone explicitly NOT include? Listing non-goals up front prevents scope creep.";

impl MilestoneWizardScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, _theme: &Theme) {
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
            MilestoneWizardStep::GoalDefinition => self.draw_goal_step(f, chunks[1]),
            MilestoneWizardStep::NonGoals => self.draw_non_goals_step(f, chunks[1]),
            MilestoneWizardStep::DocReferences => self.draw_doc_refs_step(f, chunks[1]),
            MilestoneWizardStep::AiStructuring => self.draw_ai_structuring_step(f, chunks[1]),
            MilestoneWizardStep::Failed => self.draw_failed_step(f, chunks[1]),
            _ => self.draw_stub(f, chunks[1]),
        }
        self.draw_footer(f, chunks[2]);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let step = self.step();
        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("Step {}/{}: ", step.index(), MilestoneWizardStep::total()),
                Style::default().add_modifier(Modifier::DIM),
            ),
            Span::styled(step.label(), Style::default().add_modifier(Modifier::BOLD)),
        ]))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM).title("Milestone Wizard"));
        f.render_widget(header, area);
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect) {
        let line = if let Some(err) = self.validation_error() {
            Line::from(Span::styled(
                err,
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD),
            ))
        } else {
            Line::from(Span::styled(
                "Enter: next  Shift+Enter: newline  Esc: back",
                Style::default().add_modifier(Modifier::DIM),
            ))
        };
        f.render_widget(
            Paragraph::new(line).alignment(Alignment::Center),
            area,
        );
    }

    fn draw_goal_step(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(GOAL_QUESTIONS.len() as u16 + 2), Constraint::Min(3)])
            .split(area);
        self.draw_questions(f, chunks[0], "AI", GOAL_QUESTIONS);
        self.draw_field(f, chunks[1], "Your goals", &self.payload().goals, true);
    }

    fn draw_non_goals_step(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3)])
            .split(area);
        self.draw_questions(f, chunks[0], "AI", &[NON_GOAL_PROMPT]);
        self.draw_field(
            f,
            chunks[1],
            "Non-goals",
            &self.payload().non_goals,
            true,
        );
    }

    fn draw_doc_refs_step(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Doc references (one per line, file path or URL)");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines: Vec<Line> = Vec::new();
        for (i, r) in self.payload().doc_references.iter().enumerate() {
            let valid = self
                .payload()
                .doc_reference_valid
                .get(i)
                .copied()
                .unwrap_or(false);
            let marker = if valid { "✓" } else { "✗" };
            let style = if valid {
                Style::default().fg(Color::LightGreen)
            } else {
                Style::default().fg(Color::LightRed)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", marker), style),
                Span::raw(r.clone()),
            ]));
        }
        // Active editing buffer (the line being typed)
        lines.push(Line::from(vec![
            Span::styled("> ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(self.doc_buffer().to_string()),
            Span::styled(
                "▏",
                Style::default().add_modifier(Modifier::SLOW_BLINK),
            ),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Type a path/URL and press Enter to add.  Shift+Enter advances.",
            Style::default().add_modifier(Modifier::DIM),
        )));
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_ai_structuring_step(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("AI structuring");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let label = if self.is_planning_in_flight() {
            "AI is working on the plan…"
        } else if self.has_generated_plan() {
            "Plan ready — Enter to continue to Review."
        } else {
            "Press Enter to launch the AI planner."
        };
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                label,
                Style::default().add_modifier(Modifier::BOLD),
            )),
        ];
        f.render_widget(
            Paragraph::new(lines).alignment(Alignment::Center),
            inner,
        );
    }

    fn draw_failed_step(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Failed");
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                self.failure_reason().unwrap_or("Unknown error"),
                Style::default().fg(Color::LightRed),
            )),
            Line::from(""),
            Line::from("Press Esc to go back."),
        ];
        f.render_widget(
            Paragraph::new(lines).alignment(Alignment::Center),
            inner,
        );
    }

    fn draw_stub(&self, f: &mut Frame, area: Rect) {
        let body = Paragraph::new(vec![
            Line::from(""),
            Line::from(format!(
                "Stub for `{}` — wired in #297.",
                self.step().label()
            )),
            Line::from(""),
            Line::from("Press Enter to advance, Esc to go back."),
        ])
        .alignment(Alignment::Center);
        f.render_widget(body, area);
    }

    fn draw_questions(&self, f: &mut Frame, area: Rect, role: &str, questions: &[&str]) {
        let mut lines: Vec<Line> = Vec::new();
        for q in questions {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}] ", role),
                    Style::default()
                        .fg(Color::LightCyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(q.to_string()),
            ]));
        }
        f.render_widget(
            Paragraph::new(lines).block(Block::default().borders(Borders::BOTTOM)),
            area,
        );
    }

    fn draw_field(&self, f: &mut Frame, area: Rect, title: &str, content: &str, focused: bool) {
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
            .title(title.to_string());
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines: Vec<Line> = if content.is_empty() && !focused {
            vec![Line::from(Span::styled(
                "(empty)",
                Style::default().add_modifier(Modifier::DIM),
            ))]
        } else {
            content.split('\n').map(Line::from).collect()
        };
        if focused {
            if let Some(last) = lines.last_mut() {
                last.spans.push(Span::styled(
                    "▏",
                    Style::default().add_modifier(Modifier::SLOW_BLINK),
                ));
            }
        }
        f.render_widget(Paragraph::new(lines), inner);
    }
}
