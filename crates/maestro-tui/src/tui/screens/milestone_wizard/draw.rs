use super::{MilestoneWizardScreen, MilestoneWizardStep, WizardPhase};
use crate::provider::types::ProviderKind;
use crate::tui::ds::armed::ArmedState;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::armed_bar::{ARMED_BAR_HEIGHT, ArmedBar};
use crate::tui::widgets::wizard_frame::{WizardStepHeader, field_block};
use crate::tui::widgets::{BrailleSpinner, WizardFrame};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
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
        // A planning failure stays on Plan; a write failure is Create's.
        let phase = if step == MilestoneWizardStep::Failed && !self.has_generated_plan() {
            WizardPhase::Plan
        } else {
            step.phase()
        };
        let current = phase.position();
        let errors = [current];
        let failed = step == MilestoneWizardStep::Failed;
        WizardFrame::draw_stepped(
            f,
            area,
            theme,
            WizardStepHeader {
                steps: &WizardPhase::LABELS,
                current,
                errors: if failed { &errors[..] } else { &[] },
                state: Some(Span::styled(
                    self.writes_ledger(),
                    Style::default().fg(theme.text_secondary),
                )),
            },
            None,
            |f, body_area| match step {
                MilestoneWizardStep::GoalDefinition => self.draw_goal_step(f, body_area, theme),
                MilestoneWizardStep::NonGoals => self.draw_non_goals_step(f, body_area, theme),
                MilestoneWizardStep::DocReferences => self.draw_doc_refs_step(f, body_area, theme),
                MilestoneWizardStep::AiStructuring => {
                    self.draw_ai_structuring_step(f, body_area, theme);
                }
                MilestoneWizardStep::ReviewPlan => self.draw_review_step(f, body_area, theme),
                MilestoneWizardStep::Preview => self.draw_preview_step(f, body_area, theme),
                MilestoneWizardStep::Materializing => {
                    self.draw_materializing_step(f, body_area, theme)
                }
                MilestoneWizardStep::Complete => self.draw_complete_step(f, body_area, theme),
                MilestoneWizardStep::Failed => self.draw_failed_step(f, body_area, theme),
            },
        );
    }

    fn draw_goal_step(&self, f: &mut Frame, area: Rect, theme: &Theme) {
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
        self.draw_questions(f, chunks[0], "AI", &questions, theme);
        // Block + cursor style were set by `refresh_field_blocks` on the
        // mutable draw entry point. TextArea::widget renders its own
        // content, cursor, and border.
        f.render_widget(self.goal_field.area(), chunks[1]);
    }

    fn draw_non_goals_step(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3)])
            .split(area);
        let prompt = format!(
            "What should this {}{}",
            self.milestone_label_lowercase(),
            NON_GOAL_PROMPT_SUFFIX
        );
        self.draw_questions(f, chunks[0], "AI", &[prompt], theme);
        f.render_widget(self.non_goals_field.area(), chunks[1]);
    }

    fn draw_doc_refs_step(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let outer = theme.styled_block("Doc references (one per line, file path or URL)", false);
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
                Style::default().fg(theme.accent_success)
            } else {
                Style::default().fg(theme.accent_error)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", marker), style),
                Span::styled(r.clone(), Style::default().fg(theme.text_primary)),
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
                Style::default()
                    .fg(theme.accent_identifier)
                    .add_modifier(Modifier::BOLD),
            )),
            buffer_row[0],
        );
        f.render_widget(self.doc_buffer_field.area(), buffer_row[1]);

        f.render_widget(
            Paragraph::new(Span::styled(
                "Type a path/URL and press Enter to add.  Shift+Enter advances.",
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::DIM),
            )),
            layout[2],
        );
    }

    fn draw_ai_structuring_step(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("AI structuring", false);
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
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD),
            ))
        } else {
            Line::from(Span::styled(
                "Press Enter to launch the AI planner.",
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ))
        };
        let lines = vec![Line::from(""), line];
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }

    fn draw_review_step(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = field_block("Proposed issues", true, self.validation_error(), theme);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let Some(plan) = self.generated_plan() else {
            f.render_widget(
                Paragraph::new(Span::styled(
                    "  No plan yet. Press Enter on AI Structuring to fetch one.",
                    Style::default().fg(theme.text_secondary),
                )),
                inner,
            );
            return;
        };
        let stranded = self.stranded_issues();
        let lines: Vec<Line> = plan
            .issues
            .iter()
            .enumerate()
            .map(|(i, issue)| {
                let selected = i == self.review_focus();
                let style = selection_style(theme, selected, theme.text_primary);
                let needs = stranded.iter().find(|(s, _)| *s == i).map(|(_, d)| d);
                let mark = if needs.is_some() {
                    Span::styled(
                        "[!]",
                        if selected {
                            style
                        } else {
                            style.fg(theme.accent_warning)
                        },
                    )
                } else if issue.accepted {
                    Span::styled(
                        "[a]",
                        if selected {
                            style
                        } else {
                            style.fg(theme.accent_success)
                        },
                    )
                } else {
                    Span::styled(
                        "[x]",
                        if selected {
                            style
                        } else {
                            style.fg(theme.accent_error)
                        },
                    )
                };
                let mut spans = vec![
                    Span::styled("  ", style),
                    mark,
                    Span::styled(format!(" {}", sanitize_for_terminal(&issue.title)), style),
                ];
                if let Some(deps) = needs {
                    spans.push(Span::styled(
                        format!("  needs deselected {}", blocker_titles(&plan.issues, deps)),
                        Style::default().fg(theme.accent_warning),
                    ));
                }
                Line::from(spans)
            })
            .collect();
        let count_accepted = plan.issues.iter().filter(|i| i.accepted).count();
        let count_rejected = plan.issues.len() - count_accepted;
        let mut all = lines;
        all.push(Line::from(""));
        all.push(Line::from(Span::styled(
            format!("{} accepted, {} rejected", count_accepted, count_rejected),
            Style::default().fg(theme.text_secondary),
        )));
        f.render_widget(Paragraph::new(all), inner);
    }

    fn draw_preview_step(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let [area, bar] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(ARMED_BAR_HEIGHT)])
                .areas(area);
        let place = match self.provider_kind {
            ProviderKind::AzureDevops => "Azure DevOps",
            _ => "GitHub",
        };
        let what = self.create_description();
        let exists = self.already_written_note();
        let mut armed_bar = ArmedBar::new(&self.create_arm, &what, self.now()).to(place);
        if let Some(note) = exists.as_deref() {
            armed_bar = armed_bar.already_written(note);
        }
        armed_bar.render(f, bar, theme);
        let block = theme.styled_block("Creation order", true);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let Some(plan) = self.generated_plan() else {
            f.render_widget(
                Paragraph::new(Span::styled(
                    "  No plan to preview.",
                    Style::default().fg(theme.text_secondary),
                )),
                inner,
            );
            return;
        };
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(
            plan.milestone_title.clone(),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            plan.milestone_description.clone(),
            Style::default().fg(theme.text_secondary),
        )));
        lines.push(Line::from(""));
        let levels = super::level_buckets(&plan.issues);
        for (level, indices) in levels.iter().enumerate() {
            if indices.is_empty() {
                continue;
            }
            lines.push(Line::from(Span::styled(
                format!("Level {}", level),
                Style::default()
                    .fg(theme.accent_identifier)
                    .add_modifier(Modifier::BOLD),
            )));
            for &idx in indices {
                let issue = &plan.issues[idx];
                let marker = if issue.accepted { "•" } else { "✗" };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {} ", marker),
                        Style::default().fg(if issue.accepted {
                            theme.accent_success
                        } else {
                            theme.accent_error
                        }),
                    ),
                    Span::styled(issue.title.clone(), Style::default().fg(theme.text_primary)),
                ]));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            super::sequence_line(&plan.issues),
            Style::default().fg(theme.accent_info),
        )));
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_materializing_step(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Creating", false);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let label = match self.materialize_progress() {
            Some((created, total)) => format!("Creating issue {}/{}…", created, total),
            None => "Submitting plan to provider…".to_string(),
        };
        f.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                BrailleSpinner::render(self.spinner_tick(), label, self.use_nerd_font(), theme),
                Line::from(""),
                Line::from(Span::styled(
                    "writing… wait: keys are off until the write returns",
                    Style::default().fg(theme.text_secondary),
                )),
            ])
            .alignment(Alignment::Center),
            inner,
        );
    }

    fn draw_complete_step(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Complete", false);
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
                        .fg(theme.accent_warning)
                        .add_modifier(Modifier::BOLD),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("{} #{} created", self.milestone_label(), num),
                    Style::default()
                        .fg(theme.accent_success)
                        .add_modifier(Modifier::BOLD),
                )));
            }
        }
        for n in self.created_issue_numbers() {
            lines.push(Line::from(vec![
                Span::styled("  • ", Style::default().fg(theme.text_secondary)),
                Span::styled(
                    format!("#{}", n),
                    Style::default().fg(theme.accent_identifier),
                ),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press Enter to return to Landing.",
            Style::default().fg(theme.text_secondary),
        )));
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_failed_step(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Failed", false);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                sanitize_for_terminal(self.failure_reason().unwrap_or("Unknown error")),
                Style::default().fg(theme.accent_error),
            )),
            Line::from(""),
        ];
        // A partial create: say what exists and what does not (#1284).
        if let Some(num) = self.created_milestone_number() {
            let nums = |ns: &[u64]| {
                if ns.is_empty() {
                    "none".to_string()
                } else {
                    ns.iter()
                        .map(|n| format!("#{n}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                }
            };
            lines.push(Line::from(Span::styled(
                format!(
                    "✓ {} #{num} {}",
                    self.milestone_label(),
                    if self.milestone_was_reused() {
                        "already existed"
                    } else {
                        "created"
                    }
                ),
                Style::default().fg(theme.accent_success),
            )));
            lines.push(Line::from(Span::styled(
                format!("✓ issues created: {}", nums(self.created_issue_numbers())),
                Style::default().fg(theme.accent_success),
            )));
            if !self.skipped_issue_numbers().is_empty() {
                lines.push(Line::from(Span::styled(
                    format!(
                        "✓ issues already there: {}",
                        nums(self.skipped_issue_numbers())
                    ),
                    Style::default().fg(theme.accent_success),
                )));
            }
            for title in self.not_created() {
                lines.push(Line::from(Span::styled(
                    format!("○ not created: {}", sanitize_for_terminal(title)),
                    Style::default().fg(theme.accent_warning),
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "r goes back to Create: the retry reuses these numbers and creates the rest.",
                Style::default().fg(theme.text_secondary),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "Press r to retry, Esc to go back.",
                Style::default().fg(theme.text_secondary),
            )));
        }
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }

    fn draw_questions(
        &self,
        f: &mut Frame,
        area: Rect,
        role: &str,
        questions: &[String],
        theme: &Theme,
    ) {
        let mut lines: Vec<Line> = Vec::new();
        for q in questions {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}] ", role),
                    Style::default()
                        .fg(theme.accent_info)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(q.to_string(), Style::default().fg(theme.text_primary)),
            ]));
        }
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block_plain(false)),
            area,
        );
    }
}

fn selection_style(theme: &Theme, selected: bool, default_fg: Color) -> Style {
    if selected {
        Style::default()
            .fg(theme.selection_fg)
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default().fg(default_fg)
    }
}

/// Blocker titles for a stranded row: `"feat: schema", "feat: api"`.
fn blocker_titles(issues: &[super::AiProposedIssue], deps: &[usize]) -> String {
    deps.iter()
        .map(|&d| match issues.get(d) {
            Some(dep) => format!("\"{}\"", sanitize_for_terminal(&dep.title)),
            None => format!("#{d} (not in the plan)"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Shell footer keys per step (`draw_stepped` has no footer row).
pub(super) fn footer_hints(s: &MilestoneWizardScreen) -> Vec<(&'static str, &'static str)> {
    match s.step() {
        MilestoneWizardStep::GoalDefinition => vec![
            ("Enter", "next"),
            ("Shift+Enter", "newline"),
            ("Esc", "back"),
        ],
        MilestoneWizardStep::NonGoals => vec![
            ("Enter", "next"),
            ("Shift+Enter", "newline"),
            ("Esc", "back"),
        ],
        MilestoneWizardStep::DocReferences => vec![
            ("Enter", "add reference"),
            ("Shift+Enter", "next"),
            ("Esc", "back"),
        ],
        MilestoneWizardStep::AiStructuring if !s.has_generated_plan() => {
            vec![("Enter", "run AI planner"), ("Esc", "back")]
        }
        MilestoneWizardStep::AiStructuring => vec![("Enter", "continue"), ("Esc", "back")],
        MilestoneWizardStep::ReviewPlan => vec![
            ("j k", "move"),
            ("a", "select"),
            ("x", "deselect"),
            ("Enter", "next"),
            ("Esc", "back"),
        ],
        MilestoneWizardStep::Preview => match s.create_arm_state() {
            ArmedState::Armed(_) => vec![("Enter", "create now"), ("Esc", "disarm")],
            _ => vec![("Enter", "arm create"), ("Esc", "back")],
        },
        MilestoneWizardStep::Materializing => Vec::new(),
        MilestoneWizardStep::Complete => vec![("Enter", "return")],
        MilestoneWizardStep::Failed => vec![("r", "retry"), ("Esc", "back")],
    }
}
