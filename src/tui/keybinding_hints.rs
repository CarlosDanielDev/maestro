//! Render the inline keybinding-hint bar with optional per-key dimming.
//!
//! The Overview status bar advertises hints like `[Enter] Detail
//! [c] Copy [d] Log`. When the focused tab can't be copied (no content
//! or session still streaming), the `c` hint is rendered in a muted
//! style so the user sees the option exists but is currently inactive.
//!
//! Pre-fit the hints with [`crate::tui::navigation::keymap::fit_hints_to_width`]
//! and pass the result in.

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};

use crate::tui::theme::Theme;

pub const COPY_HINT_KEY: &str = "c";

pub fn keybinding_hints_spans(
    fitted: &[(&str, &str)],
    copy_enabled: bool,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let dim_style = Style::default()
        .fg(theme.text_muted)
        .add_modifier(Modifier::DIM);
    let key_style = Style::default().fg(theme.accent_success);
    let action_style = Style::default().fg(theme.text_secondary);

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(fitted.len() * 3);
    for (i, (key, action)) in fitted.iter().enumerate() {
        let dim = !copy_enabled && *key == COPY_HINT_KEY;
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            format!("[{}]", key),
            if dim { dim_style } else { key_style },
        ));
        spans.push(Span::styled(
            format!(" {}", action),
            if dim { dim_style } else { action_style },
        ));
    }
    spans
}
