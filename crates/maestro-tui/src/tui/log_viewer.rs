use crate::session::logger::SessionLogger;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use uuid::Uuid;

/// Cached log content to avoid reading from disk every frame.
#[derive(Debug, Default)]
pub struct LogViewerCache {
    session_id: Option<Uuid>,
    content: String,
    refresh_counter: usize,
}

/// How many draw cycles between log file re-reads (50ms * 20 = ~1s).
const REFRESH_INTERVAL: usize = 20;

impl LogViewerCache {
    pub fn get_content(&mut self, session_logger: &SessionLogger, session_id: Uuid) -> &str {
        let needs_refresh =
            self.session_id != Some(session_id) || self.refresh_counter >= REFRESH_INTERVAL;

        if needs_refresh {
            self.session_id = Some(session_id);
            self.refresh_counter = 0;
            self.content = session_logger
                .read_log(session_id)
                .unwrap_or_else(|_| "No logs available for this session.".to_string());
        } else {
            self.refresh_counter += 1;
        }

        &self.content
    }

    #[allow(dead_code)] // Reason: public API for cache invalidation on session events
    pub fn invalidate(&mut self) {
        self.session_id = None;
    }
}

pub fn draw_log_viewer(
    f: &mut Frame,
    cache: &mut LogViewerCache,
    session_logger: &SessionLogger,
    session_id: Uuid,
    scroll: u16,
    area: Rect,
    theme: &Theme,
) {
    let short_id = &session_id.to_string()[..8];
    let title = format!(" Session Log: S-{} ", short_id);

    let content = cache.get_content(session_logger, session_id);

    let lines: Vec<Line> = content
        .lines()
        .map(|line| {
            let style = if line.contains("] ERROR:") {
                Style::default().fg(theme.accent_error)
            } else if line.contains("] TOOL:") {
                Style::default().fg(theme.accent_info)
            } else if line.contains("] COMPLETED:") {
                Style::default().fg(theme.accent_success)
            } else if line.contains("] THINKING:") {
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::DIM)
            } else if line.contains("] CONTEXT:") {
                Style::default().fg(theme.accent_warning)
            } else {
                Style::default().fg(theme.text_primary)
            };
            Line::from(Span::styled(line.to_string(), style))
        })
        .collect();

    let block = theme
        .styled_block(&title, true)
        .border_style(Style::default().fg(theme.border_active));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    f.render_widget(paragraph, area);
}
