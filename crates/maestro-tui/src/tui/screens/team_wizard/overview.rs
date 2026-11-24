//! Teams overview (#1286, `docs/design-system/index.html` §02.16 "Entry"):
//! the wizard's Home. Every preset grouped BUILT-IN · USER · PROJECT with
//! its shape and readiness, and the selected preset's `preset_card` beside
//! the list. `c` / `l` / `m` start Compose / Launch / Manage.

use super::TeamWizardScreen;
use crate::orchestration::team::{ResolvedTeam, SourceTier};
use crate::tui::ds::checklist::CheckState;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::preset_card::{draw_preset_card, readiness, readiness_glyph, tier_word};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use std::collections::HashMap;

/// Tiers in list order, each with where its files live.
const TIERS: [(SourceTier, &str); 3] = [
    (SourceTier::BuiltIn, "shipped with maestro, read-only"),
    (SourceTier::User, "your config dir, every repo"),
    (SourceTier::Project, ".maestro/teams in this repo"),
];

/// Card width beside the list; below `SIDE_BY_SIDE` columns it stacks.
const CARD_WIDTH: u16 = 64;
const SIDE_BY_SIDE: u16 = 100;

/// Presets in overview order: by tier (built-in, user, project), then name.
pub(super) fn ordered_presets(teams: &HashMap<String, ResolvedTeam>) -> Vec<&ResolvedTeam> {
    let rank = |t: SourceTier| TIERS.iter().position(|(tier, _)| *tier == t);
    let mut out: Vec<&ResolvedTeam> = teams.values().collect();
    out.sort_by(|a, b| (rank(a.source_tier), &a.name).cmp(&(rank(b.source_tier), &b.name)));
    out
}

pub(super) fn footer_hints() -> Vec<(&'static str, &'static str)> {
    vec![
        ("j/k", "select"),
        ("c", "compose"),
        ("l", "launch"),
        ("m", "manage"),
        ("Esc", "back"),
    ]
}

fn state_colour(state: CheckState, theme: &Theme) -> ratatui::style::Color {
    match state {
        CheckState::Ok => theme.accent_success,
        CheckState::Missing => theme.accent_error,
        CheckState::Warn => theme.accent_warning,
        CheckState::Pending => theme.text_muted,
    }
}

impl TeamWizardScreen {
    pub(super) fn draw_overview(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let presets = ordered_presets(&self.resolved_teams);
        let selected = presets.get(self.home_focus).copied();
        let (list_area, card_area) = if area.width >= SIDE_BY_SIDE {
            let [l, c] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(CARD_WIDTH)])
                .areas(area);
            (l, c)
        } else {
            let [l, c] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(12)]).areas(area);
            (l, c)
        };
        let lines = self.overview_lines(&presets, theme);
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(theme.styled_block("Teams", true)),
            list_area,
        );
        match selected {
            Some(team) => draw_preset_card(f, card_area, theme, team, &self.preflight, false),
            None => f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "No presets loaded. Press c to compose one.",
                    Style::default().fg(theme.accent_warning),
                )))
                .block(theme.styled_block("Preset", false)),
                card_area,
            ),
        }
    }

    fn overview_lines(&self, presets: &[&ResolvedTeam], theme: &Theme) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let mut index = 0;
        for (tier, note) in TIERS {
            lines.push(Line::from(vec![
                Span::styled(
                    tier_word(tier),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" · {note}"), Style::default().fg(theme.text_muted)),
            ]));
            let in_tier: Vec<&&ResolvedTeam> =
                presets.iter().filter(|t| t.source_tier == tier).collect();
            if in_tier.is_empty() {
                lines.push(Line::from(Span::styled(
                    "   (none)",
                    Style::default().fg(theme.text_muted),
                )));
            }
            for team in in_tier {
                lines.push(self.preset_row(team, index == self.home_focus, theme));
                index += 1;
            }
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(
            "TIERS  project › user › built-in: on a name clash the left one wins",
            Style::default().fg(theme.text_secondary),
        )));
        lines
    }

    fn preset_row(&self, team: &ResolvedTeam, selected: bool, theme: &Theme) -> Line<'static> {
        let (state, note) = readiness(team, &self.preflight);
        let marker = if selected { " > " } else { "   " };
        let name_style = if selected {
            Style::default()
                .fg(theme.selection_fg)
                .bg(theme.selection_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };
        Line::from(vec![
            Span::styled(marker, name_style),
            Span::styled(
                format!("{:<24}", sanitize_for_terminal(&team.name)),
                name_style,
            ),
            Span::styled(
                format!(" {:<13}", team.primitive.label()),
                Style::default().fg(theme.accent_identifier),
            ),
            Span::styled(
                format!("{} {note}", readiness_glyph(state)),
                Style::default().fg(state_colour(state, theme)),
            ),
        ])
    }
}
