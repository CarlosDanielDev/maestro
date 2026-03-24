use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

/// Draw the help overlay popup centered on the screen.
pub fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 70, area);

    // Clear background behind popup
    f.render_widget(Clear, popup);

    let help_text = vec![
        Line::from(vec![Span::styled(
            "Keyboard Shortcuts",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        section_header("Navigation"),
        key_line("Tab", "Cycle views (Overview → Dependencies → Overview)"),
        key_line("Esc", "Return to Overview / Close help"),
        key_line("Enter", "Open detail view for selected session"),
        key_line("1-9", "Jump to session detail by index"),
        key_line("?", "Toggle this help overlay"),
        Line::from(""),
        section_header("Views"),
        key_line("f", "Full-screen view for selected session"),
        key_line("$", "Cost dashboard view"),
        Line::from(""),
        section_header("Session Control"),
        key_line("p", "Pause all running sessions (SIGSTOP)"),
        key_line("r", "Resume all paused sessions (SIGCONT)"),
        key_line("k", "Kill all sessions"),
        key_line("d", "Dismiss notification banner"),
        Line::from(""),
        section_header("Scrolling"),
        key_line("↑/↓", "Scroll agent panel output"),
        key_line("Shift+↑/↓", "Scroll activity log"),
        key_line("Mouse wheel", "Scroll focused panel"),
        Line::from(""),
        section_header("General"),
        key_line("q / Ctrl+c", "Quit maestro"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press ? or Esc to close",
            Style::default().fg(Color::DarkGray),
        )]),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Help ")
                .title_alignment(Alignment::Center),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, popup);
}

fn section_header(title: &str) -> Line<'_> {
    Line::from(vec![Span::styled(
        format!("  {}", title),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )])
}

fn key_line<'a>(key: &'a str, desc: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::raw("    "),
        Span::styled(
            format!("{:<16}", key),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(desc, Style::default().fg(Color::White)),
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
