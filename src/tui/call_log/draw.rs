//! Render function for the call-log viewer (#868).

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::session::types::Session;
use crate::tui::call_log::format_row;
use crate::tui::call_log::state::CallLogState;
use crate::tui::theme::Theme;

const FOOTER_HEIGHT: u16 = 1;

/// Render the call-log pane: header, list, optional payload split, footer
/// hint row. Pure render — does not mutate `state` (the renderer clamps
/// `selected` via a local copy before drawing so the cursor never points
/// past a drained entry).
pub fn draw_call_log(
    f: &mut Frame<'_>,
    session: &Session,
    state: &CallLogState,
    area: Rect,
    theme: &Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(FOOTER_HEIGHT),
        ])
        .split(area);

    draw_header(f, session, chunks[0], theme);
    draw_body(f, session, state, chunks[1], theme);
    draw_footer(f, state.expanded, state.follow_tail, chunks[2], theme);
}

fn draw_header(f: &mut Frame<'_>, session: &Session, area: Rect, theme: &Theme) {
    let issue = session
        .issue_number
        .map(|n| format!(" • #{n}"))
        .unwrap_or_default();
    let title = format!("Call Log — {} events{}", session.call_log.len(), issue,);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme.title_accent)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(theme.border_active));
    f.render_widget(block, area);
}

fn draw_body(
    f: &mut Frame<'_>,
    session: &Session,
    state: &CallLogState,
    area: Rect,
    theme: &Theme,
) {
    if session.call_log.is_empty() {
        draw_empty_state(f, area, theme);
        return;
    }

    if state.expanded {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        draw_list(f, session, state, chunks[0], theme);
        draw_payload(f, session, state, chunks[1], theme);
    } else {
        draw_list(f, session, state, area, theme);
    }
}

fn draw_list(
    f: &mut Frame<'_>,
    session: &Session,
    state: &CallLogState,
    area: Rect,
    theme: &Theme,
) {
    let total = session.call_log.len();
    let mut selected = state.selected;
    if selected >= total {
        selected = total.saturating_sub(1);
    }

    let inner_height = area.height.saturating_sub(2) as usize;
    let scroll_offset = visible_offset(selected, state.list_scroll as usize, inner_height);

    let lines: Vec<Line<'_>> = session
        .call_log
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(inner_height)
        .map(|(idx, entry)| format_row(entry, idx == selected, theme))
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            format!(" {} / {} ", selected + 1, total),
            Style::default().fg(theme.text_secondary),
        ))
        .border_style(Style::default().fg(theme.border_inactive));
    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);
}

fn visible_offset(selected: usize, requested_offset: usize, inner_height: usize) -> usize {
    if inner_height == 0 {
        return selected;
    }
    if selected < requested_offset {
        return selected;
    }
    let max_visible = requested_offset + inner_height;
    if selected >= max_visible {
        return selected + 1 - inner_height;
    }
    requested_offset
}

fn draw_payload(
    f: &mut Frame<'_>,
    session: &Session,
    state: &CallLogState,
    area: Rect,
    theme: &Theme,
) {
    let total = session.call_log.len();
    let selected = state.selected.min(total.saturating_sub(1));
    let Some(entry) = session.call_log.get(selected) else {
        return;
    };
    let title = format!(
        " Payload ({} @ {}) ",
        entry.kind.label(),
        entry.timestamp.format("%H:%M:%S"),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, Style::default().fg(theme.title_accent)))
        .border_style(Style::default().fg(theme.border_focused));
    let para = Paragraph::new(entry.payload_json.clone())
        .block(block)
        .style(Style::default().fg(theme.text_primary))
        .wrap(Wrap { trim: false })
        .scroll((state.payload_scroll, 0));
    f.render_widget(para, area);
}

fn draw_empty_state(f: &mut Frame<'_>, area: Rect, theme: &Theme) {
    let lines = vec![
        Line::from(Span::styled(
            "No events yet",
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "The agent has not produced any tool calls or messages.",
            Style::default().fg(theme.text_secondary),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press [Esc] to return to Detail.",
            Style::default().fg(theme.text_secondary),
        )),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_inactive));
    let para = Paragraph::new(lines)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(para, area);
}

fn draw_footer(f: &mut Frame<'_>, expanded: bool, follow_tail: bool, area: Rect, theme: &Theme) {
    let base = if expanded {
        " [Esc] Back  [Enter] Collapse  [j/k] Scroll payload  [↑/↓] Move  [g/G] Top/Bottom "
    } else {
        " [Esc] Back  [Enter] Expand  [j/k] [↑/↓] Move  [g/G] Top/Bottom "
    };
    let follow = if follow_tail {
        " [f] Follow: ON "
    } else {
        " [f] Follow: off "
    };
    let hint = format!("{base} {follow}");
    let para = Paragraph::new(Line::from(Span::styled(
        hint,
        Style::default().fg(theme.text_secondary),
    )));
    f.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_offset_keeps_offset_when_selected_inside_window() {
        assert_eq!(visible_offset(5, 3, 10), 3);
    }

    #[test]
    fn visible_offset_shifts_down_when_selected_below_window() {
        // window [3, 13), selected = 15 → offset = 6 so 15 sits at the last row.
        assert_eq!(visible_offset(15, 3, 10), 6);
    }

    #[test]
    fn visible_offset_shifts_up_when_selected_above_window() {
        assert_eq!(visible_offset(1, 5, 10), 1);
    }

    #[test]
    fn visible_offset_zero_height_returns_selected() {
        assert_eq!(visible_offset(7, 3, 0), 7);
    }
}
