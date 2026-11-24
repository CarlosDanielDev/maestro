//! Plan & launch drawing (#1290, §02.17 "Run plan lanes + launch bar"):
//! the stage line, `run_plan`'s waves as lanes, NOT INCLUDED with each
//! hold's reason, the cost line, and the launch dialog.

use super::TeamWizardScreen;
use super::draw_launch::text;
use super::launch_cost::cost_lines;
use super::types::{IssueMetaMap, PlanPreview, role_label};
use crate::orchestration::types::TeamRole;
use crate::tui::ds::armed::ArmedState;
use crate::tui::screens::milestone_plan::is_other_repo;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::wizard_frame::field_block;
use crate::work::run_plan::Hold;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

/// One line of why `run_plan` left an issue out. A blocker with no issue
/// data is unknown, never called open (#1290 judge).
fn hold_note(holds: &[Hold], metas: &IssueMetaMap) -> String {
    match holds.first() {
        Some(Hold::BlockedOutside { by, .. }) if is_other_repo(*by) => {
            "⊘ blocked by an issue in another repo".to_string()
        }
        Some(Hold::BlockedOutside { by, .. }) if !metas.contains_key(by) => {
            format!("⊘ blocked by #{by} (no issue data, state unknown)")
        }
        Some(Hold::BlockedOutside { by, .. }) => {
            format!("⊘ blocked by open #{by} outside this run")
        }
        Some(Hold::WaitsOnDeferred(d)) => format!("⊘ waits on #{d}, which is left out"),
        Some(Hold::Claimed) => "◐ another session claims it".to_string(),
        Some(Hold::NoMetadata) | None => "? no issue data yet".to_string(),
    }
}

impl TeamWizardScreen {
    pub(super) fn draw_launch_plan(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = field_block("Plan & launch", true, self.launch.error, theme);
        let Some(plan) = &self.launch.plan else {
            f.render_widget(
                Paragraph::new(text(
                    sanitize_for_terminal(self.failure_reason().unwrap_or("No plan available.")),
                    Style::default().fg(theme.accent_warning),
                ))
                .block(block),
                area,
            );
            return;
        };
        let mut lines = self.stage_lines(plan, theme);
        lines.extend(self.lane_lines(plan, theme));
        lines.push(Line::default());
        lines.extend(cost_lines(plan, &self.launch_budget, theme));
        if let Some(Err(summary)) = &self.launch.preflight {
            lines.push(Line::default());
            for b in &summary.blocking {
                lines.push(text(
                    format!("✗ {}", sanitize_for_terminal(&b.render_line())),
                    Style::default().fg(theme.accent_error),
                ));
            }
        }
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(block),
            area,
        );
    }

    /// The team and the session each lane really runs (§02.17 finding 3).
    fn stage_lines(&self, plan: &PlanPreview, theme: &Theme) -> Vec<Line<'static>> {
        // As `Scheduler::agent_for_issue` picks it: the implementer's agent,
        // then its fallback, then the app default.
        let runs_as = (self.resolved_teams.get(&plan.team_name))
            .and_then(|t| t.bindings.get(&TeamRole::Implementer))
            .and_then(|b| {
                Some(b.agent.clone())
                    .filter(|a| !a.is_empty())
                    .or_else(|| b.fallback_agent.clone().filter(|a| !a.is_empty()))
            })
            .map(|a| sanitize_for_terminal(&a))
            .unwrap_or_else(|| "the default agent".to_string());
        vec![
            Line::from(vec![
                Span::styled(
                    sanitize_for_terminal(&plan.team_name),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" · {}", plan.primitive.label()),
                    Style::default().fg(theme.accent_identifier),
                ),
                Span::styled(
                    format!(
                        " · one session per issue on {runs_as} ({} only)",
                        role_label(TeamRole::Implementer)
                    ),
                    Style::default().fg(theme.text_secondary),
                ),
            ]),
            text(
                "Each wave starts when the one before it has finished.",
                Style::default().fg(theme.text_secondary),
            ),
        ]
    }

    /// `run_plan`'s waves, one lane per issue, then the issues left out.
    fn lane_lines(&self, plan: &PlanPreview, theme: &Theme) -> Vec<Line<'static>> {
        let id = Style::default().fg(theme.accent_identifier);
        let muted = Style::default().fg(theme.text_secondary);
        let per = self.launch_budget.per_session_usd;
        let cap = if per > 0.0 {
            format!("cap {}", crate::tui::ds::money::money(Some(per)))
        } else {
            "no cap".to_string()
        };
        let mut lines = Vec::new();
        for (i, wave) in plan.levels.iter().enumerate() {
            let after = if i == 0 {
                "starts now".to_string()
            } else {
                format!("after wave {i} finishes")
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("WAVE {} ", i + 1),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("· {} · {after}", lanes(wave.len())), muted),
            ]));
            for n in wave {
                let added = if plan.auto_added.contains(n) {
                    "  ! added: blocks an issue you picked"
                } else {
                    ""
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("  #{n:<6}"), id),
                    Span::styled(cap.clone(), muted),
                    Span::styled(added, Style::default().fg(theme.accent_warning)),
                ]));
            }
        }
        let left_out = plan.deferred.len() + plan.closed.len();
        if left_out > 0 {
            lines.push(text(
                format!("NOT INCLUDED · {left_out}"),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ));
            for (n, holds) in &plan.deferred {
                lines.push(Line::from(vec![
                    Span::styled(format!("  #{n:<6}"), id),
                    Span::styled(
                        hold_note(holds, &self.issue_metas),
                        Style::default().fg(theme.accent_warning),
                    ),
                ]));
            }
            for n in &plan.closed {
                lines.push(Line::from(vec![
                    Span::styled(format!("  #{n:<6}"), id),
                    Span::styled("✓ already closed", muted),
                ]));
            }
        }
        lines
    }

    /// Plan `Enter` (§02.17 "Launch dialog"): what runs, what it can cost,
    /// where it works. One `Enter` here launches.
    pub(super) fn draw_launch_dialog(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = theme.styled_block("Launch team run", true);
        let Some(plan) = &self.launch.plan else {
            f.render_widget(block, area);
            return;
        };
        let primary = Style::default().fg(theme.text_primary);
        let mut issues: Vec<String> = plan.levels.iter().flatten().map(u64::to_string).collect();
        issues.sort_by_key(|n| n.parse::<u64>().unwrap_or(0));
        let mut lines = vec![
            text(
                format!(
                    "WHAT   {} across {}",
                    sessions(plan.final_count),
                    waves(plan.levels.len())
                ),
                primary,
            ),
            text(
                format!("TEAM   {}", sanitize_for_terminal(&plan.team_name)),
                primary,
            ),
            text(
                format!(
                    "WHERE  {} .maestro/worktrees/issue-{{{}}}",
                    worktrees(plan.final_count),
                    issues.join(",")
                ),
                primary,
            ),
            Line::default(),
        ];
        lines.extend(cost_lines(plan, &self.launch_budget, theme));
        if let Some(gate) = self.dialog_gate_line(theme) {
            lines.push(Line::default());
            lines.push(gate);
        }
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(block),
            area,
        );
    }
}

impl TeamWizardScreen {
    /// Why `Enter` here does not simply launch: locked by the pre-spawn
    /// gate, or armed when the worst case is over the budget left. `None`
    /// when one `Enter` launches (the footer says so).
    fn dialog_gate_line(&self, theme: &Theme) -> Option<Line<'static>> {
        if let Some(reason) = self.launch_block_reason() {
            return Some(text(
                format!("⊘ {reason} · raise [budget] total_usd in Settings to launch"),
                Style::default()
                    .fg(theme.accent_error)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if !self.launch_over_budget() {
            return None;
        }
        Some(match self.launch.arm.state() {
            ArmedState::Armed(_) => text(
                "ARMED · Enter again within 3 s launches · Esc backs out",
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ),
            _ => text(
                "! over the budget left · Enter arms, Enter again within 3 s launches",
                Style::default().fg(theme.accent_warning),
            ),
        })
    }
}

pub(super) fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn lanes(n: usize) -> String {
    plural(n, "lane", "lanes")
}

fn sessions(n: usize) -> String {
    plural(n, "session", "sessions")
}

fn waves(n: usize) -> String {
    plural(n, "wave", "waves")
}

fn worktrees(n: usize) -> String {
    plural(n, "worktree", "worktrees")
}
