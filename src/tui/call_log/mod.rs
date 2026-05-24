//! Per-agent call-log viewer (issue #868).
//!
//! Renders the persisted [`Session::call_log`] as a chronological list of
//! parsed stream events with an optional expanded payload pane. Reached
//! from `TuiMode::Detail(id)` via the `L` chord.

pub mod draw;
pub mod state;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::session::types::{CallLogEntry, CallLogKind};
use crate::tui::theme::Theme;

/// Width budget for the right-hand preview column in `format_row`. Picked so
/// timestamp (8) + spaces (4) + kind-label (12) + preview (80) ≤ 104 and the
/// row fits the 120-cell snapshot terminal with margin.
pub const PREVIEW_MAX: usize = 80;

/// Width-stable label column for `format_row`. Equal to the longest kind
/// label (`TokenUpdate` = 11) + 1 for trailing space alignment.
pub const KIND_LABEL_WIDTH: usize = 12;

/// Strip control characters (`\n`, `\r`, `\t`, ANSI escape sequences) and
/// truncate at a UTF-8 boundary. Appends `…` if the input had to be cut.
///
/// `max` is measured in `char`s (not bytes) so multi-byte input behaves the
/// way a user expects (`"日本語"` truncated to 2 → `"日本…"`).
pub fn truncate_preview(s: &str, max: usize) -> String {
    let stripped = strip_control_chars(s);
    let char_count = stripped.chars().count();
    if char_count <= max {
        return stripped;
    }
    let mut out: String = stripped.chars().take(max).collect();
    out.push('…');
    out
}

/// Drop newlines, tabs, carriage returns, and ANSI escape sequences from
/// `s`. Preserves all other characters (including multi-byte UTF-8).
fn strip_control_chars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;
    for ch in s.chars() {
        if in_escape {
            // Escape sequences end at a letter (CSI dispatch byte) or `\x1b`-only.
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
            continue;
        }
        match ch {
            '\x1b' => in_escape = true,
            '\n' | '\r' | '\t' => {}
            _ => out.push(ch),
        }
    }
    out
}

/// Render one row of the call-log list as a styled `Line` (timestamp, kind
/// label, truncated payload preview). `selected` paints the row with the
/// theme selection background so the cursor is obvious.
pub fn format_row<'a>(entry: &'a CallLogEntry, selected: bool, theme: &Theme) -> Line<'a> {
    let ts = entry.timestamp.format("%H:%M:%S").to_string();
    let label = entry.kind.label();
    let label_padded = pad_label(label, KIND_LABEL_WIDTH);
    let preview = truncate_preview(&entry.payload_json, PREVIEW_MAX);

    let kind_color = match entry.kind {
        CallLogKind::Error | CallLogKind::Warning => theme.accent_error,
        CallLogKind::ToolUse | CallLogKind::ToolResult => theme.accent_info,
        _ => theme.text_primary,
    };

    let (row_fg, row_bg) = if selected {
        (theme.selection_fg, theme.selection_bg)
    } else {
        (theme.text_primary, Color::Reset)
    };
    let base = Style::default().fg(row_fg).bg(row_bg);
    let kind_style = if selected {
        Style::default()
            .fg(theme.selection_fg)
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(kind_color)
    };

    Line::from(vec![
        Span::styled(ts, base),
        Span::styled("  ", base),
        Span::styled(label_padded, kind_style),
        Span::styled("  ", base),
        Span::styled(preview, base),
    ])
}

fn pad_label(label: &str, width: usize) -> String {
    let len = label.chars().count();
    if len >= width {
        label.chars().take(width).collect()
    } else {
        let mut out = String::with_capacity(width);
        out.push_str(label);
        for _ in 0..(width - len) {
            out.push(' ');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{StreamEvent, TokenUsage, render_event_payload};
    use chrono::{TimeZone, Utc};

    fn entry(kind: CallLogKind, sec: u32, payload: &str) -> CallLogEntry {
        CallLogEntry {
            timestamp: Utc.with_ymd_and_hms(2026, 5, 23, 10, 42, sec).unwrap(),
            kind,
            payload_json: payload.to_string(),
        }
    }

    // ---- truncate_preview ----

    #[test]
    fn truncate_preview_short_string_returned_unchanged() {
        assert_eq!(truncate_preview("hello", 20), "hello");
    }

    #[test]
    fn truncate_preview_long_string_gets_ellipsis() {
        assert_eq!(truncate_preview("abcdefghij", 5), "abcde…");
    }

    #[test]
    fn truncate_preview_exact_max_len_no_ellipsis() {
        assert_eq!(truncate_preview("abcde", 5), "abcde");
    }

    #[test]
    fn truncate_preview_strips_newlines_and_carriage_returns() {
        let out = truncate_preview("line1\nline2\r\nline3", 40);
        assert!(!out.contains('\n'));
        assert!(!out.contains('\r'));
    }

    #[test]
    fn truncate_preview_strips_tabs() {
        let out = truncate_preview("col1\tcol2\tcol3", 40);
        assert!(!out.contains('\t'));
    }

    #[test]
    fn truncate_preview_strips_ansi_escape_sequences() {
        let out = truncate_preview("\x1b[32mgreen\x1b[0m text", 40);
        assert!(!out.contains('\x1b'));
        assert!(out.contains("green"));
        assert!(out.contains("text"));
    }

    #[test]
    fn truncate_preview_truncates_at_utf8_boundary() {
        assert_eq!(truncate_preview("日本語テスト", 2), "日本…");
    }

    // ---- format_row ----

    #[test]
    fn format_row_contains_kind_label_in_spans() {
        let e = entry(CallLogKind::ToolUse, 5, r#"{"tool":"Read"}"#);
        let theme = Theme::dark();
        let line = format_row(&e, false, &theme);
        let full: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(full.contains(CallLogKind::ToolUse.label()));
    }

    #[test]
    fn format_row_contains_timestamp_hh_mm_ss() {
        let e = entry(CallLogKind::AssistantMessage, 5, "x");
        let theme = Theme::dark();
        let line = format_row(&e, false, &theme);
        let full: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(full.contains("10:42:05"), "got: {full}");
    }

    #[test]
    fn format_row_selected_row_uses_selection_style() {
        let e = entry(CallLogKind::AssistantMessage, 1, "hi");
        let theme = Theme::dark();
        let line = format_row(&e, true, &theme);
        assert!(
            line.spans
                .iter()
                .any(|s| s.style.bg == Some(theme.selection_bg)),
            "selected row must apply selection_bg"
        );
    }

    #[test]
    fn format_row_unselected_row_does_not_use_selection_bg() {
        let e = entry(CallLogKind::Thinking, 2, "x");
        let theme = Theme::dark();
        let line = format_row(&e, false, &theme);
        assert!(
            !line
                .spans
                .iter()
                .any(|s| s.style.bg == Some(theme.selection_bg))
        );
    }

    #[test]
    fn format_row_error_kind_renders_with_accent_error_when_unselected() {
        let e = entry(CallLogKind::Error, 0, r#"{"message":"x"}"#);
        let theme = Theme::dark();
        let line = format_row(&e, false, &theme);
        assert!(
            line.spans
                .iter()
                .any(|s| s.style.fg == Some(theme.accent_error))
        );
    }

    // ---- render_event_payload (re-exported from session::types) ----

    #[test]
    fn render_event_payload_assistant_message_contains_text() {
        let event = StreamEvent::AssistantMessage {
            text: "check this out".into(),
        };
        assert!(render_event_payload(&event).contains("check this out"));
    }

    #[test]
    fn render_event_payload_tool_use_contains_tool_name() {
        let event = StreamEvent::ToolUse {
            tool: "Write".into(),
            file_path: Some("src/lib.rs".into()),
            command_preview: None,
            subagent_name: None,
        };
        assert!(render_event_payload(&event).contains("Write"));
    }

    #[test]
    fn render_event_payload_error_contains_message() {
        let event = StreamEvent::Error {
            message: "context window exceeded".into(),
        };
        assert!(render_event_payload(&event).contains("context window exceeded"));
    }

    #[test]
    fn render_event_payload_token_update_serializes_usage() {
        let event = StreamEvent::TokenUpdate {
            usage: TokenUsage::default(),
        };
        let rendered = render_event_payload(&event);
        assert!(rendered.contains("TokenUpdate"));
    }
}
