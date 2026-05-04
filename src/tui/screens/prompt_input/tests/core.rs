use super::*;

// --- Group 1: Initial state ---

#[test]
fn prompt_input_initial_state_prompt_is_empty() {
    let screen = mock_screen();
    assert_eq!(screen.prompt_text(), "");
}

#[test]
fn prompt_input_initial_focus_is_prompt_editor() {
    let screen = mock_screen();
    assert!(
        screen
            .focus_ring
            .is_focused(PromptInputScreen::PROMPT_EDITOR_PANE)
    );
}

#[test]
fn prompt_input_initial_image_list_is_empty() {
    let screen = mock_screen();
    assert!(screen.image_paths.is_empty());
}

// --- Group 2: Text input in PromptEditor ---

#[test]
fn prompt_input_typing_appends_character() {
    let mut screen = mock_screen();
    screen.handle_input(&key_event(KeyCode::Char('h')), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Char('i')), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Char('!')), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "hi!");
}

#[test]
fn prompt_input_ctrl_j_inserts_newline() {
    let mut screen = screen_with_prompt("hello");
    let action = screen.handle_input(&ctrl_key(KeyCode::Char('j')), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "hello\n");
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn prompt_input_ctrl_j_increases_line_count() {
    let mut screen = screen_with_prompt("hello");
    let before = screen.editor.lines().len();
    screen.handle_input(&ctrl_key(KeyCode::Char('j')), InputMode::Normal);
    let after = screen.editor.lines().len();
    assert!(
        after > before,
        "Ctrl+J must increase editor line count: before={}, after={}",
        before,
        after
    );
}

#[test]
fn prompt_input_shift_enter_inserts_newline() {
    let mut screen = screen_with_prompt("hello");
    let action = screen.handle_input(&shift_key(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert!(
        screen.editor.lines().len() >= 2,
        "Shift+Enter must insert newline, got {} lines",
        screen.editor.lines().len()
    );
}

#[test]
fn prompt_input_backspace_removes_last_character() {
    let mut screen = screen_with_prompt("abc");
    screen.handle_input(&key_event(KeyCode::Backspace), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "ab");
}

#[test]
fn prompt_input_backspace_on_empty_prompt_is_noop() {
    let mut screen = mock_screen();
    let action = screen.handle_input(&key_event(KeyCode::Backspace), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "");
    assert_eq!(action, ScreenAction::None);
}

// --- Group 3: Submit (Enter) ---

#[test]
fn prompt_input_enter_with_prompt_returns_launch_prompt_session() {
    let mut screen = screen_with_prompt("fix the bug");
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(
        action,
        ScreenAction::LaunchPromptSession(PromptSessionConfig {
            prompt: "fix the bug".to_string(),
            image_paths: vec![],
        })
    );
}

#[test]
fn prompt_input_enter_with_prompt_and_images_includes_image_paths() {
    let mut screen = screen_with_prompt("describe this");
    screen.image_paths = vec!["/tmp/a.png".to_string(), "/tmp/b.png".to_string()];
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(
        action,
        ScreenAction::LaunchPromptSession(PromptSessionConfig {
            prompt: "describe this".to_string(),
            image_paths: vec!["/tmp/a.png".to_string(), "/tmp/b.png".to_string()],
        })
    );
}

#[test]
fn prompt_input_enter_with_empty_prompt_is_rejected() {
    let mut screen = mock_screen();
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn prompt_input_enter_with_whitespace_only_prompt_is_rejected() {
    let mut screen = screen_with_prompt("   \n  ");
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn prompt_input_enter_in_image_path_editing_does_not_submit_session() {
    let mut screen = screen_in_image_list_focus();
    screen.editing_image_path = true;
    screen.image_path_input = "/tmp/shot.png".to_string();
    let action = screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(screen.image_paths, vec!["/tmp/shot.png".to_string()]);
}

// --- Group 4: Esc ---

#[test]
fn prompt_input_esc_returns_pop() {
    let mut screen = mock_screen();
    let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::Pop);
}

#[test]
fn prompt_input_esc_in_image_list_focus_returns_pop() {
    let mut screen = screen_in_image_list_focus();
    let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert_eq!(action, ScreenAction::Pop);
}

// --- Group 5: Tab (focus toggle) ---

#[test]
fn prompt_input_tab_switches_focus_to_image_list() {
    let mut screen = mock_screen();
    let action = screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    assert!(
        screen
            .focus_ring
            .is_focused(PromptInputScreen::IMAGE_LIST_PANE)
    );
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn prompt_input_tab_toggles_back_to_prompt_editor() {
    let mut screen = mock_screen();
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    assert!(
        screen
            .focus_ring
            .is_focused(PromptInputScreen::PROMPT_EDITOR_PANE)
    );
}

// --- Group 6: ImageList add image path ---

#[test]
fn prompt_input_key_a_in_image_list_enters_editing_mode() {
    let mut screen = screen_in_image_list_focus();
    screen.handle_input(&key_event(KeyCode::Char('a')), InputMode::Normal);
    assert!(screen.editing_image_path);
    assert_eq!(screen.image_path_input, "");
}

#[test]
fn prompt_input_typing_in_image_path_input_accumulates_text() {
    let mut screen = screen_in_image_list_focus();
    screen.handle_input(&key_event(KeyCode::Char('a')), InputMode::Normal); // enter editing mode
    let original_prompt = screen.prompt_text();
    for ch in ['/', 't', 'm', 'p'] {
        screen.handle_input(&key_event(KeyCode::Char(ch)), InputMode::Normal);
    }
    assert_eq!(screen.image_path_input, "/tmp");
    assert_eq!(screen.prompt_text(), original_prompt);
}

#[test]
fn prompt_input_enter_confirms_image_path_and_appends_to_list() {
    let mut screen = screen_in_image_list_focus();
    screen.editing_image_path = true;
    screen.image_path_input = "/tmp/shot.png".to_string();
    screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert_eq!(screen.image_paths, vec!["/tmp/shot.png".to_string()]);
    assert!(!screen.editing_image_path);
    assert_eq!(screen.image_path_input, "");
}

#[test]
fn prompt_input_enter_with_empty_image_path_is_noop() {
    let mut screen = screen_in_image_list_focus();
    screen.editing_image_path = true;
    screen.image_path_input = "".to_string();
    screen.handle_input(&key_event(KeyCode::Enter), InputMode::Normal);
    assert!(screen.image_paths.is_empty());
    assert!(!screen.editing_image_path);
}

#[test]
fn prompt_input_esc_cancels_image_path_editing() {
    let mut screen = screen_in_image_list_focus();
    screen.editing_image_path = true;
    screen.image_path_input = "/tmp/partial".to_string();
    let action = screen.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert!(screen.image_paths.is_empty());
    assert!(!screen.editing_image_path);
    assert_eq!(screen.image_path_input, "");
    assert_eq!(action, ScreenAction::None);
}

// --- Group 7: ImageList delete ---

#[test]
fn prompt_input_key_d_removes_selected_image() {
    let mut screen = screen_with_images(&["/a.png", "/b.png"]);
    screen.selected_image = 0;
    screen.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
    assert_eq!(screen.image_paths, vec!["/b.png".to_string()]);
}

#[test]
fn prompt_input_key_d_on_empty_image_list_is_noop() {
    let mut screen = screen_in_image_list_focus();
    let action = screen.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
    assert!(screen.image_paths.is_empty());
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn prompt_input_selected_image_clamps_after_deletion() {
    let mut screen = screen_with_images(&["/only.png"]);
    screen.selected_image = 0;
    screen.handle_input(&key_event(KeyCode::Char('d')), InputMode::Normal);
    assert!(screen.image_paths.is_empty());
    assert_eq!(screen.selected_image, 0);
}

// --- Group 8: ImageList navigation ---

#[test]
fn prompt_input_key_j_in_image_list_advances_selected_image() {
    let mut screen = screen_with_images(&["/a.png", "/b.png"]);
    screen.selected_image = 0;
    screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
    assert_eq!(screen.selected_image, 1);
}

#[test]
fn prompt_input_key_j_in_image_list_does_not_overflow() {
    let mut screen = screen_with_images(&["/a.png"]);
    screen.selected_image = 0;
    for _ in 0..3 {
        screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
    }
    assert_eq!(screen.selected_image, 0);
}

#[test]
fn prompt_input_key_k_in_image_list_moves_selection_up() {
    let mut screen = screen_with_images(&["/a.png", "/b.png"]);
    screen.selected_image = 1;
    screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
    assert_eq!(screen.selected_image, 0);
}

#[test]
fn prompt_input_key_k_in_image_list_does_not_underflow() {
    let mut screen = screen_with_images(&["/a.png", "/b.png"]);
    screen.selected_image = 0;
    screen.handle_input(&key_event(KeyCode::Char('k')), InputMode::Normal);
    assert_eq!(screen.selected_image, 0);
}

// --- Group 9: Input routing ---

#[test]
fn prompt_input_image_list_keys_do_not_mutate_prompt_text() {
    let mut screen = screen_in_image_list_focus();
    screen.set_editor_text("existing");
    screen.image_paths = vec!["/x.png".to_string()];
    for code in [KeyCode::Char('j'), KeyCode::Char('k'), KeyCode::Char('d')] {
        screen.handle_input(&key_event(code), InputMode::Normal);
    }
    assert_eq!(screen.prompt_text(), "existing");
}

// --- Group 10: PromptSessionConfig ---

#[test]
fn prompt_session_config_stores_prompt_and_images() {
    let cfg = PromptSessionConfig {
        prompt: "hello".to_string(),
        image_paths: vec!["/img.png".to_string()],
    };
    assert_eq!(cfg.prompt, "hello");
    assert_eq!(cfg.image_paths, vec!["/img.png".to_string()]);
}

#[test]
fn prompt_session_config_clone_is_independent() {
    let mut original = PromptSessionConfig {
        prompt: "hello".to_string(),
        image_paths: vec![],
    };
    let cloned = original.clone();
    original.prompt.push_str(" extra");
    assert_eq!(cloned.prompt, "hello");
}
