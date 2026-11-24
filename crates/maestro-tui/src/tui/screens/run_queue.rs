//! The Run queue (#1269, design `docs/design-system/index.html` §02.9 "Run
//! queue"): Milestones `R` opens it with the milestone's `RunPlan`. It shows
//! the whole decision (waves, held issues and why, worst case) and starts
//! nothing itself: `⏎` opens the shared launch dialog, the only spend step.
//!
//! `⏎` launches wave 1, the issues that can start now. A later wave starts
//! from a base that has its blockers merged, so it is planned again with `R`
//! once wave 1 lands (the live run loop is its own issue).

use super::milestone_plan::{IssueReadiness, MilestonePlan, plan_milestone};
use super::milestone_rows::{readiness_chip, row_line};
use super::{ScreenAction, sanitize_for_terminal};
use crate::tui::app::{self, App};
use crate::tui::ds::fit::ellipsize_line;
use crate::tui::ds::money::money;
use crate::tui::icons::{IconId, get_for_mode};
use crate::tui::theme::Theme;
use crate::tui::widgets::launch_dialog::{self, LaunchAction, LaunchBudget, LaunchDialog};
use crate::work::run_plan::{Hold, RunPlan};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::collections::{HashMap, HashSet};

pub struct RunQueue {
    milestone: String,
    /// Open issues the plan was made from.
    open: usize,
    plan: RunPlan,
    titles: HashMap<u64, String>,
    pub(crate) launch_dialog: Option<LaunchDialog>,
    launch_budget: LaunchBudget,
}

impl RunQueue {
    pub fn new(
        milestone: String,
        open: usize,
        plan: RunPlan,
        titles: HashMap<u64, String>,
    ) -> Self {
        Self {
            milestone,
            open,
            plan,
            titles,
            launch_dialog: None,
            launch_budget: LaunchBudget::default(),
        }
    }

    pub fn set_launch_budget(&mut self, budget: LaunchBudget) {
        if let Some(dialog) = self.launch_dialog.as_mut() {
            dialog.set_budget(budget.clone());
        }
        self.launch_budget = budget;
    }

    fn title(&self, n: u64) -> String {
        self.titles
            .get(&n)
            .cloned()
            .unwrap_or_else(|| format!("Issue #{n}"))
    }

    /// Issues the plan launches, all waves.
    fn runs(&self) -> usize {
        self.plan.waves.iter().map(Vec::len).sum()
    }

    /// The sessions `⏎` launches: wave 1, in plan order.
    pub fn sessions(&self) -> Vec<(u64, String)> {
        self.plan
            .waves
            .first()
            .map(|w| w.iter().map(|&n| (n, self.title(n))).collect())
            .unwrap_or_default()
    }

    /// Open the launch dialog on wave 1. No wave, no dialog.
    pub fn open_launch_dialog(&mut self) {
        let sessions = self.sessions();
        if sessions.is_empty() {
            return;
        }
        let mut dialog = LaunchDialog::new(sessions, Default::default());
        dialog.set_budget(self.launch_budget.clone());
        self.launch_dialog = Some(dialog);
    }

    /// `11 open → 7 run in 2 waves · 1 needs a decision · 3 deferred`.
    pub fn summary(&self) -> String {
        let runs = self.runs();
        let waves = self.plan.waves.len();
        let decide = self.plan.needs_decision.len();
        let deferred = self.plan.deferred.len().saturating_sub(decide);
        format!(
            "{} open {} {runs} run in {waves} wave{} · {decide} need{} a decision · {deferred} deferred",
            self.open,
            get_for_mode(IconId::ArrowRight, false),
            if waves == 1 { "" } else { "s" },
            if decide == 1 { "s" } else { "" },
        )
    }

    pub fn handle_input(&mut self, event: &Event) -> ScreenAction {
        if let Some(dialog) = self.launch_dialog.as_mut() {
            return match dialog.handle_input(event) {
                LaunchAction::Cancel => {
                    self.launch_dialog = None;
                    ScreenAction::None
                }
                LaunchAction::Confirm(prompt) => {
                    self.launch_dialog.take().map_or(ScreenAction::None, |d| {
                        match super::issue_browser::launch_action(d, prompt) {
                            // This screen is the queue review: no second one.
                            ScreenAction::LaunchSessions(configs) => {
                                ScreenAction::LaunchQueue(configs)
                            }
                            other => other,
                        }
                    })
                }
                LaunchAction::None => ScreenAction::None,
            };
        }
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match code {
                KeyCode::Enter => self.open_launch_dialog(),
                KeyCode::Esc => return ScreenAction::Pop,
                _ => {}
            }
        }
        ScreenAction::None
    }

    pub fn footer_hints(&self) -> Vec<(&'static str, &'static str)> {
        match self.launch_dialog.as_ref() {
            Some(dialog) => dialog.footer_hints(),
            None if self.plan.waves.is_empty() => vec![("Esc", "Back")],
            None => vec![("Enter", "Review launch"), ("Esc", "Back")],
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect, theme: &Theme, nerd: bool) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(area);
        let muted = Style::default().fg(theme.text_secondary);
        let summary = Line::from(Span::styled(format!(" {}", self.summary()), muted));
        f.render_widget(
            Paragraph::new(ellipsize_line(summary, area.width as usize)),
            chunks[0],
        );
        self.draw_plan(f, chunks[1], theme, nerd);
        self.draw_run_bar(f, chunks[2], theme, nerd);
        if let Some(dialog) = self.launch_dialog.as_ref() {
            launch_dialog::draw(f, area, dialog, theme);
        }
    }

    fn draw_plan(&self, f: &mut Frame, area: Rect, theme: &Theme, nerd: bool) {
        let runs = self.runs();
        let title = format!(
            "Run order · {runs} of {} · {}",
            self.open,
            sanitize_for_terminal(&self.milestone)
        );
        let block = theme.styled_block(&title, true);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let width = inner.width as usize;
        let mut lines: Vec<Line> = Vec::new();
        let header = |label: String| {
            Line::from(Span::styled(
                label,
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::BOLD),
            ))
        };
        for (i, wave) in self.plan.waves.iter().enumerate() {
            // Only wave 1 launches now: a later wave needs its blockers
            // merged first, so it is planned again with `R`.
            let (why, mark) = if i == 0 {
                ("start now", "[x]")
            } else {
                ("after the wave before merges", "[ ]")
            };
            lines.push(header(format!("WAVE {} · {why}", i + 1)));
            for &n in wave {
                lines.push(self.row(mark, n, self.wave_note(n), theme, width));
            }
        }
        if !self.plan.needs_decision.is_empty() {
            lines.push(header(
                "NEEDS A DECISION · already claimed, not launched".into(),
            ));
            for &n in &self.plan.needs_decision {
                lines.push(self.row(
                    "[?]",
                    n,
                    readiness_chip(IssueReadiness::Claimed, nerd),
                    theme,
                    width,
                ));
            }
        }
        let deferred: Vec<&(u64, Vec<Hold>)> = (self.plan.deferred.iter())
            .filter(|(n, _)| !self.plan.needs_decision.contains(n))
            .collect();
        if !deferred.is_empty() {
            lines.push(header(
                "DEFERRED · stay open on GitHub, not launched".into(),
            ));
            for (n, holds) in deferred {
                lines.push(self.row("[-]", *n, hold_note(holds, nerd), theme, width));
            }
        }
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "Nothing to run: every issue is closed.",
                Style::default().fg(theme.text_muted),
            )));
        }
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn wave_note(&self, n: u64) -> String {
        if let Some((_, after, _)) = self.plan.moved.iter().find(|(m, _, _)| *m == n) {
            return format!("after #{after}, same file");
        }
        if self.plan.unknown_scope.contains(&n) {
            return "! scope unknown".into();
        }
        String::new()
    }

    fn row(&self, mark: &str, n: u64, note: String, theme: &Theme, width: usize) -> Line<'static> {
        let left = vec![
            Span::styled(
                format!("  {mark} #{n} "),
                Style::default().fg(theme.accent_identifier),
            ),
            Span::styled(
                sanitize_for_terminal(&self.title(n)),
                Style::default().fg(theme.text_primary),
            ),
        ];
        let note = Span::styled(note, Style::default().fg(theme.text_secondary));
        row_line(left, vec![note], width)
    }

    fn draw_run_bar(&self, f: &mut Frame, area: Rect, theme: &Theme, nerd: bool) {
        let runs = self.runs();
        let waves = self.plan.waves.len();
        let worst = if self.plan.worst_case_usd > 0.0 {
            format!(" · worst case {}", money(Some(self.plan.worst_case_usd)))
        } else {
            String::new()
        };
        let first = self.sessions().len();
        let enter = get_for_mode(IconId::Enter, nerd);
        let next = match (first, waves) {
            (0, _) => "nothing can start now".to_string(),
            (_, 1) => format!("{enter} reviews the launch of all {first}"),
            _ => format!(
                "{enter} reviews the launch of wave 1 ({first}); plan again with R once it merges"
            ),
        };
        let lines = vec![
            Line::from(Span::styled(
                format!(
                    " {runs} session{} · {waves} wave{}{worst}",
                    if runs == 1 { "" } else { "s" },
                    if waves == 1 { "" } else { "s" },
                ),
                Style::default().fg(theme.accent_info),
            )),
            Line::from(Span::styled(
                format!(" {next}"),
                Style::default().fg(theme.text_secondary),
            )),
        ];
        let lines: Vec<Line> = lines
            .into_iter()
            .map(|l| ellipsize_line(l, area.width as usize))
            .collect();
        f.render_widget(Paragraph::new(lines), area);
    }
}

/// The right-hand reason of a deferred row.
fn hold_note(holds: &[Hold], nerd: bool) -> String {
    match holds.first() {
        Some(Hold::BlockedOutside {
            by,
            milestone: Some(m),
        }) => format!(
            "{} #{by} (milestone {m})",
            get_for_mode(IconId::XCircle, nerd)
        ),
        Some(Hold::BlockedOutside {
            by,
            milestone: None,
        }) => {
            format!("{} #{by}", get_for_mode(IconId::XCircle, nerd))
        }
        Some(Hold::WaitsOnDeferred(d)) => format!("waits on #{d}"),
        Some(Hold::Claimed) => readiness_chip(IssueReadiness::Claimed, nerd),
        Some(Hold::NoMetadata) | None => readiness_chip(IssueReadiness::Unknown, nerd),
    }
}

/// Milestones `R` (#1269): plan the selected milestone and open the Run
/// queue. Nothing spawns here. Claims are the `maestro:in-progress` label
/// plus every live session's issue.
pub fn open_run_queue(app: &mut App, milestone: u64) {
    let Some(screen) = app.screen_state.milestone_screen.as_ref() else {
        return;
    };
    let Some(entry) = screen.milestones.iter().find(|m| m.number == milestone) else {
        return;
    };
    let live: HashSet<u64> = (app.pool.all_sessions().into_iter())
        .filter(|s| !s.status.is_terminal())
        .filter_map(|s| s.issue_number)
        .collect();
    // The issues of this fetch only (the cache may hold an issue that
    // closed since). Live sessions add claims the START column, which
    // reads only the label, does not see.
    let known = screen.milestones.iter().flat_map(|m| m.issues.iter());
    let budget = crate::tui::budget_prespawn::launch_budget(app);
    let planned: anyhow::Result<MilestonePlan> =
        plan_milestone(&entry.issues, known, &live, budget.per_session_usd);
    match planned {
        Ok(p) => {
            let titles = (entry.issues.iter())
                .map(|i| (i.number, i.title.clone()))
                .collect();
            let mut run = RunQueue::new(entry.title.clone(), p.open, p.plan, titles);
            run.set_launch_budget(budget);
            app.screen_state.queue_confirmation_screen =
                Some(super::QueueConfirmationScreen::from_run(run));
            app.navigate_to(app::TuiMode::QueueConfirmation);
        }
        Err(e) => {
            let message = format!("Run not planned: {e}");
            app.push_toast(
                crate::tui::ds::toast::Toast::new(crate::tui::ds::toast::Severity::Error, message),
                std::time::Instant::now(),
            );
        }
    }
}

#[cfg(test)]
#[path = "run_queue_tests.rs"]
mod tests;
