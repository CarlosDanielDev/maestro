//! Renders the `caveman_mode` row in the Advanced settings tab.
//!
//! A standard `Toggle` widget cannot express the four-state model
//! (`(default)` suffix on `Default`, `<error>` placeholder on `Error`),
//! so this module overlays its own line on top of the row area.

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::settings::CavemanModeState;
use crate::tui::icons::{self, IconId};
use crate::tui::theme::Theme;
use crate::tui::widgets::focused_selection_style;

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

    let focused_style = focused_selection_style(theme);
    let indicator_style = if focused {
        focused_style
    } else {
        Style::default().fg(indicator_color)
    };
    let label_style = if focused {
        focused_style
    } else {
        Style::default().fg(label_color)
    };
    let value_style = if focused {
        focused_style
    } else {
        Style::default().fg(theme.text_secondary)
    };

    let line = Line::from(vec![
        Span::styled(format!("{} ", indicator), indicator_style),
        Span::styled("caveman_mode  ", label_style),
        Span::styled(state.label(), value_style),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
