use crate::tui::app::{App, TuiMode};
use crate::tui::cost_dashboard;
use crate::tui::dep_graph;
use crate::tui::detail;
use crate::tui::fullscreen;
use crate::tui::help;
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
            Constraint::Min(10),    // main content
            Constraint::Length(10), // activity log
            Constraint::Length(1),  // help bar
        ])
        .split(f.area());

    draw_status_bar(f, app, chunks[0]);

    // Render main content based on TUI mode
    match app.tui_mode {
        TuiMode::Overview => {
            let sessions = app.pool.all_sessions();
            app.panel_view.draw(f, &sessions, chunks[1]);
        }
        TuiMode::Detail(idx) => {
            let sessions = app.pool.all_sessions();
            if let Some(session) = sessions.get(idx) {
                detail::draw_detail(f, session, &app.progress_tracker, chunks[1]);
            } else {
                app.panel_view.draw(f, &sessions, chunks[1]);
            }
        }
        TuiMode::DependencyGraph => {
            dep_graph::draw_dep_graph(f, app.work_assigner.as_ref(), chunks[1]);
        }
        TuiMode::Fullscreen(idx) => {
            let sessions = app.pool.all_sessions();
            if let Some(session) = sessions.get(idx) {
                fullscreen::draw_fullscreen(f, session, &app.progress_tracker, chunks[1]);
            } else {
                app.panel_view.draw(f, &sessions, chunks[1]);
            }
        }
        TuiMode::CostDashboard => {
            let sessions = app.pool.all_sessions();
            let budget_limit = app.budget_enforcer.as_ref().map(|e| e.total_limit());
            cost_dashboard::draw_cost_dashboard(
                f,
                &sessions,
                app.total_cost,
                budget_limit,
                chunks[1],
            );
        }
    }

    // Delegate to activity log widget
    app.activity_log.draw(f, chunks[2]);

    // Draw notification banner overlay if any
    let banners = app.notifications.active_banners();
    if !banners.is_empty() {
        draw_notification_banner(f, banners[0], chunks[2]);
    }

    draw_help_bar(f, app, chunks[3]);

    // Draw help overlay on top of everything if active
    if app.show_help {
        help::draw_help_overlay(f, f.area());
    }
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

    let budget_display = match &app.budget_enforcer {
        Some(enforcer) => format!(" ${:.2}/${:.2} ", app.total_cost, enforcer.total_limit()),
        None => format!(" ${:.2} spent ", app.total_cost),
    };

    let budget_color = match &app.budget_enforcer {
        Some(enforcer) => {
            let pct = if enforcer.total_limit() > 0.0 {
                ((app.total_cost / enforcer.total_limit()) * 100.0) as u8
            } else {
                0
            };
            if pct >= 90 { Color::Red } else { Color::Yellow }
        }
        None => Color::Yellow,
    };

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
        Span::styled(budget_display, Style::default().fg(budget_color)),
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

fn draw_notification_banner(
    f: &mut Frame,
    notification: &crate::notifications::types::Notification,
    area: Rect,
) {
    let color = match notification.level {
        crate::notifications::types::InterruptLevel::Critical => Color::Red,
        crate::notifications::types::InterruptLevel::Blocker => Color::LightRed,
        _ => Color::Yellow,
    };

    let banner = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {} ", notification.level.label()),
            Style::default()
                .fg(Color::Black)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            &notification.title,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(": "),
        Span::styled(&notification.message, Style::default().fg(Color::White)),
        Span::styled("  [d]ismiss", Style::default().fg(Color::DarkGray)),
    ]));
    f.render_widget(banner, area);
}

fn draw_help_bar(f: &mut Frame, app: &App, area: Rect) {
    let mode_label = match app.tui_mode {
        TuiMode::Overview => "Overview",
        TuiMode::Detail(_) => "Detail",
        TuiMode::DependencyGraph => "Dependencies",
        TuiMode::Fullscreen(_) => "Fullscreen",
        TuiMode::CostDashboard => "Costs",
    };

    let help = Line::from(vec![
        Span::styled(
            format!(" {} ", mode_label),
            Style::default().fg(Color::Black).bg(Color::DarkGray),
        ),
        Span::raw(" "),
        Span::styled("[q]", Style::default().fg(Color::Yellow)),
        Span::raw("uit "),
        Span::styled("[Tab]", Style::default().fg(Color::Yellow)),
        Span::raw("mode "),
        Span::styled("[f]", Style::default().fg(Color::Yellow)),
        Span::raw("ull "),
        Span::styled("[$]", Style::default().fg(Color::Yellow)),
        Span::raw("cost "),
        Span::styled("[?]", Style::default().fg(Color::Yellow)),
        Span::raw("help "),
        Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
        Span::raw("back "),
        Span::styled("[p]", Style::default().fg(Color::Yellow)),
        Span::raw("ause "),
        Span::styled("[k]", Style::default().fg(Color::Yellow)),
        Span::raw("ill "),
        Span::styled("[↑↓]", Style::default().fg(Color::Yellow)),
        Span::raw("scroll"),
    ]);
    f.render_widget(Paragraph::new(help), area);
}
