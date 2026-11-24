//! Compose drawing (#1286, §02.16 "Compose frame"): the shared stepped
//! frame (`WizardFrame::draw_stepped`) with the Draft checklist, and
//! Review & save showing the exact path and TOML above the armed write.

use super::TeamWizardScreen;
use super::compose::{ComposeSourceOption, PRIMITIVE_LIST};
use super::draw::focused_line;
use super::types::{ComposeSource, ComposeStep, ComposeTier, role_label, roles_to_bind};
use crate::orchestration::types::Primitive;
use crate::tui::ds::armed::{ArmedEnter, ArmedState};
use crate::tui::ds::checklist::{CheckItem, CheckState};
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::WizardFrame;
use crate::tui::widgets::armed_bar::{ARMED_BAR_HEIGHT, ArmedBar};
use crate::tui::widgets::input;
use crate::tui::widgets::wizard_frame::{WizardChecklist, WizardStepHeader, field_block};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub(super) fn footer_hints(s: &TeamWizardScreen) -> Vec<(&'static str, &'static str)> {
    match s.compose_step() {
        ComposeStep::Source | ComposeStep::Primitive => {
            vec![("↑↓ j k", "pick"), ("Enter", "choose"), ("Esc", "back")]
        }
        ComposeStep::Roles => vec![
            ("↑↓ j k", "role"),
            ("←→ h l", "agent"),
            ("Space", "bind"),
            ("Enter", "next"),
            ("Esc", "back"),
        ],
        ComposeStep::Save => match s.compose_save_state() {
            ArmedState::Done => vec![("Enter", "return"), ("Esc", "back")],
            ArmedState::Armed(_) => vec![("Enter", "write"), ("Esc", "disarm")],
            _ => vec![("Tab", "tier"), ("Enter", "arm"), ("Esc", "back")],
        },
    }
}

impl TeamWizardScreen {
    pub(super) fn draw_compose(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let step = self.compose_step();
        let labels: Vec<&str> = ComposeStep::ALL.iter().map(|s| s.label()).collect();
        let items = self.compose_checklist();
        let choice = match &self.compose.source {
            Some(ComposeSource::Extends(name)) => {
                format!("extends {}", sanitize_for_terminal(name))
            }
            Some(ComposeSource::Blank) => "blank".to_string(),
            None => String::new(),
        };
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
                title: "Draft",
                items: &items,
            }),
            |f, body| match step {
                ComposeStep::Source => self.draw_compose_source(f, body, theme),
                ComposeStep::Primitive => self.draw_compose_primitive(f, body, theme),
                ComposeStep::Roles => self.draw_compose_roles(f, body, theme),
                ComposeStep::Save => self.draw_compose_save(f, body, theme),
            },
        );
    }

    /// Draft rows: done ✓, missing ✗ once reached, ○ ahead.
    fn compose_checklist(&self) -> Vec<CheckItem<'static>> {
        let current = self.compose_step().index();
        let roles_ok = self.compose.primitive.is_some_and(|p| {
            roles_to_bind(p)
                .iter()
                .all(|r| self.compose.bindings.contains_key(r))
        });
        let rows = [
            ("Start from", self.compose.source.is_some(), 1),
            ("Shape", self.compose.primitive.is_some(), 2),
            ("Roles bound", roles_ok, 3),
            ("Name", self.validate_save_name().is_none(), 4),
        ];
        rows.iter()
            .map(|(label, ok, index)| CheckItem {
                label,
                state: match (*ok, *index <= current) {
                    (true, _) => CheckState::Ok,
                    (false, true) => CheckState::Missing,
                    (false, false) => CheckState::Pending,
                },
                note: None,
            })
            .collect()
    }

    fn draw_compose_source(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines = vec![
            Line::from(Span::styled(
                "Start blank or extend an existing preset.",
                Style::default().fg(theme.text_secondary),
            )),
            Line::from(""),
        ];
        for (i, opt) in self.compose_source_options().iter().enumerate() {
            let label = match opt {
                ComposeSourceOption::Blank => "Blank".to_string(),
                ComposeSourceOption::Extends(name) => {
                    format!("Extends: {}", sanitize_for_terminal(name))
                }
            };
            lines.push(focused_line(theme, &label, i == self.compose.source_focus));
        }
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Start from", true)),
            area,
        );
    }

    fn draw_compose_primitive(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines = vec![Line::from(Span::styled(
            "Choose the shape. Roles listed are the ones its machine runs.",
            Style::default().fg(theme.text_secondary),
        ))];
        for (i, p) in PRIMITIVE_LIST.iter().enumerate() {
            let flow = p
                .dispatched_roles()
                .iter()
                .map(|r| role_label(*r))
                .collect::<Vec<_>>()
                .join(if *p == Primitive::FanOut {
                    " ∥ "
                } else {
                    " → "
                });
            let label = format!("{:<13} {flow}", p.label());
            lines.push(focused_line(
                theme,
                &label,
                i == self.compose.primitive_focus,
            ));
        }
        f.render_widget(
            Paragraph::new(lines).block(theme.styled_block("Shape", true)),
            area,
        );
    }

    fn draw_compose_roles(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let roles = self
            .compose
            .primitive
            .map(roles_to_bind)
            .unwrap_or_default();
        let agents = self.healthy_agents();
        let block = theme.styled_block("Roles", true);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(inner);
        let bold = Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD);

        let mut role_lines = vec![Line::from(Span::styled("Roles to bind", bold))];
        for (i, role) in roles.iter().enumerate() {
            let bound = self
                .compose
                .bindings
                .get(role)
                .map(|s| format!(" → {}", sanitize_for_terminal(s)))
                .unwrap_or_else(|| " (unbound)".to_string());
            let label = format!("{}{bound}", role_label(*role));
            role_lines.push(focused_line(theme, &label, i == self.compose.role_focus));
        }
        f.render_widget(Paragraph::new(role_lines), left);

        // Seeded entries are not a check: say so instead of "healthy".
        let checked = self.preflight.by_agent.values().any(|h| h.checked);
        let heading = if checked {
            "Healthy agents"
        } else {
            "Agents · health not checked"
        };
        let mut agent_lines = vec![Line::from(Span::styled(heading, bold))];
        if agents.is_empty() {
            agent_lines.push(Line::from(Span::styled(
                "No agents reported healthy.",
                Style::default().fg(theme.accent_warning),
            )));
        }
        for (i, a) in agents.iter().enumerate() {
            let safe = sanitize_for_terminal(a);
            agent_lines.push(focused_line(theme, &safe, i == self.compose.agent_focus));
        }
        f.render_widget(Paragraph::new(agent_lines), right);
    }

    fn draw_compose_save(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let [name_area, tier_area, file_area, bar_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(ARMED_BAR_HEIGHT),
        ])
        .areas(area);

        let error = self.validate_save_name();
        let typing = *self.compose_save_state() != ArmedState::Done;
        let preview = self.compose_preview();
        // The file the write targets, checked on disk (#1286 judge): an
        // existing one is named, and the armed ⏎ ⏎ is its confirm.
        let replaces = typing && self.compose_target_exists();
        let mut block = field_block("Name", typing, error, theme);
        if let (None, true, Ok((path, _))) = (error, replaces, &preview) {
            block = block.title_bottom(Line::from(Span::styled(
                format!(
                    " ! replaces {} ",
                    sanitize_for_terminal(&path.to_string_lossy())
                ),
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        let inner = block.inner(name_area);
        f.render_widget(block, name_area);
        input::render(&self.name_input, f, inner, theme, typing);

        let tier = match self.compose.tier {
            ComposeTier::User => "(•) user   ( ) project",
            ComposeTier::Project => "( ) user   (•) project",
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Save to ", Style::default().fg(theme.text_secondary)),
                Span::styled(tier, Style::default().fg(theme.text_primary)),
                Span::styled("   Tab switches", Style::default().fg(theme.text_muted)),
            ])),
            tier_area,
        );

        let (title, body) = match &preview {
            Ok((path, text)) => (
                format!(
                    "Will write {}",
                    sanitize_for_terminal(&path.to_string_lossy())
                ),
                text.lines()
                    .map(|l| Line::from(sanitize_for_terminal(l)))
                    .collect::<Vec<_>>(),
            ),
            Err(e) => (
                "Will write".to_string(),
                vec![Line::from(Span::styled(
                    sanitize_for_terminal(e),
                    Style::default().fg(theme.accent_error),
                ))],
            ),
        };
        f.render_widget(
            Paragraph::new(body).block(theme.styled_block(&title, false)),
            file_area,
        );

        // A bad name blocks the bar; the field border says why.
        let shown = match (error, self.compose_save_state()) {
            (Some(reason), ArmedState::Ready) => {
                let mut blocked = ArmedEnter::new();
                blocked.block(reason);
                blocked
            }
            _ => self.compose.save.clone(),
        };
        let what = match &preview {
            Ok((path, _)) => format!(
                "{} {}",
                if replaces { "replaces" } else { "writes" },
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ),
            Err(_) => "writes nothing yet".to_string(),
        };
        ArmedBar::new(&shown, &what, self.now())
            .to("disk")
            .render(f, bar_area, theme);
    }
}
