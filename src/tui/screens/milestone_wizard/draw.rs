use super::{MilestoneWizardScreen, MilestoneWizardStep};
use crate::tui::theme::Theme;
use crate::tui::widgets::{BrailleSpinner, WizardFrame, WizardFrameFooter, WizardFrameHeader};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

const GOAL_QUESTION_TAILS: &[&str] = &[
    "What problem does it solve?",
    "Who benefits when this ships?",
];

const NON_GOAL_PROMPT_SUFFIX: &str =
    " explicitly NOT include? Listing non-goals up front prevents scope creep.";

impl MilestoneWizardScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let step = self.step();
        WizardFrame::draw(
            f,
            area,
            theme,
            WizardFrameHeader {
                step_index: step.index(),
                step_total: MilestoneWizardStep::total(),
                step_label: step.label(),
            },
            WizardFrameFooter {
                validation_error: self.validation_error(),
            },
            |f, body_area| match step {
                MilestoneWizardStep::GoalDefinition => self.draw_goal_step(f, body_area),
                MilestoneWizardStep::NonGoals => self.draw_non_goals_step(f, body_area),
                MilestoneWizardStep::DocReferences => self.draw_doc_refs_step(f, body_area),
                MilestoneWizardStep::AiStructuring => {
                    self.draw_ai_structuring_step(f, body_area, theme);
                }
                MilestoneWizardStep::ReviewPlan => self.draw_review_step(f, body_area),
                MilestoneWizardStep::Preview => self.draw_preview_step(f, body_area),
                MilestoneWizardStep::Materializing => self.draw_materializing_step(f, body_area),
                MilestoneWizardStep::Complete => self.draw_complete_step(f, body_area),
                MilestoneWizardStep::Failed => self.draw_failed_step(f, body_area),
            },
        );
    }

    fn draw_goal_step(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(GOAL_QUESTION_TAILS.len() as u16 + 3),
                Constraint::Min(3),
            ])
            .split(area);
        let mut questions = Vec::with_capacity(GOAL_QUESTION_TAILS.len() + 1);
        questions.push(format!(
            "What is the main objective of this {}?",
            self.milestone_label_lowercase()
        ));
        questions.extend(GOAL_QUESTION_TAILS.iter().map(|q| (*q).to_string()));
        self.draw_questions(f, chunks[0], "AI", &questions);
        // Block + cursor style were set by `refresh_field_blocks` on the
        // mutable draw entry point. TextArea::widget renders its own
        // content, cursor, and border.
        f.render_widget(self.goal_field.area(), chunks[1]);
    }

    fn draw_non_goals_step(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3)])
            .split(area);
        let prompt = format!(
            "What should this {}{}",
            self.milestone_label_lowercase(),
            NON_GOAL_PROMPT_SUFFIX
        );
        self.draw_questions(f, chunks[0], "AI", &[prompt]);
        f.render_widget(self.non_goals_field.area(), chunks[1]);
    }

    fn draw_doc_refs_step(&self, f: &mut Frame, area: Rect) {
        let outer = Block::default()
            .borders(Borders::ALL)
            .title("Doc references (one per line, file path or URL)");
        let inner = outer.inner(area);
        f.render_widget(outer, area);

        // Top: committed references list. Middle: in-progress buffer
        // rendered by TextArea. Bottom: the help hint.
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // references list (grows)
                Constraint::Length(1), // doc_buffer_field (single line)
                Constraint::Length(2), // help hint
            ])
            .split(inner);

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
        f.render_widget(Paragraph::new(lines), layout[0]);

        // `> ` prefix + the single-line textarea side by side.
        let buffer_row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(2), Constraint::Min(1)])
            .split(layout[1]);
        f.render_widget(
            Paragraph::new(Span::styled(
                "> ",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            buffer_row[0],
        );
        f.render_widget(self.doc_buffer_field.area(), buffer_row[1]);

        f.render_widget(
            Paragraph::new(Span::styled(
                "Type a path/URL and press Enter to add.  Shift+Enter advances.",
                Style::default().add_modifier(Modifier::DIM),
            )),
            layout[2],
        );
    }

    fn draw_ai_structuring_step(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("AI structuring");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let line = if self.is_planning_in_flight() {
            BrailleSpinner::render(
                self.spinner_tick(),
                "AI is structuring your goals…",
                self.use_nerd_font(),
                theme,
            )
        } else if self.has_generated_plan() {
            Line::from(Span::styled(
                "Plan ready — Enter to continue to Review.",
                Style::default().add_modifier(Modifier::BOLD),
            ))
        } else {
            Line::from(Span::styled(
                "Press Enter to launch the AI planner.",
                Style::default().add_modifier(Modifier::BOLD),
            ))
        };
        let lines = vec![Line::from(""), line];
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
            .title("Preview  (Enter to confirm and create)");
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
            None => "Submitting plan to provider…".to_string(),
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
            let created_count = self.created_issue_numbers().len();
            let skipped_count = self.skipped_issue_numbers().len();
            if self.milestone_was_reused() {
                let mut summary = format!(
                    "Reused existing {} #{}; created {} issues",
                    self.milestone_label_lowercase(),
                    num,
                    created_count
                );
                if skipped_count > 0 {
                    let nums: Vec<String> = self
                        .skipped_issue_numbers()
                        .iter()
                        .map(|n| format!("#{}", n))
                        .collect();
                    summary.push_str(&format!(
                        ", skipped {} duplicate{} ({})",
                        skipped_count,
                        if skipped_count == 1 { "" } else { "s" },
                        nums.join(", ")
                    ));
                }
                lines.push(Line::from(Span::styled(
                    summary,
                    Style::default()
                        .fg(Color::LightYellow)
                        .add_modifier(Modifier::BOLD),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("{} #{} created", self.milestone_label(), num),
                    Style::default()
                        .fg(Color::LightGreen)
                        .add_modifier(Modifier::BOLD),
                )));
            }
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

    fn draw_questions(&self, f: &mut Frame, area: Rect, role: &str, questions: &[String]) {
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
}
