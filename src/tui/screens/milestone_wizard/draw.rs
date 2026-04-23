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

const NON_GOAL_PROMPT: &str = "What should this milestone explicitly NOT include? Listing non-goals up front prevents scope creep.";

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
            MilestoneWizardStep::ReviewPlan => self.draw_review_step(f, chunks[1]),
            MilestoneWizardStep::Preview => self.draw_preview_step(f, chunks[1]),
            MilestoneWizardStep::Materializing => self.draw_materializing_step(f, chunks[1]),
            MilestoneWizardStep::Complete => self.draw_complete_step(f, chunks[1]),
            MilestoneWizardStep::Failed => self.draw_failed_step(f, chunks[1]),
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
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title("Milestone Wizard"),
        );
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
        f.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
    }

    fn draw_goal_step(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(GOAL_QUESTIONS.len() as u16 + 2),
                Constraint::Min(3),
            ])
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
        self.draw_field(f, chunks[1], "Non-goals", &self.payload().non_goals, true);
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
            Span::styled("▏", Style::default().add_modifier(Modifier::SLOW_BLINK)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Type a path/URL and press Enter to add.  Shift+Enter advances.",
            Style::default().add_modifier(Modifier::DIM),
        )));
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_ai_structuring_step(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("AI structuring");
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
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }

    fn draw_review_step(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Review Plan  (j/k: navigate, a: accept, x: reject, Enter: continue)");
        let inner = block.inner(area);
        f.render_widget(block, area);
        let Some(plan) = self.generated_plan() else {
            f.render_widget(
                Paragraph::new("  No plan yet. Press Enter on AI Structuring to fetch one."),
                inner,
            );
            return;
        };
        let lines: Vec<Line> = plan
            .issues
            .iter()
            .enumerate()
            .map(|(i, issue)| {
                let cursor = if i == self.review_focus() { ">" } else { " " };
                let mark = if issue.accepted {
                    Span::styled("[a]", Style::default().fg(Color::LightGreen))
                } else {
                    Span::styled("[x]", Style::default().fg(Color::LightRed))
                };
                Line::from(vec![
                    Span::raw(format!("{} ", cursor)),
                    mark,
                    Span::raw(format!(" {}", issue.title)),
                ])
            })
            .collect();
        let count_accepted = plan.issues.iter().filter(|i| i.accepted).count();
        let count_rejected = plan.issues.len() - count_accepted;
        let mut all = lines;
        all.push(Line::from(""));
        all.push(Line::from(format!(
            "{} accepted, {} rejected",
            count_accepted, count_rejected
        )));
        f.render_widget(Paragraph::new(all), inner);
    }

    fn draw_preview_step(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Preview  (Enter to confirm and create on GitHub)");
        let inner = block.inner(area);
        f.render_widget(block, area);
        let Some(plan) = self.generated_plan() else {
            f.render_widget(Paragraph::new("  No plan to preview."), inner);
            return;
        };
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            plan.milestone_title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(plan.milestone_description.clone()));
        lines.push(Line::from(""));
        let levels = super::level_buckets(&plan.issues);
        for (level, indices) in levels.iter().enumerate() {
            if indices.is_empty() {
                continue;
            }
            lines.push(Line::from(Span::styled(
                format!("Level {}", level),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for &idx in indices {
                let issue = &plan.issues[idx];
                let marker = if issue.accepted { "•" } else { "✗" };
                lines.push(Line::from(format!("  {} {}", marker, issue.title)));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(super::sequence_line(&plan.issues)));
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_materializing_step(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Creating");
        let inner = block.inner(area);
        f.render_widget(block, area);
        let label = match self.materialize_progress() {
            Some((created, total)) => format!("Creating issue {}/{}…", created, total),
            None => "Submitting plan to GitHub…".to_string(),
        };
        f.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    label,
                    Style::default().add_modifier(Modifier::BOLD),
                )),
            ])
            .alignment(Alignment::Center),
            inner,
        );
    }

    fn draw_complete_step(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Complete");
        let inner = block.inner(area);
        f.render_widget(block, area);
        let mut lines: Vec<Line> = Vec::new();
        if let Some(num) = self.created_milestone_number() {
            lines.push(Line::from(Span::styled(
                format!("Milestone #{} created", num),
                Style::default()
                    .fg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        for n in self.created_issue_numbers() {
            lines.push(Line::from(format!("  • #{}", n)));
        }
        lines.push(Line::from(""));
        lines.push(Line::from("Press Enter to return to Landing."));
        f.render_widget(Paragraph::new(lines), inner);
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
            Line::from("Press r to retry, Esc to go back."),
        ];
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
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
        if focused
            && let Some(last) = lines.last_mut()
        {
            last.spans.push(Span::styled(
                "▏",
                Style::default().add_modifier(Modifier::SLOW_BLINK),
            ));
        }
        f.render_widget(Paragraph::new(lines), inner);
    }
}
