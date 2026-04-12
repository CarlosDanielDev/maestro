use super::state::MascotState;

/// Number of rows in the mascot ASCII art.
pub const MASCOT_ROWS: usize = 6;
/// Width of each mascot frame in characters.
pub const MASCOT_WIDTH: usize = 11;

const BLANK: [&str; 2] = ["           ", "           "];

/// Static frame data lookup. Returns frame A and frame B for a given (state, row).
/// Each frame string is exactly 11 chars wide (Unicode char count).
/// Out-of-range rows return blank frames.
pub struct MascotFrames;

impl MascotFrames {
    pub fn frames(state: MascotState, row: usize) -> [&'static str; 2] {
        let table: &[[&str; 2]; 6] = match state {
            MascotState::Idle => &IDLE_FRAMES,
            MascotState::Conducting => &CONDUCTING_FRAMES,
            MascotState::Thinking => &THINKING_FRAMES,
            MascotState::Happy => &HAPPY_FRAMES,
            MascotState::Sleeping => &SLEEPING_FRAMES,
            MascotState::Error => &ERROR_FRAMES,
        };
        table.get(row).copied().unwrap_or(BLANK)
    }
}

// All frames are exactly 11 chars wide (Unicode char count).
// Format: [frame_a, frame_b] — frame B provides the animation alternate.

// Idle: relaxed, blinks on frame B (eyes close)
static IDLE_FRAMES: [[&str; 2]; 6] = [
    ["  ▄▖   ▗▄  ", "  ▄▖   ▗▄  "],
    [" ▐▛█████▜▌ ", " ▐▛█████▜▌ "],
    [" ▐█◉   ◉█▌ ", " ▐█─   ─█▌ "],
    [" ▐▄ ─── ▄▌ ", " ▐▄ ─── ▄▌ "],
    [" ▐▙█████▟▌ ", " ▐▙█████▟▌ "],
    ["  ▗▘   ▝▖  ", "  ▗▘   ▝▖  "],
];

// Conducting: baton foot alternates
static CONDUCTING_FRAMES: [[&str; 2]; 6] = [
    ["  ▄▖   ▗▄  ", "  ▄▖   ▗▄  "],
    [" ▐▛█████▜▌ ", " ▐▛█████▜▌ "],
    [" ▐█◉   ◉█▌ ", " ▐█◉   ◉█▌ "],
    [" ▐▄ ▬▬▬ ▄▌ ", " ▐▄ ▬▬▬ ▄▌ "],
    [" ▐▙█████▟▌ ", " ▐▙█████▟▌ "],
    ["  ▗▘ ╱  ▝▖ ", "  ▗▘ ╲  ▝▖ "],
];

// Thinking: mouth dots cycle
static THINKING_FRAMES: [[&str; 2]; 6] = [
    ["  ▄▖   ▗▄  ", "  ▄▖   ▗▄  "],
    [" ▐▛█████▜▌ ", " ▐▛█████▜▌ "],
    [" ▐█◉   ◉█▌ ", " ▐█◉   ◉█▌ "],
    [" ▐▄ ··· ▄▌ ", " ▐▄ ... ▄▌ "],
    [" ▐▙█████▟▌ ", " ▐▙█████▟▌ "],
    ["  ▗▘   ▝▖  ", "  ▗▘   ▝▖  "],
];

// Happy: sparkle eyes, wide smile
static HAPPY_FRAMES: [[&str; 2]; 6] = [
    ["  ▄▖   ▗▄  ", "  ▄▖   ▗▄  "],
    [" ▐▛█████▜▌ ", " ▐▛█████▜▌ "],
    [" ▐█◆   ◆█▌ ", " ▐█✦   ✦█▌ "],
    [" ▐▄ \\o/ ▄▌ ", " ▐▄ \\o/ ▄▌ "],
    [" ▐▙█████▟▌ ", " ▐▙█████▟▌ "],
    ["  ▗▘   ▝▖  ", "  ▗▘   ▝▖  "],
];

// Sleeping: closed eyes, zzz alternates
static SLEEPING_FRAMES: [[&str; 2]; 6] = [
    ["  ▄▖   ▗▄  ", "  ▄▖   ▗▄  "],
    [" ▐▛█████▜▌ ", " ▐▛█████▜▌ "],
    [" ▐█─   ─█▌ ", " ▐█─   ─█▌ "],
    [" ▐▄ ─── ▄▌ ", " ▐▄ ─── ▄▌ "],
    [" ▐▙█████▟▌ ", " ▐▙█████▟▌ "],
    ["  ▗▘ zZ ▝▖ ", "  ▗▘ Zz ▝▖ "],
];

// Error: X eyes, grimace alternates
static ERROR_FRAMES: [[&str; 2]; 6] = [
    ["  ▄▖   ▗▄  ", "  ▄▖   ▗▄  "],
    [" ▐▛█████▜▌ ", " ▐▛█████▜▌ "],
    [" ▐█✕   ✕█▌ ", " ▐█✕   ✕█▌ "],
    [" ▐▄ /~\\ ▄▌ ", " ▐▄ ~~~ ▄▌ "],
    [" ▐▙█████▟▌ ", " ▐▙█████▟▌ "],
    ["  ▗▘   ▝▖  ", "  ▗▘   ▝▖  "],
];
