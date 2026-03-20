use crate::tui::app::App;
use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// Render the entire TUI.
pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // status bar
            Constraint::Min(10),    // agent panels
            Constraint::Length(10), // activity log
            Constraint::Length(1),  // help bar
        ])
        .split(f.area());

    draw_status_bar(f, app, chunks[0]);

    // Delegate to panels widget
    let sessions = app.pool.all_sessions();
    app.panel_view.draw(f, &sessions, chunks[1]);

    // Delegate to activity log widget
    app.activity_log.draw(f, chunks[2]);

    draw_help_bar(f, chunks[3]);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let elapsed = Utc::now() - app.start_time;
    let elapsed_str = format!(
        "{:02}:{:02}:{:02}",
        elapsed.num_hours(),
        elapsed.num_minutes() % 60,
        elapsed.num_seconds() % 60
    );

    let active = app.active_count();
    let total = app.pool.total_count();

    let text = Line::from(vec![
        Span::styled(
            " MAESTRO v0.1.0 ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                " {} agent{} ({} active) ",
                total,
                if total != 1 { "s" } else { "" },
                active
            ),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("  "),
        Span::styled(
            format!(" ${:.2} spent ", app.total_cost),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled(
            format!(" {} ", elapsed_str),
            Style::default().fg(Color::White),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title_alignment(ratatui::layout::Alignment::Center);

    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

fn draw_help_bar(f: &mut Frame, area: Rect) {
    let help = Line::from(vec![
        Span::styled(" [q]", Style::default().fg(Color::Yellow)),
        Span::raw("uit "),
        Span::styled("[p]", Style::default().fg(Color::Yellow)),
        Span::raw("ause "),
        Span::styled("[k]", Style::default().fg(Color::Yellow)),
        Span::raw("ill "),
        Span::styled("[↑↓]", Style::default().fg(Color::Yellow)),
        Span::raw("scroll panel "),
        Span::styled("[S-↑↓]", Style::default().fg(Color::Yellow)),
        Span::raw("scroll log"),
    ]);
    f.render_widget(Paragraph::new(help), area);
}
