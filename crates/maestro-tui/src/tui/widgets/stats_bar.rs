use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::tui::icons::{self, IconId};
use crate::tui::marquee::{MarqueeConfig, MarqueeState, visible_spans};
use crate::tui::theme::Theme;

/// Data for the compact stats bar widget.
#[derive(Debug, Clone)]
pub struct StatsBarData {
    pub loaded: bool,
    pub repo: String,
    pub branch: String,
    pub username: Option<String>,
    pub issues_open: usize,
    pub issues_closed: usize,
    pub milestone_title: Option<String>,
    pub milestone_closed: u32,
    pub milestone_total: u32,
    pub sessions_active: usize,
    pub sessions_total: usize,
    /// Number of MiniMax spawns that bypassed the 5h quota refusal via
    /// `--force-quota` in the current window (#845). Rendered only when
    /// `Some(n)` with `n > 0` — keeps no-MiniMax / no-bypass cases visually
    /// unchanged.
    pub minimax_forced_count: Option<u32>,
}

/// Compact project stats bar widget replacing the large header brand.
pub struct StatsBar<'a> {
    data: StatsBarData,
    theme: &'a Theme,
}

impl<'a> StatsBar<'a> {
    pub fn new(data: StatsBarData, theme: &'a Theme) -> Self {
        Self { data, theme }
    }

    /// Build the styled spans and render into `area`, animating with `marquee`
    /// when the content overflows the available width.
    ///
    /// Preserves the existing non-animated behavior when the line fits.
    pub fn render_with_marquee(self, area: Rect, buf: &mut Buffer, marquee: &mut MarqueeState) {
        if area.height < 1 || area.width < 2 {
            return;
        }

        let block = crate::tui::theme::Theme::stats_block(self.theme);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 {
            return;
        }

        let line = self.build_line();
        let total_width = line.width();
        let viewport_width = inner.width as usize;

        if total_width <= viewport_width {
            marquee.reset();
            Paragraph::new(line).render(inner, buf);
            return;
        }

        let overflow = total_width.saturating_sub(viewport_width);
        marquee.advance(overflow, &MarqueeConfig::default());
        let windowed = visible_spans(&line.spans, marquee.offset, viewport_width);
        Paragraph::new(Line::from(windowed)).render(inner, buf);
    }

    fn build_line(&self) -> Line<'_> {
        let username = self.data.username.as_deref().unwrap_or("unknown");

        let mut spans = vec![
            // Repo info section
            Span::styled(
                format!(" {} ", icons::get(IconId::Repo)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(&self.data.repo, Style::default().fg(self.theme.accent_info)),
            Span::styled(
                format!("  {} ", icons::get(IconId::Branch)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                &self.data.branch,
                Style::default().fg(self.theme.accent_warning),
            ),
            Span::styled(
                format!("  {} ", icons::get(IconId::User)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                format!("@{}", username),
                Style::default().fg(self.theme.accent_success),
            ),
            Span::styled("  │  ", Style::default().fg(self.theme.text_muted)),
        ];

        if !self.data.loaded {
            spans.push(Span::styled(
                "Loading...",
                Style::default().fg(self.theme.accent_warning),
            ));
            return Line::from(spans);
        }

        // Issues section
        let total_issues = self.data.issues_open + self.data.issues_closed;
        spans.extend([
            Span::styled(
                format!("{} ", icons::get(IconId::IssueOpened)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                self.data.issues_open.to_string(),
                Style::default()
                    .fg(self.theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" open ", Style::default().fg(self.theme.text_secondary)),
            Span::styled(
                format!("{} ", icons::get(IconId::CheckCircle)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                self.data.issues_closed.to_string(),
                Style::default()
                    .fg(self.theme.accent_success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" closed ({})", total_issues),
                Style::default().fg(self.theme.text_secondary),
            ),
        ]);

        // Milestone section
        if let Some(ref title) = self.data.milestone_title {
            let pct = if self.data.milestone_total > 0 {
                (self.data.milestone_closed as f64 / self.data.milestone_total as f64) * 100.0
            } else {
                0.0
            };
            let (filled, empty) = crate::tui::panels::compact_gauge_bar_counts(pct, 8);

            spans.extend([
                Span::styled("  │  ", Style::default().fg(self.theme.text_muted)),
                Span::styled(
                    format!("{} ", icons::get(IconId::Milestone)),
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(title, Style::default().fg(self.theme.accent_info)),
                Span::raw(" "),
                Span::styled(
                    icons::get(IconId::GaugeFilled).repeat(filled),
                    Style::default().fg(self.theme.accent_success),
                ),
                Span::styled(
                    icons::get(IconId::GaugeEmpty).repeat(empty),
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled(
                    format!(" {:.0}%", pct),
                    Style::default().fg(self.theme.accent_success),
                ),
            ]);
        }

        // Sessions section
        spans.extend([
            Span::styled("  │  ", Style::default().fg(self.theme.text_muted)),
            Span::styled(
                format!("{} ", icons::get(IconId::Agents)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                self.data.sessions_active.to_string(),
                Style::default()
                    .fg(self.theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" active / {} total", self.data.sessions_total),
                Style::default().fg(self.theme.text_secondary),
            ),
        ]);

        // MiniMax forced-quota indicator (#845). Hidden unless the operator
        // actually bypassed the gate at least once in the current window.
        if let Some(count) = self.data.minimax_forced_count
            && count > 0
        {
            spans.extend([
                Span::styled("  │  ", Style::default().fg(self.theme.text_muted)),
                Span::styled(
                    "QUOTA: forced ",
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(
                    count.to_string(),
                    Style::default()
                        .fg(self.theme.accent_warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" in window", Style::default().fg(self.theme.text_secondary)),
            ]);
        }

        Line::from(spans)
    }
}

impl<'a> Widget for StatsBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 || area.width < 2 {
            return;
        }

        let block = crate::tui::theme::Theme::stats_block(self.theme);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height > 0 {
            Paragraph::new(self.build_line()).render(inner, buf);
        }
    }
}

#[cfg(test)]
#[path = "stats_bar_tests.rs"]
mod tests;
