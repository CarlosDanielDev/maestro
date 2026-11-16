//! Panel border with one focus signal (design system §03 "Borders & focus").
//!
//! The border *type* carries the meaning, so focus stays visible when the
//! theme has no colour (NO_COLOR): Plain inactive, Thick active, Rounded
//! overlay, Double danger.

use ratatui::{
    layout::Alignment,
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders},
};

use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// Unfocused panel.
    Inactive,
    /// The panel that receives keys.
    Active,
    /// Floating on top of a screen (palette, toast, guard).
    Overlay,
    /// Destructive confirm.
    Danger,
}

impl Focus {
    pub fn border_type(self) -> BorderType {
        match self {
            Self::Inactive => BorderType::Plain,
            Self::Active => BorderType::Thick,
            Self::Overlay => BorderType::Rounded,
            Self::Danger => BorderType::Double,
        }
    }

    fn border_style(self, theme: &Theme) -> Style {
        let style = Style::default();
        match self {
            Self::Inactive => style.fg(theme.border_inactive),
            Self::Active => style.fg(theme.border_active).add_modifier(Modifier::BOLD),
            Self::Overlay => style.fg(theme.accent_info),
            Self::Danger => style.fg(theme.accent_error),
        }
    }
}

/// A bordered block with no title.
pub fn panel_plain(focus: Focus, theme: &Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(focus.border_type())
        .border_style(focus.border_style(theme))
}

/// A bordered block with a centered `[ title ]`.
pub fn panel<'a>(title: &str, focus: Focus, theme: &Theme) -> Block<'a> {
    panel_plain(focus, theme)
        .title(Span::styled(
            format!("[ {} ]", title.trim()),
            Style::default()
                .fg(theme.title_accent)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center)
}

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
