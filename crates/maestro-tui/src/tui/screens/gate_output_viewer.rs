//! Paged viewer for a session's gates: the `v Gates` session view (#1288).
//!
//! #1291 (§02.23 Diagnostics): a rail of every gate (result, command,
//! duration), then per failing gate its failure digest with the first
//! failure open, the output tail and the full-log path. Entries from
//! before #1291 have no output and show their one-line message.

use crate::gates::digest::digest;
use crate::icons::{self, IconId};
use crate::session::types::{GateResultEntry, Session};
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;
use crate::tui::widgets::failure_list::failure_lines;
use crate::util::sanitize::sanitize_log;
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
    back_to_summary: bool,
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

    if session.gate_results.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "No gate results recorded.",
                Style::default().fg(theme.text_muted),
            ),
        ]));
    } else {
        rail(&mut lines, session, theme);
        for entry in session.gate_results.iter().filter(|e| !e.passed) {
            failure_block(&mut lines, entry, theme);
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            // #1288: `q` never reaches this view (it is the global quit),
            // and Gates is also a session tab with no summary behind it.
            if back_to_summary {
                "[j/k] scroll   [Esc] back to summary"
            } else {
                "[j/k] scroll   [Esc] back"
            },
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

/// Output lines shown under the digest.
const TAIL_LINES: usize = 20;

/// `GATES`: one row per gate, pass or fail.
fn rail(lines: &mut Vec<Line<'static>>, session: &Session, theme: &Theme) {
    let muted = Style::default().fg(theme.text_muted);
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("GATES", muted.add_modifier(Modifier::BOLD)),
    ]));
    let widest = |f: fn(&GateResultEntry) -> &str| {
        let w = session.gate_results.iter().map(|e| f(e).chars().count());
        w.max().unwrap_or(0)
    };
    let (width, cmd_width) = (widest(|e| &e.gate), widest(|e| &e.command));
    for e in &session.gate_results {
        let (icon, color) = if e.passed {
            (IconId::ChipCompleted, theme.accent_success)
        } else {
            (IconId::ChipErrored, theme.accent_error)
        };
        let mut row = vec![
            Span::raw("  "),
            Span::styled(
                format!("{} ", icons::get(icon)),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<width$}", sanitize_for_terminal(&e.gate)),
                Style::default().fg(theme.text_primary),
            ),
        ];
        if !e.command.is_empty() {
            row.push(Span::styled(
                format!("  {:<cmd_width$}", sanitize_for_terminal(&e.command)),
                Style::default().fg(theme.text_secondary),
            ));
        }
        if e.duration_ms > 0 {
            row.push(Span::styled(
                format!("  {:.1}s", e.duration_ms as f64 / 1000.0),
                muted,
            ));
        }
        lines.push(Line::from(row));
    }
    lines.push(Line::from(""));
}

/// One failing gate: title, then digest + output tail + log path, or the
/// stored message for entries without output.
fn failure_block(lines: &mut Vec<Line<'static>>, entry: &GateResultEntry, theme: &Theme) {
    let muted = Style::default().fg(theme.text_muted);
    let mut title = vec![
        Span::raw("  "),
        Span::styled(
            format!("{} ", icons::get(IconId::ChipErrored)),
            Style::default()
                .fg(theme.accent_error)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            sanitize_for_terminal(&entry.gate),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if !entry.command.is_empty() {
        title.push(Span::styled(
            format!(" · {}", sanitize_for_terminal(&entry.command)),
            muted,
        ));
    }
    lines.push(Line::from(title));
    // Embedded newlines are split into rows; control and bidi characters
    // are replaced so compiler output cannot inject escapes (#1074).
    let text_row = |chunk: &str| {
        Line::from(vec![
            Span::raw("    "),
            Span::styled(
                sanitize_log(chunk),
                Style::default().fg(theme.text_secondary),
            ),
        ])
    };
    if entry.output.is_empty() {
        lines.extend(entry.message.split('\n').map(text_row));
        lines.push(Line::from(""));
        return;
    }
    lines.extend(failure_lines(&digest(&entry.output), "    ", theme));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled("OUTPUT TAIL", muted.add_modifier(Modifier::BOLD)),
    ]));
    let tail: Vec<&str> = entry.output.lines().collect();
    let from = tail.len().saturating_sub(TAIL_LINES);
    lines.extend(tail[from..].iter().map(|l| text_row(l)));
    if let Some(path) = &entry.log_path {
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!(
                    "full log: {}",
                    sanitize_for_terminal(&path.display().to_string())
                ),
                muted,
            ),
        ]));
    }
    lines.push(Line::from(""));
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
            .draw(|f| draw_gate_output_viewer(f, session, 0, true, f.area(), &theme))
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
