//! Paged viewer for a `FailedGates` session's full gate stderr.
//!
//! Reached via `[v]` on the failed-gates recovery modal (issue #560).
//! Activity-log entries truncate gate failure messages to one line for
//! readability; the full `gate_results[].message` text is preserved on
//! the session and surfaced here.

use crate::session::types::Session;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

/// Render the gate-output paged view for `session`. `scroll` is the
/// vertical scroll offset (in rows) — see `App.log_viewer_scroll`.
pub fn draw_gate_output_viewer(
    f: &mut Frame,
    session: &Session,
    scroll: u16,
    area: Rect,
    theme: &Theme,
) {
    let mut lines: Vec<Line> = Vec::new();

    // Header — issue label and worktree path so the user knows which
    // session they're looking at without having to flip back.
    let issue_label = session
        .issue_number
        .map(|n| format!("Issue #{}", n))
        .unwrap_or_else(|| "Session".to_string());
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            issue_label,
            Style::default()
                .fg(theme.accent_info)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    if let Some(wt) = session.worktree_path.as_ref() {
        lines.push(Line::from(vec![
            Span::raw("  Worktree: "),
            Span::styled(
                sanitize_for_terminal(&wt.display().to_string()),
                Style::default().fg(theme.accent_warning),
            ),
        ]));
    }
    lines.push(Line::from(""));

    // One block per gate failure — gate name + status + full message.
    if session.gate_results.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "No gate results recorded.",
                Style::default().fg(theme.text_muted),
            ),
        ]));
    } else {
        for entry in &session.gate_results {
            let status = if entry.passed { "PASS" } else { "FAIL" };
            let status_color = if entry.passed {
                theme.accent_success
            } else {
                theme.accent_error
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("[{}] ", status),
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    sanitize_for_terminal(&entry.gate),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            // Wrap the multi-line message — preserve embedded newlines but
            // strip other control chars so a malicious dependency or a test
            // fixture cannot inject ANSI/OSC escape sequences via compiler
            // output (security review concern #6).
            for chunk in entry.message.split('\n') {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        sanitize_for_terminal(chunk),
                        Style::default().fg(theme.text_secondary),
                    ),
                ]));
            }
            lines.push(Line::from(""));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "[j/k] scroll   [Esc/q] back to summary",
            Style::default().fg(theme.text_muted),
        ),
    ]));

    let block = theme
        .styled_block("Gate Output", false)
        .border_style(Style::default().fg(theme.accent_warning));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{GateResultEntry, Session, SessionStatus};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_to_string(session: &Session) -> String {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::dark();
        terminal
            .draw(|f| draw_gate_output_viewer(f, session, 0, f.area(), &theme))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    fn make_failed_session(issue: u64, wt: &str) -> Session {
        let mut s = Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(issue),
            None,
        );
        s.status = SessionStatus::FailedGates;
        s.worktree_path = Some(std::path::PathBuf::from(wt));
        s.gate_results = vec![
            GateResultEntry::fail(
                "clippy",
                "function 'role_for_subagent_dispatch' is never used",
            ),
            GateResultEntry::fail("label_update", "'maestro:in-progress' not found"),
        ];
        s
    }

    #[test]
    fn gate_output_viewer_renders_all_gate_failures() {
        let s = make_failed_session(560, ".maestro/worktrees/issue-560");
        let rendered = render_to_string(&s);
        assert!(
            rendered.contains("clippy"),
            "viewer must show the first gate's name"
        );
        assert!(
            rendered.contains("role_for_subagent_dispatch"),
            "viewer must show the first gate's full message, not just the truncated form"
        );
        assert!(
            rendered.contains("label_update"),
            "viewer must show ALL gate names, not just the first"
        );
    }

    #[test]
    fn gate_output_viewer_shows_worktree_path() {
        let s = make_failed_session(560, ".maestro/worktrees/issue-560");
        let rendered = render_to_string(&s);
        assert!(
            rendered.contains("issue-560"),
            "viewer must surface the worktree path so the user knows where to cd"
        );
    }
}
