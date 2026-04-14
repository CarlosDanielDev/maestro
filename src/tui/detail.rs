use crate::session::types::Session;
use crate::state::file_claims::FileClaimManager;
use crate::state::progress::ProgressTracker;
use crate::tui::icons::{self, IconId};
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

/// Render a detail view for a single session.
#[allow(dead_code)]
pub fn draw_detail(
    f: &mut Frame,
    session: &Session,
    progress_tracker: &ProgressTracker,
    area: Rect,
    theme: &Theme,
) {
    draw_detail_with_claims(f, session, progress_tracker, None, area, theme);
}

/// Render a detail view with conflict log from file claims.
pub fn draw_detail_with_claims(
    f: &mut Frame,
    session: &Session,
    progress_tracker: &ProgressTracker,
    file_claims: Option<&FileClaimManager>,
    area: Rect,
    theme: &Theme,
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

    draw_detail_header(f, session, progress_tracker, chunks[0], theme);
    draw_detail_activity(f, session, chunks[1], theme);

    if has_conflicts {
        draw_conflict_log(f, &conflicts, chunks[2], theme);
        draw_detail_files(f, session, chunks[3], theme);
    } else {
        draw_detail_files(f, session, chunks[2], theme);
    }
}

fn draw_conflict_log(
    f: &mut Frame,
    conflicts: &[&crate::state::file_claims::ConflictRecord],
    area: Rect,
    theme: &Theme,
) {
    let lines: Vec<Line> = conflicts
        .iter()
        .rev()
        .take(area.height.saturating_sub(2) as usize)
        .map(|c| {
            Line::from(vec![
                Span::styled(
                    format!(" {} ", c.detected_at.format("%H:%M:%S")),
                    Style::default().fg(theme.text_secondary),
                ),
                Span::styled("CONFLICT ", Style::default().fg(theme.accent_error)),
                Span::styled(&c.file_path, Style::default().fg(theme.text_primary)),
                Span::styled(
                    format!(
                        " (owner: S-{}, offender: S-{})",
                        &c.owner_session_id.to_string()[..8],
                        &c.offender_session_id.to_string()[..8]
                    ),
                    Style::default().fg(theme.text_secondary),
                ),
            ])
        })
        .collect();

    let title = format!("Conflicts ({})", conflicts.len());
    let block = theme
        .styled_block(&title, false)
        .border_style(Style::default().fg(theme.accent_error));

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn draw_detail_header(
    f: &mut Frame,
    session: &Session,
    progress_tracker: &ProgressTracker,
    area: Rect,
    theme: &Theme,
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

    let mut detail_spans = vec![
        Span::styled(
            format!(" {} ", session.status.label()),
            Style::default().fg(theme.accent_warning),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Phase: {}", phase_label),
            Style::default().fg(theme.accent_info),
        ),
        Span::raw("  "),
        Span::styled(
            format!("${:.2}", session.cost_usd),
            Style::default().fg(theme.accent_warning),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} tools", tools_count),
            Style::default().fg(theme.text_primary),
        ),
        Span::raw("  "),
        Span::styled(
            session.elapsed_display(),
            Style::default().fg(theme.text_primary),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Retries: {}", session.retry_count),
            Style::default().fg(if session.retry_count > 0 {
                theme.accent_error
            } else {
                theme.text_secondary
            }),
        ),
    ];
    if session.is_hollow_completion {
        detail_spans.push(Span::raw("  "));
        detail_spans.push(Span::styled(
            format!("{} HOLLOW COMPLETION", icons::get(IconId::Warning)),
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!(" {} ", label),
                Style::default()
                    .fg(theme.branding_fg)
                    .bg(theme.accent_info)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(title_text, Style::default().fg(theme.text_primary)),
        ]),
        Line::from(detail_spans),
    ];

    let block = theme
        .styled_block("Session Detail", false)
        .border_style(Style::default().fg(theme.accent_info));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn draw_detail_activity(f: &mut Frame, session: &Session, area: Rect, theme: &Theme) {
    let lines: Vec<Line> = session
        .activity_log
        .iter()
        .rev()
        .take(area.height as usize)
        .map(|entry| {
            Line::from(vec![
                Span::styled(
                    format!(" {} ", entry.timestamp.format("%H:%M:%S")),
                    Style::default().fg(theme.text_secondary),
                ),
                Span::raw(&entry.message),
            ])
        })
        .collect();

    let block = theme.styled_block("Activity Log", false);

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn draw_detail_files(f: &mut Frame, session: &Session, area: Rect, theme: &Theme) {
    let files_text = if session.files_touched.is_empty() {
        "No files touched yet".to_string()
    } else {
        session.files_touched.join(", ")
    };

    let title = format!("Files ({})", session.files_touched.len());
    let block = theme.styled_block(&title, false);

    let paragraph = Paragraph::new(files_text)
        .block(block)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}
