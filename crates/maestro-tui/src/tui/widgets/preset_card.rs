//! `preset_card` (#1286, `presetCard` in `docs/design-system/index.html`
//! §02.16): one team preset on a card. The title carries the name and a
//! tier chip, the shape sits on the right border. Body: the shape as a
//! flow, one row per role with its agent and health, the L2 row, how a TUI
//! launch runs it today, and the prompt-token estimate.
//! Names come from files on disk, so they pass `sanitize_for_terminal`.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::orchestration::cost::{
    AVG_ISSUE_CONTEXT_TOKENS_PER_PROVIDER, L2_SYSTEM_PROMPT_TOKENS, RECOVERY_BUDGET,
    RECOVERY_TOKENS_PER_ROLE, primary_rate_for_agent,
};
use crate::orchestration::team::{ResolvedTeam, SourceTier};
use crate::orchestration::types::{Primitive, TeamRole};
use crate::tui::ds::checklist::CheckState;
use crate::tui::ds::money::money;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::screens::team_wizard::types::{PreflightCache, role_label, roles_to_bind};
use crate::tui::theme::Theme;

/// The agent L2 always runs on (`min_agents`, spec §3).
const L2_AGENT: &str = "claude";

/// Role prompt size the Launch plan preview also assumes.
const AVG_ROLE_PROMPT_TOKENS: usize = 200;

/// Tier word for the chip: always the word, so it never leans on colour.
pub fn tier_word(tier: SourceTier) -> &'static str {
    match tier {
        SourceTier::BuiltIn => "BUILT-IN",
        SourceTier::User => "USER",
        SourceTier::Project => "PROJECT",
    }
}

/// Health of one agent: `Some(true)` ok, `Some(false)` down (or absent
/// from a real check: fail-closed), `None` not checked. A seeded entry
/// (`AgentHealth::checked == false`) is not a check.
fn agent_health(health: &PreflightCache, agent: &str) -> Option<bool> {
    match health.by_agent.get(agent) {
        Some(h) if h.checked => Some(h.available),
        Some(_) => None,
        None if health.by_agent.values().any(|h| h.checked) => Some(false),
        None => None,
    }
}

/// Can this preset run? `✗ no <role>` when a role it needs is unbound,
/// `! <agent> down` when a bound agent failed a real check, `○ health not
/// checked` while any agent has no real result, else `✓ ready`.
pub fn readiness(team: &ResolvedTeam, health: &PreflightCache) -> (CheckState, String) {
    let roles = roles_to_bind(team.primitive);
    if let Some(role) = roles.iter().find(|r| !team.bindings.contains_key(r)) {
        return (CheckState::Missing, format!("no {}", role_label(*role)));
    }
    let mut agents: Vec<&str> = roles
        .iter()
        .filter_map(|r| team.bindings.get(r).map(|b| b.agent.as_str()))
        .collect();
    agents.push(L2_AGENT);
    if let Some(down) = agents
        .iter()
        .find(|a| agent_health(health, a) == Some(false))
    {
        return (
            CheckState::Warn,
            format!("{} down", sanitize_for_terminal(down)),
        );
    }
    if agents.iter().any(|a| agent_health(health, a).is_none()) {
        return (CheckState::Pending, "health not checked".to_string());
    }
    (CheckState::Ok, "ready".to_string())
}

/// Glyph for a readiness state (same glyphs as `ds::checklist`).
pub fn readiness_glyph(state: CheckState) -> &'static str {
    match state {
        CheckState::Ok => "✓",
        CheckState::Missing => "✗",
        CheckState::Warn => "!",
        CheckState::Pending => "○",
    }
}

/// Draw `team` as a card in `area`.
pub fn draw_preset_card(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    team: &ResolvedTeam,
    health: &PreflightCache,
    focused: bool,
) {
    let title = format!(
        "{} · {}",
        sanitize_for_terminal(&team.name),
        tier_word(team.source_tier)
    );
    let block = theme.styled_block(&title, focused).title_top(
        Line::from(format!(" {} ", team.primitive.label()))
            .right_aligned()
            .style(Style::default().fg(theme.accent_identifier)),
    );
    let mut lines = vec![shape_line(team, health, theme), Line::default()];
    lines.extend(role_rows(team, health, theme));
    lines.push(Line::default());
    lines.push(muted(launch_note(team), theme));
    lines.push(cost_line(team, theme));
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

fn muted(text: String, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(text, Style::default().fg(theme.text_muted)))
}

/// `implementer ─▶ reviewer ─▶ docs`; unbound roles in error, down agents
/// in warning.
fn shape_line(team: &ResolvedTeam, health: &PreflightCache, theme: &Theme) -> Line<'static> {
    let sep = match team.primitive {
        Primitive::FanOut => " ∥ ",
        _ => " ─▶ ",
    };
    let mut spans = Vec::new();
    for (i, role) in team.primitive.dispatched_roles().iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(sep, Style::default().fg(theme.text_muted)));
        }
        let colour = match team.bindings.get(role) {
            None => theme.accent_error,
            Some(b) if agent_health(health, &b.agent) == Some(false) => theme.accent_warning,
            Some(_) => theme.text_primary,
        };
        spans.push(Span::styled(
            role_label(*role),
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        ));
    }
    if team.primitive == Primitive::VerdictOnly {
        spans.push(Span::styled(
            " ─▶ verdict (no code changes)",
            Style::default().fg(theme.text_muted),
        ));
    }
    Line::from(spans)
}

fn role_row(role: &str, agent: &str, note: Span<'static>, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {role:<12} "),
            Style::default().fg(theme.text_secondary),
        ),
        Span::styled(
            format!("{:<10} ", sanitize_for_terminal(agent)),
            Style::default().fg(theme.agent_color(Some(agent))),
        ),
        note,
    ])
}

fn health_note(state: Option<bool>, theme: &Theme) -> Span<'static> {
    match state {
        Some(true) => Span::styled("● ok", Style::default().fg(theme.accent_success)),
        Some(false) => Span::styled(
            "✗ down",
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        ),
        None => Span::styled("? not checked", Style::default().fg(theme.text_muted)),
    }
}

/// One row per role the shape runs, then the roles only the preflight
/// checks, then bound roles the shape never runs (dimmed), then L2.
fn role_rows(team: &ResolvedTeam, health: &PreflightCache, theme: &Theme) -> Vec<Line<'static>> {
    let dispatched = team.primitive.dispatched_roles();
    let needed = roles_to_bind(team.primitive);
    let mut rows = Vec::new();
    for role in &needed {
        let label = role_label(*role);
        let row = match team.bindings.get(role) {
            None => role_row(
                label,
                "—",
                Span::styled(
                    "✗ unbound · launch fails here",
                    Style::default()
                        .fg(theme.accent_error)
                        .add_modifier(Modifier::BOLD),
                ),
                theme,
            ),
            Some(b) if !dispatched.contains(role) => role_row(
                label,
                &b.agent,
                Span::styled(
                    "checked by preflight, not run",
                    Style::default().fg(theme.text_muted),
                ),
                theme,
            ),
            Some(b) => role_row(
                label,
                &b.agent,
                health_note(agent_health(health, &b.agent), theme),
                theme,
            ),
        };
        rows.push(row);
    }
    let mut unused: Vec<(&TeamRole, &str)> = team
        .bindings
        .iter()
        .filter(|(r, _)| !needed.contains(r))
        .map(|(r, b)| (r, b.agent.as_str()))
        .collect();
    unused.sort_by_key(|(r, _)| role_label(**r));
    for (role, agent) in unused {
        rows.push(muted(
            format!(
                " {:<12} {:<10} unused · not run by {}",
                role_label(*role),
                sanitize_for_terminal(agent),
                team.primitive.label()
            ),
            theme,
        ));
    }
    rows.push(role_row(
        "L2",
        L2_AGENT,
        health_note(agent_health(health, L2_AGENT), theme),
        theme,
    ));
    rows
}

/// The TUI launch runs one session per issue on the implementer binding
/// (`team_runner`); the other roles do not run there yet.
fn launch_note(team: &ResolvedTeam) -> String {
    let agent = team
        .bindings
        .get(&TeamRole::Implementer)
        .map(|b| sanitize_for_terminal(&b.agent))
        .unwrap_or_else(|| "unbound".to_string());
    format!("TUI launch: 1 session per issue on implementer ({agent}) · other roles not run yet")
}

/// The prompt-token estimate per issue and whether a priced agent has no
/// rate. Same coefficients as `cost.rs`, but over the roles the shape runs
/// (`dispatched_roles`), not `required_roles`; an unbound role is not priced
/// (the card already says the launch fails there).
pub fn prompt_estimate(team: &ResolvedTeam) -> (f64, bool) {
    let rate = |agent: &str| primary_rate_for_agent(agent);
    let mut usd = L2_SYSTEM_PROMPT_TOKENS as f64 * rate(L2_AGENT).unwrap_or(0.0);
    let mut unpriced = false;
    let roles = team.primitive.dispatched_roles();
    let per_role = AVG_ROLE_PROMPT_TOKENS
        + AVG_ISSUE_CONTEXT_TOKENS_PER_PROVIDER / roles.len().max(1)
        + RECOVERY_TOKENS_PER_ROLE * RECOVERY_BUDGET;
    for role in roles {
        let Some(binding) = team.bindings.get(role) else {
            continue;
        };
        match rate(&binding.agent) {
            Some(r) => usd += per_role as f64 * r,
            None => unpriced = true,
        }
    }
    (usd, unpriced)
}

/// `prompt tokens only ≈ $0.03/issue`, plus `+ $?` when a role the shape
/// runs is bound to an agent with no rate. Never a run estimate (ADR in
/// #1286).
fn cost_line(team: &ResolvedTeam, theme: &Theme) -> Line<'static> {
    let (usd, unpriced) = prompt_estimate(team);
    let extra = if unpriced { " + $?" } else { "" };
    Line::from(vec![
        Span::styled("est ", Style::default().fg(theme.text_secondary)),
        Span::styled(
            format!("prompt tokens only ≈ {}/issue{extra}", money(Some(usd))),
            Style::default().fg(theme.text_primary),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::team_wizard::test_helpers::{make_health_check, make_test_team};
    use crate::tui::screens::team_wizard::{TeamWizardScreen, types::PreflightCache};
    use insta::assert_snapshot;
    use ratatui::{Terminal, backend::TestBackend};

    fn health(checks: &[(&str, bool)]) -> PreflightCache {
        let mut s = TeamWizardScreen::new(Default::default());
        s.apply_health_check(
            checks
                .iter()
                .map(|(a, ok)| make_health_check(a, *ok))
                .collect(),
        );
        s.preflight_cache().clone()
    }

    fn draw(team: &ResolvedTeam, cache: &PreflightCache, w: u16, h: u16) -> Terminal<TestBackend> {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let theme = Theme::dark();
        t.draw(|f| draw_preset_card(f, f.area(), &theme, team, cache, true))
            .unwrap();
        t
    }

    /// Done when: "`preset_card` renders shape, roles and estimate".
    #[test]
    fn preset_card() {
        let team = make_test_team(
            "cheap-coder",
            Primitive::Pipeline,
            &[
                (TeamRole::Implementer, "minimax"),
                (TeamRole::Reviewer, "claude"),
                (TeamRole::Triager, "mystery-llm"),
            ],
            SourceTier::User,
        );
        let cache = health(&[("claude", true), ("minimax", false)]);
        assert_snapshot!(draw(&team, &cache, 80, 14).backend());
    }

    #[test]
    fn readiness_names_the_first_missing_role_then_down_agents() {
        let cache = health(&[("claude", true), ("minimax", false)]);
        let verdict = make_test_team(
            "r",
            Primitive::VerdictOnly,
            &[(TeamRole::Reviewer, "claude")],
            SourceTier::BuiltIn,
        );
        assert_eq!(
            readiness(&verdict, &cache),
            (CheckState::Missing, "no triager".into())
        );
        let down = make_test_team(
            "d",
            Primitive::SinglePass,
            &[(TeamRole::Reviewer, "minimax")],
            SourceTier::User,
        );
        assert_eq!(
            readiness(&down, &cache),
            (CheckState::Warn, "minimax down".into())
        );
        let ok = make_test_team(
            "o",
            Primitive::SinglePass,
            &[(TeamRole::Reviewer, "claude")],
            SourceTier::User,
        );
        assert_eq!(readiness(&ok, &cache), (CheckState::Ok, "ready".into()));
        assert_eq!(
            readiness(&ok, &PreflightCache::default()),
            (CheckState::Pending, "health not checked".into())
        );
    }

    #[test]
    fn card_title_strips_control_characters_from_the_name() {
        let team = make_test_team(
            "evil\x1b[2Jname",
            Primitive::SinglePass,
            &[],
            SourceTier::Project,
        );
        let t = draw(&team, &PreflightCache::default(), 60, 10);
        let top: String = (0..60)
            .map(|x| t.backend().buffer()[(x, 0)].symbol().to_string())
            .collect();
        assert!(top.contains("evil [2Jname · PROJECT"), "{top}");
    }
}
