use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::KeyBindingGroup;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
};

/// State for the full-screen help overlay.
pub struct HelpOverlayState {
    pub scroll: u16,
    pub search_query: String,
    pub search_active: bool,
}

impl HelpOverlayState {
    pub fn new() -> Self {
        Self {
            scroll: 0,
            search_query: String::new(),
            search_active: false,
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn page_down(&mut self) {
        self.scroll = self.scroll.saturating_add(10);
    }

    pub fn page_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(10);
    }

    pub fn toggle_search(&mut self) {
        self.search_active = !self.search_active;
    }

    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.search_active = false;
    }

    pub fn push_char(&mut self, c: char) {
        self.search_query.push(c);
    }

    pub fn pop_char(&mut self) {
        self.search_query.pop();
    }
}

impl Default for HelpOverlayState {
    fn default() -> Self {
        Self::new()
    }
}

/// Filter keybinding groups by a search query (case-insensitive).
/// Returns only groups that have matching bindings, with non-matching bindings removed.
pub fn filter_bindings(groups: &[KeyBindingGroup], query: &str) -> Vec<KeyBindingGroup> {
    if query.is_empty() {
        return groups.to_vec();
    }
    let query_lower = query.to_lowercase();
    groups
        .iter()
        .filter_map(|group| {
            let matching: Vec<_> = group
                .bindings
                .iter()
                .filter(|b| {
                    b.key.to_lowercase().contains(&query_lower)
                        || b.description.to_lowercase().contains(&query_lower)
                })
                .cloned()
                .collect();
            if matching.is_empty() {
                None
            } else {
                Some(KeyBindingGroup {
                    title: group.title,
                    bindings: matching,
                })
            }
        })
        .collect()
}

pub fn draw_help_overlay_with_search(
    f: &mut Frame,
    area: Rect,
    mode_km: &crate::tui::navigation::keymap::ModeKeyMap,
    input_mode: InputMode,
    scroll: u16,
    search_query: &str,
    theme: &Theme,
) {
    let popup = centered_rect(90, 90, area);

    f.render_widget(Clear, popup);

    let groups = filter_bindings(&mode_km.help_groups, search_query);

    let title = format!("Help — {} Mode", mode_km.mode_label);
    let mut help_text = vec![
        Line::from(vec![Span::styled(
            title,
            Style::default()
                .fg(theme.accent_info)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        // Show current input mode
        Line::from(vec![
            Span::styled("  Mode: ", Style::default().fg(theme.text_secondary)),
            Span::styled(
                match input_mode {
                    InputMode::Normal => "Normal",
                    InputMode::Insert => "Insert",
                },
                Style::default()
                    .fg(theme.accent_warning)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    // Show search bar if query is active
    if !search_query.is_empty() {
        help_text.push(Line::from(vec![
            Span::styled("  Search: ", Style::default().fg(theme.accent_info)),
            Span::styled(
                search_query,
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        help_text.push(Line::from(""));
    }

    render_binding_groups(&mut help_text, &groups, theme);

    // Footer with controls
    help_text.push(Line::from(vec![
        Span::styled("[j/k]", Style::default().fg(theme.accent_success)),
        Span::raw(" Scroll  "),
        Span::styled("[/]", Style::default().fg(theme.accent_success)),
        Span::raw(" Search  "),
        Span::styled("[?/F1/Esc]", Style::default().fg(theme.accent_success)),
        Span::raw(" Close"),
    ]));

    let paragraph = Paragraph::new(help_text)
        .block(
            theme
                .styled_block("Help", false)
                .border_style(Style::default().fg(theme.accent_info)),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    f.render_widget(paragraph, popup);
}

fn section_header<'a>(title: &'a str, theme: &Theme) -> Line<'a> {
    Line::from(vec![Span::styled(
        format!("  {}", title),
        Style::default()
            .fg(theme.accent_warning)
            .add_modifier(Modifier::BOLD),
    )])
}

fn key_line<'a>(key: &'a str, desc: &'a str, theme: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::raw("    "),
        Span::styled(
            format!("{:<16}", key),
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(desc, Style::default().fg(theme.text_primary)),
    ])
}

fn render_binding_groups<'a>(
    lines: &mut Vec<Line<'a>>,
    groups: &'a [KeyBindingGroup],
    theme: &Theme,
) {
    for group in groups {
        lines.push(section_header(group.title, theme));
        for binding in &group.bindings {
            lines.push(key_line(binding.key, binding.description, theme));
        }
        lines.push(Line::from(""));
    }
}

/// Create a centered rectangle within the given area.
pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
#[path = "help_tests.rs"]
mod tests;
