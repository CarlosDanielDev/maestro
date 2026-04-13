use super::widget::CLAWD_ORANGE;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// Eye states for the prompt bar companion (1 row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum EyeState {
    Waiting,
    Typing,
    Processing,
    Success,
    Error,
}

/// Compact inline mascot — just the eyes row for prompt bar use.
#[allow(dead_code)]
pub struct MascotEyes;

impl MascotEyes {
    #[allow(dead_code)]
    pub fn render_line(state: EyeState, frame_index: usize) -> Line<'static> {
        let text = match state {
            EyeState::Waiting => {
                if frame_index == 0 {
                    " \u{25C9}   \u{25C9} " // ◉   ◉
                } else {
                    " \u{2500}   \u{2500} " // ─   ─
                }
            }
            EyeState::Typing => " \u{00B7}   \u{00B7} ", // ·   ·
            EyeState::Processing => " \u{25C9}   \u{25C9} ", // ◉   ◉
            EyeState::Success => " \u{25C6}   \u{25C6} ", // ◆   ◆
            EyeState::Error => " x   x ",
        };
        Line::from(Span::styled(text, Style::default().fg(CLAWD_ORANGE)))
    }
}
