use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

/// Draw the help overlay popup centered on the screen.
pub fn draw_help_overlay(f: &mut Frame, area: Rect, theme: &Theme) {
    let popup = centered_rect(60, 70, area);

    // Clear background behind popup
    f.render_widget(Clear, popup);

    let help_text = vec![
        Line::from(vec![Span::styled(
            "Keyboard Shortcuts",
            Style::default()
                .fg(theme.accent_info)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        section_header("Navigation", theme),
        key_line(
            "Tab",
            "Cycle views (Overview → Dependencies → Overview)",
            theme,
        ),
        key_line("Esc", "Return to Overview / Close help", theme),
        key_line("Enter", "Open detail view for selected session", theme),
        key_line("1-9", "Jump to session detail by index", theme),
        key_line("?", "Toggle this help overlay", theme),
        Line::from(""),
        section_header("Views", theme),
        key_line("f", "Full-screen view for selected session", theme),
        key_line("$", "Cost dashboard view", theme),
        Line::from(""),
        section_header("Session Control", theme),
        key_line("p", "Pause all running sessions (SIGSTOP)", theme),
        key_line("r", "Resume all paused sessions (SIGCONT)", theme),
        key_line("k", "Kill all sessions", theme),
        key_line("d", "Dismiss notification banner", theme),
        Line::from(""),
        section_header("Scrolling", theme),
        key_line("↑/↓", "Scroll agent panel output", theme),
        key_line("Shift+↑/↓", "Scroll activity log", theme),
        key_line("Mouse wheel", "Scroll focused panel", theme),
        Line::from(""),
        section_header("General", theme),
        key_line("q / Ctrl+c", "Quit maestro", theme),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press ? or Esc to close",
            Style::default().fg(theme.text_secondary),
        )]),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent_info))
                .title(" Help ")
                .title_alignment(Alignment::Center),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, popup);
}

fn section_header<'a>(title: &'a str, theme: &Theme) -> Line<'a> {
    Line::from(vec![Span::styled(
        format!("  {}", title),
        Style::default()
            .fg(theme.accent_warning)
            .add_modifier(Modifier::BOLD),
    )])
}

fn key_line<'a>(key: &'a str, desc: &'a str, theme: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::raw("    "),
        Span::styled(
            format!("{:<16}", key),
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(desc, Style::default().fg(theme.text_primary)),
    ])
}

/// Create a centered rectangle within the given area.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
