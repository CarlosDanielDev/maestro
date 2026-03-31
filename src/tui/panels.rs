use crate::session::types::{Session, SessionStatus};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Wrap},
};

/// Maximum columns to display side-by-side.
const MAX_VISIBLE_COLUMNS: usize = 6;

pub struct PanelView {
    pub selected: Option<usize>,
    /// Scroll offset for the message area in agent panels.
    pub scroll_offset: u16,
}

impl PanelView {
    pub fn new() -> Self {
        Self {
            selected: None,
            scroll_offset: 0,
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn selected_index(&self) -> usize {
        self.selected.unwrap_or(0)
    }

    pub fn draw(&self, f: &mut Frame, sessions: &[&Session], area: Rect) {
        if sessions.is_empty() {
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

        let visible = sessions.len().min(MAX_VISIBLE_COLUMNS);
        let constraints: Vec<Constraint> = (0..visible)
            .map(|_| Constraint::Ratio(1, visible as u32))
            .collect();

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(area);

        for (i, session) in sessions.iter().take(visible).enumerate() {
            let is_selected = self.selected == Some(i);
            draw_single_panel(f, session, columns[i], is_selected, self.scroll_offset);
        }
    }
}

fn draw_single_panel(f: &mut Frame, session: &Session, area: Rect, is_selected: bool, scroll: u16) {
    let status_color = status_to_color(session.status);

    let fork_indicator = if session.parent_session_id.is_some() {
        format!(" [fork:{}]", session.fork_depth)
    } else {
        String::new()
    };

    let title = match (session.issue_number, &session.issue_title) {
        (Some(n), Some(t)) => {
            let max_title_len = 30;
            let short_title: String = if t.chars().count() > max_title_len {
                let truncated: String = t.chars().take(max_title_len - 1).collect();
                format!("{}…", truncated)
            } else {
                t.clone()
            };
            format!(" #{} — {}{} ", n, short_title, fork_indicator)
        }
        (Some(n), None) => format!(" #{}{} ", n, fork_indicator),
        _ => format!(" {}{} ", &session.id.to_string()[..8], fork_indicator),
    };

    let border_style = if is_selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(status_color)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status + elapsed
            Constraint::Length(1), // cost + files
            Constraint::Length(2), // context gauge
            Constraint::Length(1), // current activity
            Constraint::Min(1),    // last message (scrollable)
        ])
        .split(inner);

    // Status line
    let status_line = Line::from(vec![
        Span::styled(
            format!("{} {} ", session.status.symbol(), session.status.label()),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(session.elapsed_display(), Style::default().fg(Color::White)),
    ]);
    f.render_widget(Paragraph::new(status_line), chunks[0]);

    // Cost + file count
    let cost_line = Line::from(vec![
        Span::styled(
            format!("${:.2}", session.cost_usd),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} files", session.files_touched.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
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
    let gauge_label = if ctx_pct > 70.0 {
        format!("ctx: {:.0}% OVERFLOW", ctx_pct)
    } else {
        format!("ctx: {:.0}%", ctx_pct)
    };
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(gauge_color))
        .label(gauge_label)
        .percent(ctx_pct as u16);
    f.render_widget(gauge, chunks[2]);

    // Current activity
    let activity = Line::from(Span::styled(
        format!("> {}", session.current_activity),
        Style::default().fg(Color::Cyan),
    ));
    f.render_widget(Paragraph::new(activity), chunks[3]);

    // Last message (scrollable)
    let msg = Paragraph::new(session.last_message.clone())
        .style(Style::default().fg(Color::DarkGray))
        .wrap(Wrap { trim: true })
        .scroll((scroll, 0));
    f.render_widget(msg, chunks[4]);
}

fn status_to_color(status: SessionStatus) -> Color {
    match status {
        SessionStatus::Running => Color::Green,
        SessionStatus::Completed => Color::Blue,
        SessionStatus::Errored => Color::Red,
        SessionStatus::Paused => Color::Yellow,
        SessionStatus::Killed => Color::Red,
        SessionStatus::Queued => Color::DarkGray,
        SessionStatus::Spawning => Color::Cyan,
        SessionStatus::Stalled => Color::Yellow,
        SessionStatus::Retrying => Color::Magenta,
    }
}
