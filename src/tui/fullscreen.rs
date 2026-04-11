use crate::session::types::Session;
use crate::state::progress::ProgressTracker;
use crate::tui::markdown::render_markdown;
use crate::tui::spinner;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

/// Draw a full-screen view for a single agent session.
pub fn draw_fullscreen(
    f: &mut Frame,
    session: &Session,
    progress_tracker: &ProgressTracker,
    area: Rect,
    theme: &Theme,
    spinner_tick: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // session header
            Constraint::Min(10),   // output
            Constraint::Length(3), // footer with stats
        ])
        .split(area);

    draw_session_header(f, session, chunks[0], theme);
    draw_session_output(f, session, chunks[1], theme);
    draw_session_footer(f, session, progress_tracker, chunks[2], theme, spinner_tick);
}

fn draw_session_header(f: &mut Frame, session: &Session, area: Rect, theme: &Theme) {
    let title = match session.issue_number {
        Some(n) => {
            let issue_title = session.issue_title.as_deref().unwrap_or("untitled");
            format!("#{} — {}", n, issue_title)
        }
        None => format!("Session {}", &session.id.to_string()[..8]),
    };

    let status_color = theme.status_color(session.status);

    let header = Line::from(vec![
        Span::styled(
            format!(" {} {} ", session.status.symbol(), session.status.label()),
            Style::default()
                .fg(theme.branding_fg)
                .bg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            &title,
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("model: {}", session.model),
            Style::default().fg(theme.text_secondary),
        ),
        Span::raw("  "),
        Span::styled(
            format!("mode: {}", session.mode),
            Style::default().fg(theme.text_secondary),
        ),
    ]);

    let block = theme
        .styled_block_plain(false)
        .border_style(Style::default().fg(status_color));

    f.render_widget(Paragraph::new(header).block(block), area);
}

fn draw_session_output(f: &mut Frame, session: &Session, area: Rect, theme: &Theme) {
    let inner_width = area.width.saturating_sub(2);
    let md_text = if session.last_message.is_empty() {
        ratatui::text::Text::raw("Waiting for output...")
    } else {
        render_markdown(&session.last_message, theme, inner_width)
    };

    let line_count = md_text.lines.len() as u16;
    let inner_height = area.height.saturating_sub(2);
    let scroll_offset = line_count.saturating_sub(inner_height);

    let paragraph = Paragraph::new(md_text)
        .block(theme.styled_block("Agent Output", false))
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    f.render_widget(paragraph, area);
}

fn draw_session_footer(
    f: &mut Frame,
    session: &Session,
    progress_tracker: &ProgressTracker,
    area: Rect,
    theme: &Theme,
    spinner_tick: usize,
) {
    let elapsed = session.elapsed_display();
    let files_count = session.files_touched.len();

    let phase = progress_tracker
        .get(&session.id)
        .map(|p| p.phase.label().to_string())
        .unwrap_or_else(|| "—".into());

    let footer = Line::from(vec![
        Span::styled(" Cost: ", Style::default().fg(theme.text_secondary)),
        Span::styled(
            format!("${:.2}", session.cost_usd),
            Style::default().fg(theme.accent_warning),
        ),
        Span::raw("  "),
        Span::styled(" Elapsed: ", Style::default().fg(theme.text_secondary)),
        Span::styled(elapsed, Style::default().fg(theme.text_primary)),
        Span::raw("  "),
        Span::styled(" Files: ", Style::default().fg(theme.text_secondary)),
        Span::styled(
            files_count.to_string(),
            Style::default().fg(theme.accent_info),
        ),
        Span::raw("  "),
        Span::styled(" Phase: ", Style::default().fg(theme.text_secondary)),
        Span::styled(phase, Style::default().fg(theme.accent_success)),
        Span::raw("  "),
        Span::styled(" Activity: ", Style::default().fg(theme.text_secondary)),
        Span::styled(
            {
                let phase = spinner::animation_phase(
                    session.status,
                    session.is_thinking,
                    &session.current_activity,
                );
                let thinking_elapsed = session.thinking_started_at.map(|t| t.elapsed());
                spinner::animated_activity(
                    phase,
                    spinner_tick,
                    &session.current_activity,
                    thinking_elapsed,
                )
            },
            Style::default().fg(theme.text_primary),
        ),
        Span::raw("    "),
        Span::styled(
            "[Esc] back  [↑↓] scroll",
            Style::default().fg(theme.text_secondary),
        ),
    ]);

    let block = theme.styled_block_plain(false);

    f.render_widget(Paragraph::new(footer).block(block), area);
}
