//! Marquee/carousel animation for overflowing text in TUI lists.
//!
//! Provides a tick-driven state machine that scrolls long text horizontally
//! with configurable pause durations at start and end positions.

/// Phase of the marquee scroll cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarqueePhase {
    /// Pausing at the beginning (showing text start).
    PauseStart,
    /// Scrolling left, revealing hidden text.
    Scrolling,
    /// Pausing at the end (showing text end).
    PauseEnd,
}

/// Immutable tuning parameters for marquee animation timing.
#[derive(Debug, Clone, Copy)]
pub struct MarqueeConfig {
    /// How many ticks to dwell at the start before scrolling begins.
    pub pause_start_ticks: u64,
    /// How many ticks to dwell at the end before looping.
    pub pause_end_ticks: u64,
    /// Number of characters advanced per tick while scrolling.
    pub chars_per_tick: u64,
}

impl Default for MarqueeConfig {
    fn default() -> Self {
        // At ~30 fps draw rate: 45 ticks ≈ 1.5s pause, 1 char/tick ≈ 30 chars/sec
        Self {
            pause_start_ticks: 45,
            pause_end_ticks: 45,
            chars_per_tick: 1,
        }
    }
}

/// Per-element mutable animation state.
#[derive(Debug, Clone)]
pub struct MarqueeState {
    pub phase: MarqueePhase,
    pub tick: u64,
    pub offset: usize,
}

impl MarqueeState {
    pub fn new() -> Self {
        Self {
            phase: MarqueePhase::PauseStart,
            tick: 0,
            offset: 0,
        }
    }

    /// Advance one logical tick.
    ///
    /// `overflow` is `text_len.saturating_sub(viewport_width)`.
    /// When overflow is 0 the state stays at PauseStart with offset 0.
    pub fn advance(&mut self, overflow: usize, cfg: &MarqueeConfig) {
        if overflow == 0 {
            return;
        }

        if self.phase == MarqueePhase::PauseStart {
            self.tick += 1;
            if self.tick >= cfg.pause_start_ticks {
                self.phase = MarqueePhase::Scrolling;
                self.tick = 0;
            } else {
                return;
            }
        }

        if self.phase == MarqueePhase::Scrolling {
            self.offset = (self.offset + cfg.chars_per_tick as usize).min(overflow);
            self.tick += 1;
            if self.offset >= overflow {
                self.phase = MarqueePhase::PauseEnd;
                self.tick = 0;
            }
            return;
        }

        if self.phase == MarqueePhase::PauseEnd {
            self.tick += 1;
            if self.tick >= cfg.pause_end_ticks {
                self.phase = MarqueePhase::PauseStart;
                self.tick = 0;
                self.offset = 0;
            }
        }
    }

    /// Reset to initial state.
    pub fn reset(&mut self) {
        self.phase = MarqueePhase::PauseStart;
        self.tick = 0;
        self.offset = 0;
    }
}

/// Return the visible substring of `text` starting at char-`offset`
/// and spanning `viewport_width` characters.
///
/// Always returns exactly `viewport_width` characters (right-padded with spaces).
pub fn visible_slice(text: &str, offset: usize, viewport_width: usize) -> String {
    if viewport_width == 0 {
        return String::new();
    }

    let chars: Vec<char> = text.chars().collect();
    if offset >= chars.len() {
        return " ".repeat(viewport_width);
    }

    let end = (offset + viewport_width).min(chars.len());
    let visible: String = chars[offset..end].iter().collect();
    let padding = viewport_width.saturating_sub(end - offset);
    if padding > 0 {
        format!("{}{}", visible, " ".repeat(padding))
    } else {
        visible
    }
}

/// True when the text overflows the viewport.
#[inline]
pub fn needs_scroll(text_len: usize, viewport_width: usize) -> bool {
    text_len > viewport_width
}

// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    // --- Suite 1: needs_scroll ---

    #[test]
    fn needs_scroll_returns_false_when_text_fits_exactly() {
        assert!(!needs_scroll(20, 20));
    }

    #[test]
    fn needs_scroll_returns_false_when_text_is_shorter() {
        assert!(!needs_scroll(5, 20));
    }

    #[test]
    fn needs_scroll_returns_true_when_text_overflows_by_one() {
        assert!(needs_scroll(21, 20));
    }

    #[test]
    fn needs_scroll_returns_true_when_text_is_much_longer() {
        assert!(needs_scroll(80, 20));
    }

    #[test]
    fn needs_scroll_zero_width_viewport_returns_true_for_nonempty() {
        assert!(needs_scroll(1, 0));
    }

    #[test]
    fn needs_scroll_empty_text_returns_false() {
        assert!(!needs_scroll(0, 20));
    }

    // --- Suite 2: visible_slice ---

    #[test]
    fn visible_slice_at_offset_zero_returns_first_n_chars() {
        let text = "Hello, world! This is a long title";
        assert_eq!(visible_slice(text, 0, 13), "Hello, world!");
    }

    #[test]
    fn visible_slice_mid_scroll_returns_correct_window() {
        let text = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        assert_eq!(visible_slice(text, 5, 10), "FGHIJKLMNO");
    }

    #[test]
    fn visible_slice_at_max_offset_shows_end_of_text() {
        let text = "ABCDEFGHIJ";
        assert_eq!(visible_slice(text, 6, 4), "GHIJ");
    }

    #[test]
    fn visible_slice_offset_beyond_text_returns_empty_padded() {
        let result = visible_slice("Hi", 10, 5);
        assert_eq!(result.len(), 5);
        assert!(result.chars().all(|c| c == ' '));
    }

    #[test]
    fn visible_slice_empty_string_returns_spaces() {
        assert_eq!(visible_slice("", 0, 10), "          ");
    }

    #[test]
    fn visible_slice_zero_width_returns_empty_string() {
        assert_eq!(visible_slice("Something", 0, 0), "");
    }

    #[test]
    fn visible_slice_text_shorter_than_viewport_pads_with_spaces() {
        let result = visible_slice("Hi", 0, 8);
        assert_eq!(result, "Hi      ");
        assert_eq!(result.chars().count(), 8);
    }

    #[test]
    fn visible_slice_returns_correct_result_for_ascii_at_every_offset() {
        let text = "ABCDEFGHIJKLMNOPQRST"; // 20 chars
        let viewport_width = 5;
        for offset in 0..=15usize {
            let result = visible_slice(text, offset, viewport_width);
            let expected = &text[offset..offset + viewport_width];
            assert_eq!(
                result, expected,
                "offset={offset} expected={expected:?} got={result:?}"
            );
        }
    }

    #[test]
    fn visible_slice_unicode_multibyte_does_not_panic() {
        let text = "Fix \u{1F41B} crash in parser loop";
        let result = visible_slice(text, 4, 10);
        assert_eq!(result.chars().count(), 10);
    }

    // --- Suite 3: MarqueeState::new ---

    #[test]
    fn marquee_state_new_initial_phase_is_pause_start() {
        assert_eq!(MarqueeState::new().phase, MarqueePhase::PauseStart);
    }

    #[test]
    fn marquee_state_new_initial_offset_is_zero() {
        assert_eq!(MarqueeState::new().offset, 0);
    }

    #[test]
    fn marquee_state_new_initial_tick_is_zero() {
        assert_eq!(MarqueeState::new().tick, 0);
    }

    // --- Suite 4: advance — PauseStart ---

    #[test]
    fn advance_in_pause_start_does_not_change_offset() {
        let mut state = MarqueeState::new();
        let cfg = MarqueeConfig {
            pause_start_ticks: 45,
            ..MarqueeConfig::default()
        };
        state.advance(10, &cfg);
        assert_eq!(state.offset, 0);
        assert_eq!(state.phase, MarqueePhase::PauseStart);
    }

    #[test]
    fn advance_in_pause_start_increments_tick() {
        let mut state = MarqueeState::new();
        let cfg = MarqueeConfig::default();
        state.advance(10, &cfg);
        assert_eq!(state.tick, 1);
    }

    #[test]
    fn advance_transitions_to_scrolling_after_pause_start_ticks() {
        let cfg = MarqueeConfig {
            pause_start_ticks: 3,
            pause_end_ticks: 45,
            chars_per_tick: 1,
        };
        let mut state = MarqueeState::new();
        state.advance(10, &cfg);
        state.advance(10, &cfg);
        state.advance(10, &cfg);
        // Falls through from PauseStart to Scrolling and scrolls in same tick
        assert_eq!(state.phase, MarqueePhase::Scrolling);
        assert_eq!(state.offset, 1);
    }

    #[test]
    fn advance_does_not_transition_one_tick_before_pause_start_ends() {
        let cfg = MarqueeConfig {
            pause_start_ticks: 3,
            pause_end_ticks: 45,
            chars_per_tick: 1,
        };
        let mut state = MarqueeState::new();
        state.advance(10, &cfg);
        state.advance(10, &cfg);
        assert_eq!(state.phase, MarqueePhase::PauseStart);
    }

    // --- Suite 5: advance — Scrolling ---

    #[test]
    fn advance_scrolling_increments_offset_by_chars_per_tick() {
        let cfg = MarqueeConfig {
            pause_start_ticks: 0,
            pause_end_ticks: 99,
            chars_per_tick: 1,
        };
        let mut state = MarqueeState::new();
        state.advance(10, &cfg); // transitions to Scrolling, then scrolls
        assert_eq!(state.phase, MarqueePhase::Scrolling);
        assert_eq!(state.offset, 1);
    }

    #[test]
    fn advance_scrolling_transitions_to_pause_end_when_offset_reaches_overflow() {
        let cfg = MarqueeConfig {
            pause_start_ticks: 0,
            pause_end_ticks: 99,
            chars_per_tick: 1,
        };
        let mut state = MarqueeState::new();
        state.advance(3, &cfg); // offset=1
        state.advance(3, &cfg); // offset=2
        state.advance(3, &cfg); // offset=3 == overflow → PauseEnd
        assert_eq!(state.phase, MarqueePhase::PauseEnd);
        assert_eq!(state.offset, 3);
    }

    #[test]
    fn advance_scrolling_does_not_exceed_overflow() {
        let cfg = MarqueeConfig {
            pause_start_ticks: 0,
            pause_end_ticks: 999,
            chars_per_tick: 1,
        };
        let mut state = MarqueeState::new();
        for _ in 0..50 {
            state.advance(5, &cfg);
            assert!(state.offset <= 5);
        }
    }

    #[test]
    fn advance_scrolling_offset_increases_each_tick() {
        let cfg = MarqueeConfig {
            pause_start_ticks: 0,
            pause_end_ticks: 99,
            chars_per_tick: 1,
        };
        let mut state = MarqueeState::new();
        for _ in 0..5 {
            state.advance(20, &cfg);
        }
        assert_eq!(state.offset, 5);
    }

    // --- Suite 6: advance — PauseEnd ---

    fn state_at_pause_end(overflow: usize) -> (MarqueeState, MarqueeConfig) {
        let cfg = MarqueeConfig {
            pause_start_ticks: 0,
            pause_end_ticks: 10,
            chars_per_tick: 1,
        };
        let mut state = MarqueeState::new();
        // With pause_start=0, PauseStart falls through to Scrolling on first tick.
        // So `overflow` advances reach PauseEnd.
        for _ in 0..overflow {
            state.advance(overflow, &cfg);
        }
        assert_eq!(state.phase, MarqueePhase::PauseEnd);
        (state, cfg)
    }

    #[test]
    fn advance_in_pause_end_does_not_change_offset() {
        let (mut state, cfg) = state_at_pause_end(5);
        let offset_before = state.offset;
        state.advance(5, &cfg);
        assert_eq!(state.offset, offset_before);
    }

    #[test]
    fn advance_pause_end_transitions_to_pause_start_after_dwell() {
        let cfg = MarqueeConfig {
            pause_start_ticks: 0,
            pause_end_ticks: 2,
            chars_per_tick: 1,
        };
        let mut state = MarqueeState::new();
        // Reach PauseEnd: with pause_start=0 fall-through, overflow=5 takes 5 advances
        for _ in 0..5usize {
            state.advance(5, &cfg);
        }
        assert_eq!(state.phase, MarqueePhase::PauseEnd);
        // Burn through dwell (2 ticks)
        state.advance(5, &cfg);
        state.advance(5, &cfg);
        assert_eq!(state.phase, MarqueePhase::PauseStart);
        assert_eq!(state.offset, 0);
        assert_eq!(state.tick, 0);
    }

    #[test]
    fn advance_pause_end_does_not_loop_one_tick_before_dwell_ends() {
        let cfg = MarqueeConfig {
            pause_start_ticks: 0,
            pause_end_ticks: 2,
            chars_per_tick: 1,
        };
        let mut state = MarqueeState::new();
        for _ in 0..5usize {
            state.advance(5, &cfg);
        }
        assert_eq!(state.phase, MarqueePhase::PauseEnd);
        state.advance(5, &cfg); // 1 of 2 dwell ticks
        assert_eq!(state.phase, MarqueePhase::PauseEnd);
    }

    // --- Suite 7: reset ---

    #[test]
    fn reset_from_scrolling_returns_to_pause_start() {
        let cfg = MarqueeConfig {
            pause_start_ticks: 0,
            pause_end_ticks: 99,
            chars_per_tick: 1,
        };
        let mut state = MarqueeState::new();
        state.advance(10, &cfg);
        state.advance(10, &cfg);
        state.reset();
        assert_eq!(state.phase, MarqueePhase::PauseStart);
        assert_eq!(state.offset, 0);
        assert_eq!(state.tick, 0);
    }

    #[test]
    fn reset_from_pause_end_returns_to_pause_start() {
        let cfg = MarqueeConfig {
            pause_start_ticks: 0,
            pause_end_ticks: 99,
            chars_per_tick: 1,
        };
        let mut state = MarqueeState::new();
        for _ in 0..=5usize {
            state.advance(5, &cfg);
        }
        state.reset();
        assert_eq!(state.phase, MarqueePhase::PauseStart);
        assert_eq!(state.offset, 0);
        assert_eq!(state.tick, 0);
    }

    #[test]
    fn reset_is_idempotent_on_new_state() {
        let mut state = MarqueeState::new();
        state.reset();
        assert_eq!(state.phase, MarqueePhase::PauseStart);
        assert_eq!(state.offset, 0);
        assert_eq!(state.tick, 0);
    }

    // --- Suite 8: full cycle ---

    #[test]
    fn full_cycle_pause_start_scroll_pause_end_loops() {
        let cfg = MarqueeConfig {
            pause_start_ticks: 2,
            pause_end_ticks: 2,
            chars_per_tick: 1,
        };
        let overflow = 4usize;
        let mut state = MarqueeState::new();

        // Tick 1: PauseStart dwell (tick becomes 1, < 2)
        state.advance(overflow, &cfg);
        assert_eq!(
            (state.phase, state.offset),
            (MarqueePhase::PauseStart, 0),
            "tick 1: still in PauseStart"
        );

        // Tick 2: PauseStart tick=2 >= 2 → falls through to Scrolling, offset=1
        state.advance(overflow, &cfg);
        assert_eq!(
            (state.phase, state.offset),
            (MarqueePhase::Scrolling, 1),
            "tick 2: transition + first scroll"
        );

        // Ticks 3-5: Scrolling, offset 2→4
        for expected_offset in 2..=4usize {
            state.advance(overflow, &cfg);
            assert_eq!(
                state.offset, expected_offset,
                "scroll tick: expected offset {expected_offset}"
            );
        }
        assert_eq!(state.phase, MarqueePhase::PauseEnd);

        // Ticks 6-7: PauseEnd dwell
        state.advance(overflow, &cfg);
        assert_eq!(
            (state.phase, state.offset),
            (MarqueePhase::PauseEnd, 4),
            "pause end tick 1"
        );
        state.advance(overflow, &cfg);
        assert_eq!(state.phase, MarqueePhase::PauseStart);
        assert_eq!(state.offset, 0);
    }

    #[test]
    fn full_cycle_with_zero_overflow_stays_at_pause_start() {
        let cfg = MarqueeConfig::default();
        let mut state = MarqueeState::new();
        for i in 0..100u64 {
            state.advance(0, &cfg);
            assert_eq!(
                state.phase,
                MarqueePhase::PauseStart,
                "tick {i}: zero overflow must never leave PauseStart"
            );
            assert_eq!(state.offset, 0);
        }
    }

    // --- Suite 9: edge cases ---

    #[test]
    fn advance_with_overflow_one_scrolls_to_one_then_loops() {
        let cfg = MarqueeConfig {
            pause_start_ticks: 0,
            pause_end_ticks: 0,
            chars_per_tick: 1,
        };
        let mut state = MarqueeState::new();

        // First advance: pause_start=0 → Scrolling, offset becomes 1 == overflow → PauseEnd
        state.advance(1, &cfg);
        assert_eq!(state.phase, MarqueePhase::PauseEnd);
        assert_eq!(state.offset, 1);

        // Second advance: pause_end=0 → PauseStart, offset resets
        state.advance(1, &cfg);
        assert_eq!(state.phase, MarqueePhase::PauseStart);
        assert_eq!(state.offset, 0);

        // After enough advances the loop completes repeatedly
        for _ in 0..10 {
            state.advance(1, &cfg);
            assert!(state.offset <= 1);
        }
    }
}
