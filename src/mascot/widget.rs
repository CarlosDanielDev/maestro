use super::MascotStyle;
use super::frames::{MASCOT_ROWS, MascotFrames};
use super::sprites::{SPRITE_H, SPRITE_W, pixel, sprite};
use super::state::MascotState;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

/// Clawd orange: #D77757 — default mascot color when no theme is available.
#[allow(dead_code)]
pub const CLAWD_ORANGE: Color = Color::Rgb(215, 119, 87);

/// Renders the mascot in either the legacy ASCII-block art (6 rows × 11 cells)
/// or the new pixel-art sprite path (128×128 downsampled to the caller's area
/// via half-block encoding).
pub struct MascotWidget {
    state: MascotState,
    frame_index: usize,
    color: Color,
    style: MascotStyle,
}

impl MascotWidget {
    /// Creates a new widget with the default render style (`Ascii`). Use
    /// [`MascotWidget::with_style`] to switch to the sprite path.
    pub fn new(state: MascotState, frame_index: usize, color: Color) -> Self {
        Self {
            state,
            frame_index,
            color,
            style: MascotStyle::Ascii,
        }
    }

    /// Sets the render style. Builder-style so existing 3-arg `new(...)` call
    /// sites can opt into sprites without churning their signature.
    pub fn with_style(mut self, style: MascotStyle) -> Self {
        self.style = style;
        self
    }
}

impl Widget for MascotWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.style {
            MascotStyle::Ascii => render_ascii(self.state, self.frame_index, self.color, area, buf),
            MascotStyle::Sprite => {
                render_sprite(self.state, self.frame_index, self.color, area, buf)
            }
        }
    }
}

fn render_ascii(
    state: MascotState,
    frame_index: usize,
    color: Color,
    area: Rect,
    buf: &mut Buffer,
) {
    let style = Style::default().fg(color);
    let max_rows = MASCOT_ROWS.min(area.height as usize);
    for row in 0..max_rows {
        let pair = MascotFrames::frames(state, row);
        let text = if frame_index == 0 { pair[0] } else { pair[1] };
        let y = area.y + row as u16;
        buf.set_string(area.x, y, text, style);
    }
}

fn render_sprite(
    state: MascotState,
    frame_index: usize,
    color: Color,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let bm = sprite(state, frame_index);
    let style = Style::default().fg(color);
    let w = area.width as u32;
    let h = area.height as u32;
    let sprite_w = SPRITE_W as u32;
    let sprite_h = SPRITE_H as u32;

    for y_cell in 0..h {
        let src_y_top = (2 * y_cell * sprite_h) / (2 * h);
        let src_y_bot = ((2 * y_cell + 1) * sprite_h) / (2 * h);
        for x in 0..w {
            let src_x = (x * sprite_w) / w;
            let top = pixel(bm, SPRITE_W, src_x as u16, src_y_top as u16);
            let bot = pixel(bm, SPRITE_W, src_x as u16, src_y_bot as u16);
            let ch = match (top, bot) {
                (false, false) => continue,
                (true, false) => "\u{2580}", // ▀
                (false, true) => "\u{2584}", // ▄
                (true, true) => "\u{2588}",  // █
            };
            buf.set_string(area.x + x as u16, area.y + y_cell as u16, ch, style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_sprite_to_buffer(state: MascotState, color: Color, w: u16, h: u16) -> Buffer {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        let widget = MascotWidget::new(state, 0, color).with_style(MascotStyle::Sprite);
        widget.render(area, &mut buf);
        buf
    }

    fn is_halfblock_or_space(sym: &str) -> bool {
        matches!(sym, " " | "\u{2580}" | "\u{2584}" | "\u{2588}")
    }

    #[test]
    fn widget_sprite_path_only_emits_halfblock_chars() {
        let buf = render_sprite_to_buffer(MascotState::Error, CLAWD_ORANGE, 20, 10);
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let sym = buf.cell((x, y)).map_or(" ", |c| c.symbol());
                assert!(
                    is_halfblock_or_space(sym),
                    "unexpected cell at ({x},{y}): {sym:?}"
                );
            }
        }
    }

    #[test]
    fn widget_sprite_path_fg_color_on_non_space_cells() {
        let color = Color::Red;
        let buf = render_sprite_to_buffer(MascotState::Happy, color, 20, 10);
        let mut found_colored = false;
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let Some(cell) = buf.cell((x, y)) else {
                    continue;
                };
                let sym = cell.symbol();
                if sym != " " {
                    assert_eq!(
                        cell.style().fg,
                        Some(color),
                        "cell ({x},{y}) {sym:?} missing fg"
                    );
                    found_colored = true;
                }
            }
        }
        assert!(found_colored, "expected at least one colored cell");
    }

    #[test]
    fn widget_sprite_path_renders_safely_into_smaller_area() {
        // 8×4 area against a 128×128 sprite — should not panic.
        let buf = render_sprite_to_buffer(MascotState::Idle, CLAWD_ORANGE, 8, 4);
        assert_eq!(buf.area.width, 8);
        assert_eq!(buf.area.height, 4);
    }

    #[test]
    fn widget_sprite_path_zero_height_is_noop() {
        let area = Rect::new(0, 0, 20, 0);
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 1));
        let widget =
            MascotWidget::new(MascotState::Idle, 0, CLAWD_ORANGE).with_style(MascotStyle::Sprite);
        widget.render(area, &mut buf); // must not panic
    }

    #[test]
    fn widget_sprite_path_different_states_produce_different_output() {
        let a = render_sprite_to_buffer(MascotState::Error, CLAWD_ORANGE, 32, 16);
        let b = render_sprite_to_buffer(MascotState::Happy, CLAWD_ORANGE, 32, 16);
        let mut any_diff = false;
        for y in 0..a.area.height {
            for x in 0..a.area.width {
                if a.cell((x, y)).map(|c| c.symbol().to_string())
                    != b.cell((x, y)).map(|c| c.symbol().to_string())
                {
                    any_diff = true;
                    break;
                }
            }
            if any_diff {
                break;
            }
        }
        assert!(any_diff, "different states should render different output");
    }

    #[test]
    fn widget_sprite_path_deterministic() {
        let a = render_sprite_to_buffer(MascotState::Conducting, CLAWD_ORANGE, 24, 12);
        let b = render_sprite_to_buffer(MascotState::Conducting, CLAWD_ORANGE, 24, 12);
        for y in 0..a.area.height {
            for x in 0..a.area.width {
                assert_eq!(
                    a.cell((x, y)).map(|c| c.symbol().to_string()),
                    b.cell((x, y)).map(|c| c.symbol().to_string()),
                    "non-deterministic render at ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn widget_ascii_path_unchanged_default() {
        // Constructing without with_style() must route through the ascii path.
        let area = Rect::new(0, 0, 11, 6);
        let mut buf = Buffer::empty(area);
        let widget = MascotWidget::new(MascotState::Idle, 0, CLAWD_ORANGE);
        widget.render(area, &mut buf);
        // Top row of Idle ASCII frames starts with " \u{2584}\u{2596}".
        let row0: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 0))
                    .map_or(String::new(), |c| c.symbol().to_string())
            })
            .collect();
        assert!(
            row0.contains('\u{2584}') || row0.contains('\u{2596}'),
            "ASCII default row0 should contain block chars, got {row0:?}"
        );
    }
}
