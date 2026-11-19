//! Row cell formatting for the Needs-you queue, split out of
//! `attention.rs` to keep it under the 400-line cap (#1266).

use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

/// Pad a span to `width` columns (one space at least).
pub fn pad(span: Span<'static>, width: usize) -> Span<'static> {
    let gap = width.saturating_sub(span.content.width()).max(1);
    Span::styled(format!("{}{}", span.content, " ".repeat(gap)), span.style)
}

/// `45s`, `12m`, `3h`, `2d`.
pub fn age(d: chrono::Duration) -> String {
    let s = d.num_seconds().max(0);
    match s {
        0..60 => format!("{s}s"),
        60..3_600 => format!("{}m", s / 60),
        3_600..86_400 => format!("{}h", s / 3_600),
        _ => format!("{}d", s / 86_400),
    }
}
