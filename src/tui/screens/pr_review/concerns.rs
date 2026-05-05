//! Concerns-panel overlay rendered on the PR review screen (#327).
//!
//! Displays the parsed `ReviewReport` from `App::pending_review_report`
//! with severity coloring, file:line refs, and the cursor's accept/reject
//! affordance. The accept-key handler in `actions.rs` mutates the
//! corresponding `Concern.status`.

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

use crate::review::types::{ConcernStatus, ReviewReport, Severity};
use crate::tui::theme::Theme;
use crate::tui::widgets::focused_selection_style;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use unicode_width::UnicodeWidthStr;

pub fn draw(f: &mut Frame, area: Rect, report: &ReviewReport, cursor: usize, theme: &Theme) {
    let (critical, warning, suggestion) = report.severity_counts();
    let title = format!(
        " Review concerns: {} (PR #{}, {critical} critical / {warning} warning / {suggestion} suggestion) ",
        report.concerns.len(),
        report.pr_number,
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent_identifier))
        .title(title);
    let inner = block.inner(area);

    if report.concerns.is_empty() {
        f.render_widget(
            ratatui::widgets::Paragraph::new("No concerns raised — clean review.")
                .style(Style::default().fg(theme.accent_success))
                .block(block),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = report
        .concerns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let focused = i == cursor;
            let prefix = "  ".to_string();
            let sev_color = match c.severity {
                Severity::Critical => Color::Red,
                Severity::Warning => Color::Yellow,
                Severity::Suggestion => Color::Cyan,
            };
            let status_label = match c.status {
                ConcernStatus::Pending => "[ ]",
                ConcernStatus::Accepted => "[A]",
                ConcernStatus::Rejected => "[R]",
                ConcernStatus::Applied => "[✓]",
            };
            let line_ref = c.line.map(|l| format!(":{l}")).unwrap_or_default();
            let status = format!("{status_label} ");
            let severity = format!("[{}] ", c.severity.label());
            let location = format!("{}{line_ref} — ", c.file.display());
            let message = c.message.clone();
            let row_style = if focused {
                focused_selection_style(theme)
            } else {
                Style::default().fg(theme.text_primary)
            };
            let muted_style = if focused {
                row_style
            } else {
                Style::default().fg(theme.text_secondary)
            };
            let severity_style = if focused {
                row_style
            } else {
                Style::default().fg(sev_color).add_modifier(Modifier::BOLD)
            };
            let content_width = prefix.width()
                + status.as_str().width()
                + severity.as_str().width()
                + location.as_str().width()
                + message.as_str().width();
            let trailing = inner.width.saturating_sub(content_width as u16) as usize;
            let mut spans = vec![
                Span::styled(prefix, row_style),
                Span::styled(status, muted_style),
                Span::styled(severity, severity_style),
                Span::styled(location, muted_style),
                Span::styled(message, row_style),
            ];
            if trailing > 0 {
                spans.push(Span::styled(" ".repeat(trailing), row_style));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    f.render_widget(List::new(items).block(block), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::types::{Concern, ConcernId, PrNumber};
    use std::path::PathBuf;

    fn report_with_each_severity() -> ReviewReport {
        let mut r = ReviewReport::new(PrNumber(7), "claude");
        for sev in [Severity::Critical, Severity::Warning, Severity::Suggestion] {
            r.concerns.push(Concern {
                id: ConcernId::new(),
                severity: sev,
                file: PathBuf::from("src/x.rs"),
                line: Some(42),
                message: format!("{} concern", sev.label()),
                suggested_diff: None,
                status: ConcernStatus::Pending,
            });
        }
        r
    }

    #[test]
    fn draw_does_not_panic_with_empty_report() {
        use ratatui::backend::TestBackend;
        let backend = TestBackend::new(80, 10);
        let mut term = ratatui::Terminal::new(backend).expect("term");
        let report = ReviewReport::new(PrNumber(1), "claude");
        let theme = Theme::default();
        term.draw(|f| draw(f, f.area(), &report, 0, &theme))
            .expect("draw");
    }

    #[test]
    fn draw_handles_each_severity() {
        use ratatui::backend::TestBackend;
        let backend = TestBackend::new(120, 10);
        let mut term = ratatui::Terminal::new(backend).expect("term");
        let report = report_with_each_severity();
        let theme = Theme::default();
        term.draw(|f| draw(f, f.area(), &report, 1, &theme))
            .expect("draw");
    }
}
