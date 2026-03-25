use crate::session::types::Session;
use crate::state::progress::ProgressTracker;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Draw a full-screen view for a single agent session.
pub fn draw_fullscreen(
    f: &mut Frame,
    session: &Session,
    progress_tracker: &ProgressTracker,
    area: Rect,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // session header
            Constraint::Min(10),   // output
            Constraint::Length(3), // footer with stats
        ])
        .split(area);

    draw_session_header(f, session, chunks[0]);
    draw_session_output(f, session, chunks[1]);
    draw_session_footer(f, session, progress_tracker, chunks[2]);
}

fn draw_session_header(f: &mut Frame, session: &Session, area: Rect) {
    let title = match session.issue_number {
        Some(n) => {
            let issue_title = session.issue_title.as_deref().unwrap_or("untitled");
            format!("#{} — {}", n, issue_title)
        }
        None => format!("Session {}", &session.id.to_string()[..8]),
    };

    let status_color = match session.status {
        crate::session::types::SessionStatus::Running => Color::Green,
        crate::session::types::SessionStatus::Completed => Color::Cyan,
        crate::session::types::SessionStatus::Errored => Color::Red,
        crate::session::types::SessionStatus::Paused => Color::Yellow,
        _ => Color::White,
    };

    let header = Line::from(vec![
        Span::styled(
            format!(" {} {} ", session.status.symbol(), session.status.label()),
            Style::default()
                .fg(Color::Black)
                .bg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            &title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("model: {}", session.model),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled(
            format!("mode: {}", session.mode),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(status_color));

    f.render_widget(Paragraph::new(header).block(block), area);
}

fn draw_session_output(f: &mut Frame, session: &Session, area: Rect) {
    let output = if session.last_message.is_empty() {
        "Waiting for output...".to_string()
    } else {
        session.last_message.clone()
    };

    let paragraph = Paragraph::new(output)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Agent Output "),
        )
        .wrap(Wrap { trim: false })
        .scroll((
            // Show the last lines — scroll to bottom
            {
                let inner_height = area.height.saturating_sub(2);
                let line_count = session.last_message.lines().count() as u16;
                line_count.saturating_sub(inner_height)
            },
            0,
        ));

    f.render_widget(paragraph, area);
}

fn draw_session_footer(
    f: &mut Frame,
    session: &Session,
    progress_tracker: &ProgressTracker,
    area: Rect,
) {
    let elapsed = session.elapsed_display();
    let files_count = session.files_touched.len();

    let phase = progress_tracker
        .get(&session.id)
        .map(|p| p.phase.label().to_string())
        .unwrap_or_else(|| "—".into());

    let footer = Line::from(vec![
        Span::styled(" Cost: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("${:.2}", session.cost_usd),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled(" Elapsed: ", Style::default().fg(Color::DarkGray)),
        Span::styled(elapsed, Style::default().fg(Color::White)),
        Span::raw("  "),
        Span::styled(" Files: ", Style::default().fg(Color::DarkGray)),
        Span::styled(files_count.to_string(), Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled(" Phase: ", Style::default().fg(Color::DarkGray)),
        Span::styled(phase, Style::default().fg(Color::Green)),
        Span::raw("  "),
        Span::styled(" Activity: ", Style::default().fg(Color::DarkGray)),
        Span::styled(&session.current_activity, Style::default().fg(Color::White)),
        Span::raw("    "),
        Span::styled(
            "[Esc] back  [↑↓] scroll",
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    f.render_widget(Paragraph::new(footer).block(block), area);
}
