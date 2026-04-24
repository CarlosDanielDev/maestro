//! 1-bit pixel-art mascot sprites exported from image2cpp.
//!
//! Each sprite is a 128×128 bitmap stored MSB-first in `src/mascot/sprites/*.bin`
//! and embedded via `include_bytes!`. Bytes per row = 128 / 8 = 16; bytes per
//! sprite = 128 × 16 = 2048.
//!
//! B-frames alias A-frames until second frames are authored — animation is a
//! no-op visually while the state machine keeps flipping `frame_index`.

use super::state::MascotState;

/// Sprite canvas width in pixels.
pub const SPRITE_W: u16 = 128;
/// Sprite canvas height in pixels.
pub const SPRITE_H: u16 = 128;

pub const SPRITE_ERROR_A: &[u8] = include_bytes!("sprites/error.bin");
pub const SPRITE_ERROR_B: &[u8] = SPRITE_ERROR_A;
pub const SPRITE_SLEEPING_A: &[u8] = include_bytes!("sprites/sleeping.bin");
pub const SPRITE_SLEEPING_B: &[u8] = SPRITE_SLEEPING_A;
pub const SPRITE_HAPPY_A: &[u8] = include_bytes!("sprites/happy.bin");
pub const SPRITE_HAPPY_B: &[u8] = SPRITE_HAPPY_A;
pub const SPRITE_THINKING_A: &[u8] = include_bytes!("sprites/thinking.bin");
pub const SPRITE_THINKING_B: &[u8] = SPRITE_THINKING_A;
pub const SPRITE_CONDUCTING_A: &[u8] = include_bytes!("sprites/conducting.bin");
pub const SPRITE_CONDUCTING_B: &[u8] = SPRITE_CONDUCTING_A;
pub const SPRITE_IDLE_A: &[u8] = include_bytes!("sprites/idle.bin");
pub const SPRITE_IDLE_B: &[u8] = SPRITE_IDLE_A;

/// Returns the sprite bitmap for a `(state, frame)` pair. `frame` is either
/// `0` (A frame) or any other value (B frame). B currently aliases A for
/// every state — the same `&'static [u8]` is returned either way.
pub fn sprite(state: MascotState, frame: usize) -> &'static [u8] {
    let a_b = match state {
        MascotState::Error => [SPRITE_ERROR_A, SPRITE_ERROR_B],
        MascotState::Sleeping => [SPRITE_SLEEPING_A, SPRITE_SLEEPING_B],
        MascotState::Happy => [SPRITE_HAPPY_A, SPRITE_HAPPY_B],
        MascotState::Thinking => [SPRITE_THINKING_A, SPRITE_THINKING_B],
        MascotState::Conducting => [SPRITE_CONDUCTING_A, SPRITE_CONDUCTING_B],
        MascotState::Idle => [SPRITE_IDLE_A, SPRITE_IDLE_B],
    };
    if frame == 0 { a_b[0] } else { a_b[1] }
}

/// Unpacks a single pixel from a 1-bpp MSB-first bitmap. `w` is the bitmap
/// width in pixels (must be a multiple of 8). Returns `true` when the pixel
/// is lit. Out-of-range `(x, y)` are clamped to the last in-range pixel so
/// callers don't have to guard the edges when downscaling.
pub fn pixel(bm: &[u8], w: u16, x: u16, y: u16) -> bool {
    let w = w.max(1) as usize;
    let stride = w.div_ceil(8);
    let total_rows = bm.len() / stride.max(1);
    let x = (x as usize).min(w - 1);
    let y = (y as usize).min(total_rows.saturating_sub(1));
    let byte_idx = y * stride + x / 8;
    let Some(byte) = bm.get(byte_idx) else {
        return false;
    };
    let bit = 7 - (x % 8);
    (byte >> bit) & 1 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_states() -> [MascotState; 6] {
        [
            MascotState::Error,
            MascotState::Sleeping,
            MascotState::Happy,
            MascotState::Thinking,
            MascotState::Conducting,
            MascotState::Idle,
        ]
    }

    #[test]
    fn sprite_w_and_h_constants() {
        assert_eq!(SPRITE_W, 128);
        assert_eq!(SPRITE_H, 128);
    }

    #[test]
    fn every_sprite_is_2048_bytes() {
        for state in all_states() {
            for frame in 0..=1 {
                let bm = sprite(state, frame);
                assert_eq!(bm.len(), 2048, "sprite({state:?}, {frame}) wrong length");
            }
        }
    }

    #[test]
    fn every_sprite_has_at_least_one_nonzero_byte() {
        for state in all_states() {
            for frame in 0..=1 {
                let bm = sprite(state, frame);
                assert!(
                    bm.iter().any(|b| *b != 0),
                    "sprite({state:?}, {frame}) is all-zero"
                );
            }
        }
    }

    #[test]
    fn pixel_unpack_msb_first() {
        let bm: [u8; 2] = [0b1000_0000, 0b0000_0001];
        assert!(pixel(&bm, 8, 0, 0));
        assert!(pixel(&bm, 8, 7, 1));
        for x in 1..8 {
            assert!(!pixel(&bm, 8, x, 0), "byte 0 bit {x} should be off");
        }
        for x in 0..7 {
            assert!(!pixel(&bm, 8, x, 1), "byte 1 bit {x} should be off");
        }
    }

    #[test]
    fn frame_b_matches_frame_a_for_every_state() {
        for state in all_states() {
            assert_eq!(
                sprite(state, 0),
                sprite(state, 1),
                "frame B must match frame A for {state:?}"
            );
        }
    }

    #[test]
    fn sprite_dispatch_covers_all_states() {
        for state in all_states() {
            assert!(!sprite(state, 0).is_empty());
        }
    }
}
