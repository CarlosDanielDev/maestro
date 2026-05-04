use crate::flags::FlagSource;
use crate::tui::icons::{self, IconId};
use crate::tui::screens::{draw_keybinds_bar, sanitize_for_terminal};
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{CAVEMAN_LABEL, SettingsScreen, SettingsTab, caveman_row};

/// Returns the formatted single-line message for a flash slot if the slot is
/// non-empty and within the 5-second visibility window.
fn active_flash_message(slot: &Option<(String, std::time::Instant)>) -> Option<String> {
    slot.as_ref()
        .filter(|(_, t)| t.elapsed().as_secs() < 5)
        .map(|(m, _)| {
            let first = m.lines().next().unwrap_or(m);
            crate::tui::ui::truncate_str(&sanitize_for_terminal(first), 80).into_owned()
        })
}
impl SettingsScreen {
    fn draw_tab_bar(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut spans = Vec::new();
        for (i, tab) in SettingsTab::ALL.iter().enumerate() {
            let style = if i == self.active_tab {
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(theme.text_secondary)
            };
            if i > 0 {
                spans.push(Span::styled(
                    " │ ",
                    Style::default().fg(theme.border_inactive),
                ));
            }
            spans.push(Span::styled(tab.label(), style));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn field_height(&self, tab: usize, field_idx: usize) -> u16 {
        if self
            .feedback_for(tab, field_idx)
            .is_some_and(|fb| !fb.message.is_empty())
        {
            2
        } else {
            1
        }
    }

    fn draw_fields(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let visible_height = area.height;
        let field_count = self.field_count();
        let field_index = self.field_index;
        let tab = self.active_tab;

        // Compute cumulative heights to determine scroll position
        let field_heights: Vec<u16> = (0..field_count)
            .map(|i| self.field_height(tab, i))
            .collect();

        // Adjust scroll so the focused field is visible
        if field_index < self.scroll_offset {
            self.scroll_offset = field_index;
        }
        // Scroll down if focused is below viewport
        loop {
            let mut y: u16 = 0;
            for i in self.scroll_offset..=field_index.min(field_count.saturating_sub(1)) {
                y += field_heights.get(i).copied().unwrap_or(1);
            }
            if y > visible_height && self.scroll_offset < field_index {
                self.scroll_offset += 1;
            } else {
                break;
            }
        }

        let scroll_offset = self.scroll_offset;
        let active_tab = self.active_tab();
        let caveman_state = &self.caveman_state;
        let fields = &self.fields_per_tab[tab];
        let mut y_offset: u16 = 0;
        for (field_idx, field) in fields.iter().enumerate().skip(scroll_offset) {
            let h = field_heights.get(field_idx).copied().unwrap_or(1);
            if y_offset + h > visible_height {
                break;
            }
            let focused = field_idx == field_index;
            let field_area = Rect {
                x: area.x,
                y: area.y + y_offset,
                width: area.width,
                height: h,
            };
            if active_tab == SettingsTab::Advanced && field.widget.label() == CAVEMAN_LABEL {
                caveman_row::render_caveman_row(f, field_area, caveman_state, focused, theme);
            } else {
                let validation = self.feedback_for(tab, field_idx).cloned();
                field
                    .widget
                    .draw(f, field_area, theme, focused, validation.as_ref());
            }
            y_offset += h;
        }
    }

    fn draw_feature_flags(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let flags = self.feature_flags.all_with_source();

        // Header row
        let header_style = Style::default()
            .fg(theme.text_secondary)
            .add_modifier(Modifier::BOLD);
        let header = Line::from(vec![
            Span::styled(format!("  {:<22}", "Flag"), header_style),
            Span::styled(format!("{:<10}", "State"), header_style),
            Span::styled(format!("{:<12}", "Source"), header_style),
            Span::styled("Description", header_style),
        ]);
        if area.height > 0 {
            f.render_widget(Paragraph::new(header), Rect { height: 1, ..area });
        }

        let data_area = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        };

        for (i, (flag, enabled, source)) in flags.iter().enumerate() {
            if i >= data_area.height as usize {
                break;
            }
            let focused = i == self.flags_selected;
            let (state_label, state_style) = if *enabled {
                ("+ ON ", Style::default().fg(theme.accent_success))
            } else {
                ("- OFF", Style::default().fg(theme.text_muted))
            };
            let source_label = match source {
                FlagSource::Default => "default",
                FlagSource::Config => "config",
                FlagSource::Cli => "CLI",
            };
            let row_style = if focused {
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD)
            } else if *enabled {
                Style::default().fg(theme.text_primary)
            } else {
                Style::default().fg(theme.text_muted)
            };
            let prefix = if focused {
                format!("{} ", icons::get(IconId::Selector))
            } else {
                "  ".to_string()
            };

            let line = Line::from(vec![
                Span::styled(format!("{}{:<22}", prefix, flag.name()), row_style),
                Span::styled(format!("{:<10}", state_label), state_style),
                Span::styled(format!("{:<12}", source_label), row_style),
                Span::styled(flag.description(), row_style),
            ]);
            let row_area = Rect {
                y: data_area.y + i as u16,
                height: 1,
                ..data_area
            };
            f.render_widget(Paragraph::new(line), row_area);
        }
    }

    pub(super) fn draw_screen(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let error_msg = active_flash_message(&self.save_error_flash);
        let caveman_msg = active_flash_message(&self.caveman_status_flash);
        let error_title;
        let caveman_title;
        let (title, title_color) = if let Some(msg) = error_msg {
            error_title = format!(" Settings [Save failed: {}] ", msg);
            (error_title.as_str(), theme.accent_error)
        } else if let Some(msg) = caveman_msg {
            caveman_title = format!(" Settings [{}] ", msg);
            (caveman_title.as_str(), theme.accent_success)
        } else if self.has_validation_errors() {
            (" Settings [Errors] ", theme.accent_error)
        } else if self.is_dirty() {
            (" Settings [Modified] ", theme.accent_success)
        } else if self.save_flash.is_some_and(|t| t.elapsed().as_secs() < 2) {
            (" Settings [Saved] ", theme.accent_success)
        } else {
            (" Settings ", theme.accent_success)
        };

        let block = theme
            .styled_block(title, false)
            .border_style(Style::default().fg(title_color));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if inner.height < 4 || inner.width < 20 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // tab bar
                Constraint::Length(1), // separator
                Constraint::Min(1),    // field list
                Constraint::Length(1), // keybinds
            ])
            .split(inner);

        self.draw_tab_bar(f, chunks[0], theme);

        let sep = "─".repeat(inner.width as usize);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                sep,
                Style::default().fg(theme.border_inactive),
            ))),
            chunks[1],
        );

        if self.active_tab() == SettingsTab::Flags {
            self.draw_feature_flags(f, chunks[2], theme);
        } else {
            self.draw_fields(f, chunks[2], theme);
        }

        if self.confirm_discard {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        "Unsaved changes. Discard? ",
                        Style::default()
                            .fg(theme.accent_warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("(y/n)", Style::default().fg(theme.text_secondary)),
                ])),
                chunks[3],
            );
        } else if self.active_tab() == SettingsTab::Flags {
            draw_keybinds_bar(
                f,
                chunks[3],
                &[("Tab", "Tab"), ("↑/↓", "Navigate"), ("Esc", "Back")],
                theme,
            );
        } else {
            let edit_hint = self
                .current_fields()
                .get(self.field_index)
                .map(|field| field.widget.edit_hint());
            let mut entries: Vec<(&str, &str)> = Vec::with_capacity(5);
            entries.push(("Tab", "Tab"));
            entries.push(("↑/↓", "Field"));
            if let Some((key, label)) = edit_hint {
                entries.push((key, label));
            }
            entries.push(("Ctrl+s", "Save"));
            entries.push(("Esc", "Back"));
            draw_keybinds_bar(f, chunks[3], &entries, theme);
        }
    }
}
