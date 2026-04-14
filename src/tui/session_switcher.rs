use crate::session::types::Session;
use crate::tui::icons::{self, IconId};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::tui::theme::Theme;

/// Overlay state for the session switcher.
#[derive(Default)]
pub struct SessionSwitcher {
    pub selected_index: usize,
    pub filter_text: String,
}

impl SessionSwitcher {
    /// Filter sessions by prompt, issue number, or status.
    pub fn filtered_sessions<'a>(&self, sessions: &[&'a Session]) -> Vec<&'a Session> {
        if self.filter_text.is_empty() {
            return sessions.to_vec();
        }
        let filter = self.filter_text.to_lowercase();
        sessions
            .iter()
            .copied()
            .filter(|s| {
                s.prompt.to_lowercase().contains(&filter)
                    || s.issue_number
                        .map(|n| n.to_string() == self.filter_text)
                        .unwrap_or(false)
                    || s.issue_title
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&filter)
                    || s.status.label().to_lowercase().contains(&filter)
            })
            .collect()
    }

    /// Get the currently selected session from the filtered list.
    pub fn selected_session<'a>(&self, sessions: &[&'a Session]) -> Option<&'a Session> {
        let filtered = self.filtered_sessions(sessions);
        if filtered.is_empty() {
            return None;
        }
        let idx = self.selected_index.min(filtered.len() - 1);
        Some(filtered[idx])
    }

    pub fn move_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn move_down(&mut self, session_count: usize) {
        if session_count > 0 && self.selected_index + 1 < session_count {
            self.selected_index += 1;
        }
    }

    /// Draw the switcher as a centered overlay.
    pub fn draw(&self, f: &mut Frame, area: Rect, sessions: &[&Session], theme: &Theme) {
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

        // Session list
        let mut lines = Vec::new();
        for (i, session) in filtered.iter().enumerate() {
            let is_selected = i == self.selected_index.min(filtered.len().saturating_sub(1));
            let prefix = if is_selected {
                format!("{} ", icons::get(IconId::Selector))
            } else {
                "  ".to_string()
            };

            let status_symbol = session.status.symbol();
            let label = if let Some(num) = session.issue_number {
                format!(
                    "#{} {}",
                    num,
                    session
                        .issue_title
                        .as_deref()
                        .unwrap_or(&session.prompt[..session.prompt.len().min(40)])
                )
            } else {
                session.prompt[..session.prompt.len().min(50)].to_string()
            };

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

        if lines.is_empty() {
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
            Span::styled("[Enter]", Style::default().fg(theme.accent_success)),
            Span::raw(" View  "),
            Span::styled("[Esc]", Style::default().fg(theme.accent_success)),
            Span::raw(" Close"),
        ]);
        f.render_widget(Paragraph::new(footer), chunks[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::Session;

    fn make_session(prompt: &str, issue: Option<u64>) -> Session {
        Session::new(prompt.into(), "opus".into(), "orchestrator".into(), issue)
    }

    #[test]
    fn default_has_index_zero_and_empty_filter() {
        let s = SessionSwitcher::default();
        assert_eq!(s.selected_index, 0);
        assert!(s.filter_text.is_empty());
    }

    #[test]
    fn filtered_sessions_with_empty_filter_returns_all() {
        let s1 = make_session("fix auth", None);
        let s2 = make_session("add dashboard", None);
        let s3 = make_session("fix login", None);
        let sessions = vec![&s1, &s2, &s3];
        let sw = SessionSwitcher::default();
        let result = sw.filtered_sessions(&sessions);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn filtered_sessions_matches_prompt_substring() {
        let s1 = make_session("fix auth bug", None);
        let s2 = make_session("add dashboard", None);
        let s3 = make_session("fix login", None);
        let sessions = vec![&s1, &s2, &s3];
        let sw = SessionSwitcher {
            selected_index: 0,
            filter_text: "fix".into(),
        };
        let result = sw.filtered_sessions(&sessions);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn filtered_sessions_matches_issue_number() {
        let s1 = make_session("work on auth", Some(42));
        let s2 = make_session("dashboard", Some(99));
        let s3 = make_session("no issue", None);
        let sessions = vec![&s1, &s2, &s3];
        let sw = SessionSwitcher {
            selected_index: 0,
            filter_text: "42".into(),
        };
        let result = sw.filtered_sessions(&sessions);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].issue_number, Some(42));
    }

    #[test]
    fn filtered_sessions_case_insensitive() {
        let s = make_session("Fix Auth Bug", None);
        let sessions = vec![&s];
        let sw = SessionSwitcher {
            selected_index: 0,
            filter_text: "fix".into(),
        };
        assert_eq!(sw.filtered_sessions(&sessions).len(), 1);
    }

    #[test]
    fn filtered_sessions_empty_pool_returns_empty() {
        let sessions: Vec<&Session> = vec![];
        let sw = SessionSwitcher::default();
        assert!(sw.filtered_sessions(&sessions).is_empty());
    }

    #[test]
    fn filtered_sessions_no_match_returns_empty() {
        let s1 = make_session("fix auth", None);
        let s2 = make_session("add dashboard", None);
        let sessions = vec![&s1, &s2];
        let sw = SessionSwitcher {
            selected_index: 0,
            filter_text: "zzznomatch".into(),
        };
        assert!(sw.filtered_sessions(&sessions).is_empty());
    }

    #[test]
    fn selected_session_clamps_to_filtered_len() {
        let s1 = make_session("fix auth", None);
        let s2 = make_session("fix login", None);
        let sessions = vec![&s1, &s2];
        let sw = SessionSwitcher {
            selected_index: 10,
            filter_text: "fix".into(),
        };
        let result = sw.selected_session(&sessions);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, s2.id);
    }

    #[test]
    fn selected_session_zero_returns_first() {
        let s1 = make_session("fix auth", None);
        let s2 = make_session("fix login", None);
        let s3 = make_session("fix ui", None);
        let sessions = vec![&s1, &s2, &s3];
        let sw = SessionSwitcher {
            selected_index: 0,
            filter_text: "fix".into(),
        };
        assert_eq!(sw.selected_session(&sessions).unwrap().id, s1.id);
    }

    #[test]
    fn move_up_at_zero_stays_at_zero() {
        let mut sw = SessionSwitcher::default();
        sw.move_up();
        assert_eq!(sw.selected_index, 0);
    }

    #[test]
    fn move_down_increments() {
        let mut sw = SessionSwitcher::default();
        sw.move_down(5);
        assert_eq!(sw.selected_index, 1);
    }

    #[test]
    fn move_down_at_max_stays() {
        let mut sw = SessionSwitcher {
            selected_index: 2,
            filter_text: String::new(),
        };
        sw.move_down(3);
        assert_eq!(sw.selected_index, 2);
    }
}
