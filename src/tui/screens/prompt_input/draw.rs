use super::super::draw_keybinds_bar;
use super::super::wrap::{scroll_offset_for_cursor, wrap_lines};
use super::PromptInputScreen;
use crate::tui::icons::{self, IconId};
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

impl PromptInputScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let show_toggle = self.detected_issue_numbers.len() >= 2;
        let toggle_height = if show_toggle { 1 } else { 0 };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(6),                // prompt editor
                Constraint::Length(toggle_height), // unified PR toggle (conditional)
                Constraint::Length(8),             // image list
                Constraint::Length(1),             // keybinds bar
            ])
            .split(area);

        // Prompt editor — custom wrapped rendering
        let editor_title = match self.history_indicator() {
            Some(ref indicator) => format!("Compose Prompt [history {}]", indicator),
            None => "Compose Prompt".to_string(),
        };
        let editor_block = theme.styled_block(&editor_title, self.is_prompt_editor_focused());
        let inner = editor_block.inner(chunks[0]);

        // Read logical lines and cursor from the editing backend
        let logical_lines: Vec<String> = self.editor.lines().to_vec();
        let (cursor_row, cursor_col) = self.editor.cursor();

        // Compute wrapped lines and visual cursor position
        let wrap_result = wrap_lines(&logical_lines, (cursor_row, cursor_col), inner.width);
        let scroll = scroll_offset_for_cursor(wrap_result.cursor.0 as usize, inner.height as usize);

        // Build styled visual lines with issue reference highlighting
        let visual_lines: Vec<Line> = wrap_result
            .lines
            .iter()
            .map(|s| {
                Line::from(crate::tui::issue_refs::highlight_issue_refs(
                    s.as_str(),
                    theme.accent_identifier,
                    theme.text_primary,
                ))
            })
            .collect();

        // Render placeholder when empty
        let paragraph = if logical_lines.len() == 1 && logical_lines[0].is_empty() {
            Paragraph::new(vec![Line::from(Span::styled(
                "Type your prompt here...",
                Style::default().fg(theme.text_secondary),
            ))])
        } else {
            Paragraph::new(visual_lines)
        };

        f.render_widget(
            paragraph.block(editor_block).scroll((scroll as u16, 0)),
            chunks[0],
        );

        // Place cursor manually
        if self.is_prompt_editor_focused() {
            let cursor_visual_row = wrap_result.cursor.0 as usize;
            let cursor_visual_col = wrap_result.cursor.1;
            if cursor_visual_row >= scroll && (cursor_visual_row - scroll) < inner.height as usize {
                f.set_cursor_position(Position::new(
                    inner.x + cursor_visual_col,
                    inner.y + (cursor_visual_row - scroll) as u16,
                ));
            }
        }

        // Image list
        let image_title = format!("Attachments ({})", self.image_paths.len());
        let image_block = theme.styled_block(&image_title, self.is_image_list_focused());

        let mut lines: Vec<Line> = Vec::new();
        if self.image_paths.is_empty() && !self.editing_image_path {
            lines.push(Line::from(Span::styled(
                "  (no images attached)",
                Style::default().fg(theme.text_secondary),
            )));
        }
        for (i, path) in self.image_paths.iter().enumerate() {
            let style = if i == self.selected_image && self.is_image_list_focused() {
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            let prefix = if i == self.selected_image && self.is_image_list_focused() {
                " > "
            } else {
                "   "
            };
            lines.push(Line::from(Span::styled(
                format!("{}{}", prefix, path),
                style,
            )));
        }
        if self.editing_image_path {
            lines.push(Line::from(vec![
                Span::styled("  Path: ", Style::default().fg(theme.accent_warning)),
                Span::styled(
                    &self.image_path_input,
                    Style::default().fg(theme.text_primary),
                ),
                Span::styled("_", Style::default().fg(theme.accent_success)),
            ]));
        }
        if self.is_image_list_focused() && !self.editing_image_path {
            lines.push(Line::from(Span::styled(
                "  [a] Add   [d] Remove   [Ctrl+V] Paste",
                Style::default().fg(theme.text_secondary),
            )));
        }

        // Unified PR toggle (conditional)
        if show_toggle {
            crate::tui::widgets::unified_pr_toggle::draw_unified_pr_toggle(
                f,
                chunks[1],
                self.unified_pr,
                theme,
            );
        }

        let image_list = Paragraph::new(lines).block(image_block);
        f.render_widget(image_list, chunks[2]);

        // Status message or keybinds bar
        let history_keys = format!(
            "{}/{}",
            icons::get(IconId::ArrowUp),
            icons::get(IconId::ArrowDown)
        );
        if let Some(ref msg) = self.status_message {
            let status = Paragraph::new(Line::from(Span::styled(
                format!(" {} ", msg),
                Style::default().fg(theme.accent_warning),
            )));
            f.render_widget(status, chunks[3]);
        } else if show_toggle {
            draw_keybinds_bar(
                f,
                chunks[3],
                &[
                    ("Enter", "Submit"),
                    ("Ctrl+U", "Unified PR"),
                    ("Ctrl+J", "New line"),
                    (&history_keys, "History"),
                    ("Ctrl+V", "Paste"),
                    ("Tab", "Switch"),
                    ("Esc", "Cancel"),
                ],
                theme,
            );
        } else {
            draw_keybinds_bar(
                f,
                chunks[3],
                &[
                    ("Enter", "Submit"),
                    ("Ctrl+J", "New line"),
                    (&history_keys, "History"),
                    ("Ctrl+V", "Paste"),
                    ("Tab", "Switch"),
                    ("Esc", "Cancel"),
                ],
                theme,
            );
        }
    }
}
