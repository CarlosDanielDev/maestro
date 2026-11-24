use crate::session::types::Session;
use crate::state::progress::ProgressTracker;
use crate::tui::markdown::render_markdown;
use crate::tui::screens::session::output::OutputScroll;
use crate::tui::spinner;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

/// Draw the Output view of one session (#1288: the session screen draws
/// the header). `scroll` follows the tail until the user scrolls up (#1240).
pub fn draw_fullscreen(
    f: &mut Frame,
    session: &Session,
    progress_tracker: &ProgressTracker,
    area: Rect,
    theme: &Theme,
    spinner_tick: usize,
    scroll: &mut OutputScroll,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // output
            Constraint::Length(3), // footer with stats
        ])
        .split(area);

    draw_session_output(f, session, chunks[0], theme, scroll);
    draw_session_footer(f, session, progress_tracker, chunks[1], theme, spinner_tick);
}

fn draw_session_output(
    f: &mut Frame,
    session: &Session,
    area: Rect,
    theme: &Theme,
    scroll: &mut OutputScroll,
) {
    let inner_width = area.width.saturating_sub(2);
    let md_text = if session.last_message.is_empty() {
        ratatui::text::Text::raw("Waiting for output...")
    } else {
        render_markdown(&session.last_message, theme, inner_width)
    };

    let line_count = md_text.lines.len() as u16;
    let inner_height = area.height.saturating_sub(2);
    let tail = line_count.saturating_sub(inner_height);
    let scroll_offset = scroll.offset(tail);
    let title = if scroll.follow {
        "Agent Output · following".to_string()
    } else {
        format!(
            "Agent Output · {} rows below · End follows",
            tail - scroll_offset
        )
    };

    let paragraph = Paragraph::new(md_text)
        .block(theme.styled_block(&title, false))
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
