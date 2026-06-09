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
        // Show the time in the computer's local zone, not UTC — `started_at`
        // is stored as UTC but the header is for the human at the terminal.
        let hhmm = turn
            .started_at
            .with_timezone(&chrono::Local)
            .format("%H:%M")
            .to_string();
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
#[path = "history_tests.rs"]
mod tests;
