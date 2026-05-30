//! Input pane for the Interaction screen (#736).
//!
//! Renders the contents of the multi-line `tui-textarea` editor inside a
//! titled block. The editor owns the text buffer and cursor logic; this
//! pane only paints its lines (or a placeholder when empty). Submission
//! (`SendTurn`) is wired in #738.

use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use tui_textarea::TextArea;

/// Render the input editor into `area`.
pub(super) fn draw_input(f: &mut Frame, area: Rect, theme: &Theme, editor: &TextArea<'static>) {
    let block = theme.styled_block("Message", true);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = editor.lines();
    let is_empty = lines.len() == 1 && lines[0].is_empty();
    let paragraph = if is_empty {
        Paragraph::new(Line::from(Span::styled(
            "Type a prompt to begin…",
            Style::default().fg(theme.text_secondary),
        )))
    } else {
        let rendered: Vec<Line> = lines
            .iter()
            .map(|l| {
                Line::from(Span::styled(
                    l.clone(),
                    Style::default().fg(theme.text_primary),
                ))
            })
            .collect();
        Paragraph::new(rendered)
    };
    f.render_widget(paragraph, inner);
}
