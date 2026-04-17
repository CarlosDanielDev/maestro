use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::provider::github::ci::{CheckConclusion, CheckRunDetail, CheckStatus};
use crate::tui::icons::{self, IconId};
use crate::tui::theme::Theme;

/// Default maximum number of visible check rows before truncation.
const DEFAULT_MAX_VISIBLE_ROWS: usize = 8;

/// A compact TUI widget that renders live CI check-run status for a PR.
///
/// Each check run gets a row with a status icon, the check name, and elapsed time.
/// A summary footer shows aggregate counts.
pub struct CiMonitorWidget<'a> {
    checks: &'a [CheckRunDetail],
    pr_number: Option<u64>,
    max_visible_rows: usize,
    theme: &'a Theme,
}

impl<'a> CiMonitorWidget<'a> {
    pub fn new(checks: &'a [CheckRunDetail], theme: &'a Theme) -> Self {
        Self {
            checks,
            pr_number: None,
            max_visible_rows: DEFAULT_MAX_VISIBLE_ROWS,
            theme,
        }
    }

    pub fn pr_number(mut self, pr: u64) -> Self {
        self.pr_number = Some(pr);
        self
    }

    pub fn max_visible_rows(mut self, max: usize) -> Self {
        self.max_visible_rows = max;
        self
    }
}

/// Map a check's status+conclusion to a display icon.
fn status_icon(status: CheckStatus, conclusion: CheckConclusion) -> &'static str {
    match (status, conclusion) {
        (CheckStatus::Completed, CheckConclusion::Success) => icons::get(IconId::CheckCircle),
        (CheckStatus::Completed, CheckConclusion::Failure)
        | (CheckStatus::Completed, CheckConclusion::TimedOut)
        | (CheckStatus::Completed, CheckConclusion::StartupFailure) => icons::get(IconId::XCircle),
        (CheckStatus::Completed, CheckConclusion::Skipped) => icons::get(IconId::Skip),
        (CheckStatus::Completed, CheckConclusion::Cancelled) => icons::get(IconId::Skip),
        (CheckStatus::Completed, _) => icons::get(IconId::CheckCircle),
        (CheckStatus::InProgress, _) => icons::get(IconId::Hourglass),
        _ => icons::get(IconId::Hourglass),
    }
}

/// Map a check's status+conclusion to a theme color.
fn status_color(status: CheckStatus, conclusion: CheckConclusion, theme: &Theme) -> Style {
    let color = match (status, conclusion) {
        (CheckStatus::Completed, CheckConclusion::Success) => theme.accent_success,
        (CheckStatus::Completed, CheckConclusion::Failure)
        | (CheckStatus::Completed, CheckConclusion::TimedOut)
        | (CheckStatus::Completed, CheckConclusion::StartupFailure) => theme.accent_error,
        (CheckStatus::Completed, CheckConclusion::Skipped)
        | (CheckStatus::Completed, CheckConclusion::Cancelled) => theme.text_muted,
        (CheckStatus::Completed, _) => theme.accent_success,
        (CheckStatus::InProgress, _) => theme.accent_warning,
        _ => theme.text_secondary,
    };
    Style::default().fg(color)
}

/// Format elapsed seconds into a human-readable string.
fn format_elapsed(secs: Option<u64>) -> String {
    match secs {
        Some(s) if s >= 60 => format!("{}m{}s", s / 60, s % 60),
        Some(s) => format!("{}s", s),
        None => "—".to_string(),
    }
}

/// Compute summary counts from check details.
struct Summary {
    passed: usize,
    failed: usize,
    running: usize,
    skipped: usize,
    total: usize,
}

impl Summary {
    fn from_checks(checks: &[CheckRunDetail]) -> Self {
        let mut passed = 0;
        let mut failed = 0;
        let mut running = 0;
        let mut skipped = 0;

        for check in checks {
            match (check.status, check.conclusion) {
                (CheckStatus::Completed, CheckConclusion::Success)
                | (CheckStatus::Completed, CheckConclusion::Neutral) => passed += 1,
                (CheckStatus::Completed, CheckConclusion::Failure)
                | (CheckStatus::Completed, CheckConclusion::TimedOut)
                | (CheckStatus::Completed, CheckConclusion::StartupFailure) => failed += 1,
                (CheckStatus::Completed, CheckConclusion::Skipped)
                | (CheckStatus::Completed, CheckConclusion::Cancelled) => skipped += 1,
                (CheckStatus::Completed, _) => passed += 1,
                _ => running += 1,
            }
        }

        Self {
            passed,
            failed,
            running,
            skipped,
            total: checks.len(),
        }
    }

    fn to_line(&self, theme: &Theme) -> Line<'static> {
        let mut spans = Vec::new();

        spans.push(Span::styled(
            format!("{}/{} passed", self.passed, self.total),
            Style::default().fg(theme.accent_success),
        ));

        if self.failed > 0 {
            spans.push(Span::raw(" · "));
            spans.push(Span::styled(
                format!("{} failed", self.failed),
                Style::default().fg(theme.accent_error),
            ));
        }

        if self.running > 0 {
            spans.push(Span::raw(" · "));
            spans.push(Span::styled(
                format!("{} running", self.running),
                Style::default().fg(theme.accent_warning),
            ));
        }

        if self.skipped > 0 {
            spans.push(Span::raw(" · "));
            spans.push(Span::styled(
                format!("{} skipped", self.skipped),
                Style::default().fg(theme.text_muted),
            ));
        }

        Line::from(spans)
    }
}

impl Widget for CiMonitorWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = match self.pr_number {
            Some(pr) => format!(" CI Monitor — PR #{} ", pr),
            None => " CI Monitor ".to_string(),
        };

        let block = self.theme.styled_block(&title, false);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        // Empty state
        if self.checks.is_empty() {
            let empty = Paragraph::new(Line::from(Span::styled(
                "No active CI checks",
                Style::default().fg(self.theme.text_muted),
            )));
            empty.render(inner, buf);
            return;
        }

        // Reserve 2 lines: 1 blank separator + 1 summary footer
        let footer_lines = 2;
        let available_rows = inner.height.saturating_sub(footer_lines) as usize;
        let visible_count = available_rows
            .min(self.max_visible_rows)
            .min(self.checks.len());
        let truncated = self.checks.len().saturating_sub(visible_count);

        let mut lines: Vec<Line> = Vec::new();

        for check in self.checks.iter().take(visible_count) {
            let icon = status_icon(check.status, check.conclusion);
            let style = status_color(check.status, check.conclusion, self.theme);
            let elapsed = format_elapsed(check.elapsed_secs);

            // Build the row: " icon name          elapsed "
            let icon_and_name = format!(" {} {}", icon, check.name);
            let padding_needed = (inner.width as usize)
                .saturating_sub(icon_and_name.len())
                .saturating_sub(elapsed.len())
                .saturating_sub(1); // trailing space
            let padding = " ".repeat(padding_needed.max(1));

            lines.push(Line::from(vec![
                Span::styled(icon_and_name, style),
                Span::raw(padding),
                Span::styled(elapsed, style),
            ]));
        }

        // Show "+N more" if truncated
        if truncated > 0 {
            lines.push(Line::from(Span::styled(
                format!(" +{} more", truncated),
                Style::default().fg(self.theme.text_muted),
            )));
        }

        // Blank separator before summary
        lines.push(Line::from(""));

        // Summary footer
        let summary = Summary::from_checks(self.checks);
        let mut summary_line = summary.to_line(self.theme);
        summary_line.spans.insert(0, Span::raw(" "));

        lines.push(summary_line);

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn make_check(
        name: &str,
        status: CheckStatus,
        conclusion: CheckConclusion,
        elapsed: Option<u64>,
    ) -> CheckRunDetail {
        CheckRunDetail {
            name: name.to_string(),
            status,
            conclusion,
            started_at: None,
            elapsed_secs: elapsed,
        }
    }

    fn render_widget(checks: &[CheckRunDetail], pr: Option<u64>, max_rows: usize) -> String {
        let backend = TestBackend::new(50, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::dark();

        terminal
            .draw(|f| {
                let mut widget = CiMonitorWidget::new(checks, &theme).max_visible_rows(max_rows);
                if let Some(pr_num) = pr {
                    widget = widget.pr_number(pr_num);
                }
                widget.render(f.area(), f.buffer_mut());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let mut output = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                output.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn empty_state_shows_no_active_checks() {
        let output = render_widget(&[], Some(42), 8);
        assert!(
            output.contains("No active CI checks"),
            "Expected 'No active CI checks' in output:\n{}",
            output
        );
    }

    #[test]
    fn title_shows_pr_number() {
        let output = render_widget(&[], Some(42), 8);
        assert!(
            output.contains("CI Monitor") && output.contains("PR #42"),
            "Expected title with PR #42 in output:\n{}",
            output
        );
    }

    #[test]
    fn title_without_pr_number() {
        let output = render_widget(&[], None, 8);
        assert!(
            output.contains("CI Monitor"),
            "Expected 'CI Monitor' in output:\n{}",
            output
        );
        assert!(
            !output.contains("PR #"),
            "Should not contain PR # when no PR number:\n{}",
            output
        );
    }

    #[test]
    fn single_passing_check() {
        let checks = vec![make_check(
            "build",
            CheckStatus::Completed,
            CheckConclusion::Success,
            Some(12),
        )];
        let output = render_widget(&checks, Some(1), 8);
        assert!(
            output.contains("\u{f42e}"),
            "Expected check icon:\n{}",
            output
        );
        assert!(output.contains("build"), "Expected check name:\n{}", output);
        assert!(output.contains("12s"), "Expected elapsed time:\n{}", output);
        assert!(
            output.contains("1/1 passed"),
            "Expected summary:\n{}",
            output
        );
    }

    #[test]
    fn mixed_statuses_render_correct_icons() {
        let checks = vec![
            make_check(
                "build",
                CheckStatus::Completed,
                CheckConclusion::Success,
                Some(12),
            ),
            make_check(
                "test",
                CheckStatus::InProgress,
                CheckConclusion::None,
                Some(8),
            ),
            make_check(
                "lint",
                CheckStatus::Completed,
                CheckConclusion::Failure,
                Some(3),
            ),
            make_check(
                "deploy",
                CheckStatus::Completed,
                CheckConclusion::Skipped,
                None,
            ),
        ];
        let output = render_widget(&checks, Some(42), 8);

        assert!(
            output.contains("\u{f42e}"),
            "Expected check icon for passed:\n{}",
            output
        );
        assert!(
            output.contains("\u{f251}"),
            "Expected hourglass for in-progress:\n{}",
            output
        );
        assert!(
            output.contains("\u{f467}"),
            "Expected x-circle for failed:\n{}",
            output
        );
        assert!(
            output.contains("\u{f4a7}"),
            "Expected skip icon for skipped:\n{}",
            output
        );
    }

    #[test]
    fn summary_footer_counts_match() {
        let checks = vec![
            make_check(
                "a",
                CheckStatus::Completed,
                CheckConclusion::Success,
                Some(1),
            ),
            make_check(
                "b",
                CheckStatus::Completed,
                CheckConclusion::Success,
                Some(2),
            ),
            make_check(
                "c",
                CheckStatus::Completed,
                CheckConclusion::Failure,
                Some(3),
            ),
            make_check("d", CheckStatus::InProgress, CheckConclusion::None, Some(4)),
            make_check("e", CheckStatus::Completed, CheckConclusion::Skipped, None),
        ];
        let output = render_widget(&checks, Some(1), 8);

        assert!(
            output.contains("2/5 passed"),
            "Expected 2/5 passed:\n{}",
            output
        );
        assert!(
            output.contains("1 failed"),
            "Expected 1 failed:\n{}",
            output
        );
        assert!(
            output.contains("1 running"),
            "Expected 1 running:\n{}",
            output
        );
        assert!(
            output.contains("1 skipped"),
            "Expected 1 skipped:\n{}",
            output
        );
    }

    #[test]
    fn truncation_shows_plus_n_more() {
        let checks: Vec<CheckRunDetail> = (0..6)
            .map(|i| {
                make_check(
                    &format!("check-{}", i),
                    CheckStatus::Completed,
                    CheckConclusion::Success,
                    Some(i as u64),
                )
            })
            .collect();
        // max_visible_rows = 3, so 3 visible + "+3 more"
        let output = render_widget(&checks, Some(1), 3);

        assert!(
            output.contains("+3 more"),
            "Expected '+3 more' truncation message:\n{}",
            output
        );
    }

    #[test]
    fn no_truncation_when_within_limit() {
        let checks = vec![
            make_check(
                "a",
                CheckStatus::Completed,
                CheckConclusion::Success,
                Some(1),
            ),
            make_check(
                "b",
                CheckStatus::Completed,
                CheckConclusion::Success,
                Some(2),
            ),
        ];
        let output = render_widget(&checks, Some(1), 8);

        assert!(
            !output.contains("+"),
            "Should not contain truncation message:\n{}",
            output
        );
    }

    #[test]
    fn elapsed_time_formats_minutes() {
        let checks = vec![make_check(
            "slow-test",
            CheckStatus::Completed,
            CheckConclusion::Success,
            Some(150),
        )];
        let output = render_widget(&checks, Some(1), 8);

        assert!(
            output.contains("2m30s"),
            "Expected 2m30s for 150 seconds:\n{}",
            output
        );
    }

    #[test]
    fn elapsed_time_none_shows_dash() {
        let checks = vec![make_check(
            "queued",
            CheckStatus::Queued,
            CheckConclusion::None,
            None,
        )];
        let output = render_widget(&checks, Some(1), 8);

        assert!(
            output.contains("—"),
            "Expected em-dash for no elapsed time:\n{}",
            output
        );
    }

    #[test]
    fn status_icon_mapping_comprehensive() {
        // Passed
        assert_eq!(
            status_icon(CheckStatus::Completed, CheckConclusion::Success),
            "\u{f42e}"
        );
        // Failed
        assert_eq!(
            status_icon(CheckStatus::Completed, CheckConclusion::Failure),
            "\u{f467}"
        );
        // Timed out
        assert_eq!(
            status_icon(CheckStatus::Completed, CheckConclusion::TimedOut),
            "\u{f467}"
        );
        // Skipped
        assert_eq!(
            status_icon(CheckStatus::Completed, CheckConclusion::Skipped),
            "\u{f4a7}"
        );
        // Cancelled
        assert_eq!(
            status_icon(CheckStatus::Completed, CheckConclusion::Cancelled),
            "\u{f4a7}"
        );
        // In progress
        assert_eq!(
            status_icon(CheckStatus::InProgress, CheckConclusion::None),
            "\u{f251}"
        );
        // Queued
        assert_eq!(
            status_icon(CheckStatus::Queued, CheckConclusion::None),
            "\u{f251}"
        );
        // Pending
        assert_eq!(
            status_icon(CheckStatus::Pending, CheckConclusion::None),
            "\u{f251}"
        );
        // Neutral (completed but neutral)
        assert_eq!(
            status_icon(CheckStatus::Completed, CheckConclusion::Neutral),
            "\u{f42e}"
        );
    }

    #[test]
    fn summary_all_passed() {
        let checks = vec![
            make_check(
                "a",
                CheckStatus::Completed,
                CheckConclusion::Success,
                Some(1),
            ),
            make_check(
                "b",
                CheckStatus::Completed,
                CheckConclusion::Success,
                Some(2),
            ),
        ];
        let summary = Summary::from_checks(&checks);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.running, 0);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.total, 2);
    }

    #[test]
    fn summary_empty_checks() {
        let summary = Summary::from_checks(&[]);
        assert_eq!(summary.passed, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.running, 0);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.total, 0);
    }

    #[test]
    fn format_elapsed_seconds_only() {
        assert_eq!(format_elapsed(Some(45)), "45s");
    }

    #[test]
    fn format_elapsed_with_minutes() {
        assert_eq!(format_elapsed(Some(90)), "1m30s");
    }

    #[test]
    fn format_elapsed_none() {
        assert_eq!(format_elapsed(None), "—");
    }

    #[test]
    fn format_elapsed_zero() {
        assert_eq!(format_elapsed(Some(0)), "0s");
    }

    #[test]
    fn widget_handles_zero_area_gracefully() {
        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::dark();
        let checks = vec![make_check(
            "build",
            CheckStatus::Completed,
            CheckConclusion::Success,
            Some(1),
        )];

        // Render into a zero-height area — should not panic
        terminal
            .draw(|f| {
                let widget = CiMonitorWidget::new(&checks, &theme);
                let tiny_area = Rect::new(0, 0, 40, 2); // borders take 2 lines, inner = 0
                widget.render(tiny_area, f.buffer_mut());
            })
            .unwrap();
    }
}
