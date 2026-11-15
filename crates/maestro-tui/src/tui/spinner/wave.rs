//! Braille "agent responding" wave indicator (#990 follow-up). Split out of
//! `spinner.rs` to keep that file under the 400-line cap. Pure, tick-driven,
//! fixed-width so the surrounding layout never shifts.

use super::tool_frame;

/// Width (cells) of the wave bar.
const WAVE_WIDTH: usize = 6;

/// Density ramp, empty → full: ` ⠁⠃⠇⠧⠷⠿`. The head of the comet uses the last
/// (densest) glyph; the tail fades toward the first.
const WAVE_LEVELS: &[char] = &[
    ' ', '\u{2801}', '\u{2803}', '\u{2807}', '\u{2827}', '\u{2837}', '\u{283F}',
];

/// A braille "comet" that sweeps left→right as `tick` advances — the calm
/// "agent is responding" indicator. Brightest at the head, fading to a tail
/// behind it, with a brief clear at the end of each sweep for a gentle pulse.
pub fn braille_wave(tick: usize) -> String {
    let tail = WAVE_LEVELS.len() - 1;
    let span = WAVE_WIDTH + tail; // head travels fully off the right before repeating
    let head = (tick % span) as isize;
    (0..WAVE_WIDTH as isize)
        .map(|i| {
            let d = head - i; // how far cell `i` sits behind the head
            if (0..=tail as isize).contains(&d) {
                WAVE_LEVELS[WAVE_LEVELS.len() - 1 - d as usize]
            } else {
                ' '
            }
        })
        .collect()
}

/// The "agent responding" indicator string: the braille wave on Nerd-Font
/// terminals, an ASCII progress bar ([`tool_frame`]) otherwise so non-nerd
/// terminals still get motion. Fixed width in both modes.
pub fn responding_wave(tick: usize, use_nerd_font: bool) -> String {
    if use_nerd_font {
        braille_wave(tick)
    } else {
        tool_frame(tick).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn braille_wave_is_fixed_width() {
        // Width never changes, so the surrounding layout never shifts.
        for t in 0..40 {
            assert_eq!(
                braille_wave(t).chars().count(),
                WAVE_WIDTH,
                "wave width drifted at tick {t}"
            );
        }
    }

    #[test]
    fn braille_wave_head_sweeps_left_to_right() {
        // tick 0: brightest head at the left edge.
        let f0: Vec<char> = braille_wave(0).chars().collect();
        assert_eq!(f0[0], '\u{283F}', "head starts at cell 0");
        assert_eq!(f0[1], ' ', "nothing ahead of the head yet");
        // tick 1: the head moved one cell right; cell 0 now trails (dimmer).
        let f1: Vec<char> = braille_wave(1).chars().collect();
        assert_eq!(f1[1], '\u{283F}', "head moved to cell 1");
        assert!(
            f1[0] != ' ' && f1[0] != '\u{283F}',
            "cell 0 fades to a tail"
        );
    }

    #[test]
    fn braille_wave_clears_once_per_cycle() {
        // The sweep ends with an all-blank frame for a gentle pulse before it
        // repeats, so there is exactly one empty frame per period.
        let period = WAVE_WIDTH + WAVE_LEVELS.len() - 1;
        let blanks = (0..period)
            .filter(|&t| braille_wave(t).trim().is_empty())
            .count();
        assert_eq!(blanks, 1, "expected exactly one clear frame per cycle");
    }

    #[test]
    fn responding_wave_falls_back_to_ascii_without_nerd_font() {
        assert_eq!(responding_wave(0, false), tool_frame(0));
        assert_eq!(responding_wave(0, true), braille_wave(0));
    }
}
