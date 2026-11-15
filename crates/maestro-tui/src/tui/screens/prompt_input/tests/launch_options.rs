//! Launch-option checkbox tests for the free-form prompt screen (#919) —
//! split from `core.rs` to keep it under the 400-line guardrail.

use super::*;
use crate::tui::screens::test_helpers::key_event;
use crossterm::event::KeyCode;
#[test]
fn prompt_input_space_toggles_focused_checkbox() {
    let mut screen = mock_screen();
    // editor → images → produce_pr
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    assert!(screen.is_produce_pr_focused());
    assert!(screen.produce_pr, "default on");
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    assert!(
        !screen.produce_pr && !screen.interaction,
        "Space toggles only Produce PR"
    );

    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    assert!(screen.is_interaction_focused());
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal);
    assert!(screen.interaction, "Space toggles Interaction");
}

#[test]
fn prompt_input_submit_carries_launch_options() {
    let mut screen = mock_screen();
    screen.set_editor_text("do the thing");
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal); // produce_pr off
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Char(' ')), InputMode::Normal); // interaction on

    // Enter from a checkbox stop launches (parity with the issue dialog).
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    match action {
        ScreenAction::LaunchPromptSession(cfg) => {
            assert_eq!(cfg.prompt, "do the thing");
            assert!(!cfg.produce_pr);
            assert!(cfg.interaction);
        }
        other => panic!("expected LaunchPromptSession, got {other:?}"),
    }
}

#[test]
fn prompt_input_launch_defaults_seed_checkboxes() {
    let screen = PromptInputScreen::new().with_launch_defaults((false, true));
    assert_eq!((screen.produce_pr, screen.interaction), (false, true));
}
