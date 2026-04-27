//! Transient banner shown after a Copy action — overlays the status bar
//! row for ~2 s with a success or error message. See
//! [`crate::tui::app::clipboard_action::COPY_TOAST_TTL_MS`].

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy)]
pub(crate) enum CopyToastKind {
    Success,
    Error,
}

pub fn draw_copy_toast(
    f: &mut Frame,
    area: Rect,
    kind: CopyToastKind,
    message: &str,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let banner = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    let bg = match kind {
        CopyToastKind::Success => theme.accent_success,
        CopyToastKind::Error => theme.accent_error,
    };
    let style = Style::default()
        .fg(theme.branding_fg)
        .bg(bg)
        .add_modifier(Modifier::BOLD);
    f.render_widget(Clear, banner);
    let p = Paragraph::new(Line::from(Span::styled(format!(" {} ", message), style)));
    f.render_widget(p, banner);
}
