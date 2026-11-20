//! Home's Wide Insight column card and the first-run logo (#1279,
//! `docs/design-system/index.html` §02 `whatsNew()`, finding 5).

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::WhatsNew;
use crate::tui::ds::bar_row::key_style;
use crate::tui::ds::panel::{Focus, panel};
use crate::tui::screens::landing::draw::{ellipsize, header_for_width};
use crate::tui::theme::Theme;

/// Rows Mission Control keeps under the logo; shorter bodies skip it.
const BODY_UNDER_LOGO: u16 = 24;

/// The What's new card: one line per highlight of the installed version,
/// then `n all notes` (no key hints in Home panel titles). Sized to its
/// lines; the rest of `area` stays blank.
pub(super) fn whats_new(f: &mut Frame, area: Rect, card: &WhatsNew, theme: &Theme) {
    let muted = Style::new().fg(theme.text_muted);
    let rows = card.items.len().max(1) as u16 + 3;
    let [area, _] = Layout::vertical([Constraint::Length(rows), Constraint::Min(0)]).areas(area);
    let block = panel(
        &format!("What's new v{}", card.version),
        Focus::Inactive,
        theme,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    let width = inner.width as usize;
    let mut lines: Vec<Line> = if card.items.is_empty() {
        vec![Line::styled(
            format!(" No notes for v{}.", card.version),
            muted,
        )]
    } else {
        let plus = Style::new()
            .fg(theme.accent_success)
            .add_modifier(Modifier::BOLD);
        card.items
            .iter()
            .map(|item| {
                Line::from(vec![
                    Span::styled(" + ", plus),
                    Span::styled(
                        ellipsize(item, width.saturating_sub(3)),
                        Style::new().fg(theme.text_primary),
                    ),
                ])
            })
            .collect()
    };
    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled("n", key_style(theme)),
        Span::styled(" all notes", muted),
    ]));
    f.render_widget(Paragraph::new(lines), inner);
}

/// First run only: the big logo and version on top of Home, when the body
/// still has [`BODY_UNDER_LOGO`] rows under it. Returns the area left for
/// Mission Control.
pub(super) fn first_run_logo(f: &mut Frame, area: Rect, theme: &Theme, nerd_font: bool) -> Rect {
    let header = header_for_width(nerd_font, area.width);
    // Logo, version, one blank row.
    let logo_h = header.len() as u16 + 2;
    if area.height < logo_h + BODY_UNDER_LOGO {
        return area;
    }
    let [logo, rest] =
        Layout::vertical([Constraint::Length(logo_h), Constraint::Min(0)]).areas(area);
    let style = Style::new()
        .fg(theme.accent_success)
        .add_modifier(Modifier::BOLD);
    let mut lines: Vec<Line> = header.iter().map(|l| Line::styled(*l, style)).collect();
    lines.push(Line::styled(
        format!("v{}", env!("CARGO_PKG_VERSION")),
        Style::new().fg(theme.text_secondary),
    ));
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), logo);
    rest
}
