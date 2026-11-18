//! gh auth expired line (#1241, design §05 "PR action while gh is signed
//! out"). Drawn on the activity-log row while `App::gh_auth_ok` is false:
//! the state, what it locks, and the fix. Pure draw — the recheck lives
//! in `app/gh_auth.rs`, never on the draw path.

use crate::tui::theme::Theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// The header word.
pub(crate) const GH_AUTH_EXPIRED: &str = "⊘ gh auth expired";

/// One line: `⊘ gh auth expired` chip, then what is locked and the fix.
pub(crate) fn line(theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {GH_AUTH_EXPIRED} "),
            Style::default()
                .fg(theme.branding_fg)
                .bg(theme.accent_error)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " PR create/retry locked · fix: ",
            Style::default().fg(theme.text_muted),
        ),
        Span::styled("gh auth login", Style::default().fg(theme.accent_info)),
        Span::styled(", then Shift+P", Style::default().fg(theme.text_muted)),
    ])
}

/// Draw the line on the first row of `area`.
pub fn draw(f: &mut Frame, area: Rect, theme: &Theme) {
    f.render_widget(Paragraph::new(line(theme)), area);
}
