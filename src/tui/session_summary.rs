use crate::tui::app::types::{CompletionSummaryData, SessionSummaryState};
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

pub fn draw_session_summary(
    f: &mut Frame,
    summary: &CompletionSummaryData,
    state: Option<&SessionSummaryState>,
    area: Rect,
    theme: &Theme,
) {
    let scroll_offset = state.map(|s| s.scroll_offset).unwrap_or(0);
    let selected_index = state.map(|s| s.selected_index).unwrap_or(0);
    let expanded = state.map(|s| &s.expanded);

    let title = format!("Session Summary ({} sessions)", summary.session_count);
    let block = theme
        .styled_block(&title, true)
        .border_style(Style::default().fg(theme.border_focused));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled(
            format!("Total Cost: ${:.2}", summary.total_cost_usd),
            Style::default()
                .fg(theme.accent_warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} sessions", summary.session_count),
            Style::default().fg(theme.text_secondary),
        ),
    ]));
    lines.push(Line::raw(""));

    for (i, sl) in summary.sessions.iter().enumerate() {
        let status_color = theme.status_color(sl.status);
        let is_expanded = expanded
            .map(|e| e.contains(&sl.session_id))
            .unwrap_or(false);
        let is_selected = i == selected_index;

        let expand_marker = if is_expanded { "\u{f078}" } else { "\u{f054}" };
        let select_marker = if is_selected { ">" } else { " " };

        lines.push(Line::from(vec![
            Span::styled(
                format!("{}{} ", select_marker, expand_marker),
                Style::default().fg(if is_selected {
                    theme.border_focused
                } else {
                    theme.text_muted
                }),
            ),
            Span::styled(
                format!("{} {} ", sl.status.symbol(), sl.status.label()),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(sl.label.clone(), Style::default().fg(theme.text_primary)),
            Span::raw("  "),
            Span::styled(
                format!("${:.2}", sl.cost_usd),
                Style::default().fg(theme.accent_warning),
            ),
            Span::raw("  "),
            Span::styled(
                sl.elapsed.clone(),
                Style::default().fg(theme.text_secondary),
            ),
        ]));

        if is_expanded {
            if !sl.model.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("   Model: {}", sl.model),
                    Style::default().fg(theme.text_secondary),
                )));
            }
            if !sl.pr_link.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("   PR: {}", sl.pr_link),
                    Style::default().fg(theme.accent_info),
                )));
            }
            if !sl.error_summary.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("   Error: {}", sl.error_summary),
                    Style::default().fg(theme.accent_error),
                )));
            }
            for gf in &sl.gate_failures {
                lines.push(Line::from(Span::styled(
                    format!("   Gate [{}]: {}", gf.gate, gf.message),
                    Style::default().fg(theme.accent_error),
                )));
            }
            lines.push(Line::raw(""));
        }
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("[Esc]", Style::default().fg(theme.keybind_key)),
        Span::raw(" back  "),
        Span::styled("[Enter]", Style::default().fg(theme.keybind_key)),
        Span::raw(" expand  "),
        Span::styled("[j/k]", Style::default().fg(theme.keybind_key)),
        Span::raw(" navigate"),
    ]));

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .scroll((scroll_offset, 0));
    f.render_widget(paragraph, inner);
}
