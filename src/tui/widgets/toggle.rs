use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::theme::Theme;

use super::WidgetAction;

pub struct Toggle {
    pub label: String,
    pub value: bool,
}

impl Toggle {
    pub fn new(label: impl Into<String>, value: bool) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> WidgetAction {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.value = !self.value;
                WidgetAction::Changed
            }
            _ => WidgetAction::None,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect, theme: &Theme, focused: bool) {
        use crate::tui::icons::{self, IconId};
        let indicator = icons::get(if self.value {
            IconId::CheckboxOn
        } else {
            IconId::CheckboxOff
        });
        let check_color = if self.value {
            theme.accent_success
        } else {
            if focused {
                theme.text_primary
            } else {
                theme.text_muted
            }
        };
        let check_style = if focused {
            Style::default()
                .fg(check_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(check_color)
        };
        let label_style = if focused {
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };

        let line = Line::from(vec![
            Span::styled(format!("{} ", indicator), check_style),
            Span::styled(&self.label, label_style),
        ]);
        f.render_widget(Paragraph::new(line), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn toggle_flips_on_space() {
        let mut t = Toggle::new("test", false);
        assert!(!t.value);
        let action = t.handle_input(key(KeyCode::Char(' ')));
        assert!(t.value);
        assert_eq!(action, WidgetAction::Changed);
    }

    #[test]
    fn toggle_flips_on_enter() {
        let mut t = Toggle::new("test", true);
        let action = t.handle_input(key(KeyCode::Enter));
        assert!(!t.value);
        assert_eq!(action, WidgetAction::Changed);
    }

    #[test]
    fn toggle_ignores_other_keys() {
        let mut t = Toggle::new("test", false);
        let action = t.handle_input(key(KeyCode::Char('x')));
        assert!(!t.value);
        assert_eq!(action, WidgetAction::None);
    }

    #[test]
    fn toggle_double_flip_returns_to_original() {
        let mut t = Toggle::new("test", false);
        t.handle_input(key(KeyCode::Char(' ')));
        t.handle_input(key(KeyCode::Char(' ')));
        assert!(!t.value);
    }

    // --- Issue #433: checkbox glyph codepoint correctness ---
    //
    // MODE is a global AtomicU8 in icon_mode. Tests that mutate it must
    // serialize to avoid racing each other.
    static TOGGLE_RENDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// RAII guard that restores Nerd Font mode on drop, even when the test
    /// panics. Ensures later tests sharing `TOGGLE_RENDER_LOCK` start from a
    /// known mode.
    struct ModeRestoreGuard;
    impl Drop for ModeRestoreGuard {
        fn drop(&mut self) {
            crate::tui::icons::init_from_config(false);
        }
    }

    fn render_toggle_indicator(value: bool, nerd: bool) -> String {
        use crate::tui::icons::init_from_config;
        use ratatui::{Terminal, backend::TestBackend};

        let _lock = TOGGLE_RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        init_from_config(!nerd);
        let _restore = ModeRestoreGuard;

        let backend = TestBackend::new(20, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::dark();
        let toggle = Toggle::new("label", value);
        terminal
            .draw(|f| {
                toggle.draw(f, f.area(), &theme, false);
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let mut row = String::new();
        for x in 0..buf.area.width {
            row.push_str(buf[(x, 0)].symbol());
        }
        row
    }

    fn nerd_symbol(id: crate::tui::icons::IconId) -> &'static str {
        crate::tui::icons::get_for_mode(id, true)
    }

    #[test]
    fn draw_renders_nerd_checkbox_on() {
        use crate::tui::icons::IconId;
        let row = render_toggle_indicator(true, true);
        let expected = nerd_symbol(IconId::CheckboxOn);
        assert!(
            row.contains(expected),
            "Expected nerd CheckboxOn {expected:?} in row, got: {row:?}"
        );
    }

    #[test]
    fn draw_renders_nerd_checkbox_off() {
        use crate::tui::icons::IconId;
        let row = render_toggle_indicator(false, true);
        let expected = nerd_symbol(IconId::CheckboxOff);
        assert!(
            row.contains(expected),
            "Expected nerd CheckboxOff {expected:?} in row, got: {row:?}"
        );
    }

    #[test]
    fn draw_renders_ascii_checkbox_on() {
        let row = render_toggle_indicator(true, false);
        assert!(
            row.contains("[x]"),
            "Expected ASCII CheckboxOn '[x]' in row, got: {row:?}"
        );
    }

    #[test]
    fn draw_renders_ascii_checkbox_off() {
        let row = render_toggle_indicator(false, false);
        assert!(
            row.contains("[ ]"),
            "Expected ASCII CheckboxOff '[ ]' in row, got: {row:?}"
        );
    }

    #[test]
    fn draw_renders_label_text() {
        let row = render_toggle_indicator(true, false);
        assert!(
            row.contains("label"),
            "Expected label text 'label' in row, got: {row:?}"
        );
    }
}
