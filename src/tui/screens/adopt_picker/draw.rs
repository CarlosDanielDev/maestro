use super::AdoptPickerScreen;
use crate::tui::help::centered_rect;
use crate::tui::screens::draw_keybinds_bar;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};
use std::time::SystemTime;

impl AdoptPickerScreen {
    pub(super) fn draw_impl(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(1)])
            .split(area);

        let block = theme.styled_block("Adopt Session", false);
        let inner = block.inner(outer[0]);
        f.render_widget(block, outer[0]);

        if inner.height < 2 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        self.draw_filter_line(f, chunks[0], theme);

        if self.sessions.is_empty() {
            self.draw_empty_state(
                f,
                chunks[1],
                theme,
                "No Claude Code sessions found",
                "Transcripts under ~/.claude/projects appear here once Claude Code has run.",
            );
        } else if self.filtered_indices.is_empty() {
            self.draw_empty_state(
                f,
                chunks[1],
                theme,
                &format!("No sessions match '{}'", self.filter_text),
                "Backspace edits the filter; Esc clears it.",
            );
        } else {
            self.draw_rows(f, chunks[1], theme);
        }

        draw_keybinds_bar(
            f,
            outer[1],
            &[
                ("↑/↓", "Navigate"),
                ("Type", "Filter"),
                ("Enter", "Adopt"),
                ("Esc", "Back"),
            ],
            theme,
        );

        if self.confirm_pending() {
            self.draw_confirm_modal(f, area, theme);
        }
    }

    fn draw_filter_line(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let text = if self.filter_text.is_empty() {
            "Type to filter…".to_string()
        } else {
            format!("Filter: {}▏", self.filter_text)
        };
        let line = Line::from(Span::styled(text, Style::default().fg(theme.text_muted)));
        f.render_widget(Paragraph::new(line), area);
    }

    fn draw_rows(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.last_visible_height = area.height as usize;
        self.sync_scroll();

        let lines: Vec<Line> = self
            .filtered_sessions()
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(area.height as usize)
            .map(|(i, session)| self.row_line(i, session, theme))
            .collect();
        f.render_widget(Paragraph::new(lines), area);
    }

    fn row_line(
        &self,
        index: usize,
        session: &crate::session::discovery::DiscoveredSession,
        theme: &Theme,
    ) -> Line<'static> {
        let is_selected = index == self.selected;
        let prefix = if is_selected { "> " } else { "  " };
        let base = if is_selected {
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };
        let title = if session.title.is_empty() {
            "(untitled)".to_string()
        } else {
            session.title.clone()
        };

        let mut spans = vec![
            Span::styled(prefix.to_string(), base),
            Span::styled(
                format!("{} ", super::repo_name(&session.cwd)),
                Style::default().fg(theme.accent_success),
            ),
            Span::styled(title, base),
            Span::styled(
                format!("  {}", relative_time(session.last_active, self.now)),
                Style::default().fg(theme.text_muted),
            ),
        ];
        if self.is_live(session) {
            spans.push(Span::styled(
                "  [LIVE]".to_string(),
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        Line::from(spans)
    }

    fn draw_empty_state(
        &self,
        f: &mut Frame,
        area: Rect,
        theme: &Theme,
        headline: &str,
        hint: &str,
    ) {
        let lines = vec![
            Line::from(Span::styled(
                headline.to_string(),
                Style::default().fg(theme.text_primary),
            )),
            Line::from(Span::styled(
                hint.to_string(),
                Style::default().fg(theme.text_muted),
            )),
        ];
        f.render_widget(Paragraph::new(lines), area);
    }

    fn draw_confirm_modal(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let popup = centered_rect(60, 60, area);
        f.render_widget(Clear, popup);
        let block = theme.styled_block("Session may be live", true);
        let inner = block.inner(popup);
        f.render_widget(block, popup);

        let lines = vec![
            Line::from(Span::styled(
                "This transcript was modified recently.",
                Style::default().fg(theme.text_primary),
            )),
            Line::from(Span::styled(
                "The session may still be running elsewhere.",
                Style::default().fg(theme.text_primary),
            )),
            Line::default(),
            Line::from(Span::styled(
                "Adopt anyway?",
                Style::default().fg(theme.accent_warning),
            )),
            Line::default(),
            Line::from(vec![
                Span::styled("[y]", Style::default().fg(theme.accent_success)),
                Span::raw(" Adopt   "),
                Span::styled("[n/Esc]", Style::default().fg(theme.text_muted)),
                Span::raw(" Cancel"),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), inner);
    }
}

/// Coarse "how long ago" for the last-active column. A future mtime
/// (clock skew) renders as "now".
fn relative_time(last_active: SystemTime, now: SystemTime) -> String {
    let Ok(age) = now.duration_since(last_active) else {
        return "now".to_string();
    };
    let secs = age.as_secs();
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3_600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3_600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}
