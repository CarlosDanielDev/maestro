use super::*;

// --- Group 13: Prompt history navigation ---

fn screen_with_history(prompts: &[&str]) -> PromptInputScreen {
    let mut s = mock_screen();
    s.set_history(prompts.iter().map(|p| p.to_string()).collect());
    s
}

#[test]
fn plain_up_with_empty_history_is_noop() {
    let mut screen = mock_screen();
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "");
    assert!(screen.history_cursor.is_none());
}

#[test]
fn plain_up_enters_history_and_shows_last_entry() {
    let mut screen = screen_with_history(&["first", "second"]);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "second");
    assert_eq!(screen.history_cursor, Some(1));
}

#[test]
fn plain_up_twice_shows_previous_entry() {
    let mut screen = screen_with_history(&["first", "second"]);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "first");
    assert_eq!(screen.history_cursor, Some(0));
}

#[test]
fn plain_up_at_oldest_entry_stays() {
    let mut screen = screen_with_history(&["only"]);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "only");
    assert_eq!(screen.history_cursor, Some(0));
}

#[test]
fn plain_down_past_history_restores_draft() {
    let mut screen = screen_with_history(&["old"]);
    screen.set_editor_text("my draft");
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "old");
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "my draft");
    assert!(screen.history_cursor.is_none());
}

#[test]
fn plain_down_without_history_cursor_is_noop() {
    let mut screen = screen_with_history(&["old"]);
    screen.set_editor_text("current");
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "current");
    assert!(screen.history_cursor.is_none());
}

#[test]
fn typing_resets_history_cursor() {
    let mut screen = screen_with_history(&["old"]);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.history_cursor, Some(0));
    screen.handle_input(&key_event(KeyCode::Char('x')), InputMode::Normal);
    assert!(screen.history_cursor.is_none());
}

#[test]
fn backspace_resets_history_cursor() {
    let mut screen = screen_with_history(&["old"]);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Backspace), InputMode::Normal);
    assert!(screen.history_cursor.is_none());
}

// --- Group 13b: Edge-case tests for draft preservation ---

#[test]
fn up_on_multiline_prompt_moves_cursor_not_history() {
    let mut screen = screen_with_history(&["old"]);
    screen.set_editor_text("line one\nline two");
    assert_eq!(screen.editor.cursor().0, 1, "precondition: cursor on row 1");
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert!(screen.history_cursor.is_none(), "must not enter history");
    assert_eq!(screen.editor.cursor().0, 0, "cursor moved up within editor");
}

#[test]
fn down_on_multiline_prompt_moves_cursor_not_history() {
    let mut screen = screen_with_history(&["old"]);
    screen.set_editor_text("line one\nline two");
    screen
        .editor
        .move_cursor(tui_textarea::CursorMove::Jump(0, 0));
    assert_eq!(screen.editor.cursor().0, 0, "precondition: cursor on row 0");
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert!(screen.history_cursor.is_none(), "no history navigation");
    assert_eq!(
        screen.editor.cursor().0,
        1,
        "cursor moved down within editor"
    );
}

#[test]
fn draft_is_empty_string_when_no_prior_input() {
    let mut screen = screen_with_history(&["a", "b"]);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "");
    assert_eq!(screen.draft_prompt, "");
}

#[test]
fn draft_preserved_across_multiple_history_jumps() {
    let mut screen = screen_with_history(&["a", "b", "c"]);
    screen.set_editor_text("my draft");
    // Navigate to oldest entry
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "a");
    // Navigate all the way back
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Down), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "my draft");
    assert!(screen.history_cursor.is_none());
}

#[test]
fn navigating_history_then_set_history_clears_draft_and_cursor() {
    let mut screen = screen_with_history(&["old prompt"]);
    screen.set_editor_text("work in progress");
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    assert_eq!(screen.history_cursor, Some(0));
    screen.set_history(vec![]);
    assert!(screen.history_cursor.is_none());
    assert_eq!(screen.draft_prompt, "");
}

// --- Group 13c: History indicator ---

#[test]
fn history_indicator_is_none_when_not_in_history_mode() {
    let screen = screen_with_history(&["a", "b"]);
    assert!(screen.history_indicator().is_none());
}

#[test]
fn history_indicator_shows_one_based_position_and_total() {
    let mut screen = screen_with_history(&["a", "b", "c"]);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    let indicator = screen
        .history_indicator()
        .expect("indicator must be Some in history mode");
    assert!(
        indicator.contains("3/3"),
        "expected '3/3' in indicator, got: {indicator}"
    );
}

#[test]
fn history_indicator_updates_on_further_navigation() {
    let mut screen = screen_with_history(&["a", "b", "c"]);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Up), InputMode::Normal);
    let indicator = screen.history_indicator().unwrap();
    assert!(
        indicator.contains("2/3"),
        "expected '2/3' in indicator, got: {indicator}"
    );
}

#[test]
fn prompt_input_ctrl_v_text_in_editor_appends_to_prompt() {
    let mut screen = PromptInputScreen::with_clipboard(MockClipboard::with_text("/tmp/img.png"));
    screen.set_editor_text("my prompt ");
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "my prompt /tmp/img.png");
}

// --- Issue #303: Unified PR toggle in prompt composition ---

#[test]
fn detected_refs_update_on_typing() {
    let mut screen = mock_screen();
    // Type "fix #10 and #20"
    for c in "fix #10 and #20".chars() {
        screen.handle_input(&key_event(KeyCode::Char(c)), InputMode::Normal);
    }
    assert_eq!(screen.detected_issue_numbers, vec![10, 20]);
}

#[test]
fn detected_refs_single_ref_no_toggle() {
    let mut screen = mock_screen();
    for c in "fix #10 only".chars() {
        screen.handle_input(&key_event(KeyCode::Char(c)), InputMode::Normal);
    }
    assert_eq!(screen.detected_issue_numbers.len(), 1);
    assert!(!screen.unified_pr);
}

#[test]
fn ctrl_u_toggles_unified_pr_with_two_refs() {
    let mut screen = mock_screen();
    screen.set_editor_text("fix #10 and #20");
    screen.refresh_detected_refs();
    assert_eq!(screen.detected_issue_numbers.len(), 2);
    // Toggle on
    screen.handle_input(&ctrl_key(KeyCode::Char('u')), InputMode::Normal);
    assert!(screen.unified_pr);
    // Toggle off
    screen.handle_input(&ctrl_key(KeyCode::Char('u')), InputMode::Normal);
    assert!(!screen.unified_pr);
}

#[test]
fn ctrl_u_ignored_with_fewer_than_two_refs() {
    let mut screen = mock_screen();
    screen.set_editor_text("fix #10 only");
    screen.refresh_detected_refs();
    screen.handle_input(&ctrl_key(KeyCode::Char('u')), InputMode::Normal);
    assert!(!screen.unified_pr);
}

#[test]
fn auto_clear_unified_when_refs_drop_below_two() {
    let mut screen = mock_screen();
    screen.set_editor_text("fix #10 and #20");
    screen.refresh_detected_refs();
    screen.handle_input(&ctrl_key(KeyCode::Char('u')), InputMode::Normal);
    assert!(screen.unified_pr);
    // Delete text to remove one ref
    screen.set_editor_text("fix #10 only");
    screen.refresh_detected_refs();
    assert!(!screen.unified_pr);
}

#[test]
fn submit_unified_returns_launch_unified_session() {
    let mut screen = mock_screen();
    screen.set_editor_text("fix #10 and #20");
    screen.refresh_detected_refs();
    screen.handle_input(&ctrl_key(KeyCode::Char('u')), InputMode::Normal);
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    match action {
        ScreenAction::LaunchUnifiedSession(config) => {
            assert_eq!(config.issues.len(), 2);
            assert!(config.custom_prompt.unwrap().contains("#10"));
        }
        other => panic!("Expected LaunchUnifiedSession, got {:?}", other),
    }
}

#[test]
fn submit_normal_when_not_unified() {
    let mut screen = mock_screen();
    screen.set_editor_text("fix #10 and #20");
    screen.refresh_detected_refs();
    // Don't toggle unified — submit normally
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    match action {
        ScreenAction::LaunchPromptSession(config) => {
            assert!(config.prompt.contains("#10"));
        }
        other => panic!("Expected LaunchPromptSession, got {:?}", other),
    }
}
