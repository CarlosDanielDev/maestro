//! `ds::checklist_panel` and `ds::ready_strip`: the live readiness checklist
//! (`checklistPanel` / `readyStrip` in `docs/design-system/index.html`, §02.2
//! Issue wizard and §04 Kit v2). The panel sits beside a form on Wide; the
//! strip is its one-line form on Regular and Compact.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::theme::Theme;

/// State of one checklist row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    /// `✓` done.
    Ok,
    /// `✗` required and missing now.
    Missing,
    /// `!` present but worth a look.
    Warn,
    /// `○` not reached yet (a future step). Never red.
    Pending,
}

impl CheckState {
    /// Glyph and its style. Meaning rides on the glyph and BOLD, so it reads
    /// under NO_COLOR too.
    fn glyph(self, theme: &Theme) -> (&'static str, Style) {
        let fg = |c| Style::default().fg(c);
        match self {
            Self::Ok => ("✓", fg(theme.accent_success)),
            Self::Missing => ("✗", fg(theme.accent_error).add_modifier(Modifier::BOLD)),
            Self::Warn => ("!", fg(theme.accent_warning).add_modifier(Modifier::BOLD)),
            Self::Pending => ("○", fg(theme.text_muted)),
        }
    }
}

/// One checklist row: a label, its state and an optional short note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckItem<'a> {
    pub label: &'a str,
    pub state: CheckState,
    pub note: Option<&'a str>,
}

/// `(ready, total)`: only `Ok` rows are ready; every row counts in the total.
pub fn ready_count(items: &[CheckItem]) -> (usize, usize) {
    let ready = items.iter().filter(|i| i.state == CheckState::Ok).count();
    (ready, items.len())
}

/// `n/m` in success when all are ready, else warning; bold either way.
fn count_span(items: &[CheckItem], theme: &Theme) -> Span<'static> {
    let (ready, total) = ready_count(items);
    let colour = if ready == total {
        theme.accent_success
    } else {
        theme.accent_warning
    };
    Span::styled(
        format!("{ready}/{total}"),
        Style::default().fg(colour).add_modifier(Modifier::BOLD),
    )
}

/// Draw the checklist as a panel titled `title`, with `n/m ready` on the
/// top-right border.
pub fn checklist_panel(
    f: &mut Frame,
    area: Rect,
    title: &str,
    items: &[CheckItem],
    focused: bool,
    theme: &Theme,
) {
    let muted = Style::default().fg(theme.text_muted);
    let block = theme.styled_block(title, focused).title_top(
        Line::from(vec![
            count_span(items, theme),
            Span::styled(" ready ", muted),
        ])
        .right_aligned(),
    );
    let rows: Vec<Line> = items
        .iter()
        .map(|it| {
            let (glyph, style) = it.state.glyph(theme);
            let label = match it.state {
                CheckState::Pending => muted,
                _ => Style::default().fg(theme.text_primary),
            };
            let mut spans = vec![
                Span::styled(format!(" {glyph} "), style),
                Span::styled(it.label, label),
            ];
            if let Some(note) = it.note {
                let note_style = match it.state {
                    CheckState::Missing => Style::default().fg(theme.accent_error),
                    _ => muted,
                };
                spans.push(Span::styled(format!("  {note}"), note_style));
            }
            Line::from(spans)
        })
        .collect();
    f.render_widget(Paragraph::new(rows).block(block), area);
}

/// One line: `{label} n/m ready   ✗ Missing note   ! Warn note`, cut with `…`
/// at `width`. `compact` hides the notes.
pub fn ready_strip(
    label: &str,
    items: &[CheckItem],
    compact: bool,
    width: u16,
    theme: &Theme,
) -> Line<'static> {
    const MORE: &str = "   …";
    let muted = Style::default().fg(theme.text_muted);
    let width = usize::from(width);
    let mut spans = vec![
        Span::styled(format!(" {label} "), muted),
        count_span(items, theme),
        Span::styled(" ready", muted),
    ];
    let mut used: usize = spans.iter().map(Span::width).sum();
    let problems: Vec<&CheckItem> = items
        .iter()
        .filter(|i| matches!(i.state, CheckState::Missing | CheckState::Warn))
        .collect();
    for (n, it) in problems.iter().enumerate() {
        let (glyph, style) = it.state.glyph(theme);
        let mut seg = vec![
            Span::styled(format!("   {glyph} "), style),
            Span::styled(
                it.label.to_string(),
                Style::default().fg(theme.text_primary),
            ),
        ];
        if let (Some(note), false) = (it.note, compact) {
            let note_style = match it.state {
                CheckState::Missing => Style::default().fg(theme.accent_error),
                _ => muted,
            };
            seg.push(Span::styled(format!(" {note}"), note_style));
        }
        let seg_width: usize = seg.iter().map(Span::width).sum();
        // Keep room for the `…` unless this is the last problem row.
        let room = if n + 1 == problems.len() {
            width
        } else {
            width.saturating_sub(MORE.chars().count())
        };
        if used + seg_width > room {
            if used + MORE.chars().count() <= width {
                spans.push(Span::styled(MORE, muted));
            }
            break;
        }
        used += seg_width;
        spans.extend(seg);
    }
    Line::from(spans)
}

#[cfg(test)]
#[path = "checklist_tests.rs"]
mod tests;
