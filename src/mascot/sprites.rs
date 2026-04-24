//! 1-bit pixel-art mascot sprites exported from image2cpp.
//!
//! Each sprite is a 128×128 bitmap stored MSB-first in `src/mascot/sprites/*.bin`
//! and embedded via `include_bytes!`. Bytes per row = 128 / 8 = 16; bytes per
//! sprite = 128 × 16 = 2048.

use super::state::MascotState;

/// Sprite canvas width in pixels.
pub const SPRITE_W: u16 = 128;
/// Sprite canvas height in pixels.
pub const SPRITE_H: u16 = 128;

const SPRITE_ERROR: &[u8] = include_bytes!("sprites/error.bin");
const SPRITE_SLEEPING: &[u8] = include_bytes!("sprites/sleeping.bin");
const SPRITE_HAPPY: &[u8] = include_bytes!("sprites/happy.bin");
const SPRITE_THINKING: &[u8] = include_bytes!("sprites/thinking.bin");
const SPRITE_CONDUCTING: &[u8] = include_bytes!("sprites/conducting.bin");
const SPRITE_IDLE: &[u8] = include_bytes!("sprites/idle.bin");

const _: () = {
    let expected = (SPRITE_H as usize) * (SPRITE_W as usize).div_ceil(8);
    assert!(SPRITE_ERROR.len() == expected, "error.bin wrong size");
    assert!(SPRITE_SLEEPING.len() == expected, "sleeping.bin wrong size");
    assert!(SPRITE_HAPPY.len() == expected, "happy.bin wrong size");
    assert!(SPRITE_THINKING.len() == expected, "thinking.bin wrong size");
    assert!(
        SPRITE_CONDUCTING.len() == expected,
        "conducting.bin wrong size"
    );
    assert!(SPRITE_IDLE.len() == expected, "idle.bin wrong size");
};

/// Returns the sprite bitmap for a given state. `_frame` is accepted for API
/// symmetry with the animator's two-frame flip but is ignored: every state
/// currently has a single authored frame. A future second-frame rollout can
/// branch on `frame` without churning call sites.
pub fn sprite(state: MascotState, _frame: usize) -> &'static [u8] {
    match state {
        MascotState::Error => SPRITE_ERROR,
        MascotState::Sleeping => SPRITE_SLEEPING,
        MascotState::Happy => SPRITE_HAPPY,
        MascotState::Thinking => SPRITE_THINKING,
        MascotState::Conducting => SPRITE_CONDUCTING,
        MascotState::Idle => SPRITE_IDLE,
    }
}

/// Unpacks a single pixel from a 1-bpp MSB-first bitmap. `w` is the bitmap
/// width in pixels (must be a multiple of 8). Out-of-range `(x, y)` are
/// clamped so callers don't have to guard the edges when downscaling.
pub fn pixel(bm: &[u8], w: u16, x: u16, y: u16) -> bool {
    let w = w.max(1) as usize;
    let stride = w.div_ceil(8);
    let total_rows = bm.len() / stride;
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
            let bm = sprite(state, 0);
            assert!(bm.iter().any(|b| *b != 0), "sprite({state:?}) is all-zero");
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
    fn sprite_is_frame_invariant_until_b_frames_authored() {
        for state in all_states() {
            assert_eq!(sprite(state, 0), sprite(state, 1));
        }
    }

    #[test]
    fn sprite_dispatch_covers_all_states() {
        for state in all_states() {
            assert!(!sprite(state, 0).is_empty());
        }
    }
}
