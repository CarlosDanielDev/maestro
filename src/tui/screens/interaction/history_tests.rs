//! Unit tests for the Interaction history card renderer (#987).
//! Split out of history.rs to keep that file under the 400-line cap.

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

/// True if `header` carries `{role} · HH:MM` (any HH:MM). The header time is
/// rendered in the machine's local zone, so the exact value is not pinned —
/// only the `role · ` separator and a well-formed `HH:MM` are asserted.
fn header_has_role_and_time(header: &str, role: &str) -> bool {
    let Some(rest) = header.split(&format!("{role} · ")).nth(1) else {
        return false;
    };
    let t: Vec<char> = rest.chars().take(5).collect();
    matches!(
        t.as_slice(),
        [a, b, ':', c, d]
            if a.is_ascii_digit() && b.is_ascii_digit()
            && c.is_ascii_digit() && d.is_ascii_digit()
    )
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
    assert!(line_text(&lines[0]).starts_with("╭─ you · "));
    assert!(header_has_role_and_time(&line_text(&lines[0]), "you"));
    assert!(line_text(&lines[0]).ends_with('╮'));
    assert!(line_text(&lines[1]).starts_with("│ "));
    assert!(line_text(&lines[1]).ends_with(" │"));
    assert!(line_text(&lines[2]).starts_with('╰'));
    assert!(line_text(&lines[2]).ends_with('╯'));
    assert_eq!(line_text(&lines[3]).trim(), "");
}

#[test]
fn build_lines_streaming_card_is_closed_box_with_footer() {
    // A streaming card must render a complete box (bottom border present),
    // never an open-bottomed "clipped" box while the agent responds.
    let theme = Theme::dark();
    let history = vec![turn(TurnRole::Agent, "Working on it", true)];
    let lines = build_lines(&history, &theme, 80);
    assert_eq!(
        count_prefix(&lines, "╰"),
        1,
        "streaming card must be a closed box (footer present)"
    );
}

#[test]
fn streaming_header_renders_the_spinner_frame() {
    // The streaming header animates: the trailing indicator is the passed
    // spinner frame, not a static ellipsis.
    let theme = Theme::dark();
    let history = vec![turn(TurnRole::Agent, "hi", true)];
    let lines = build_lines_core(&history, &theme, 80, '⠋', false);
    assert!(
        line_text(&lines[0]).contains('⠋'),
        "streaming header carries the spinner frame"
    );
    assert!(
        !line_text(&lines[0]).contains('…'),
        "spinner replaces the static ellipsis"
    );
}

#[test]
fn streaming_body_shows_typing_cursor_when_enabled() {
    let theme = Theme::dark();
    let history = vec![turn(TurnRole::Agent, "hello", true)];
    let lines = build_lines_core(&history, &theme, 80, '⠋', true);
    assert!(
        line_text(&lines[1]).contains('▌'),
        "streaming body shows a typing cursor, got: {}",
        line_text(&lines[1])
    );
}

#[test]
fn settled_card_has_no_spinner_or_cursor() {
    let theme = Theme::dark();
    let history = vec![turn(TurnRole::Agent, "done", false)];
    let lines = build_lines_core(&history, &theme, 80, '⠋', true);
    assert!(
        !line_text(&lines[0]).contains('⠋'),
        "settled header has no spinner"
    );
    assert!(
        !line_text(&lines[1]).contains('▌'),
        "settled body has no typing cursor"
    );
    assert_eq!(
        count_prefix(&lines, "╰"),
        1,
        "settled card stays a closed box"
    );
}

#[test]
fn build_lines_header_contains_role_and_time() {
    let theme = Theme::dark();
    let history = vec![turn(TurnRole::User, "hello", false)];
    let lines = build_lines(&history, &theme, 80);
    let header = line_text(&lines[0]);
    assert!(header_has_role_and_time(&header, "you"), "header: {header}");
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
            header_has_role_and_time(&header, word),
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
