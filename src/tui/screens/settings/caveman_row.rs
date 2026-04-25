//! Renders the `caveman_mode` row in the Advanced settings tab.
//!
//! A standard `Toggle` widget cannot express the four-state model
//! (`(default)` suffix on `Default`, `<error>` placeholder on `Error`),
//! so this module overlays its own line on top of the row area.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::settings::CavemanModeState;
use crate::tui::icons::{self, IconId};
use crate::tui::theme::Theme;

pub fn render_caveman_row(
    f: &mut Frame,
    area: Rect,
    state: &CavemanModeState,
    focused: bool,
    theme: &Theme,
) {
    let indicator = match state {
        CavemanModeState::ExplicitTrue => icons::get(IconId::CheckboxOn),
        CavemanModeState::ExplicitFalse | CavemanModeState::Default => {
            icons::get(IconId::CheckboxOff)
        }
        CavemanModeState::Error(_) => icons::get(IconId::CheckboxOff),
    };

    let indicator_color = match state {
        CavemanModeState::ExplicitTrue => theme.accent_success,
        CavemanModeState::Error(_) => theme.accent_error,
        _ if focused => theme.text_primary,
        _ => theme.text_muted,
    };

    let label_color = match state {
        CavemanModeState::Error(_) => theme.accent_error,
        _ if focused => theme.accent_success,
        _ => theme.text_primary,
    };

    let label_modifier = if focused {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };

    let line = Line::from(vec![
        Span::styled(
            format!("{} ", indicator),
            Style::default().fg(indicator_color),
        ),
        Span::styled(
            "caveman_mode  ",
            Style::default()
                .fg(label_color)
                .add_modifier(label_modifier),
        ),
        Span::styled(state.label(), Style::default().fg(theme.text_secondary)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
