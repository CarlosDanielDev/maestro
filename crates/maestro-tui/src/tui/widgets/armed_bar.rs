//! `ArmedBar`: draws an `ArmedEnter` and the exact write it will do.
//! Every GitHub write a screen starts goes through this bar
//! (see `docs/tui-design-system.md`, "Armed-Enter").

use std::time::Instant;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::ds::armed::{ArmedEnter, ArmedState};
use crate::tui::icons::{self, IconId};
use crate::tui::screens::sanitize_for_terminal;
use crate::tui::theme::Theme;

/// Rows the bar needs: border, state line, detail line, border.
pub const ARMED_BAR_HEIGHT: u16 = 4;

/// One write bar. `description` is the exact write, for example
/// `creates 1 issue in owner/repo`. No GitHub types: callers build the text.
pub struct ArmedBar<'a> {
    armed: &'a ArmedEnter,
    description: &'a str,
    now: Instant,
}

impl<'a> ArmedBar<'a> {
    pub fn new(armed: &'a ArmedEnter, description: &'a str, now: Instant) -> Self {
        Self {
            armed,
            description,
            now,
        }
    }

    pub fn render(self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.render_in_mode(f, area, theme, crate::tui::icons::use_nerd_font());
    }

    fn render_in_mode(self, f: &mut Frame, area: Rect, theme: &Theme, nerd: bool) {
        let (icon, word, color, detail) = match self.armed.state() {
            ArmedState::Blocked(reason) => (
                IconId::Warning,
                "BLOCKED",
                theme.accent_warning,
                format!("{} · Enter does nothing", sanitize_for_terminal(reason)),
            ),
            ArmedState::Ready => (
                IconId::Play,
                "READY",
                theme.accent_success,
                "Enter to arm · nothing written yet".to_string(),
            ),
            ArmedState::Armed(_) => {
                let left = self.armed.remaining(self.now).unwrap_or_default();
                let secs = left.as_millis().div_ceil(1000);
                (
                    IconId::Hourglass,
                    "ARMED",
                    theme.accent_warning,
                    format!("Enter again to write · {secs}s · Esc disarms"),
                )
            }
            ArmedState::Working => (
                IconId::Sync,
                "WORKING",
                theme.accent_info,
                "writing to GitHub…".to_string(),
            ),
            ArmedState::Done => (
                IconId::CheckCircle,
                "DONE",
                theme.accent_success,
                "written to GitHub".to_string(),
            ),
            ArmedState::Failed(fix) => (
                IconId::XCircle,
                "FAILED",
                theme.accent_error,
                sanitize_for_terminal(fix),
            ),
        };

        let block = theme
            .styled_block("Write to GitHub", false)
            .border_style(Style::default().fg(color));
        let bold = Style::default().fg(color).add_modifier(Modifier::BOLD);
        let lines = vec![
            Line::from(vec![
                Span::styled(format!("{} {word}", icons::get_for_mode(icon, nerd)), bold),
                Span::raw("  "),
                Span::styled(
                    sanitize_for_terminal(self.description),
                    Style::default().fg(theme.text_primary),
                ),
            ]),
            Line::from(Span::styled(
                detail,
                Style::default().fg(theme.text_secondary),
            )),
        ];
        f.render_widget(Paragraph::new(lines).block(block), area);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use insta::assert_snapshot;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    const WRITE: &str = "creates 1 issue in CarlosDanielDev/maestro";

    fn draw(armed: &ArmedEnter, now: Instant) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(72, ARMED_BAR_HEIGHT)).unwrap();
        let theme = Theme::dark();
        terminal
            .draw(|f| ArmedBar::new(armed, WRITE, now).render_in_mode(f, f.area(), &theme, false))
            .unwrap();
        terminal
    }

    /// Ready, then Enter at `t0`, then Enter at `t0 + 1s`.
    fn working(t0: Instant) -> ArmedEnter {
        let mut armed = ArmedEnter::new();
        armed.on_enter(t0);
        armed.on_enter(t0 + Duration::from_secs(1));
        armed
    }

    #[test]
    fn armed_bar_blocked() {
        let mut armed = ArmedEnter::new();
        armed.block("2 [TODO] placeholders would be published");
        assert_snapshot!(draw(&armed, Instant::now()).backend());
    }

    #[test]
    fn armed_bar_ready() {
        assert_snapshot!(draw(&ArmedEnter::new(), Instant::now()).backend());
    }

    #[test]
    fn armed_bar_armed() {
        let t0 = Instant::now();
        let mut armed = ArmedEnter::new();
        armed.on_enter(t0);
        // 1.2s in: 1.8s left, shown rounded up as 2s.
        assert_snapshot!(draw(&armed, t0 + Duration::from_millis(1200)).backend());
    }

    #[test]
    fn armed_bar_working() {
        let t0 = Instant::now();
        assert_snapshot!(draw(&working(t0), t0).backend());
    }

    #[test]
    fn armed_bar_done() {
        let t0 = Instant::now();
        let mut armed = working(t0);
        armed.finish(Ok(()));
        assert_snapshot!(draw(&armed, t0).backend());
    }

    #[test]
    fn armed_bar_failed() {
        let t0 = Instant::now();
        let mut armed = working(t0);
        armed.finish(Err("gh: HTTP 401 · run `gh auth login`, then Enter".into()));
        assert_snapshot!(draw(&armed, t0).backend());
    }

    #[test]
    fn armed_bar_sanitizes_description() {
        let mut terminal = Terminal::new(TestBackend::new(72, ARMED_BAR_HEIGHT)).unwrap();
        let theme = Theme::dark();
        let armed = ArmedEnter::new();
        terminal
            .draw(|f| {
                ArmedBar::new(&armed, "creates \x1b[31mred\x1b[0m", Instant::now()).render_in_mode(
                    f,
                    f.area(),
                    &theme,
                    false,
                )
            })
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            !text.contains('\x1b'),
            "escape reached the buffer: {text:?}"
        );
    }
}
