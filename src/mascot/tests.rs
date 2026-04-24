use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;

use crate::mascot::animator::{Clock, MascotAnimator};
use crate::mascot::derive_dashboard_mascot_state;
use crate::mascot::frames::AsciiMascotFrames;
use crate::mascot::state::MascotState;
use crate::mascot::widget::{CLAWD_ORANGE, MascotWidget};
use crate::session::types::SessionStatus;

// ---------------------------------------------------------------------------
// MockClock
// ---------------------------------------------------------------------------

struct MockClock {
    base: Instant,
    offset_ms: std::cell::Cell<u64>,
}

impl MockClock {
    fn new() -> Self {
        Self {
            base: Instant::now(),
            offset_ms: std::cell::Cell::new(0),
        }
    }

    fn advance(&self, ms: u64) {
        self.offset_ms.set(self.offset_ms.get() + ms);
    }
}

impl Clock for MockClock {
    fn now(&self) -> Instant {
        self.base + Duration::from_millis(self.offset_ms.get())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn all_states() -> [MascotState; 6] {
    [
        MascotState::Idle,
        MascotState::Conducting,
        MascotState::Thinking,
        MascotState::Happy,
        MascotState::Sleeping,
        MascotState::Error,
    ]
}

fn render_to_buffer(widget: MascotWidget, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);
    buf
}

fn buffer_row_string(buf: &Buffer, y: u16) -> String {
    let mut s = String::new();
    for x in 0..buf.area.width {
        s.push_str(buf.cell((x, y)).map_or(" ", |c| c.symbol()));
    }
    s
}

// ---------------------------------------------------------------------------
// Suite 1 — MascotState
// ---------------------------------------------------------------------------

#[test]
fn mascot_state_auto_revert_happy_returns_2000ms() {
    assert_eq!(MascotState::Happy.auto_revert_ms(), Some(2000));
}

#[test]
fn mascot_state_auto_revert_idle_returns_none() {
    assert_eq!(MascotState::Idle.auto_revert_ms(), None);
}

#[test]
fn mascot_state_auto_revert_all_non_happy_return_none() {
    for state in [
        MascotState::Idle,
        MascotState::Conducting,
        MascotState::Thinking,
        MascotState::Sleeping,
        MascotState::Error,
    ] {
        assert_eq!(
            state.auto_revert_ms(),
            None,
            "Expected None for {:?}",
            state
        );
    }
}

#[test]
fn mascot_state_copy_and_partial_eq() {
    let a = MascotState::Idle;
    let b = a;
    assert_eq!(a, b);
    assert_ne!(MascotState::Idle, MascotState::Happy);
}

// ---------------------------------------------------------------------------
// Suite 2 — MascotFrames
// ---------------------------------------------------------------------------

#[test]
fn mascot_frames_each_string_is_exactly_11_chars_wide() {
    for state in all_states() {
        for row in 0..6usize {
            let pair = AsciiMascotFrames::frames(state, row);
            assert_eq!(
                pair[0].chars().count(),
                11,
                "state={:?} row={} frame=0 content={:?}",
                state,
                row,
                pair[0]
            );
            assert_eq!(
                pair[1].chars().count(),
                11,
                "state={:?} row={} frame=1 content={:?}",
                state,
                row,
                pair[1]
            );
        }
    }
}

#[test]
fn mascot_frames_all_36_pairs_are_non_empty() {
    for state in all_states() {
        for row in 0..6usize {
            let pair = AsciiMascotFrames::frames(state, row);
            assert!(
                !pair[0].is_empty(),
                "state={:?} row={} frame=0 empty",
                state,
                row
            );
            assert!(
                !pair[1].is_empty(),
                "state={:?} row={} frame=1 empty",
                state,
                row
            );
        }
    }
}

#[test]
fn mascot_frames_out_of_range_row_returns_blank() {
    for state in all_states() {
        for row in [6usize, 7, 100] {
            let pair = AsciiMascotFrames::frames(state, row);
            assert_eq!(
                pair[0], pair[1],
                "Blank frames should be identical: state={:?} row={}",
                state, row
            );
            assert!(
                pair[0].chars().all(|c| c == ' '),
                "Out-of-range frame must be blank: state={:?} row={} got={:?}",
                state,
                row,
                pair[0]
            );
        }
    }
}

#[test]
fn mascot_frames_at_least_one_row_animates_per_state() {
    for state in all_states() {
        let has_animation = (0..6usize)
            .map(|row| AsciiMascotFrames::frames(state, row))
            .any(|pair| pair[0] != pair[1]);
        assert!(has_animation, "State {:?} has no animating rows", state);
    }
}

// ---------------------------------------------------------------------------
// Suite 3 — MascotAnimator
// ---------------------------------------------------------------------------

#[test]
fn animator_initial_frame_index_is_zero() {
    let clock = MockClock::new();
    let animator = MascotAnimator::new(&clock);
    assert_eq!(animator.frame_index(), 0);
}

#[test]
fn animator_initial_state_is_idle() {
    let clock = MockClock::new();
    let animator = MascotAnimator::new(&clock);
    assert_eq!(animator.state(), MascotState::Idle);
}

#[test]
fn animator_tick_before_850ms_does_not_flip_frame() {
    let clock = MockClock::new();
    let mut animator = MascotAnimator::new(&clock);
    clock.advance(849);
    animator.tick(&clock);
    assert_eq!(animator.frame_index(), 0);
}

#[test]
fn animator_tick_at_exactly_850ms_flips_frame() {
    let clock = MockClock::new();
    let mut animator = MascotAnimator::new(&clock);
    clock.advance(850);
    animator.tick(&clock);
    assert_eq!(animator.frame_index(), 1);
}

#[test]
fn animator_tick_flips_back_to_zero_after_second_850ms() {
    let clock = MockClock::new();
    let mut animator = MascotAnimator::new(&clock);
    clock.advance(850);
    animator.tick(&clock);
    assert_eq!(animator.frame_index(), 1);
    clock.advance(850);
    animator.tick(&clock);
    assert_eq!(animator.frame_index(), 0);
}

#[test]
fn animator_set_state_changes_state() {
    let clock = MockClock::new();
    let mut animator = MascotAnimator::new(&clock);
    animator.set_state(MascotState::Conducting, &clock);
    assert_eq!(animator.state(), MascotState::Conducting);
}

#[test]
fn animator_set_state_happy_schedules_revert_to_idle() {
    let clock = MockClock::new();
    let mut animator = MascotAnimator::new(&clock);
    animator.set_state(MascotState::Happy, &clock);
    assert_eq!(animator.state(), MascotState::Happy);
    clock.advance(2000);
    animator.tick(&clock);
    assert_eq!(animator.state(), MascotState::Idle);
}

#[test]
fn animator_set_state_happy_does_not_revert_before_2000ms() {
    let clock = MockClock::new();
    let mut animator = MascotAnimator::new(&clock);
    animator.set_state(MascotState::Happy, &clock);
    clock.advance(1999);
    animator.tick(&clock);
    assert_eq!(animator.state(), MascotState::Happy);
}

#[test]
fn animator_non_happy_states_do_not_auto_revert() {
    for state in [
        MascotState::Conducting,
        MascotState::Thinking,
        MascotState::Sleeping,
        MascotState::Error,
    ] {
        let clock = MockClock::new();
        let mut animator = MascotAnimator::new(&clock);
        animator.set_state(state, &clock);
        clock.advance(10_000);
        animator.tick(&clock);
        assert_eq!(
            animator.state(),
            state,
            "State {:?} unexpectedly reverted",
            state
        );
    }
}

#[test]
fn animator_set_state_resets_frame_index_to_zero() {
    let clock = MockClock::new();
    let mut animator = MascotAnimator::new(&clock);
    clock.advance(850);
    animator.tick(&clock);
    assert_eq!(animator.frame_index(), 1);
    animator.set_state(MascotState::Conducting, &clock);
    assert_eq!(animator.frame_index(), 0);
}

// ---------------------------------------------------------------------------
// Suite 4 — MascotWidget
// ---------------------------------------------------------------------------

#[test]
fn widget_renders_six_rows() {
    let buf = render_to_buffer(MascotWidget::new(MascotState::Idle, 0, CLAWD_ORANGE), 11, 6);
    for row in 0..6u16 {
        let line = buffer_row_string(&buf, row);
        assert!(!line.trim().is_empty(), "Row {} should not be blank", row);
    }
}

#[test]
fn widget_content_matches_frames_data() {
    for state in all_states() {
        for frame_index in [0usize, 1] {
            let buf = render_to_buffer(MascotWidget::new(state, frame_index, CLAWD_ORANGE), 11, 6);
            for row in 0..6u16 {
                let rendered = buffer_row_string(&buf, row);
                let expected = AsciiMascotFrames::frames(state, row as usize)[frame_index];
                assert_eq!(
                    rendered, expected,
                    "state={:?} frame={} row={}",
                    state, frame_index, row
                );
            }
        }
    }
}

#[test]
fn widget_applies_clawd_orange_to_non_space_cells() {
    let buf = render_to_buffer(
        MascotWidget::new(MascotState::Conducting, 0, CLAWD_ORANGE),
        11,
        6,
    );
    let expected_fg = Color::Rgb(215, 119, 87);
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            if let Some(cell) = buf.cell((x, y)) {
                if cell.symbol() != " " {
                    assert_eq!(
                        cell.fg,
                        expected_fg,
                        "Wrong fg at ({}, {}): symbol={:?}",
                        x,
                        y,
                        cell.symbol()
                    );
                }
            }
        }
    }
}

#[test]
fn widget_renders_safely_into_smaller_area() {
    let _buf = render_to_buffer(MascotWidget::new(MascotState::Idle, 0, CLAWD_ORANGE), 5, 3);
}

#[test]
fn widget_renders_safely_into_zero_height_area() {
    let area = Rect::new(0, 0, 11, 0);
    let mut buf = Buffer::empty(area);
    MascotWidget::new(MascotState::Idle, 0, CLAWD_ORANGE).render(area, &mut buf);
}

#[test]
fn widget_different_states_produce_different_output() {
    let outputs: Vec<String> = all_states()
        .iter()
        .map(|&state| {
            let buf = render_to_buffer(MascotWidget::new(state, 0, CLAWD_ORANGE), 11, 6);
            (0..6u16)
                .map(|y| buffer_row_string(&buf, y))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect();

    for i in 0..outputs.len() {
        for j in (i + 1)..outputs.len() {
            assert_ne!(
                outputs[i],
                outputs[j],
                "States {:?} and {:?} render identically",
                all_states()[i],
                all_states()[j]
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Suite 5 — derive_dashboard_mascot_state
// ---------------------------------------------------------------------------

#[test]
fn derive_state_empty_returns_idle() {
    assert_eq!(derive_dashboard_mascot_state([].iter()), MascotState::Idle);
}

#[test]
fn derive_state_single_running_returns_conducting() {
    assert_eq!(
        derive_dashboard_mascot_state([SessionStatus::Running].iter()),
        MascotState::Conducting
    );
}

#[test]
fn derive_state_single_spawning_returns_conducting() {
    assert_eq!(
        derive_dashboard_mascot_state([SessionStatus::Spawning].iter()),
        MascotState::Conducting
    );
}

#[test]
fn derive_state_running_and_completed_returns_conducting() {
    assert_eq!(
        derive_dashboard_mascot_state([SessionStatus::Completed, SessionStatus::Running].iter()),
        MascotState::Conducting
    );
}

#[test]
fn derive_state_all_completed_returns_happy() {
    assert_eq!(
        derive_dashboard_mascot_state([SessionStatus::Completed, SessionStatus::Completed].iter()),
        MascotState::Happy
    );
}

#[test]
fn derive_state_single_errored_returns_error() {
    assert_eq!(
        derive_dashboard_mascot_state([SessionStatus::Errored].iter()),
        MascotState::Error
    );
}

#[test]
fn derive_state_errored_beats_completed() {
    assert_eq!(
        derive_dashboard_mascot_state([SessionStatus::Completed, SessionStatus::Errored].iter()),
        MascotState::Error
    );
}

#[test]
fn derive_state_errored_beats_running() {
    assert_eq!(
        derive_dashboard_mascot_state([SessionStatus::Running, SessionStatus::Errored].iter()),
        MascotState::Error
    );
}

#[test]
fn derive_state_queued_only_returns_idle() {
    assert_eq!(
        derive_dashboard_mascot_state([SessionStatus::Queued].iter()),
        MascotState::Idle
    );
}
