//! Launch drawing (#1290, §02.17): the shared stepped frame with three
//! steps (Team · Input · Plan & launch) and a Preflight checklist, the Team
//! and Input steps, and the run / result states. Plan & launch and the
//! dialog are in `draw_launch_plan.rs`.

use super::TeamWizardScreen;
use super::draw::focused_line;
use super::draw_launch_plan::plural;
use super::launch::INPUT_KINDS;
use super::overview::ordered_presets;
use super::types::{LaunchInputKind, LaunchPhase, LaunchStep};
use crate::tui::ds::checklist::{CheckItem, CheckState};
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::WizardFrame;
use crate::tui::widgets::input;
use crate::tui::widgets::preset_card::draw_preset_card;
use crate::tui::widgets::wizard_frame::{WizardChecklist, WizardStepHeader, field_block};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

/// Card width beside the team list, shown from `CARD_FROM` columns.
const CARD_WIDTH: u16 = 60;
const CARD_FROM: u16 = 100;

pub(super) fn footer_hints(s: &TeamWizardScreen) -> Vec<(&'static str, &'static str)> {
    match s.launch_phase() {
        LaunchPhase::Editing => match s.launch_step() {
            LaunchStep::Input if s.launch.finding => {
                vec![("↑↓", "match"), ("Enter", "pick"), ("Esc", "kinds")]
            }
            LaunchStep::Team | LaunchStep::Input => {
                vec![("↑↓ j k", "pick"), ("Enter", "choose"), ("Esc", "back")]
            }
            LaunchStep::Plan => vec![("Enter", "review launch"), ("Esc", "back")],
        },
        LaunchPhase::Dialog if s.launch_block_reason().is_some() => vec![("Esc", "back to plan")],
        LaunchPhase::Dialog => vec![("Enter", "launch"), ("Esc", "back to plan")],
        LaunchPhase::Executing => vec![("Esc", "cancel run")],
        LaunchPhase::Launched => vec![("Enter", "return")],
        LaunchPhase::Failed => vec![("r", "re-plan"), ("Esc", "close")],
    }
}

pub(super) fn text(s: impl Into<String>, style: Style) -> Line<'static> {
    Line::from(Span::styled(s.into(), style))
}

impl TeamWizardScreen {
    pub(super) fn draw_launch(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let step = self.launch_step();
        let labels: Vec<&str> = LaunchStep::ALL.iter().map(|s| s.label()).collect();
        let items = self.launch_checklist();
        let choice = self.launch_choice();
        WizardFrame::draw_stepped(
            f,
            area,
            theme,
            WizardStepHeader {
                steps: &labels,
                current: step.index() - 1,
                errors: &[],
                state: (!choice.is_empty())
                    .then(|| Span::styled(choice, Style::default().fg(theme.text_secondary))),
            },
            Some(WizardChecklist {
                title: "Preflight",
                items: &items,
            }),
            |f, body| match self.launch_phase() {
                LaunchPhase::Editing => match step {
                    LaunchStep::Team => self.draw_launch_team(f, body, theme),
                    LaunchStep::Input => self.draw_launch_input(f, body, theme),
                    LaunchStep::Plan => self.draw_launch_plan(f, body, theme),
                },
                LaunchPhase::Dialog => self.draw_launch_dialog(f, body, theme),
                LaunchPhase::Executing => {
                    if !self.draw_launch_waves(f, body, theme) {
                        draw_result(
                            f,
                            body,
                            theme,
                            "Executing",
                            "Dispatching team run…",
                            theme.accent_info,
                        );
                    }
                }
                LaunchPhase::Launched => draw_result(
                    f,
                    body,
                    theme,
                    "Launched",
                    "Team run dispatched. Spawned is not finished: follow it on the Dashboard.",
                    theme.accent_success,
                ),
                LaunchPhase::Failed => draw_result(
                    f,
                    body,
                    theme,
                    "Launch Failed",
                    &sanitize_for_terminal(self.failure_reason().unwrap_or("unknown error")),
                    theme.accent_error,
                ),
            },
        );
    }

    /// The choices so far, right of the stepper: team · input.
    fn launch_choice(&self) -> String {
        let mut parts = Vec::new();
        if let Some(team) = &self.launch.selected_team {
            parts.push(sanitize_for_terminal(team));
        }
        match (self.launch.input_kind, self.launch.manual_issues.as_slice()) {
            (_, []) => {}
            (LaunchInputKind::Issue, [n]) => parts.push(format!("#{n}")),
            (_, issues) => parts.push(plural(issues.len(), "issue", "issues")),
        }
        parts.join(" · ")
    }

    fn launch_checklist(&self) -> Vec<CheckItem<'static>> {
        let state = |ok: bool, reached: bool| match (ok, reached) {
            (true, _) => CheckState::Ok,
            (false, true) => CheckState::Missing,
            (false, false) => CheckState::Pending,
        };
        let step = self.launch_step().index();
        let input_ok = !self.launch.manual_issues.is_empty();
        let preflight = match &self.launch.preflight {
            Some(Ok(())) => CheckState::Ok,
            Some(Err(_)) => CheckState::Missing,
            None => CheckState::Pending,
        };
        let runs = match &self.launch.plan {
            Some(p) if p.final_count > 0 => CheckState::Ok,
            Some(_) => CheckState::Missing,
            None => CheckState::Pending,
        };
        vec![
            CheckItem {
                label: "Team",
                state: state(self.launch.selected_team.is_some(), step >= 1),
                note: None,
            },
            CheckItem {
                label: "Input",
                state: state(input_ok, step >= 2),
                note: None,
            },
            CheckItem {
                label: "Pre-flight",
                state: preflight,
                note: None,
            },
            CheckItem {
                label: "Something will run",
                state: runs,
                note: None,
            },
        ]
    }

    fn draw_launch_team(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let presets = ordered_presets(&self.resolved_teams);
        let (list_area, card_area) = if area.width >= CARD_FROM {
            let [l, c] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(CARD_WIDTH)])
                .areas(area);
            (l, Some(c))
        } else {
            (area, None)
        };
        let mut lines = vec![text(
            "Pick the preset to run. Each issue runs as one session on its implementer.",
            Style::default().fg(theme.text_secondary),
        )];
        if presets.is_empty() {
            lines.push(text(
                "No team presets · press c on Teams to compose one",
                Style::default().fg(theme.accent_warning),
            ));
        }
        for (i, team) in presets.iter().enumerate() {
            lines.push(self.preset_row(team, i == self.launch.team_focus, theme));
        }
        let block = field_block("Team", true, self.launch.error, theme);
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(block),
            list_area,
        );
        if let (Some(card), Some(team)) = (card_area, presets.get(self.launch.team_focus)) {
            draw_preset_card(f, card, theme, team, &self.preflight, false);
        }
    }

    fn draw_launch_input(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let preselect = u16::from(self.initial_input().is_some());
        let [kinds_area, find_area, matches_area] = Layout::vertical([
            Constraint::Length(INPUT_KINDS.len() as u16 + 2 + preselect),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .areas(area);
        let mut lines = Vec::new();
        if let Some(input) = self.initial_input() {
            lines.push(text(
                super::draw::preselect_label(input),
                Style::default()
                    .fg(theme.accent_info)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        for (i, kind) in INPUT_KINDS.iter().enumerate() {
            let (name, what) = match kind {
                LaunchInputKind::Issue => ("Issue", "one issue you find by number"),
                LaunchInputKind::IssueSet => {
                    ("Issue set", "the issues you brought from Milestones")
                }
                LaunchInputKind::Milestone => ("Milestone", "the open issues of that milestone"),
                LaunchInputKind::IdeaInbox => {
                    ("Idea inbox", "⊘ not available yet · no triager input")
                }
            };
            let row = format!("{name:<12}{what}");
            if *kind == LaunchInputKind::IdeaInbox {
                // Disabled (§02.17 finding 5): drawn, never chosen.
                let marker = if i == self.launch.input_focus {
                    ">"
                } else {
                    " "
                };
                lines.push(text(
                    format!(" {marker} {row}"),
                    Style::default().fg(theme.text_muted),
                ));
            } else {
                lines.push(focused_line(theme, &row, i == self.launch.input_focus));
            }
        }
        let kinds_focused = !self.launch.finding;
        let error = if kinds_focused {
            self.launch.error
        } else {
            None
        };
        f.render_widget(
            Paragraph::new(lines).block(field_block("Run on", kinds_focused, error, theme)),
            kinds_area,
        );

        let issue_kind = INPUT_KINDS.get(self.launch.input_focus) == Some(&LaunchInputKind::Issue);
        if !issue_kind {
            return;
        }
        let error = if self.launch.finding {
            self.launch.error
        } else {
            None
        };
        let block = field_block("Find issue", self.launch.finding, error, theme);
        let inner = block.inner(find_area);
        f.render_widget(block, find_area);
        input::render(&self.issue_input, f, inner, theme, self.launch.finding);

        let candidates = self.autocomplete_candidates();
        let mut lines = Vec::new();
        if self.issue_metas.is_empty() {
            lines.push(text(
                "Loading open issues · gh issue list",
                Style::default().fg(theme.text_muted),
            ));
        } else if !self.issue_input.is_empty() && candidates.is_empty() {
            lines.push(text(
                "No open issue starts with that number",
                Style::default().fg(theme.accent_warning),
            ));
        }
        for (i, n) in candidates.iter().enumerate() {
            let focused = self.launch.autocomplete_focus == Some(i);
            lines.push(focused_line(theme, &format!("#{n}  open"), focused));
        }
        f.render_widget(Paragraph::new(lines), matches_area);
    }
}

/// A run or result state in the body. Its keys live in the footer only
/// (§02.17 finding 9), so no "press Enter" line here.
fn draw_result(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    title: &str,
    body: &str,
    color: ratatui::style::Color,
) {
    let lines = vec![
        Line::default(),
        text(
            body.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .alignment(ratatui::layout::Alignment::Center)
            .wrap(Wrap { trim: true })
            .block(theme.styled_block(title, false)),
        area,
    );
}
