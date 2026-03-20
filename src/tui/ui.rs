use crate::tui::app::App;
use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
};

/// Render the entire TUI.
pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // status bar
            Constraint::Min(10),   // agent panels
            Constraint::Length(10), // activity log
            Constraint::Length(1), // help bar
        ])
        .split(f.area());

    draw_status_bar(f, app, chunks[0]);
    draw_agent_panels(f, app, chunks[1]);
    draw_activity_log(f, app, chunks[2]);
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
    let total = app.sessions.len();

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
            format!(" {} agent{} ({} active) ", total, if total != 1 { "s" } else { "" }, active),
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

fn draw_agent_panels(f: &mut Frame, app: &App, area: Rect) {
    if app.sessions.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" No sessions ");
        let msg = Paragraph::new("Waiting for sessions to start…")
            .style(Style::default().fg(Color::DarkGray))
            .block(block)
            .wrap(Wrap { trim: true });
        f.render_widget(msg, area);
        return;
    }

    let n = app.sessions.len();
    let constraints: Vec<Constraint> = (0..n)
        .map(|_| Constraint::Ratio(1, n as u32))
        .collect();

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, managed) in app.sessions.iter().enumerate() {
        draw_single_agent(f, &managed.session, columns[i]);
    }
}

fn draw_single_agent(f: &mut Frame, session: &crate::session::types::Session, area: Rect) {
    let status_color = match session.status {
        crate::session::types::SessionStatus::Running => Color::Green,
        crate::session::types::SessionStatus::Completed => Color::Blue,
        crate::session::types::SessionStatus::Errored => Color::Red,
        crate::session::types::SessionStatus::Paused => Color::Yellow,
        crate::session::types::SessionStatus::Killed => Color::Red,
        crate::session::types::SessionStatus::Queued => Color::DarkGray,
        crate::session::types::SessionStatus::Spawning => Color::Cyan,
    };

    let title = match session.issue_number {
        Some(n) => format!(" #{} ", n),
        None => format!(" {} ", &session.id.to_string()[..8]),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(status_color))
        .title(title);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status line
            Constraint::Length(1), // cost
            Constraint::Length(2), // context gauge
            Constraint::Length(1), // current activity
            Constraint::Min(1),   // last message
        ])
        .split(inner);

    // Status line
    let status_line = Line::from(vec![
        Span::styled(
            format!("{} {} ", session.status.symbol(), session.status.label()),
            Style::default().fg(status_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            session.elapsed_display(),
            Style::default().fg(Color::White),
        ),
    ]);
    f.render_widget(Paragraph::new(status_line), chunks[0]);

    // Cost
    let cost_line = Line::from(Span::styled(
        format!("${:.2}", session.cost_usd),
        Style::default().fg(Color::Yellow),
    ));
    f.render_widget(Paragraph::new(cost_line), chunks[1]);

    // Context gauge
    let ctx_pct = (session.context_pct * 100.0).min(100.0);
    let gauge_color = if ctx_pct > 70.0 {
        Color::Red
    } else if ctx_pct > 40.0 {
        Color::Yellow
    } else {
        Color::Green
    };
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(gauge_color))
        .label(format!("ctx: {:.0}%", ctx_pct))
        .percent(ctx_pct as u16);
    f.render_widget(gauge, chunks[2]);

    // Current activity
    let activity = Line::from(Span::styled(
        format!("> {}", session.current_activity),
        Style::default().fg(Color::Cyan),
    ));
    f.render_widget(Paragraph::new(activity), chunks[3]);

    // Last message
    let msg = Paragraph::new(session.last_message.clone())
        .style(Style::default().fg(Color::DarkGray))
        .wrap(Wrap { trim: true });
    f.render_widget(msg, chunks[4]);
}

fn draw_activity_log(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Activity Log ");

    let inner_height = area.height.saturating_sub(2) as usize;
    let start = app.activity_log.len().saturating_sub(inner_height);

    let items: Vec<ListItem> = app.activity_log[start..]
        .iter()
        .map(|entry| {
            let time = entry.timestamp.format("%H:%M:%S");
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{} ", time),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("[{}] ", entry.session_label),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(&entry.message),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_help_bar(f: &mut Frame, area: Rect) {
    let help = Line::from(vec![
        Span::styled(" [q]", Style::default().fg(Color::Yellow)),
        Span::raw("uit "),
        Span::styled("[p]", Style::default().fg(Color::Yellow)),
        Span::raw("ause "),
        Span::styled("[k]", Style::default().fg(Color::Yellow)),
        Span::raw("ill "),
        Span::styled("[r]", Style::default().fg(Color::Yellow)),
        Span::raw("efresh "),
        Span::styled("[?]", Style::default().fg(Color::Yellow)),
        Span::raw("help"),
    ]);
    f.render_widget(Paragraph::new(help), area);
}
