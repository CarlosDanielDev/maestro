use super::state::MascotState;

/// Number of rows in the mascot ASCII art.
pub const MASCOT_ROWS: usize = 6;
/// Width of each mascot frame in characters.
pub const MASCOT_WIDTH: usize = 11;

const BLANK: [&str; 2] = ["           ", "           "];

/// Static frame data lookup. Returns frame A and frame B for a given (state, row).
/// Each frame string is exactly 11 chars wide (Unicode char count).
/// Uses only single-width ASCII/basic characters for terminal compatibility.
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

// All frames are exactly 11 chars wide, using ASCII-safe characters.
// [frame_a, frame_b] — frame B provides the animation alternate.

// Idle: relaxed, blinks on frame B
static IDLE_FRAMES: [[&str; 2]; 6] = [
    ["   /\\_/\\   ", "   /\\_/\\   "],
    ["  ( o.o )  ", "  ( -.- )  "],
    ["  /> ^ <\\  ", "  /> ^ <\\  "],
    ["  /|   |\\  ", "  /|   |\\  "],
    ["  (_| |_)  ", "  (_| |_)  "],
    ["    m m    ", "    m m    "],
];

// Conducting: baton foot alternates
static CONDUCTING_FRAMES: [[&str; 2]; 6] = [
    ["   /\\_/\\   ", "   /\\_/\\   "],
    ["  ( o.o )  ", "  ( o.o )  "],
    ["  /> ~ <\\  ", "  /> ~ <\\  "],
    ["  /| / |\\  ", "  /| \\ |\\  "],
    ["  (_| |_)  ", "  (_| |_)  "],
    ["    m m    ", "    m m    "],
];

// Thinking: dots cycle in mouth
static THINKING_FRAMES: [[&str; 2]; 6] = [
    ["   /\\_/\\   ", "   /\\_/\\   "],
    ["  ( o.o )  ", "  ( o.o )  "],
    ["  /> . <\\  ", "  />.. <\\  "],
    ["  /|   |\\  ", "  /|   |\\  "],
    ["  (_| |_)  ", "  (_| |_)  "],
    ["    m m    ", "    m m    "],
];

// Happy: sparkle eyes, wide smile
static HAPPY_FRAMES: [[&str; 2]; 6] = [
    ["   /\\_/\\   ", "   /\\_/\\   "],
    ["  ( ^.^ )  ", "  ( *.* )  "],
    ["  /> w <\\  ", "  /> w <\\  "],
    ["  /|   |\\  ", "  /|   |\\  "],
    ["  (_| |_)  ", "  (_| |_)  "],
    ["    m m    ", "    m m    "],
];

// Sleeping: closed eyes, zzz alternates
static SLEEPING_FRAMES: [[&str; 2]; 6] = [
    ["   /\\_/\\   ", "   /\\_/\\   "],
    ["  ( -.- )  ", "  ( -.- )  "],
    ["  /> ~ <\\  ", "  /> ~ <\\  "],
    ["  /|   |\\  ", "  /|   |\\  "],
    ["  (_| |_)  ", "  (_| |_)  "],
    ["   m zZ m  ", "   m Zz m  "],
];

// Error: X eyes, grimace alternates
static ERROR_FRAMES: [[&str; 2]; 6] = [
    ["   /\\_/\\   ", "   /\\_/\\   "],
    ["  ( x.x )  ", "  ( X.X )  "],
    ["  /> n <\\  ", "  /> ~ <\\  "],
    ["  /|   |\\  ", "  /|   |\\  "],
    ["  (_| |_)  ", "  (_| |_)  "],
    ["    m m    ", "    m m    "],
];
