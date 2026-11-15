//! Rendering routines for [`super::DynamicRowsWidget`]. Split from the
//! widget's state machine to keep each file ≤ 400 LOC per RUST-GUARDRAILS §1.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Row, Table},
};

use crate::tui::theme::Theme;
use crate::tui::widgets::WidgetKind;

use super::dynamic_rows::DynamicRowsWidget;

pub(super) fn draw(
    widget: &DynamicRowsWidget,
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    _focused: bool,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    let header_area = chunks[0];
    let inner = chunks[1];

    let header = Paragraph::new(Line::from(Span::styled(
        format!("{}:", widget.label),
        Style::default()
            .fg(theme.text_secondary)
            .add_modifier(Modifier::BOLD),
    )));
    f.render_widget(header, header_area);

    if widget.rows().is_empty() {
        let hint = Paragraph::new(vec![
            Line::from(Span::styled(
                "No rows yet.",
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "[a] add first row",
                Style::default().fg(theme.text_muted),
            )),
        ]);
        f.render_widget(hint, inner);
    } else {
        let headers: Vec<&str> = widget.entry_fields.iter().map(|f| f.key).collect();
        let widths: Vec<Constraint> = headers.iter().map(|_| Constraint::Min(8)).collect();
        let mut rows: Vec<Row> = Vec::with_capacity(widget.rows().len());
        for (idx, entry) in widget.rows().iter().enumerate() {
            let cells: Vec<String> = entry
                .fields
                .iter()
                .map(|sf| widget_display(&sf.widget))
                .collect();
            let style = if Some(idx) == widget.focused_row() {
                Style::default()
                    .fg(theme.selection_fg)
                    .bg(theme.selection_bg)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(theme.text_primary)
            };
            rows.push(Row::new(cells).style(style));
        }
        let table = Table::new(rows, widths).header(
            Row::new(
                headers
                    .into_iter()
                    .map(|h| h.to_string())
                    .collect::<Vec<_>>(),
            )
            .style(
                Style::default()
                    .fg(theme.text_secondary)
                    .add_modifier(Modifier::BOLD),
            ),
        );
        f.render_widget(table, inner);
    }

    if widget.undo_active()
        && let Some(label) = widget.undo_label()
    {
        let banner_y = area.y + area.height.saturating_sub(1);
        let banner_area = Rect::new(area.x, banner_y, area.width, 1);
        let banner = Paragraph::new(Line::from(Span::styled(
            format!("Removed '{}' — [u] to undo", label),
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        )));
        f.render_widget(banner, banner_area);
    }

    if let Some(modal) = widget.add_modal() {
        modal.draw(f, area, theme);
    }
    if let Some(modal) = widget.remove_modal() {
        modal.draw(f, area, theme);
    }
}

fn widget_display(widget: &WidgetKind) -> String {
    match widget {
        WidgetKind::Toggle(w) => {
            if w.value {
                "yes".into()
            } else {
                "no".into()
            }
        }
        WidgetKind::TextInput(w) => w.value.clone(),
        WidgetKind::NumberStepper(w) => w.value.to_string(),
        WidgetKind::Dropdown(w) => w.selected_value().to_string(),
        WidgetKind::ListEditor(w) => w.items.join(","),
        _ => String::new(),
    }
}
