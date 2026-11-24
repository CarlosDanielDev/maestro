use super::ReleaseNotesScreen;
use crate::tui::markdown::render_markdown;
use crate::tui::theme::Theme;
use ratatui::{Frame, layout::Rect, style::Style, widgets::Paragraph};

pub fn draw_release_notes(
    screen: &mut ReleaseNotesScreen,
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
) {
    let block = theme
        .styled_block("Release Notes", false)
        .border_style(Style::default().fg(theme.accent_info));
    let inner = block.inner(area);

    // Re-wrap on resize (§02.24 audit #10): the cache holds one width.
    if screen.cached_content.is_none() || screen.cached_width != inner.width {
        let content = render_markdown(screen.source, theme, inner.width);
        screen.total_lines = content.lines.len() as u16;
        screen.cached_content = Some(content);
        screen.cached_width = inner.width;
        screen.scroll_offset = screen
            .scroll_offset
            .min(screen.total_lines.saturating_sub(1));
    }

    let paragraph = if let Some(ref content) = screen.cached_content {
        Paragraph::new(content.clone())
            .scroll((screen.scroll_offset, 0))
            .block(block)
    } else {
        Paragraph::new("Loading...").block(block)
    };

    f.render_widget(paragraph, area);
}

/// Footer hints (#1258).
pub(super) fn footer_hints() -> Vec<(&'static str, &'static str)> {
    vec![
        ("j/k", "Scroll"),
        ("PgDn/PgUp", "Page"),
        ("Home/End", "Jump"),
        ("Esc", "Back"),
    ]
}
