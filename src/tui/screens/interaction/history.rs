//! History pane for the Interaction screen (#736).
//!
//! Renders `Vec<TurnRecord>` as a scrollable transcript. Each turn gets a
//! role-colored prefix (`you ▸` / `agent ▸` / `sys ▸`). A streaming turn
//! (`finished_at.is_none()`) gets a trailing `…` marker. Rendering is
//! UI-only — no spawn or event logic (those land in #737/#738).

use crate::session::interaction::{TurnRecord, TurnRole};
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

/// Map a turn author to a theme color. Reuses existing tokens so the three
/// roles stay visually distinct without introducing a new palette entry.
pub(super) fn role_color(role: TurnRole, theme: &Theme) -> Color {
    match role {
        TurnRole::User => theme.accent_info,
        TurnRole::Agent => theme.accent_success,
        TurnRole::System => theme.text_secondary,
    }
}

/// Short role prefix shown at the start of a turn's first line.
fn role_prefix(role: TurnRole) -> &'static str {
    match role {
        TurnRole::User => "you   ▸ ",
        TurnRole::Agent => "agent ▸ ",
        TurnRole::System => "sys   ▸ ",
    }
}

/// Build the flat list of visual lines for a transcript. One logical line
/// per `\n` segment; the first segment of each turn carries the colored
/// role prefix, continuation lines are indented to align under it. A
/// streaming turn appends a `…` marker to its last segment. Turn content is
/// run through `sanitize_for_terminal` to neutralize control characters
/// (defensive for #737, which feeds raw agent stdout into `content`).
pub(super) fn build_lines<'a>(history: &'a [TurnRecord], theme: &Theme) -> Vec<Line<'a>> {
    let mut lines: Vec<Line> = Vec::new();
    for turn in history {
        let color = role_color(turn.role, theme);
        let prefix = role_prefix(turn.role);
        let streaming = turn.finished_at.is_none();
        let segments: Vec<&str> = turn.content.split('\n').collect();
        let last = segments.len().saturating_sub(1);
        for (idx, segment) in segments.iter().enumerate() {
            let mut spans: Vec<Span> = Vec::new();
            if idx == 0 {
                spans.push(Span::styled(prefix, Style::default().fg(color)));
            } else {
                spans.push(Span::raw("        "));
            }
            spans.push(Span::styled(
                crate::tui::screens::sanitize_for_terminal(segment),
                Style::default().fg(theme.text_primary),
            ));
            if streaming && idx == last {
                spans.push(Span::styled(" …", Style::default().fg(color)));
            }
            lines.push(Line::from(spans));
        }
    }
    lines
}

/// Render the transcript into `area` at the given vertical scroll offset.
/// When `history` is empty, render an action-oriented empty state instead
/// of a blank pane.
pub(super) fn draw_history(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    history: &[TurnRecord],
    offset: usize,
    issue_number: u64,
    issue_title: &str,
) {
    if history.is_empty() {
        f.render_widget(starter_hint(theme, issue_number, issue_title), area);
        return;
    }
    let lines = build_lines(history, theme);
    // Soft-wrap long turns so agent responses break into chat-style lines
    // instead of running off the right edge. `trim: false` keeps the leading
    // indent on continuation segments.
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((offset as u16, 0));
    f.render_widget(paragraph, area);
}

/// Total visual rows the transcript occupies at `width`, accounting for the
/// soft-wrap in [`draw_history`]. Drives the scroll math (`scroll_offset` and
/// auto-scroll) so the pane pins the true wrapped bottom, not the logical-line
/// bottom — without this, long wrapped turns scroll the newest text out of view.
pub(super) fn visual_total(history: &[TurnRecord], theme: &Theme, width: u16) -> usize {
    let w = (width as usize).max(1);
    build_lines(history, theme)
        .iter()
        .map(|line| line.width().max(1).div_ceil(w))
        .sum()
}

/// Action-oriented empty state: names the work and suggests a first prompt so
/// the user isn't staring at a blank pane (#738 QA — "starter hint").
fn starter_hint<'a>(theme: &Theme, issue_number: u64, issue_title: &'a str) -> Paragraph<'a> {
    let work = if issue_title.is_empty() {
        format!("Working on issue #{issue_number}")
    } else {
        format!("Working on #{issue_number} — {issue_title}")
    };
    let lines = vec![
        Line::from(Span::styled(work, Style::default().fg(theme.text_primary))),
        Line::from(""),
        Line::from(Span::styled(
            "No messages yet — type a prompt below to start.",
            Style::default().fg(theme.text_secondary),
        )),
        Line::from(Span::styled(
            "Try: \"Summarize this issue and propose a step-by-step plan.\"",
            Style::default().fg(theme.accent_info),
        )),
    ];
    Paragraph::new(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::Theme;
    use chrono::{TimeZone, Utc};

    fn turn(role: TurnRole, content: &str, streaming: bool) -> TurnRecord {
        let at = Utc.with_ymd_and_hms(2026, 1, 1, 9, 0, 0).unwrap();
        TurnRecord {
            role,
            content: content.to_string(),
            started_at: at,
            finished_at: if streaming { None } else { Some(at) },
        }
    }

    #[test]
    fn role_color_three_roles_produce_three_distinct_colors() {
        let theme = Theme::dark();
        let user = role_color(TurnRole::User, &theme);
        let agent = role_color(TurnRole::Agent, &theme);
        let system = role_color(TurnRole::System, &theme);
        assert_ne!(user, agent, "User and Agent must differ");
        assert_ne!(user, system, "User and System must differ");
        assert_ne!(agent, system, "Agent and System must differ");
    }

    #[test]
    fn role_color_user_maps_to_accent_info() {
        let theme = Theme::dark();
        assert_eq!(role_color(TurnRole::User, &theme), theme.accent_info);
    }

    #[test]
    fn role_color_agent_maps_to_accent_success() {
        let theme = Theme::dark();
        assert_eq!(role_color(TurnRole::Agent, &theme), theme.accent_success);
    }

    #[test]
    fn role_color_system_maps_to_text_secondary() {
        let theme = Theme::dark();
        assert_eq!(role_color(TurnRole::System, &theme), theme.text_secondary);
    }

    #[test]
    fn build_lines_one_line_per_turn_for_single_line_content() {
        let theme = Theme::dark();
        let history = vec![
            turn(TurnRole::User, "hi", false),
            turn(TurnRole::Agent, "ok", false),
        ];
        let lines = build_lines(&history, &theme);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn build_lines_splits_multiline_content() {
        let theme = Theme::dark();
        let history = vec![turn(TurnRole::Agent, "line1\nline2\nline3", false)];
        let lines = build_lines(&history, &theme);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn visual_total_counts_wrapped_rows() {
        let theme = Theme::dark();
        // One turn whose single line is far wider than a narrow viewport.
        let history = vec![turn(TurnRole::Agent, &"x".repeat(100), false)];
        let narrow = visual_total(&history, &theme, 20);
        let wide = visual_total(&history, &theme, 200);
        assert_eq!(wide, 1, "a 108-col line fits one row at width 200");
        assert!(
            narrow > wide,
            "the same line must wrap into more rows when narrow: {narrow} vs {wide}"
        );
    }

    #[test]
    fn build_lines_empty_content_yields_one_line() {
        let theme = Theme::dark();
        let history = vec![turn(TurnRole::User, "", false)];
        let lines = build_lines(&history, &theme);
        assert_eq!(lines.len(), 1);
    }
}
