//! Rendering for the Team Wizard. Top-level dispatcher routes by mode and
//! delegates to per-flow draw helpers.

use super::types::{LaunchInputKind, LaunchStep, ManageStep, PreflightSummary};
use super::{TeamLaunchInput, TeamWizardMode, TeamWizardScreen};
use crate::orchestration::types::Primitive;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::{WizardFrame, WizardFrameFooter, WizardFrameHeader};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

impl TeamWizardScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        match self.mode() {
            TeamWizardMode::Home => self.draw_overview(f, area, theme),
            TeamWizardMode::Compose => self.draw_compose(f, area, theme),
            TeamWizardMode::Launch => self.draw_launch(f, area, theme),
            TeamWizardMode::Manage => self.draw_manage(f, area, theme),
        }
    }

    pub(super) fn draw_terminal_state(
        &self,
        f: &mut Frame,
        area: Rect,
        theme: &Theme,
        title: &str,
        body: &str,
        color: ratatui::style::Color,
    ) {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                body.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press Enter to return.",
                Style::default().fg(theme.text_secondary),
            )),
        ];
        f.render_widget(
            Paragraph::new(lines)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
                .block(theme.styled_block(title, false)),
            area,
        );
    }

    // ── Launch ──────────────────────────────────────────────────────────

    fn draw_launch(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let step = self.launch_step();
        WizardFrame::draw(
            f,
            area,
            theme,
            WizardFrameHeader {
                step_index: step.index(),
                step_total: LaunchStep::total(),
                step_label: step.label(),
            },
            WizardFrameFooter {
                validation_error: self.validation_error(),
                hints: Some(launch_footer_hint(step)),
            },
            |f, body_area| match step {
                LaunchStep::TeamPicker => self.draw_launch_team_picker(f, body_area, theme),
                LaunchStep::InputPicker => self.draw_launch_input_picker(f, body_area, theme),
                LaunchStep::IssuePicker => self.draw_launch_issue_picker(f, body_area, theme),
                LaunchStep::PlanPreview => self.draw_launch_plan_preview(f, body_area, theme),
                LaunchStep::Confirm => self.draw_launch_confirm(f, body_area, theme),
                LaunchStep::Executing => {
                    if !self.draw_launch_waves(f, body_area, theme) {
                        self.draw_terminal_state(
                            f,
                            body_area,
                            theme,
                            "Executing",
                            "Dispatching team run…",
                            theme.accent_info,
                        );
                    }
                }
                LaunchStep::LaunchSuccess => self.draw_terminal_state(
                    f,
                    body_area,
                    theme,
                    "Launched",
                    "Team run dispatched.",
                    theme.accent_success,
                ),
                LaunchStep::LaunchFailed => self.draw_terminal_state(
                    f,
                    body_area,
                    theme,
                    "Launch Failed",
                    &sanitize_for_terminal(self.failure_reason().unwrap_or("unknown error")),
                    theme.accent_error,
                ),
            },
        );
    }

    fn draw_launch_team_picker(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut names: Vec<&str> = self.resolved_teams.keys().map(String::as_str).collect();
        names.sort();
        let mut lines = vec![Line::from(Span::styled(
            "Pick a team",
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ))];
        if names.is_empty() {
            lines.push(Line::from(Span::styled(
                "No teams loaded — check user/project preset directories.",
                Style::default().fg(theme.accent_warning),
            )));
        }
        for (i, name) in names.iter().enumerate() {
            let safe = sanitize_for_terminal(name);
            lines.push(focused_line(theme, &safe, i == self.launch.team_focus));
        }
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Team Picker", false)),
            area,
        );
    }

    fn draw_launch_input_picker(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let kinds = [
            ("Issue", LaunchInputKind::Issue),
            ("IssueSet", LaunchInputKind::IssueSet),
            ("Milestone", LaunchInputKind::Milestone),
            ("IdeaInbox", LaunchInputKind::IdeaInbox),
        ];
        let mut lines = vec![];
        if let Some(input) = self.initial_input() {
            lines.push(Line::from(Span::styled(
                preselect_label(input),
                Style::default()
                    .fg(theme.accent_info)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            "Choose input kind",
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )));
        for (i, (label, _)) in kinds.iter().enumerate() {
            lines.push(focused_line(theme, label, i == self.launch.input_focus));
        }
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Input", false)),
            area,
        );
    }

    fn draw_launch_issue_picker(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let buffer = sanitize_for_terminal(&self.launch.manual_issue_input);
        let mut lines = vec![
            Line::from(Span::styled(
                "Enter issue number — paste with Ctrl+V or pick from suggestions",
                Style::default().fg(theme.text_secondary),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("> {buffer}"),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )),
        ];

        let candidates = self.autocomplete_candidates();
        if !candidates.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Suggestions:",
                Style::default().fg(theme.text_secondary),
            )));
            for (i, n) in candidates.iter().enumerate() {
                let focused = self.launch.autocomplete_focus == Some(i);
                lines.push(focused_line(theme, &format!("#{n}"), focused));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "[Ctrl+V] paste   [↑/↓] suggest   [Tab] accept   [Enter] continue   [Esc] back",
            Style::default().fg(theme.text_secondary),
        )));
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Issue Number", false)),
            area,
        );
    }

    fn draw_launch_plan_preview(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Plan Preview", false);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let mut lines: Vec<Line> = Vec::new();

        if let Some(plan) = &self.launch.plan {
            lines.push(Line::from(Span::styled(
                format!(
                    "Team {} ({})  ≈ ${:.4} (rough estimate, ±50%)",
                    sanitize_for_terminal(&plan.team_name),
                    primitive_label(plan.primitive),
                    plan.estimated_cost_usd
                ),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )));
            if plan.original_count != plan.final_count {
                lines.push(Line::from(Span::styled(
                    format!(
                        "Auto-expanded: {} → {} (added {})",
                        plan.original_count,
                        plan.final_count,
                        plan.auto_added
                            .iter()
                            .map(|n| format!("#{n}"))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                    Style::default().fg(theme.accent_warning),
                )));
            }
            for (level, issues) in plan.levels.iter().enumerate() {
                let label = issues
                    .iter()
                    .map(|n| format!("#{n}"))
                    .collect::<Vec<_>>()
                    .join(" ∥ ");
                lines.push(Line::from(Span::styled(
                    format!("Level {level}: {label}"),
                    Style::default().fg(theme.accent_identifier),
                )));
            }
            lines.push(Line::from(Span::styled(
                format!("Concurrency: max {} parallel", plan.max_parallel),
                Style::default().fg(theme.text_secondary),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "No plan available.",
                Style::default().fg(theme.accent_warning),
            )));
        }
        lines.push(Line::from(""));
        if let Some(Err(summary)) = &self.launch.preflight {
            self.append_preflight_summary(&mut lines, summary, theme);
        }
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn append_preflight_summary(
        &self,
        lines: &mut Vec<Line<'static>>,
        summary: &PreflightSummary,
        theme: &Theme,
    ) {
        if !summary.blocking.is_empty() {
            lines.push(Line::from(Span::styled(
                "Pre-flight failures:".to_string(),
                Style::default()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            )));
            for block in &summary.blocking {
                let line_text = format!("✗ {}", sanitize_for_terminal(&block.render_line()));
                lines.push(Line::from(Span::styled(
                    line_text,
                    Style::default().fg(theme.accent_error),
                )));
            }
            lines.push(Line::from(Span::styled(
                "Confirm disabled until pre-flight passes.",
                Style::default().fg(theme.accent_error),
            )));
        }
        for w in &summary.warnings {
            lines.push(Line::from(Span::styled(
                format!("⚠ {w}"),
                Style::default().fg(theme.accent_warning),
            )));
        }
    }

    fn draw_launch_confirm(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines = vec![Line::from(Span::styled(
            "Ready to launch.",
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD),
        ))];
        if let Some(team) = &self.launch.selected_team {
            lines.push(Line::from(Span::styled(
                format!("Team: {}", sanitize_for_terminal(team)),
                Style::default().fg(theme.text_primary),
            )));
        }
        if let Some(plan) = &self.launch.plan {
            lines.push(Line::from(Span::styled(
                format!(
                    "{} issues across {} levels",
                    plan.final_count,
                    plan.levels.len()
                ),
                Style::default().fg(theme.text_secondary),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "[Enter] confirm   [Esc] back",
            Style::default().fg(theme.text_secondary),
        )));
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Confirm", false)),
            area,
        );
    }

    // ── Manage ──────────────────────────────────────────────────────────

    fn draw_manage(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let step = self.manage_step();
        WizardFrame::draw(
            f,
            area,
            theme,
            WizardFrameHeader {
                step_index: step.index(),
                step_total: ManageStep::total(),
                step_label: step.label(),
            },
            WizardFrameFooter {
                validation_error: self.validation_error(),
                hints: Some(manage_footer_hint(step)),
            },
            |f, body_area| match step {
                ManageStep::List => self.draw_manage_list(f, body_area, theme),
                ManageStep::DeleteConfirm => self.draw_manage_delete_confirm(f, body_area, theme),
                ManageStep::DeleteSuccess => self.draw_terminal_state(
                    f,
                    body_area,
                    theme,
                    "Deleted",
                    "Team preset removed.",
                    theme.accent_success,
                ),
                ManageStep::DeleteFailed => self.draw_terminal_state(
                    f,
                    body_area,
                    theme,
                    "Delete Failed",
                    &sanitize_for_terminal(
                        self.manage.last_error.as_deref().unwrap_or("unknown error"),
                    ),
                    theme.accent_error,
                ),
            },
        );
    }

    fn draw_manage_list(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let teams = self.manage_list_teams();
        let mut lines = vec![Line::from(Span::styled(
            "User-tier presets (built-ins / project-tier are read-only)",
            Style::default().fg(theme.text_secondary),
        ))];
        if teams.is_empty() {
            lines.push(Line::from(Span::styled(
                "No user presets — use Compose to create one.",
                Style::default().fg(theme.accent_warning),
            )));
        }
        for (i, t) in teams.iter().enumerate() {
            let label = format!(
                "{} ({})",
                sanitize_for_terminal(&t.name),
                primitive_label(t.primitive)
            );
            lines.push(focused_line(theme, &label, i == self.manage.selected_index));
        }
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Manage", false)),
            area,
        );
    }

    fn draw_manage_delete_confirm(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let target = self
            .manage
            .pending_delete
            .as_deref()
            .unwrap_or("(no target)");
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent_error))
            .title(Line::styled(
                " Confirm Delete ",
                Style::default()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("Delete preset “{}”?", sanitize_for_terminal(target)),
                Style::default()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "[y] yes   [n] cancel",
                Style::default().fg(theme.text_secondary),
            )),
        ];
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }
}

pub(super) fn focused_line(theme: &Theme, label: &str, selected: bool) -> Line<'static> {
    let prefix = if selected { ">" } else { " " };
    let style = if selected {
        Style::default()
            .fg(theme.selection_fg)
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_primary)
    };
    Line::from(Span::styled(format!(" {prefix} {label}"), style))
}

fn launch_footer_hint(step: LaunchStep) -> &'static str {
    match step {
        LaunchStep::TeamPicker => "[↑/↓ j/k] pick   [Enter] choose   [Esc] back",
        LaunchStep::InputPicker => "[↑/↓ j/k] pick   [Enter] continue   [Esc] back",
        LaunchStep::IssuePicker => {
            "[digits] type   [Ctrl+V] paste   [↑/↓] suggest   [Tab] accept   [Enter] continue   [Esc] back"
        }
        LaunchStep::PlanPreview => "[Enter] confirm   [Esc] back",
        LaunchStep::Confirm => "[Enter] launch   [Esc] back",
        LaunchStep::Executing => "Waves run in order   [Esc] cancel run",
        LaunchStep::LaunchSuccess => "[Enter] return",
        LaunchStep::LaunchFailed => "[r] retry   [Esc] back",
    }
}

fn manage_footer_hint(step: ManageStep) -> &'static str {
    match step {
        ManageStep::List => "[↑/↓ j/k] navigate   [e] edit   [d] delete   [Esc] back",
        ManageStep::DeleteConfirm => "[y] yes   [n] cancel",
        ManageStep::DeleteSuccess => "[Enter] return",
        ManageStep::DeleteFailed => "[r] retry   [Esc] back",
    }
}

pub fn primitive_label(p: Primitive) -> &'static str {
    p.label()
}

fn preselect_label(input: &TeamLaunchInput) -> String {
    match input {
        TeamLaunchInput::Issue { number, title } => {
            format!(
                "Pre-selected: issue #{number} — {}",
                sanitize_for_terminal(title)
            )
        }
        TeamLaunchInput::Milestone {
            number,
            title,
            seed_issues,
        } => format!(
            "Pre-selected: milestone #{number} ({}) — {} issues",
            sanitize_for_terminal(title),
            seed_issues.len()
        ),
    }
}
