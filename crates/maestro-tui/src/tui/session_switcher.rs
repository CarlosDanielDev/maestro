use crate::session::types::{Session, SessionStatus};
use crate::tui::icons::{self, IconId};
use crate::tui::spinner::graph_node_frame;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::tui::theme::Theme;

/// Overlay state for the session switcher. Typing filters (#1288); the
/// selection is a session id, so a session finishing mid-pick cannot shift
/// the target.
#[derive(Default)]
pub struct SessionSwitcher {
    pub selected: Option<uuid::Uuid>,
    pub filter_text: String,
}

/// Rank like Home's attention order: what needs the user first (§02.13).
fn urgency(s: &Session) -> u8 {
    use SessionStatus::*;
    match s.status {
        FailedGates | Errored => 0,
        NeedsReview => 1,
        NeedsPr | CiFix | ConflictFix => 2,
        Completed | Killed => 4,
        _ => 3,
    }
}

impl SessionSwitcher {
    /// Sessions matching the filter (prompt, issue number, title, status),
    /// most urgent first; ties keep pool order.
    pub fn filtered_sessions<'a>(&self, sessions: &[&'a Session]) -> Vec<&'a Session> {
        let filter = self.filter_text.to_lowercase();
        let mut out: Vec<&Session> = sessions
            .iter()
            .copied()
            .filter(|s| {
                filter.is_empty()
                    || s.prompt.to_lowercase().contains(&filter)
                    || s.issue_number
                        .is_some_and(|n| n.to_string() == self.filter_text)
                    || s.issue_title
                        .as_deref()
                        .is_some_and(|t| t.to_lowercase().contains(&filter))
                    || s.status.label().to_lowercase().contains(&filter)
            })
            .collect();
        out.sort_by_key(|s| urgency(s));
        out
    }

    /// The selected session: the one with the kept id while it matches,
    /// else the first match (none while the kept id is filtered out, so
    /// `Enter` never opens a session the user did not pick).
    pub fn selected_session<'a>(&self, sessions: &[&'a Session]) -> Option<&'a Session> {
        let filtered = self.filtered_sessions(sessions);
        match self.selected {
            Some(id) => filtered.into_iter().find(|s| s.id == id),
            None => filtered.first().copied(),
        }
    }

    pub fn move_up(&mut self, sessions: &[&Session]) {
        self.step(sessions, false);
    }

    pub fn move_down(&mut self, sessions: &[&Session]) {
        self.step(sessions, true);
    }

    fn step(&mut self, sessions: &[&Session], down: bool) {
        let filtered = self.filtered_sessions(sessions);
        let at = self
            .selected_session(sessions)
            .and_then(|s| filtered.iter().position(|f| f.id == s.id));
        let next = match at {
            Some(i) if down => (i + 1).min(filtered.len().saturating_sub(1)),
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.selected = filtered.get(next).map(|s| s.id);
    }

    pub fn push_char(&mut self, c: char) {
        self.filter_text.push(c);
    }

    pub fn pop_char(&mut self) {
        self.filter_text.pop();
    }

    /// Draw the switcher as a centered overlay.
    pub fn draw(
        &self,
        f: &mut Frame,
        area: Rect,
        sessions: &[&Session],
        theme: &Theme,
        spinner_tick: usize,
        use_nerd_font: bool,
    ) {
        // Overlay: 60% width, 70% height, centered
        let overlay_width = (area.width as f32 * 0.6).max(40.0) as u16;
        let overlay_height = (area.height as f32 * 0.7).max(10.0) as u16;
        let x = area.x + (area.width.saturating_sub(overlay_width)) / 2;
        let y = area.y + (area.height.saturating_sub(overlay_height)) / 2;
        let overlay = Rect::new(
            x,
            y,
            overlay_width.min(area.width),
            overlay_height.min(area.height),
        );

        f.render_widget(Clear, overlay);

        let block = theme
            .styled_block("Sessions [w]", false)
            .border_style(Style::default().fg(theme.accent_success));
        let inner = block.inner(overlay);
        f.render_widget(block, overlay);

        if inner.height < 2 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        let filtered = self.filtered_sessions(sessions);
        let selected = self.selected_session(sessions).map(|s| s.id);

        // The filter, as in the ⌃K palette: typing filters at once.
        let mut lines = vec![Line::from(vec![
            Span::styled("› ", Style::default().fg(theme.accent_info)),
            Span::styled(
                crate::tui::screens::sanitize_for_terminal(&self.filter_text),
                Style::default().fg(theme.text_primary),
            ),
        ])];
        for session in &filtered {
            let is_selected = selected == Some(session.id);
            let prefix = if is_selected {
                format!("{} ", icons::get(IconId::Selector))
            } else {
                "  ".to_string()
            };

            let status_symbol =
                session_switcher_status_symbol(session.status, spinner_tick, use_nerd_font);
            let label = row_label(session);

            let style = if is_selected {
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };

            lines.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::raw(format!("{} ", status_symbol)),
                Span::styled(label, style),
                Span::styled(
                    format!("  {}", session.elapsed_display()),
                    Style::default().fg(theme.text_muted),
                ),
            ]));
        }

        if filtered.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No sessions",
                Style::default().fg(theme.text_muted),
            )));
        }

        f.render_widget(Paragraph::new(lines), chunks[0]);

        // Footer
        let footer = Line::from(vec![
            Span::styled("[↑/↓]", Style::default().fg(theme.accent_success)),
            Span::raw(" Navigate  "),
            Span::styled("[type]", Style::default().fg(theme.accent_success)),
            Span::raw(" Filter  "),
            Span::styled("[Enter]", Style::default().fg(theme.accent_success)),
            Span::raw(" View  "),
            Span::styled("[Esc]", Style::default().fg(theme.accent_success)),
            Span::raw(" Close"),
        ]);
        f.render_widget(Paragraph::new(footer), chunks[1]);
    }
}

/// `#412 title` (or the prompt's first 40 chars), else the prompt's first
/// 50 chars. Cut on chars, so multi-byte text never splits (#1288).
fn row_label(session: &Session) -> String {
    let prompt = |n: usize| session.prompt.chars().take(n).collect::<String>();
    match session.issue_number {
        Some(num) => match session.issue_title.as_deref() {
            Some(title) => format!("#{num} {title}"),
            None => format!("#{num} {}", prompt(40)),
        },
        None => prompt(50),
    }
}

fn session_switcher_status_symbol(
    status: SessionStatus,
    spinner_tick: usize,
    use_nerd_font: bool,
) -> String {
    if matches!(
        status,
        SessionStatus::Running
            | SessionStatus::Spawning
            | SessionStatus::Retrying
            | SessionStatus::GatesRunning
            | SessionStatus::CiFix
            | SessionStatus::ConflictFix
    ) {
        graph_node_frame(spinner_tick, use_nerd_font).to_string()
    } else {
        status.symbol().to_string()
    }
}

#[cfg(test)]
#[path = "session_switcher_tests.rs"]
mod tests;
