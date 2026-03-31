use crate::session::types::Session;
use crate::state::file_claims::FileClaimManager;
use crate::state::progress::ProgressTracker;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Render a detail view for a single session.
#[allow(dead_code)]
pub fn draw_detail(
    f: &mut Frame,
    session: &Session,
    progress_tracker: &ProgressTracker,
    area: Rect,
) {
    draw_detail_with_claims(f, session, progress_tracker, None, area);
}

/// Render a detail view with conflict log from file claims.
pub fn draw_detail_with_claims(
    f: &mut Frame,
    session: &Session,
    progress_tracker: &ProgressTracker,
    file_claims: Option<&FileClaimManager>,
    area: Rect,
) {
    let conflicts = file_claims
        .map(|fc| fc.conflicts_for_session(session.id))
        .unwrap_or_default();
    let has_conflicts = !conflicts.is_empty();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if has_conflicts {
            vec![
                Constraint::Length(5), // header
                Constraint::Min(5),    // activity log
                Constraint::Length(5), // conflicts
                Constraint::Length(5), // files
            ]
        } else {
            vec![
                Constraint::Length(5), // header
                Constraint::Min(5),    // activity log
                Constraint::Length(5), // files
            ]
        })
        .split(area);

    draw_detail_header(f, session, progress_tracker, chunks[0]);
    draw_detail_activity(f, session, chunks[1]);

    if has_conflicts {
        draw_conflict_log(f, &conflicts, chunks[2]);
        draw_detail_files(f, session, chunks[3]);
    } else {
        draw_detail_files(f, session, chunks[2]);
    }
}

fn draw_conflict_log(
    f: &mut Frame,
    conflicts: &[&crate::state::file_claims::ConflictRecord],
    area: Rect,
) {
    let lines: Vec<Line> = conflicts
        .iter()
        .rev()
        .take(area.height.saturating_sub(2) as usize)
        .map(|c| {
            Line::from(vec![
                Span::styled(
                    format!(" {} ", c.detected_at.format("%H:%M:%S")),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled("CONFLICT ", Style::default().fg(Color::Red)),
                Span::styled(&c.file_path, Style::default().fg(Color::White)),
                Span::styled(
                    format!(
                        " (owner: S-{}, offender: S-{})",
                        &c.owner_session_id.to_string()[..8],
                        &c.offender_session_id.to_string()[..8]
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(format!(" Conflicts ({}) ", conflicts.len()));

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn draw_detail_header(
    f: &mut Frame,
    session: &Session,
    progress_tracker: &ProgressTracker,
    area: Rect,
) {
    let phase_label = progress_tracker
        .get(&session.id)
        .map(|p| p.phase.label())
        .unwrap_or("UNKNOWN");

    let tools_count = progress_tracker
        .get(&session.id)
        .map(|p| p.tools_used_count)
        .unwrap_or(0);

    let label = match session.issue_number {
        Some(n) => format!("#{}", n),
        None => format!("S-{}", &session.id.to_string()[..8]),
    };

    let title_text = session
        .issue_title
        .as_deref()
        .unwrap_or(&session.prompt[..session.prompt.len().min(60)]);

    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!(" {} ", label),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(title_text, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                format!(" {} ", session.status.label()),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  "),
            Span::styled(
                format!("Phase: {}", phase_label),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw("  "),
            Span::styled(
                format!("${:.2}", session.cost_usd),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{} tools", tools_count),
                Style::default().fg(Color::White),
            ),
            Span::raw("  "),
            Span::styled(session.elapsed_display(), Style::default().fg(Color::White)),
            Span::raw("  "),
            Span::styled(
                format!("Retries: {}", session.retry_count),
                Style::default().fg(if session.retry_count > 0 {
                    Color::Red
                } else {
                    Color::DarkGray
                }),
            ),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Session Detail ");

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn draw_detail_activity(f: &mut Frame, session: &Session, area: Rect) {
    let lines: Vec<Line> = session
        .activity_log
        .iter()
        .rev()
        .take(area.height as usize)
        .map(|entry| {
            Line::from(vec![
                Span::styled(
                    format!(" {} ", entry.timestamp.format("%H:%M:%S")),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(&entry.message),
            ])
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Activity Log ");

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn draw_detail_files(f: &mut Frame, session: &Session, area: Rect) {
    let files_text = if session.files_touched.is_empty() {
        "No files touched yet".to_string()
    } else {
        session.files_touched.join(", ")
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(format!(" Files ({}) ", session.files_touched.len()));

    let paragraph = Paragraph::new(files_text)
        .block(block)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}
