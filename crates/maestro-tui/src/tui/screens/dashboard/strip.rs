//! The Dashboard view strip (§02.1, split out of `draw.rs` for the file
//! size budget): `o Ops · c Cost · t Tokens · g Deps`, Deps' lens switch
//! (#1292), and the summary on the right.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::usage::View;
use super::{Dashboard, DashboardView};
use crate::tui::ds::bar_row::key_style;
use crate::tui::ds::breakpoint::Breakpoint;
use crate::tui::theme::Theme;
use crate::tui::widgets::focused_selection_style;

/// `o Ops · c Cost · t Tokens · g Deps`, the active view highlighted; on
/// Deps the lens switch follows (#1292). Right side: the Ops summary, the
/// Deps counts, or the scope of every Usage number (`this run · since
/// 10:00 UTC`).
pub(super) fn draw_view_strip(
    f: &mut Frame,
    area: Rect,
    view: &DashboardView<'_>,
    dash: &Dashboard,
    bp: Breakpoint,
    theme: &Theme,
) {
    let active = dash.view;
    let sel = focused_selection_style(theme);
    let muted = Style::new().fg(theme.text_muted);
    let mut spans = vec![Span::raw(" ")];
    for (i, (k, label, v)) in [
        ("o", "Ops", Some(View::Ops)),
        ("c", "Cost", Some(View::Cost)),
        ("t", "Tokens", Some(View::Tokens)),
        ("g", "Deps", Some(View::Deps)),
    ]
    .into_iter()
    .enumerate()
    {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        if v == Some(active) {
            spans.push(Span::styled(format!(" {k} {label} "), sel));
            continue;
        }
        let key = if v.is_some() { key_style(theme) } else { muted };
        spans.push(Span::styled(k, key));
        spans.push(Span::styled(format!(" {label}"), muted));
    }
    if active == View::Deps {
        spans.extend(super::deps::lens_spans(dash, view.files_enabled, theme));
    }
    let left = Line::from(spans);
    let left_w = left.width() as u16;
    f.render_widget(Paragraph::new(left), area);
    if bp == Breakpoint::Compact {
        return;
    }
    let summary = if active == View::Deps {
        super::deps::summary(view)
    } else if active == View::Ops {
        let live = view
            .sessions
            .iter()
            .filter(|s| !s.status.is_terminal())
            .count();
        let pipe = view.ci.len() + view.pending.len();
        format!("{live} live · {pipe} in PR pipeline ")
    } else {
        format!("{} ", super::usage::scope_line(&view.usage))
    };
    let w = summary.width() as u16;
    if w < area.width / 2 && left_w + w < area.width {
        let r = Rect::new(area.x + area.width - w, area.y, w, 1);
        f.render_widget(Paragraph::new(Span::styled(summary, muted)), r);
    }
}
