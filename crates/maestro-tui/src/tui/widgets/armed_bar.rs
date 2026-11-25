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
    /// Where the write lands, for the title and the working/done lines.
    place: &'a str,
    /// What an earlier run already wrote (`1 of 3 issues already exist`):
    /// replaces `nothing written yet` and follows the armed countdown.
    written: Option<&'a str>,
}

impl<'a> ArmedBar<'a> {
    pub fn new(armed: &'a ArmedEnter, description: &'a str, now: Instant) -> Self {
        Self {
            armed,
            description,
            now,
            place: "GitHub",
            written: None,
        }
    }

    /// The write retries one that stopped partway: say what already
    /// exists instead of `nothing written yet` (#1284).
    pub fn already_written(mut self, note: &'a str) -> Self {
        self.written = Some(note);
        self
    }

    /// Name where the write lands when it is not GitHub (`disk` for a
    /// file the screen writes).
    pub fn to(mut self, place: &'a str) -> Self {
        self.place = place;
        self
    }

    pub fn render(self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.render_in_mode(f, area, theme, crate::tui::icons::use_nerd_font());
    }

    fn render_in_mode(self, f: &mut Frame, area: Rect, theme: &Theme, nerd: bool) {
        let enter = icons::get_for_mode(IconId::Enter, nerd);
        let (icon, word, color, detail) = match self.armed.state() {
            ArmedState::Blocked(reason) => (
                IconId::Warning,
                "BLOCKED",
                theme.accent_warning,
                format!("{} · {enter} does nothing", sanitize_for_terminal(reason)),
            ),
            ArmedState::Ready => (
                IconId::Play,
                "READY",
                theme.accent_success,
                format!(
                    "{enter} to arm · {}",
                    sanitize_for_terminal(self.written.unwrap_or("nothing written yet"))
                ),
            ),
            ArmedState::Armed(_) => {
                let left = self.armed.remaining(self.now).unwrap_or_default();
                let secs = left.as_millis().div_ceil(1000);
                (
                    IconId::Hourglass,
                    "ARMED",
                    theme.accent_warning,
                    match self.written {
                        Some(note) => format!(
                            "{enter} again to write · {secs}s · Esc disarms · {}",
                            sanitize_for_terminal(note)
                        ),
                        None => format!("{enter} again to write · {secs}s · Esc disarms"),
                    },
                )
            }
            ArmedState::Working => (
                IconId::Sync,
                "WORKING",
                theme.accent_info,
                format!("writing to {}…", self.place),
            ),
            ArmedState::Done => (
                IconId::CheckCircle,
                "DONE",
                theme.accent_success,
                format!("written to {}", self.place),
            ),
            ArmedState::Failed(fix) => (
                IconId::XCircle,
                "FAILED",
                theme.accent_error,
                sanitize_for_terminal(fix),
            ),
        };

        let title = format!("Write to {}", self.place);
        let block = theme
            .styled_block(&title, false)
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

    fn text_in_nerd_mode(armed: &ArmedEnter, now: Instant) -> String {
        let mut terminal = Terminal::new(TestBackend::new(72, ARMED_BAR_HEIGHT)).unwrap();
        let theme = Theme::dark();
        terminal
            .draw(|f| ArmedBar::new(armed, WRITE, now).render_in_mode(f, f.area(), &theme, true))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn armed_bar_nerd_mode_draws_return_symbol_for_every_key_label() {
        let t0 = Instant::now();
        let mut blocked = ArmedEnter::new();
        blocked.block("nothing to write");
        let mut armed = ArmedEnter::new();
        armed.on_enter(t0);
        for state in [blocked, ArmedEnter::new(), armed] {
            let text = text_in_nerd_mode(&state, t0);
            assert!(text.contains('\u{23ce}'), "no ⏎ in nerd mode: {text:?}");
            assert!(!text.contains("Enter"), "key word in nerd mode: {text:?}");
        }
    }

    #[test]
    fn armed_bar_already_written_replaces_nothing_written_yet() {
        let t0 = Instant::now();
        let mut armed = ArmedEnter::new();
        let text = |armed: &ArmedEnter| {
            let mut terminal = Terminal::new(TestBackend::new(72, ARMED_BAR_HEIGHT)).unwrap();
            let theme = Theme::dark();
            terminal
                .draw(|f| {
                    ArmedBar::new(armed, WRITE, t0)
                        .already_written("1 of 3 issues already exist")
                        .render_in_mode(f, f.area(), &theme, false)
                })
                .unwrap();
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>()
        };
        let ready = text(&armed);
        assert!(ready.contains("1 of 3 issues already exist"), "{ready:?}");
        assert!(!ready.contains("nothing written yet"), "{ready:?}");
        armed.on_enter(t0);
        let armed_text = text(&armed);
        assert!(
            armed_text.contains("Esc disarms · 1 of 3"),
            "{armed_text:?}"
        );
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
