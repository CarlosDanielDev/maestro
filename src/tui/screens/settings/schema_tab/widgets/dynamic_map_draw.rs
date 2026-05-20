//! Rendering routines for [`super::DynamicMapWidget`]. Split from the
//! widget's state machine to keep each file ≤ 400 LOC per RUST-GUARDRAILS §1.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
};

use crate::tui::theme::Theme;

use super::dynamic_map::{DynamicMapWidget, MapFocus};
use super::entry_state::EntryState;

pub(super) fn draw(
    widget: &DynamicMapWidget,
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    focused: bool,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", widget.label))
        .border_style(Style::default().fg(if focused {
            theme.border_active
        } else {
            theme.border_inactive
        }));
    f.render_widget(block.clone(), area);
    let inner = block.inner(area);

    if widget.entries().is_empty() {
        let hint = Paragraph::new(vec![
            Line::from(Span::styled(
                "No entries yet.",
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "[a] add first entry",
                Style::default().fg(theme.text_muted),
            )),
        ]);
        f.render_widget(hint, inner);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(1)])
            .split(inner);

        let titles: Vec<Line> = widget
            .entries()
            .iter()
            .map(|e| Line::from(e.id.as_str()))
            .collect();
        let tabs = Tabs::new(titles)
            .select(widget.active_index().unwrap_or(0))
            .highlight_style(
                Style::default()
                    .fg(theme.accent_info)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            );
        f.render_widget(tabs, chunks[0]);

        if let Some(entry) = widget.active_entry() {
            draw_entry_fields(f, chunks[1], theme, entry, widget.focus());
        }
    }

    if widget.undo_active()
        && let Some(label) = widget.undo_label()
    {
        let banner_y = area.y + area.height.saturating_sub(1);
        let banner_area = Rect::new(area.x + 1, banner_y, area.width.saturating_sub(2), 1);
        let banner = Paragraph::new(Line::from(vec![Span::styled(
            format!("Removed '{}' — [u] to undo", label),
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        )]));
        f.render_widget(banner, banner_area);
    }

    if let Some(modal) = widget.add_modal() {
        modal.draw(f, area, theme);
    }
    if let Some(modal) = widget.remove_modal() {
        modal.draw(f, area, theme);
    }
}

fn draw_entry_fields(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    entry: &EntryState,
    focus: &MapFocus,
) {
    let n = entry.fields.len() as u16;
    if n == 0 {
        return;
    }
    let constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Length(1)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    for (idx, sf) in entry.fields.iter().enumerate() {
        let focused = matches!(focus, MapFocus::EntryField(n) if *n == idx);
        sf.widget.draw(f, rows[idx], theme, focused, None);
    }
}
