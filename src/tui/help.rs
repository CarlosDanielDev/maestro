use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBindingGroup, global_keybindings};
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

/// Draw the help overlay popup centered on the screen.
/// Shows global keybindings and context-sensitive screen keybindings.
pub fn draw_help_overlay(
    f: &mut Frame,
    area: Rect,
    screen_bindings: &[KeyBindingGroup],
    input_mode: InputMode,
    scroll: u16,
    theme: &Theme,
) {
    let popup = centered_rect(60, 70, area);

    // Clear background behind popup
    f.render_widget(Clear, popup);

    let mut help_text = vec![
        Line::from(vec![Span::styled(
            "Keyboard Shortcuts",
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

    // Render screen-specific bindings first, then global bindings
    render_binding_groups(&mut help_text, screen_bindings, theme);
    let globals = global_keybindings();
    render_binding_groups(&mut help_text, &globals, theme);

    help_text.push(Line::from(vec![Span::styled(
        "Press ? or Esc to close",
        Style::default().fg(theme.text_secondary),
    )]));

    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent_info))
                .title(" Help ")
                .title_alignment(Alignment::Center),
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
