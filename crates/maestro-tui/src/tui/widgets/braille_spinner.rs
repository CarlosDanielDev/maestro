use crate::tui::spinner::graph_node_frame;
use crate::tui::theme::Theme;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

pub struct BrailleSpinner;

impl BrailleSpinner {
    /// Render a single spinner line as `<glyph> <label>`.
    pub fn render<'a>(
        tick: usize,
        label: impl Into<std::borrow::Cow<'a, str>>,
        use_nerd_font: bool,
        theme: &Theme,
    ) -> Line<'a> {
        let label = label.into();
        Line::from(vec![
            Span::styled(
                graph_node_frame(tick, use_nerd_font).to_string(),
                Style::default().fg(theme.accent_info),
            ),
            Span::raw(" "),
            Span::styled(
                label,
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_cycles_braille_frames_in_nerd_font_mode() {
        let theme = Theme::dark();
        let expected = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

        for (tick, glyph) in expected.into_iter().enumerate() {
            let line = BrailleSpinner::render(tick, "Loading", true, &theme);
            assert_eq!(line.spans[0].content, glyph.to_string());
            assert_eq!(line.spans[2].content, "Loading");
        }
    }

    #[test]
    fn render_uses_ascii_fallback_without_nerd_font() {
        let theme = Theme::dark();
        let expected = ['|', '/', '-', '\\', '|'];

        for (tick, glyph) in expected.into_iter().enumerate() {
            let line = BrailleSpinner::render(tick, "Loading", false, &theme);
            assert_eq!(line.spans[0].content, glyph.to_string());
            assert_eq!(line.spans[2].content, "Loading");
        }
    }

    #[test]
    fn render_is_pure_for_same_inputs() {
        let theme = Theme::dark();
        let first = BrailleSpinner::render(3, "Loading", true, &theme);
        let second = BrailleSpinner::render(3, "Loading", true, &theme);

        assert_eq!(first, second);
    }
}
