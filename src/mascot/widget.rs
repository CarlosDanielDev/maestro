use super::frames::{MASCOT_ROWS, MASCOT_WIDTH, MascotFrames};
use super::state::MascotState;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

/// Clawd orange: #D77757
pub const CLAWD_ORANGE: Color = Color::Rgb(215, 119, 87);

/// Renders 6 rows of mascot ASCII art at 11-char width.
pub struct MascotWidget {
    state: MascotState,
    frame_index: usize,
}

impl MascotWidget {
    pub fn new(state: MascotState, frame_index: usize) -> Self {
        Self { state, frame_index }
    }
}

impl Widget for MascotWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let style = Style::default().fg(CLAWD_ORANGE);
        let max_rows = MASCOT_ROWS.min(area.height as usize);
        let max_width = MASCOT_WIDTH.min(area.width as usize);

        for row in 0..max_rows {
            let pair = MascotFrames::frames(self.state, row);
            let text = if self.frame_index == 0 {
                pair[0]
            } else {
                pair[1]
            };
            let y = area.y + row as u16;
            for (col, ch) in text.chars().enumerate() {
                if col >= max_width {
                    break;
                }
                let mut tmp = [0u8; 4];
                buf.set_string(area.x + col as u16, y, ch.encode_utf8(&mut tmp), style);
            }
        }
    }
}
