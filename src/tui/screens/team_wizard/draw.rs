//! Rendering for the Team Wizard. Top-level dispatcher routes by mode and
//! delegates to per-flow draw helpers.

use super::compose::PRIMITIVE_LIST;
use super::types::{
    ComposeStep, ComposeTier, LaunchInputKind, LaunchStep, ManageStep, PreflightSummary, role_label,
};
use super::{TeamLaunchInput, TeamWizardMode, TeamWizardScreen};
use crate::orchestration::types::Primitive;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::{WizardFrame, WizardFrameFooter, WizardFrameHeader};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

impl TeamWizardScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        match self.mode() {
            TeamWizardMode::Home => self.draw_home(f, area, theme),
            TeamWizardMode::Compose => self.draw_compose(f, area, theme),
            TeamWizardMode::Launch => self.draw_launch(f, area, theme),
            TeamWizardMode::Manage => self.draw_manage(f, area, theme),
        }
    }

    fn draw_home(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        WizardFrame::draw(
            f,
            area,
            theme,
            WizardFrameHeader {
                step_index: 0,
                step_total: 0,
                step_label: "Teams",
            },
            WizardFrameFooter {
                validation_error: None,
                hints: Some("[c] Compose   [l] Launch   [m] Manage   [Esc] Back"),
            },
            |f, body_area| {
                let lines = vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "Pick a team flow:",
                        Style::default()
                            .fg(theme.text_primary)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::from(""),
                    home_entry_line(theme, "c", "Compose", "Build or extend a team preset"),
                    home_entry_line(theme, "l", "Launch", "Run a team on issues / a milestone"),
                    home_entry_line(theme, "m", "Manage", "Edit or delete user-tier presets"),
                ];
                f.render_widget(
                    Paragraph::new(lines)
                        .alignment(Alignment::Center)
                        .block(theme.styled_block("Team Wizard", false)),
                    body_area,
                );
            },
        );
    }

    fn draw_compose(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let step = self.compose_step();
        WizardFrame::draw(
            f,
            area,
            theme,
            WizardFrameHeader {
                step_index: step.index(),
                step_total: ComposeStep::total(),
                step_label: step.label(),
            },
            WizardFrameFooter {
                validation_error: self.validation_error(),
                hints: Some(compose_footer_hint(step)),
            },
            |f, body_area| match step {
                ComposeStep::Source => self.draw_compose_source(f, body_area, theme),
                ComposeStep::Primitive => self.draw_compose_primitive(f, body_area, theme),
                ComposeStep::Roles => self.draw_compose_roles(f, body_area, theme),
                ComposeStep::Overrides => self.draw_compose_overrides(f, body_area, theme),
                ComposeStep::Save => self.draw_compose_save(f, body_area, theme),
                ComposeStep::SaveSuccess => self.draw_terminal_state(
                    f,
                    body_area,
                    theme,
                    "Saved",
                    &sanitize_for_terminal(self.compose.name.as_str()),
                    theme.accent_success,
                ),
                ComposeStep::SaveFailed => self.draw_terminal_state(
                    f,
                    body_area,
                    theme,
                    "Save Failed",
                    &sanitize_for_terminal(self.failure_reason().unwrap_or("unknown error")),
                    theme.accent_error,
                ),
            },
        );
    }

    fn draw_compose_source(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let opts = self.compose_source_options();
        let mut lines = vec![
            Line::from(Span::styled(
                "Start blank or extend an existing preset.",
                Style::default().fg(theme.text_secondary),
            )),
            Line::from(""),
        ];
        for (i, opt) in opts.iter().enumerate() {
            let label = match opt {
                super::compose::ComposeSourceOption::Blank => "Blank".to_string(),
                super::compose::ComposeSourceOption::Extends(name) => {
                    format!("Extends: {}", sanitize_for_terminal(name))
                }
            };
            lines.push(focused_line(theme, &label, i == self.compose.source_focus));
        }
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Source", false)),
            area,
        );
    }

    fn draw_compose_primitive(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines = vec![Line::from(Span::styled(
            "Choose the primitive that matches the team's shape.",
            Style::default().fg(theme.text_secondary),
        ))];
        for (i, p) in PRIMITIVE_LIST.iter().enumerate() {
            let required = p.required_roles();
            let role_summary = if required.is_empty() {
                "no required roles".to_string()
            } else {
                required
                    .iter()
                    .map(|r| role_label(*r))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let label = format!("{}: {}", primitive_label(*p), role_summary);
            lines.push(focused_line(
                theme,
                &label,
                i == self.compose.primitive_focus,
            ));
        }
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Primitive", false)),
            area,
        );
    }

    fn draw_compose_roles(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let primitive = self.compose.primitive.unwrap_or(Primitive::SinglePass);
        let roles = primitive.required_roles();
        let agents = self.healthy_agents();
        let block = theme.styled_block("Roles", false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(inner);

        let mut role_lines = vec![Line::from(Span::styled(
            "Roles to bind",
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ))];
        if roles.is_empty() {
            role_lines.push(Line::from(Span::styled(
                "No required roles for this primitive.",
                Style::default().fg(theme.text_secondary),
            )));
        }
        for (i, role) in roles.iter().enumerate() {
            let bound = self
                .compose
                .bindings
                .get(role)
                .map(|s| format!(" → {s}"))
                .unwrap_or_else(|| " (unbound)".to_string());
            let label = format!("{}{bound}", role_label(*role));
            role_lines.push(focused_line(theme, &label, i == self.compose.role_focus));
        }
        f.render_widget(Paragraph::new(role_lines), columns[0]);

        let mut agent_lines = vec![Line::from(Span::styled(
            "Healthy agents",
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ))];
        if agents.is_empty() {
            agent_lines.push(Line::from(Span::styled(
                "No agents reported healthy.",
                Style::default().fg(theme.accent_warning),
            )));
        }
        for (i, a) in agents.iter().enumerate() {
            agent_lines.push(focused_line(theme, a, i == self.compose.agent_focus));
        }
        f.render_widget(Paragraph::new(agent_lines), columns[1]);
    }

    fn draw_compose_overrides(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Optional per-role overrides (mode / model / prompt addendum).",
                Style::default().fg(theme.text_primary),
            )),
            Line::from(Span::styled(
                "Press Enter to skip — defaults preserve the binding's agent settings.",
                Style::default().fg(theme.text_secondary),
            )),
        ];
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Overrides", false)),
            area,
        );
    }

    fn draw_compose_save(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let tier = match self.compose.tier {
            ComposeTier::User => "user",
            ComposeTier::Project => "project",
        };
        let lines = vec![
            Line::from(Span::styled(
                "Name (alphanumeric + hyphen, no leading dot, no slashes)",
                Style::default().fg(theme.text_secondary),
            )),
            Line::from(Span::styled(
                format!("> {}", sanitize_for_terminal(&self.compose.name)),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("Tier: {tier}    [Tab] toggle    [Enter] save"),
                Style::default().fg(theme.text_secondary),
            )),
        ];
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Save", false)),
            area,
        );
    }

    fn draw_terminal_state(
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
                LaunchStep::PlanPreview => self.draw_launch_plan_preview(f, body_area, theme),
                LaunchStep::Confirm => self.draw_launch_confirm(f, body_area, theme),
                LaunchStep::Executing => self.draw_terminal_state(
                    f,
                    body_area,
                    theme,
                    "Executing",
                    "Dispatching team run…",
                    theme.accent_info,
                ),
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

fn focused_line(theme: &Theme, label: &str, selected: bool) -> Line<'static> {
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

fn home_entry_line(theme: &Theme, key: &str, label: &str, description: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("[{}] ", key),
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{label}: "),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            description.to_string(),
            Style::default().fg(theme.text_secondary),
        ),
    ])
}

fn compose_footer_hint(step: ComposeStep) -> &'static str {
    match step {
        ComposeStep::Source => "[↑/↓ j/k] pick   [Enter] choose   [Esc] back",
        ComposeStep::Primitive => "[↑/↓ j/k] pick   [Enter] choose   [Esc] back",
        ComposeStep::Roles => {
            "[↑/↓ j/k] role   [←/→ h/l] agent   [Space] bind   [Enter] next   [Esc] back"
        }
        ComposeStep::Overrides => "[Enter] continue   [Esc] back",
        ComposeStep::Save => "[Tab] tier   [Enter] save   [Esc] back",
        ComposeStep::SaveSuccess => "[Enter] return",
        ComposeStep::SaveFailed => "[r] retry   [Esc] back",
    }
}

fn launch_footer_hint(step: LaunchStep) -> &'static str {
    match step {
        LaunchStep::TeamPicker => "[↑/↓ j/k] pick   [Enter] choose   [Esc] back",
        LaunchStep::InputPicker => "[↑/↓ j/k] pick   [Enter] continue   [Esc] back",
        LaunchStep::PlanPreview => "[Enter] confirm   [Esc] back",
        LaunchStep::Confirm => "[Enter] launch   [Esc] back",
        LaunchStep::Executing => "Dispatching…",
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
