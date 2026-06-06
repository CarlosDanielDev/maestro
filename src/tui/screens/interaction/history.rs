//! History pane for the Interaction screen (#736, redesigned in #987).
//!
//! Renders `Vec<TurnRecord>` as a scrollable transcript of opencode-style
//! bordered cards. Each turn is a rounded box titled `role · HH:MM` with a
//! role-colored border (user=`accent_info`, agent=`accent_success`,
//! system=`text_secondary`). The body is produced by the shared
//! `tui::markdown::render_markdown` (markdown + syntect-highlighted code),
//! truncated to the inner width so wide code never pushes the right border
//! off-screen. A streaming turn (`finished_at.is_none()`) shows `…` in its
//! header and omits the bottom border until it settles. Rendering is
//! UI-only — no spawn or event logic. Turn content is run through
//! `sanitize_for_terminal` to neutralize control characters (defensive for
//! #737, which feeds raw agent stdout into `content`).

use crate::session::interaction::{TurnRecord, TurnRole};
use crate::tui::markdown::render_markdown;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Smallest card width we will render at. Below this the borders collapse, so
/// clamp to keep the saturating arithmetic well-defined.
const MIN_CARD_WIDTH: usize = 8;

/// Map a turn author to a theme color. Reuses existing tokens so the three
/// roles stay visually distinct without introducing a new palette entry.
pub(super) fn role_color(role: TurnRole, theme: &Theme) -> Color {
    match role {
        TurnRole::User => theme.accent_info,
        TurnRole::Agent => theme.accent_success,
        TurnRole::System => theme.text_secondary,
    }
}

/// Short role word shown in the card header (`role · HH:MM`).
fn role_word(role: TurnRole) -> &'static str {
    match role {
        TurnRole::User => "you",
        TurnRole::Agent => "agent",
        TurnRole::System => "sys",
    }
}

/// Truncate `spans` to at most `max_cols` display columns, preserving each
/// span's style. The boundary span is split on a `char` boundary so style
/// runs are never broken. Returns the kept spans and the columns used.
fn truncate_spans(spans: Vec<Span<'static>>, max_cols: usize) -> (Vec<Span<'static>>, usize) {
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for span in spans {
        if used >= max_cols {
            break;
        }
        let span_cols = span.content.chars().count();
        if used + span_cols <= max_cols {
            used += span_cols;
            out.push(span);
        } else {
            let take = max_cols - used;
            let clipped: String = span.content.chars().take(take).collect();
            used += take;
            out.push(Span::styled(clipped, span.style));
            break;
        }
    }
    (out, used)
}

/// Wrap one body line's styled spans in the `│ … │` gutter, truncating to
/// `inner_width` and right-padding so the closing border lands at exactly
/// `card_width`. Body span styles (syntect colors, bold, code) are untouched —
/// only the gutter glyphs carry the role border color.
fn box_body_line(spans: Vec<Span<'static>>, inner_width: usize, border: Style) -> Line<'static> {
    let (mut truncated, used) = truncate_spans(spans, inner_width);
    let pad = inner_width.saturating_sub(used);
    let mut out: Vec<Span<'static>> = Vec::with_capacity(truncated.len() + 2);
    out.push(Span::styled("│ ".to_string(), border));
    out.append(&mut truncated);
    out.push(Span::styled(format!("{} │", " ".repeat(pad)), border));
    Line::from(out)
}

/// Build the card header: `╭─ {role} · {HH:MM} [ …] ───╮`, padded with `─`
/// to `card_width`. The whole header is one styled span in the role color.
/// A streaming turn carries a trailing `…` after the time.
fn header_line(
    role_word: &str,
    hhmm: &str,
    streaming: bool,
    card_width: usize,
    border: Style,
) -> Line<'static> {
    let mut label = format!("╭─ {role_word} · {hhmm}");
    if streaming {
        label.push_str(" …");
    }
    label.push(' ');
    let label_cols = label.chars().count();
    // +1 reserves the closing ╮ column.
    let fill = card_width.saturating_sub(label_cols + 1);
    label.push_str(&"─".repeat(fill));
    label.push('╮');
    Line::from(Span::styled(label, border))
}

/// Build the card footer `╰───╯` spanning `card_width`. Only emitted for
/// settled turns.
fn footer_line(card_width: usize, border: Style) -> Line<'static> {
    let fill = card_width.saturating_sub(2);
    Line::from(Span::styled(format!("╰{}╯", "─".repeat(fill)), border))
}

/// Build the flat list of visual lines for a transcript as bordered cards.
/// Each turn becomes a header line, one or more `│`-gutter body lines (from
/// `render_markdown`, truncated to the inner width), a footer line when
/// settled, and a blank separator. The flat `Vec<Line>` shape is preserved so
/// the scroll math in [`visual_total`] stays a 1:1 row count.
pub(super) fn build_lines(history: &[TurnRecord], theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let card_width = (width as usize).max(MIN_CARD_WIDTH);
    let inner_width = card_width.saturating_sub(4);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for turn in history {
        let border = Style::default().fg(role_color(turn.role, theme));
        let streaming = turn.finished_at.is_none();
        let hhmm = turn.started_at.format("%H:%M").to_string();
        lines.push(header_line(
            role_word(turn.role),
            &hhmm,
            streaming,
            card_width,
            border,
        ));
        let content = crate::tui::screens::sanitize_for_terminal(&turn.content);
        let body = render_markdown(&content, theme, inner_width as u16);
        if body.lines.is_empty() {
            lines.push(box_body_line(Vec::new(), inner_width, border));
        } else {
            for body_line in body.lines {
                lines.push(box_body_line(body_line.spans, inner_width, border));
            }
        }
        if !streaming {
            lines.push(footer_line(card_width, border));
        }
        lines.push(Line::from(""));
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
    // Cards are pre-boxed to exactly `area.width`, so no soft-wrap is needed
    // (wrapping would corrupt the borders). Scroll vertically by the offset.
    let lines = build_lines(history, theme, area.width);
    let paragraph = Paragraph::new(lines).scroll((offset as u16, 0));
    f.render_widget(paragraph, area);
}

/// Total visual rows the transcript occupies at `width`. Cards are pre-boxed
/// to exactly `width`, so every built line is one visual row — this is a 1:1
/// count of [`build_lines`]. Drives the scroll math (`scroll_offset` and
/// auto-scroll) so the pane pins the true bottom, not stale.
pub(super) fn visual_total(history: &[TurnRecord], theme: &Theme, width: u16) -> usize {
    let w = (width as usize).max(1);
    build_lines(history, theme, width)
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

    /// Collect the plain Unicode text of a line, stripping all style.
    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn count_prefix(lines: &[Line<'_>], prefix: &str) -> usize {
        lines
            .iter()
            .filter(|l| line_text(l).starts_with(prefix))
            .count()
    }

    // -----------------------------------------------------------------------
    // role_color mapping (unchanged behavior)
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Card structure
    // -----------------------------------------------------------------------

    #[test]
    fn build_lines_settled_card_has_header_body_footer_and_blank() {
        let theme = Theme::dark();
        let history = vec![turn(TurnRole::User, "hi", false)];
        let lines = build_lines(&history, &theme, 80);
        assert_eq!(lines.len(), 4, "header + body + footer + blank");
        assert!(line_text(&lines[0]).starts_with("╭─ you · 09:00"));
        assert!(line_text(&lines[0]).ends_with('╮'));
        assert!(line_text(&lines[1]).starts_with("│ "));
        assert!(line_text(&lines[1]).ends_with(" │"));
        assert!(line_text(&lines[2]).starts_with('╰'));
        assert!(line_text(&lines[2]).ends_with('╯'));
        assert_eq!(line_text(&lines[3]).trim(), "");
    }

    #[test]
    fn build_lines_streaming_card_has_no_footer() {
        let theme = Theme::dark();
        let history = vec![turn(TurnRole::Agent, "Working on it", true)];
        let lines = build_lines(&history, &theme, 80);
        assert_eq!(
            count_prefix(&lines, "╰"),
            0,
            "streaming turn must omit footer"
        );
        assert!(
            line_text(&lines[0]).contains('…'),
            "streaming header carries …"
        );
    }

    #[test]
    fn build_lines_header_contains_role_and_time() {
        let theme = Theme::dark();
        let history = vec![turn(TurnRole::User, "hello", false)];
        let lines = build_lines(&history, &theme, 80);
        let header = line_text(&lines[0]);
        assert!(header.contains("you · 09:00"), "header: {header}");
        assert!(header.starts_with("╭─"));
        assert!(header.ends_with('╮'));
    }

    #[test]
    fn build_lines_header_role_words() {
        let theme = Theme::dark();
        for (role, word) in [
            (TurnRole::User, "you"),
            (TurnRole::Agent, "agent"),
            (TurnRole::System, "sys"),
        ] {
            let history = vec![turn(role, "x", false)];
            let lines = build_lines(&history, &theme, 80);
            let header = line_text(&lines[0]);
            assert!(
                header.contains(&format!("{word} · 09:00")),
                "{role:?} header: {header}"
            );
            assert!(!header.contains('▸'), "no legacy arrow glyph");
        }
    }

    #[test]
    fn build_lines_header_span_uses_role_color() {
        let theme = Theme::dark();
        for (role, color) in [
            (TurnRole::User, theme.accent_info),
            (TurnRole::Agent, theme.accent_success),
            (TurnRole::System, theme.text_secondary),
        ] {
            let history = vec![turn(role, "x", false)];
            let lines = build_lines(&history, &theme, 80);
            assert!(
                lines[0].spans.iter().any(|s| s.style.fg == Some(color)),
                "{role:?} header must carry its border color"
            );
        }
    }

    #[test]
    fn build_lines_body_lines_wrapped_in_border_pipes() {
        let theme = Theme::dark();
        let history = vec![turn(TurnRole::Agent, "- alpha\n- beta", false)];
        let lines = build_lines(&history, &theme, 80);
        let body: Vec<_> = lines
            .iter()
            .filter(|l| line_text(l).starts_with("│ "))
            .collect();
        assert!(body.len() >= 2, "multiline markdown body spans >= 2 rows");
        for line in body {
            let t = line_text(line);
            assert!(t.starts_with("│ "), "body row left gutter: {t}");
            assert!(t.ends_with(" │"), "body row right gutter: {t}");
        }
    }

    #[test]
    fn build_lines_body_preserves_markdown_styling() {
        let theme = Theme::dark();
        // Bold markdown must survive into a boxed body span.
        let history = vec![turn(TurnRole::Agent, "**bold**", false)];
        let lines = build_lines(&history, &theme, 80);
        let has_bold = lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
            s.style
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD)
        });
        assert!(has_bold, "render_markdown bold styling must be preserved");
    }

    #[test]
    fn build_lines_empty_content_body_has_one_padded_line() {
        let theme = Theme::dark();
        let history = vec![turn(TurnRole::User, "", false)];
        let lines = build_lines(&history, &theme, 80);
        assert_eq!(lines.len(), 4, "header + 1 empty body + footer + blank");
        assert!(line_text(&lines[1]).starts_with('│'));
        assert!(line_text(&lines[1]).ends_with('│'));
    }

    #[test]
    fn build_lines_content_truncated_to_inner_width() {
        let theme = Theme::dark();
        let width: u16 = 40;
        let history = vec![turn(TurnRole::User, &"x".repeat(120), false)];
        let lines = build_lines(&history, &theme, width);
        for line in &lines {
            assert!(
                line.width() <= width as usize,
                "no line may exceed card width {width}: got {} ({:?})",
                line.width(),
                line_text(line)
            );
        }
    }

    #[test]
    fn build_lines_narrow_width_does_not_panic() {
        let theme = Theme::dark();
        let history = vec![turn(TurnRole::User, "hello world", false)];
        let lines = build_lines(&history, &theme, 40);
        assert!(line_text(&lines[0]).starts_with("╭─"));
        assert!(line_text(&lines[0]).ends_with('╮'));
    }

    #[test]
    fn build_lines_blank_separator_between_two_cards() {
        let theme = Theme::dark();
        let history = vec![
            turn(TurnRole::User, "hi", false),
            turn(TurnRole::Agent, "ok", false),
        ];
        let lines = build_lines(&history, &theme, 80);
        assert_eq!(count_prefix(&lines, "╭─ you"), 1);
        assert_eq!(count_prefix(&lines, "╭─ agent"), 1);
        assert_eq!(count_prefix(&lines, "╰"), 2, "two footers");
        // A blank line sits before the second card's header.
        let agent_idx = lines
            .iter()
            .position(|l| line_text(l).starts_with("╭─ agent"))
            .expect("agent header present");
        assert_eq!(
            line_text(&lines[agent_idx - 1]).trim(),
            "",
            "blank separator precedes the next card"
        );
    }

    #[test]
    fn build_lines_streaming_ellipsis_in_header_not_body() {
        let theme = Theme::dark();
        let history = vec![turn(TurnRole::Agent, "content here", true)];
        let lines = build_lines(&history, &theme, 80);
        assert!(line_text(&lines[0]).contains('…'), "header carries …");
        let body = line_text(&lines[1]);
        assert!(!body.trim_end().ends_with('…'), "body must not end with …");
    }

    #[test]
    fn build_lines_two_single_line_turns_emit_two_cards() {
        let theme = Theme::dark();
        let history = vec![
            turn(TurnRole::User, "hi", false),
            turn(TurnRole::Agent, "ok", false),
        ];
        let lines = build_lines(&history, &theme, 80);
        // Each settled single-line turn => header + body + footer + blank.
        assert_eq!(lines.len(), 8);
        assert!(line_text(&lines[0]).starts_with("╭─ you"));
        assert!(line_text(&lines[4]).starts_with("╭─ agent"));
    }

    #[test]
    fn build_lines_multiline_content_emits_multiple_body_rows() {
        let theme = Theme::dark();
        let history = vec![turn(TurnRole::Agent, "- a\n- b\n- c", false)];
        let lines = build_lines(&history, &theme, 80);
        let body_rows = count_prefix(&lines, "│ ");
        assert!(body_rows >= 3, "three list items => >= 3 body rows");
        assert_eq!(count_prefix(&lines, "╭─"), 1, "single card header");
        assert_eq!(count_prefix(&lines, "╰"), 1, "single card footer");
    }

    #[test]
    fn visual_total_counts_card_rows() {
        let theme = Theme::dark();
        let history = vec![turn(TurnRole::User, "hi", false)];
        let total = visual_total(&history, &theme, 80);
        assert!(total >= 4, "card occupies at least 4 rows: {total}");
    }

    #[test]
    fn visual_total_equals_built_line_count() {
        // Cards are pre-boxed to exactly `width`, so no line wraps: the visual
        // row count is just the number of built lines. This keeps scroll math
        // a straight 1:1 with the flat line vector.
        let theme = Theme::dark();
        let history = vec![
            turn(TurnRole::Agent, &"x".repeat(100), false),
            turn(TurnRole::User, "short", false),
        ];
        for width in [40u16, 80, 200] {
            let total = visual_total(&history, &theme, width);
            let built = build_lines(&history, &theme, width).len();
            assert_eq!(
                total, built,
                "visual_total must match line count at {width}"
            );
        }
    }
}
