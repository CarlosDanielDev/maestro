const BRAILLE_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Returns the braille spinner character for a given tick index.
pub fn spinner_frame(tick: usize) -> char {
    BRAILLE_FRAMES[tick % BRAILLE_FRAMES.len()]
}

/// Format elapsed thinking time for display.
pub fn format_thinking_elapsed(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs >= 60 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{:.1}s", d.as_secs_f64())
    }
}

/// Build the full spinner activity string: `⠹ Thinking... 3.2s`
pub fn thinking_activity(tick: usize, elapsed: std::time::Duration) -> String {
    format!(
        "{} Thinking... {}",
        spinner_frame(tick),
        format_thinking_elapsed(elapsed)
    )
}

/// Total number of braille frames in the spinner cycle.
#[allow(dead_code)] // Reason: public constant for spinner consumers
pub const FRAME_COUNT: usize = 10;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn spinner_cycles_through_all_10_braille_frames() {
        let expected = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        for (i, &ch) in expected.iter().enumerate() {
            assert_eq!(spinner_frame(i), ch, "frame {} mismatch", i);
        }
    }

    #[test]
    fn spinner_wraps_around_after_10_frames() {
        assert_eq!(spinner_frame(0), spinner_frame(10));
        assert_eq!(spinner_frame(3), spinner_frame(13));
        assert_eq!(spinner_frame(9), spinner_frame(19));
    }

    #[test]
    fn frame_count_matches_braille_frames_len() {
        assert_eq!(FRAME_COUNT, BRAILLE_FRAMES.len());
    }

    #[test]
    fn format_elapsed_sub_second() {
        let d = Duration::from_millis(500);
        assert_eq!(format_thinking_elapsed(d), "0.5s");
    }

    #[test]
    fn format_elapsed_seconds() {
        let d = Duration::from_secs_f64(3.2);
        assert_eq!(format_thinking_elapsed(d), "3.2s");
    }

    #[test]
    fn format_elapsed_zero() {
        let d = Duration::from_secs(0);
        assert_eq!(format_thinking_elapsed(d), "0.0s");
    }

    #[test]
    fn format_elapsed_minutes_transition() {
        let d = Duration::from_secs(65);
        assert_eq!(format_thinking_elapsed(d), "1m05s");
    }

    #[test]
    fn format_elapsed_exact_minute() {
        let d = Duration::from_secs(60);
        assert_eq!(format_thinking_elapsed(d), "1m00s");
    }

    #[test]
    fn format_elapsed_multiple_minutes() {
        let d = Duration::from_secs(185);
        assert_eq!(format_thinking_elapsed(d), "3m05s");
    }

    #[test]
    fn thinking_activity_combines_frame_and_elapsed() {
        let d = Duration::from_secs_f64(3.2);
        let result = thinking_activity(2, d);
        assert_eq!(result, "⠹ Thinking... 3.2s");
    }

    #[test]
    fn thinking_activity_with_different_tick() {
        let d = Duration::from_secs(0);
        let result = thinking_activity(0, d);
        assert!(result.starts_with('⠋'));
        assert!(result.contains("Thinking..."));
    }
}
