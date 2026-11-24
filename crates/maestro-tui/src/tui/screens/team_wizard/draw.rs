//! Rendering for the Team Wizard. Top-level dispatcher routes by mode and
//! delegates to per-flow draw helpers.

use super::types::ManageStep;
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

pub(super) fn preselect_label(input: &TeamLaunchInput) -> String {
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
