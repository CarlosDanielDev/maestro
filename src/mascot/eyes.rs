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
pub struct MascotEyes;

impl MascotEyes {
    /// Render a single line of mascot eyes for the given state and frame index.
    pub fn render_line(state: EyeState, frame_index: usize) -> Line<'static> {
        let text = match state {
            EyeState::Waiting => {
                if frame_index == 0 {
                    " ▐█◉   ◉█▌ "
                } else {
                    " ▐█─   ─█▌ "
                }
            }
            EyeState::Typing => " ▐█·   ·█▌ ",
            EyeState::Processing => " ▐█◉   ◉█▌ ",
            EyeState::Success => " ▐█◆   ◆█▌ ",
            EyeState::Error => " ▐█✕   ✕█▌ ",
        };
        Line::from(Span::styled(text, Style::default().fg(CLAWD_ORANGE)))
    }
}
