use crate::session::types::SessionStatus;

const BRAILLE_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Tool-use animation: progress bar style.
const TOOL_FRAMES: &[&str] = &["[>  ]", "[>> ]", "[>>>]", "[ >>]", "[  >]", "[   ]"];

/// Spawning animation: growing dots.
const SPAWNING_FRAMES: &[&str] = &[".", "..", "...", "....", ".....", "......"];

/// Idle pulse: subtle breathing effect.
const IDLE_FRAMES: &[char] = &['_', '-', '~', '-'];

/// Visual animation phase, determined from session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationPhase {
    Thinking,
    ToolUse,
    Spawning,
    Idle,
    /// Terminal states — no animation.
    None,
}

/// Determine the animation phase from session state.
pub fn animation_phase(
    status: SessionStatus,
    is_thinking: bool,
    current_activity: &str,
) -> AnimationPhase {
    if is_thinking {
        return AnimationPhase::Thinking;
    }
    match status {
        SessionStatus::Spawning => AnimationPhase::Spawning,
        SessionStatus::Running => {
            if current_activity.starts_with("Read:")
                || current_activity.starts_with("Write:")
                || current_activity.starts_with("Edit:")
                || current_activity.starts_with("$ ")
                || current_activity.starts_with("Using ")
                || current_activity.starts_with("Grep:")
                || current_activity.starts_with("Glob:")
                || current_activity.starts_with("Bash:")
            {
                AnimationPhase::ToolUse
            } else {
                AnimationPhase::Idle
            }
        }
        _ if status.is_terminal() => AnimationPhase::None,
        _ => AnimationPhase::Idle,
    }
}

/// Returns the braille spinner character for a given tick index.
pub fn spinner_frame(tick: usize) -> char {
    BRAILLE_FRAMES[tick % BRAILLE_FRAMES.len()]
}

/// Returns the tool-use animation frame for a given tick.
pub fn tool_frame(tick: usize) -> &'static str {
    TOOL_FRAMES[tick % TOOL_FRAMES.len()]
}

/// Returns the spawning animation frame for a given tick.
pub fn spawning_frame(tick: usize) -> &'static str {
    SPAWNING_FRAMES[tick % SPAWNING_FRAMES.len()]
}

/// Returns the idle pulse character for a given tick.
pub fn idle_pulse(tick: usize) -> char {
    IDLE_FRAMES[tick % IDLE_FRAMES.len()]
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

/// Unified activity string for any animation phase.
pub fn animated_activity(
    phase: AnimationPhase,
    tick: usize,
    activity: &str,
    thinking_elapsed: Option<std::time::Duration>,
) -> String {
    match phase {
        AnimationPhase::Thinking => {
            let elapsed = thinking_elapsed.unwrap_or_default();
            thinking_activity(tick, elapsed)
        }
        AnimationPhase::ToolUse => {
            format!("{} {}", tool_frame(tick), activity)
        }
        AnimationPhase::Spawning => {
            format!(
                "{} Initializing{}",
                spinner_frame(tick),
                spawning_frame(tick)
            )
        }
        AnimationPhase::Idle => {
            format!("{} {}", idle_pulse(tick), activity)
        }
        AnimationPhase::None => activity.to_string(),
    }
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

    // --- Issue #199: Multi-phase animation tests ---

    #[test]
    fn tool_frame_cycles_correctly() {
        let f0 = tool_frame(0);
        let f1 = tool_frame(1);
        assert_ne!(f0, f1, "sequential tool frames should differ");
        assert_eq!(
            tool_frame(0),
            tool_frame(6),
            "tool frames should wrap at boundary"
        );
    }

    #[test]
    fn spawning_frame_cycles_correctly() {
        let f0 = spawning_frame(0);
        let f1 = spawning_frame(1);
        assert_ne!(f0, f1, "sequential spawning frames should differ");
        assert_eq!(
            spawning_frame(0),
            spawning_frame(6),
            "spawning frames should wrap"
        );
    }

    #[test]
    fn idle_pulse_cycles_correctly() {
        let f0 = idle_pulse(0);
        let f1 = idle_pulse(1);
        assert_ne!(f0, f1, "sequential idle frames should differ");
        assert_eq!(
            idle_pulse(0),
            idle_pulse(4),
            "idle frames should wrap at boundary"
        );
    }

    #[test]
    fn animation_phase_thinking_overrides_status() {
        use crate::session::types::SessionStatus;
        let phase = animation_phase(SessionStatus::Running, true, "something");
        assert_eq!(phase, AnimationPhase::Thinking);
    }

    #[test]
    fn animation_phase_tool_use_from_bash_activity() {
        use crate::session::types::SessionStatus;
        let phase = animation_phase(SessionStatus::Running, false, "$ cargo test");
        assert_eq!(phase, AnimationPhase::ToolUse);
    }

    #[test]
    fn animation_phase_tool_use_from_read_activity() {
        use crate::session::types::SessionStatus;
        let phase = animation_phase(SessionStatus::Running, false, "Read: /src/main.rs");
        assert_eq!(phase, AnimationPhase::ToolUse);
    }

    #[test]
    fn animation_phase_spawning() {
        use crate::session::types::SessionStatus;
        let phase = animation_phase(SessionStatus::Spawning, false, "");
        assert_eq!(phase, AnimationPhase::Spawning);
    }

    #[test]
    fn animation_phase_terminal_is_none() {
        use crate::session::types::SessionStatus;
        let phase = animation_phase(SessionStatus::Completed, false, "done");
        assert_eq!(phase, AnimationPhase::None);
    }

    #[test]
    fn animation_phase_idle_for_running_without_tool() {
        use crate::session::types::SessionStatus;
        let phase = animation_phase(SessionStatus::Running, false, "Working on feature");
        assert_eq!(phase, AnimationPhase::Idle);
    }

    #[test]
    fn animated_activity_thinking_includes_elapsed() {
        let result = animated_activity(
            AnimationPhase::Thinking,
            0,
            "",
            Some(Duration::from_secs(5)),
        );
        assert!(result.contains("Thinking..."));
        assert!(result.contains("5.0s"));
    }

    #[test]
    fn animated_activity_tool_use_includes_activity() {
        let result = animated_activity(AnimationPhase::ToolUse, 0, "$ cargo test", None);
        assert!(result.contains("$ cargo test"));
        assert!(result.contains('['));
    }

    #[test]
    fn animated_activity_none_returns_plain_activity() {
        let result = animated_activity(AnimationPhase::None, 0, "Completed", None);
        assert_eq!(result, "Completed");
    }
}
