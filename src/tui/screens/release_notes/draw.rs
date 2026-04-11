use super::ReleaseNotesScreen;
use crate::changelog;
use crate::tui::markdown::render_markdown;
use crate::tui::screens::draw_keybinds_bar;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Paragraph},
};

pub fn draw_release_notes(
    screen: &mut ReleaseNotesScreen,
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let block = Block::default()
        .title(" Release Notes ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_info));
    let inner = block.inner(chunks[0]);

    if screen.cached_content.is_none() {
        let content = render_markdown(changelog::changelog_raw(), theme, inner.width);
        screen.total_lines = content.lines.len() as u16;
        screen.cached_content = Some(content);
    }

    let paragraph = if let Some(ref content) = screen.cached_content {
        Paragraph::new(content.clone())
            .scroll((screen.scroll_offset, 0))
            .block(block)
    } else {
        Paragraph::new("Loading...").block(block)
    };

    f.render_widget(paragraph, chunks[0]);

    let bindings = vec![
        ("j/k", "Scroll"),
        ("PgDn/PgUp", "Page"),
        ("Home/End", "Jump"),
        ("Esc", "Back"),
    ];
    draw_keybinds_bar(f, chunks[1], &bindings, theme);
}
