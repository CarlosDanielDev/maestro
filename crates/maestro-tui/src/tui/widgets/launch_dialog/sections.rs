//! SPEND, WHERE and button rows of the launch dialog, and the small style
//! helpers they share with `draw.rs`.

use super::{LaunchDialog, LaunchMode};
use crate::tui::ds::gauge::{bar, gauge_level};
use crate::tui::ds::money::money;
use crate::tui::theme::Theme;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

pub(super) fn spend_lines(d: &LaunchDialog, theme: &Theme) -> Vec<Line<'static>> {
    let mut rows = Vec::new();
    let b = &d.budget;
    let capped = b.total_usd > 0.0;
    let spent = if capped {
        format!(
            "{} of {} spent",
            money(Some(b.spent)),
            money(Some(b.total_usd))
        )
    } else {
        format!("{} spent · no total cap", money(Some(b.spent)))
    };
    rows.push(header("SPEND", "", &spent, theme));
    let n = d.sessions();
    let each = if n > 1 { " each" } else { "" };
    let stop = if b.per_session_usd <= 0.0 {
        "  ·  no per-session cap".to_string()
    } else if n > 1 {
        format!("  ·  stops at {} each", money(Some(b.per_session_usd)))
    } else {
        format!(
            "  ·  stops at {} (per-session cap)",
            money(Some(b.per_session_usd))
        )
    };
    rows.push(Line::from(vec![
        Span::styled(
            if b.model.is_empty() {
                "default model".to_string()
            } else {
                b.model.clone()
            },
            Style::default()
                .fg(theme.accent_info)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  ·  est ≥ {}{each}", money(Some(b.estimate_each()))),
            Style::default().fg(theme.text_primary),
        ),
        Span::styled(stop, muted(theme)),
    ]));
    // No per-session cap: there is no worst case to state.
    if b.per_session_usd <= 0.0 {
        return rows;
    }
    let worst = n as f64 * b.per_session_usd;
    if !capped {
        rows.push(Line::from(vec![
            Span::styled("worst case ", muted(theme)),
            Span::styled(money(Some(worst)), Style::default().fg(theme.text_primary)),
        ]));
        return rows;
    }
    let ratio = (b.spent + worst) / b.total_usd;
    let level = gauge_level(ratio).style(theme);
    let (filled, empty) = bar(ratio, 16, crate::icon_mode::use_nerd_font());
    rows.push(Line::from(vec![
        Span::styled("worst case ", muted(theme)),
        Span::styled(filled, level),
        Span::styled(empty, Style::default().fg(theme.gauge_background)),
        Span::styled(
            format!(" {}", money(Some(worst))),
            level.add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" → {:.0}% of cap", ratio * 100.0), level),
        Span::styled(
            format!("  ·  {} left now", money(Some(b.total_usd - b.spent))),
            muted(theme),
        ),
    ]));
    rows
}

pub(super) fn where_line(d: &LaunchDialog, theme: &Theme) -> Line<'static> {
    let id = Style::default().fg(theme.accent_identifier);
    let numbers: Vec<u64> = d.selected_issues.iter().map(|(n, _)| *n).collect();
    if d.sessions() > 1 {
        let list: Vec<String> = numbers.iter().map(u64::to_string).collect();
        return Line::from(vec![
            Span::styled(
                format!("{} worktrees ", numbers.len()),
                Style::default().fg(theme.text_primary),
            ),
            Span::styled(
                format!(".maestro/worktrees/issue-{{{}}}", list.join(",")),
                id,
            ),
        ]);
    }
    let branch = if d.is_multi() {
        crate::provider::github::pr::unified_branch_name(&numbers)
    } else {
        format!("maestro/issue-{}", numbers[0])
    };
    let slug = branch.trim_start_matches("maestro/").to_string();
    Line::from(vec![
        Span::styled(format!(".maestro/worktrees/{slug}"), id),
        Span::styled("  branch ", muted(theme)),
        Span::styled(branch, id),
    ])
}

pub(super) fn button_line(d: &LaunchDialog, theme: &Theme) -> Line<'static> {
    let esc = [
        Span::styled(
            "   Esc ",
            Style::default()
                .fg(theme.keybind_key)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("cancel", muted(theme)),
    ];
    if let Some(reason) = d.block_reason() {
        let mut spans = vec![
            Span::styled(" ⏎ launch ", muted(theme).add_modifier(Modifier::DIM)),
            Span::styled(
                format!("  ⊘ locked: {reason}"),
                Style::default().fg(theme.accent_error),
            ),
        ];
        spans.extend(esc);
        return Line::from(spans);
    }
    let n = d.sessions();
    let label = if n > 1 {
        format!(" ⏎ Launch {n} ")
    } else {
        " ⏎ Launch ".to_string()
    };
    let note = match d.mode {
        LaunchMode::Autonomous => "starts now · no GitHub writes until its PR",
        LaunchMode::ChatUntilPr | LaunchMode::ChatOpen => "opens the chat screen",
    };
    let mut spans = vec![
        Span::styled(
            label,
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        ),
        Span::styled(format!("  {note}"), muted(theme)),
    ];
    spans.extend(esc);
    Line::from(spans)
}

pub(super) fn header(
    label: &'static str,
    key: &'static str,
    hint: &str,
    theme: &Theme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            label,
            Style::default()
                .fg(theme.text_secondary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            key,
            Style::default()
                .fg(theme.keybind_key)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if key.is_empty() {
                hint.to_string()
            } else {
                format!(" {hint}")
            },
            muted(theme),
        ),
    ])
}

pub(super) fn radio(on: bool, theme: &Theme) -> Span<'static> {
    if on {
        Span::styled(
            "● ",
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("○ ", muted(theme))
    }
}

pub(super) fn choice(text: &str, on: bool, theme: &Theme) -> Span<'static> {
    let style = if on {
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD)
    } else {
        muted(theme)
    };
    Span::styled(text.to_string(), style)
}

pub(super) fn muted(theme: &Theme) -> Style {
    Style::default().fg(theme.text_muted)
}

pub(super) fn warn(theme: &Theme) -> Style {
    Style::default().fg(theme.accent_warning)
}

pub(super) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}
