use super::{FleetBoardScreen, FleetKind, FleetRow, FleetSort};
use crate::tui::screens::adopt_picker::relative_time;
use crate::tui::screens::draw_keybinds_bar;
use crate::tui::theme::Theme;
use crate::util::sanitize::sanitize_log;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

// Every transcript-derived string (title, repo, cwd, status) passes
// `sanitize_log` here, at the render sink: ESC/BEL and the rest of the
// control range become spaces before they reach a terminal cell.

impl FleetBoardScreen {
    pub(super) fn draw_impl(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(1)])
            .split(area);

        let title = match self.sort() {
            FleetSort::ActivityDesc => "Fleet Board (last active first)",
            FleetSort::StaleFirst => "Fleet Board (stale first)",
        };
        let block = theme.styled_block(title, false);
        let inner = block.inner(outer[0]);
        f.render_widget(block, outer[0]);

        if inner.height < 2 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

        self.draw_filter_line(f, chunks[0], theme);

        if self.rows_is_empty() {
            self.draw_empty_state(
                f,
                chunks[1],
                theme,
                "No sessions yet",
                "Pool sessions and Claude Code transcripts under ~/.claude/projects appear here.",
            );
        } else if self.filtered_rows().is_empty() {
            self.draw_empty_state(
                f,
                chunks[1],
                theme,
                &format!("No sessions match '{}'", sanitize_log(&self.filter_text)),
                "Backspace edits the filter; Esc clears it.",
            );
        } else {
            self.draw_rows(f, chunks[1], theme);
        }

        self.draw_detail_line(f, chunks[2], theme);
        self.draw_status_line(f, chunks[3], theme);

        draw_keybinds_bar(
            f,
            outer[1],
            &[
                ("↑/↓", "Navigate"),
                ("Type", "Filter"),
                ("Tab", "Sort"),
                ("Enter", "Open"),
                ("Esc", "Back"),
            ],
            theme,
        );
    }

    fn draw_filter_line(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let text = if self.filter_text.is_empty() {
            "Type to filter…".to_string()
        } else {
            format!("Filter: {}▏", sanitize_log(&self.filter_text))
        };
        let line = Line::from(Span::styled(text, Style::default().fg(theme.text_muted)));
        f.render_widget(Paragraph::new(line), area);
    }

    /// The directory of the selected row. The row shows only the repo name,
    /// so this is what the user checks before acting on it.
    fn draw_detail_line(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let text = match self.selected_row().and_then(|r| r.cwd.as_ref()) {
            Some(cwd) => format!("cwd: {}", sanitize_log(&cwd.display().to_string())),
            None => String::new(),
        };
        let line = Line::from(Span::styled(text, Style::default().fg(theme.text_muted)));
        f.render_widget(Paragraph::new(line), area);
    }

    /// Feedback for the last refused action, on the board itself.
    fn draw_status_line(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let Some(message) = self.status() else {
            return;
        };
        let line = Line::from(Span::styled(
            sanitize_log(message),
            Style::default().fg(theme.accent_warning),
        ));
        f.render_widget(Paragraph::new(line), area);
    }

    fn draw_rows(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.set_visible_height(area.height as usize);

        let lines: Vec<Line> = self
            .visible_window(area.height as usize)
            .into_iter()
            .map(|(i, row)| self.row_line(i, row, theme))
            .collect();
        f.render_widget(Paragraph::new(lines), area);
    }

    fn row_line(&self, index: usize, row: &FleetRow, theme: &Theme) -> Line<'static> {
        let is_selected = index == self.selected_index();
        let prefix = if is_selected { "> " } else { "  " };
        let base = if is_selected {
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };
        let title = if row.title.is_empty() {
            "(untitled)".to_string()
        } else {
            sanitize_log(&row.title)
        };
        let (badge, badge_color) = kind_badge(row.kind, theme);
        let last_active = match row.last_active {
            Some(at) => relative_time(at, self.now()),
            None => "never started".to_string(),
        };

        Line::from(vec![
            Span::styled(prefix.to_string(), base),
            Span::styled(format!("{badge:<9}"), Style::default().fg(badge_color)),
            Span::styled(
                format!("{:<14} ", sanitize_log(&row.repo)),
                Style::default().fg(theme.accent_success),
            ),
            Span::styled(title, base),
            Span::styled(
                format!("  {}", sanitize_log(&row.status)),
                Style::default().fg(theme.text_muted),
            ),
            Span::styled(
                format!("  {last_active}"),
                Style::default().fg(theme.text_muted),
            ),
        ])
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
}

fn kind_badge(kind: FleetKind, theme: &Theme) -> (&'static str, ratatui::style::Color) {
    match kind {
        FleetKind::Managed => ("MANAGED", theme.accent_success),
        FleetKind::External => ("EXTERNAL", theme.accent_warning),
        FleetKind::Adopted => ("ADOPTED", theme.text_primary),
    }
}
