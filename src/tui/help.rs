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

/// State for the full-screen help overlay (#281).
pub struct HelpOverlayState {
    pub scroll: u16,
    pub search_query: String,
    pub search_active: bool,
    pub total_lines: u16,
}

impl HelpOverlayState {
    pub fn new() -> Self {
        Self {
            scroll: 0,
            search_query: String::new(),
            search_active: false,
            total_lines: 0,
        }
    }

    pub fn scroll_down(&mut self) {
        if self.scroll < self.total_lines {
            self.scroll = self.scroll.saturating_add(1);
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn page_down(&mut self) {
        if self.scroll + 10 <= self.total_lines {
            self.scroll = self.scroll.saturating_add(10);
        } else {
            self.scroll = self.total_lines;
        }
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
        Span::styled(
            "[j/k]",
            Style::default().fg(theme.accent_success),
        ),
        Span::raw(" Scroll  "),
        Span::styled(
            "[/]",
            Style::default().fg(theme.accent_success),
        ),
        Span::raw(" Search  "),
        Span::styled(
            "[Esc]",
            Style::default().fg(theme.accent_success),
        ),
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
mod tests {
    use super::*;
    use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup};

    fn make_groups() -> Vec<KeyBindingGroup> {
        vec![
            KeyBindingGroup {
                title: "Scrolling",
                bindings: vec![
                    KeyBinding {
                        key: "Up/Down",
                        description: "Scroll agent panel output",
                    },
                    KeyBinding {
                        key: "Shift+Up/Down",
                        description: "Scroll activity log",
                    },
                ],
            },
            KeyBindingGroup {
                title: "General",
                bindings: vec![KeyBinding {
                    key: "q",
                    description: "Quit maestro",
                }],
            },
        ]
    }

    #[test]
    fn help_overlay_state_new_is_default() {
        let state = HelpOverlayState::new();
        assert_eq!(state.scroll, 0);
        assert!(state.search_query.is_empty());
        assert!(!state.search_active);
        assert_eq!(state.total_lines, 0);
    }

    #[test]
    fn help_overlay_state_scroll_down_increments() {
        let mut state = HelpOverlayState::new();
        state.total_lines = 10;
        state.scroll = 3;
        state.scroll_down();
        assert_eq!(state.scroll, 4);
    }

    #[test]
    fn help_overlay_state_scroll_up_decrements() {
        let mut state = HelpOverlayState::new();
        state.scroll = 3;
        state.scroll_up();
        assert_eq!(state.scroll, 2);
    }

    #[test]
    fn help_overlay_state_scroll_up_saturates_at_zero() {
        let mut state = HelpOverlayState::new();
        state.scroll = 0;
        state.scroll_up();
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn help_overlay_state_scroll_down_clamps_at_total_lines() {
        let mut state = HelpOverlayState::new();
        state.total_lines = 10;
        state.scroll = 9;
        state.scroll_down();
        assert_eq!(state.scroll, 10);
        state.scroll_down();
        assert_eq!(state.scroll, 10);
    }

    #[test]
    fn help_overlay_state_toggle_search_activates_then_deactivates() {
        let mut state = HelpOverlayState::new();
        assert!(!state.search_active);
        state.toggle_search();
        assert!(state.search_active);
        state.toggle_search();
        assert!(!state.search_active);
    }

    #[test]
    fn help_overlay_state_push_char_appends() {
        let mut state = HelpOverlayState::new();
        state.push_char('s');
        state.push_char('c');
        assert_eq!(state.search_query, "sc");
    }

    #[test]
    fn help_overlay_state_pop_char_removes_last() {
        let mut state = HelpOverlayState::new();
        state.search_query = "scroll".to_string();
        state.pop_char();
        assert_eq!(state.search_query, "scrol");
    }

    #[test]
    fn help_overlay_state_pop_char_on_empty_is_noop() {
        let mut state = HelpOverlayState::new();
        state.pop_char();
        assert!(state.search_query.is_empty());
    }

    #[test]
    fn help_overlay_state_clear_search_resets() {
        let mut state = HelpOverlayState::new();
        state.search_query = "foo".to_string();
        state.search_active = true;
        state.clear_search();
        assert!(state.search_query.is_empty());
        assert!(!state.search_active);
    }

    #[test]
    fn filter_bindings_empty_query_returns_all() {
        let groups = make_groups();
        let result = filter_bindings(&groups, "");
        assert_eq!(result.len(), groups.len());
        let total_in: usize = groups.iter().map(|g| g.bindings.len()).sum();
        let total_out: usize = result.iter().map(|g| g.bindings.len()).sum();
        assert_eq!(total_in, total_out);
    }

    #[test]
    fn filter_bindings_scroll_matches_relevant() {
        let groups = make_groups();
        let result = filter_bindings(&groups, "scroll");
        assert!(!result.is_empty());
        let all_descs: Vec<&str> = result
            .iter()
            .flat_map(|g| g.bindings.iter())
            .map(|b| b.description)
            .collect();
        for desc in &all_descs {
            assert!(
                desc.to_lowercase().contains("scroll"),
                "non-scroll binding survived filter: {}",
                desc
            );
        }
    }

    #[test]
    fn filter_bindings_is_case_insensitive() {
        let groups = make_groups();
        let lower = filter_bindings(&groups, "scroll");
        let upper = filter_bindings(&groups, "SCROLL");
        let count_lower: usize = lower.iter().map(|g| g.bindings.len()).sum();
        let count_upper: usize = upper.iter().map(|g| g.bindings.len()).sum();
        assert_eq!(count_lower, count_upper);
    }

    #[test]
    fn filter_bindings_no_match_returns_empty() {
        let groups = make_groups();
        let result = filter_bindings(&groups, "zzznomatch");
        assert!(result.is_empty());
    }

    #[test]
    fn filter_bindings_matches_key() {
        let groups = make_groups();
        let result = filter_bindings(&groups, "shift");
        assert!(!result.is_empty());
        let matched: Vec<&str> = result
            .iter()
            .flat_map(|g| g.bindings.iter())
            .map(|b| b.key)
            .collect();
        assert!(matched.iter().any(|k| k.to_lowercase().contains("shift")));
    }
}
