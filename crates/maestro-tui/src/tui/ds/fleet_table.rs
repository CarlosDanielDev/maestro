//! Fleet table (`fleet` in `docs/design-system/index.html` §02 Home, #1264):
//! this run's agents as one row each, status chip first.
//!
//! Regular and Wide show `STATUS ISSUE TITLE MODEL CONTEXT COST TIME`;
//! Compact keeps five columns, `STATUS ISSUE TITLE CTX COST`. Rows are plain
//! data ([`FleetRow`]), so the table is drawn without an `App`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::session::types::SessionStatus;
use crate::tui::ds::bar_row::key_style;
use crate::tui::ds::gauge::bar;
use crate::tui::ds::money::money;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::ds::status_chip::status_chip_for_mode;
use crate::tui::screens::landing::draw::ellipsize;
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;

/// One agent, already formatted where it needs a clock.
#[derive(Debug, Clone, PartialEq)]
pub struct FleetRow {
    pub status: SessionStatus,
    pub issue: Option<u64>,
    /// Issue title, or the prompt when the session has no issue.
    pub title: String,
    pub model: String,
    /// Context used, `0.0..=1.0`.
    pub context: f64,
    /// `None` when the model was unpriced (`$?`).
    pub cost: Option<f64>,
    /// `12m03s`, or `—` before the session started.
    pub elapsed: String,
}

/// Widest chip (`✗ gates failed`) plus one space.
const STATUS_W: usize = 16;
const ISSUE_W: usize = 7;
const MODEL_W: usize = 16;
/// Bar, space, `100%`, space.
const CONTEXT_W: usize = 15;
const CONTEXT_BAR_W: usize = 9;
const CTX_W: usize = 5;
const COST_W: usize = 8;
const TIME_W: usize = 8;

pub fn draw(
    f: &mut Frame,
    area: Rect,
    rows: &[FleetRow],
    compact: bool,
    theme: &Theme,
    nerd_font: bool,
) {
    let title = if rows.is_empty() {
        "Fleet".to_string()
    } else {
        format!("Fleet ({})", rows.len())
    };
    let block = panel(&title, Focus::Inactive, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = if rows.is_empty() {
        vec![empty_line(theme)]
    } else {
        let title_w = title_width(inner.width as usize, compact);
        std::iter::once(header(title_w, compact, theme))
            .chain(
                rows.iter()
                    .map(|r| row(r, title_w, compact, theme, nerd_font)),
            )
            .collect()
    };
    f.render_widget(Paragraph::new(lines), inner);
}

/// The TITLE column takes what the fixed columns leave (one space each side).
fn title_width(inner: usize, compact: bool) -> usize {
    let fixed = if compact {
        STATUS_W + ISSUE_W + CTX_W + COST_W
    } else {
        STATUS_W + ISSUE_W + MODEL_W + CONTEXT_W + COST_W + TIME_W
    };
    inner.saturating_sub(fixed + 2)
}

fn empty_line(theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(" No agents yet. ", Style::new().fg(theme.text_primary)),
        Span::styled("i", key_style(theme)),
        Span::styled(" new issue", Style::new().fg(theme.text_muted)),
    ])
}

fn header(title_w: usize, compact: bool, theme: &Theme) -> Line<'static> {
    let mut text = format!(
        " {:<STATUS_W$}{:<ISSUE_W$}{:<title_w$}",
        "STATUS", "ISSUE", "TITLE"
    );
    if compact {
        text.push_str(&format!("{:>CTX_W$}{:>COST_W$}", "CTX", "COST"));
    } else {
        text.push_str(&format!(
            "{:<MODEL_W$}{:<CONTEXT_W$}{:>COST_W$}{:>TIME_W$}",
            "MODEL", "CONTEXT", "COST", "TIME"
        ));
    }
    Line::from(Span::styled(
        text,
        Style::new()
            .fg(theme.text_muted)
            .add_modifier(Modifier::BOLD),
    ))
}

fn row(
    r: &FleetRow,
    title_w: usize,
    compact: bool,
    theme: &Theme,
    nerd_font: bool,
) -> Line<'static> {
    let muted = Style::new().fg(theme.text_muted);
    let text = Style::new().fg(theme.text_primary);
    let chip = status_chip_for_mode(r.status, theme, nerd_font);
    let gap = STATUS_W.saturating_sub(chip.content.width()).max(1);
    let issue = r.issue.map(|n| format!("#{n}")).unwrap_or_default();
    let title = ellipsize(&sanitize_for_terminal(&r.title), title_w.saturating_sub(1));
    let pct = format!("{:.0}%", (r.context.clamp(0.0, 1.0) * 100.0));
    let ctx_style = context_style(r.context, theme);
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(format!("{}{}", chip.content, " ".repeat(gap)), chip.style),
        Span::styled(
            format!("{issue:<ISSUE_W$}"),
            Style::new()
                .fg(theme.accent_identifier)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{title:<title_w$}"), text),
    ];
    if compact {
        spans.push(Span::styled(format!("{pct:>CTX_W$}"), ctx_style));
    } else {
        let model = ellipsize(&sanitize_for_terminal(&r.model), MODEL_W - 1);
        let (filled, empty) = bar(r.context, CONTEXT_BAR_W, nerd_font);
        spans.push(Span::styled(format!("{model:<MODEL_W$}"), muted));
        spans.push(Span::styled(filled, ctx_style));
        spans.push(Span::styled(empty, muted));
        spans.push(Span::styled(format!(" {pct:>4} "), ctx_style));
    }
    spans.push(Span::styled(format!("{:>COST_W$}", money(r.cost)), text));
    if !compact {
        spans.push(Span::styled(format!("{:>TIME_W$}", r.elapsed), muted));
    }
    Line::from(spans)
}

/// Context keeps its own 50% / 70% steps (the overflow actions), not
/// `gauge_level`; see `docs/tui-design-system.md`.
fn context_style(context: f64, theme: &Theme) -> Style {
    let color = if context >= 0.7 {
        theme.gauge_high
    } else if context >= 0.5 {
        theme.gauge_medium
    } else {
        theme.gauge_low
    };
    Style::new().fg(color)
}

#[cfg(test)]
#[path = "fleet_table_tests.rs"]
mod tests;
