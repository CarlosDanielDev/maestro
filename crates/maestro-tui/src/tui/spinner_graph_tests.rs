//! Tests for `graph_node_frame` (issue #529).

use super::{NERD_FRAME_COUNT, graph_node_frame, spinner_frame};

#[test]
fn graph_node_frame_braille_cycles_all_10_frames() {
    let expected = [
        '\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}',
        '\u{2827}', '\u{2807}', '\u{280F}',
    ];
    for (i, &ch) in expected.iter().enumerate() {
        assert_eq!(graph_node_frame(i, true), ch, "braille frame {i} mismatch");
    }
    let unique: std::collections::HashSet<char> = expected.iter().copied().collect();
    assert_eq!(unique.len(), 10, "all 10 braille frames must be distinct");
}

#[test]
fn graph_node_frame_braille_wraps_at_10() {
    assert_eq!(graph_node_frame(0, true), graph_node_frame(10, true));
    assert_eq!(graph_node_frame(9, true), graph_node_frame(19, true));
    assert_ne!(graph_node_frame(0, true), graph_node_frame(1, true));
}

#[test]
fn graph_node_frame_ascii_delegates_to_spinner_frame() {
    for i in 0..4 {
        assert_eq!(
            graph_node_frame(i, false),
            spinner_frame(i),
            "ascii fallback frame {i} must match spinner_frame",
        );
    }
}

#[test]
fn graph_node_frame_ascii_wraps_at_4_not_10() {
    assert_eq!(graph_node_frame(0, false), graph_node_frame(4, false));
    assert_ne!(graph_node_frame(0, false), graph_node_frame(1, false));
}

#[test]
fn graph_node_frame_braille_large_tick_wraps_correctly() {
    assert_eq!(graph_node_frame(100, true), graph_node_frame(0, true));
}

#[test]
fn nerd_frame_count_equals_10() {
    assert_eq!(NERD_FRAME_COUNT, 10);
}
