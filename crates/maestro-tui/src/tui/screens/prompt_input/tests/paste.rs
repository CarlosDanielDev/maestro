use super::*;

// --- Group 14: Bracketed paste (Event::Paste) ---

#[test]
fn paste_text_inserts_verbatim_into_editor_when_prompt_focused() {
    let mut screen = mock_screen();
    assert!(
        screen
            .focus_ring
            .is_focused(PromptInputScreen::PROMPT_EDITOR_PANE)
    );

    screen.paste_text("hello world");

    assert_eq!(screen.prompt_text(), "hello world");
}

#[test]
fn paste_text_preserves_embedded_newlines_as_newline_chars() {
    let mut screen = mock_screen();

    screen.paste_text("line1\nline2\nline3");

    assert_eq!(screen.prompt_text(), "line1\nline2\nline3");
    assert_eq!(screen.editor.lines().len(), 3);
}

#[test]
fn paste_text_never_submits_even_with_trailing_newline() {
    let mut screen = mock_screen();
    let event = crossterm::event::Event::Paste("line1\nline2\n".to_string());

    let action = screen.handle_input(&event, InputMode::Normal);

    assert_eq!(action, ScreenAction::None);
    assert!(!screen.prompt_text().is_empty());
}

#[test]
fn paste_text_multiline_shell_payload_returns_screen_action_none() {
    let mut screen = screen_with_prompt("existing text");
    let payload = "gh issue create --title \"test\"\necho hello\nrm -rf /tmp/test\n";
    let event = crossterm::event::Event::Paste(payload.to_string());

    let action = screen.handle_input(&event, InputMode::Normal);

    assert_eq!(
        action,
        ScreenAction::None,
        "Event::Paste must never return LaunchPromptSession regardless of payload newlines"
    );
}

#[test]
fn paste_text_resets_history_cursor() {
    let mut screen = mock_screen();
    screen.history_cursor = Some(2);

    screen.paste_text("some text");

    assert!(screen.history_cursor.is_none());
}

#[test]
fn paste_text_sets_status_message() {
    let mut screen = mock_screen();

    screen.paste_text("any content");

    assert!(
        screen.status_message.is_some(),
        "paste_text must set a status_message"
    );
}

#[test]
fn paste_text_empty_string_is_noop_for_content() {
    let mut screen = mock_screen();

    screen.paste_text("");

    assert_eq!(screen.prompt_text(), "");
}

#[test]
fn handle_input_event_paste_inserts_content_into_editor() {
    let mut screen = mock_screen();
    let event = crossterm::event::Event::Paste("injected via bracketed paste".to_string());

    screen.handle_input(&event, InputMode::Normal);

    assert_eq!(screen.prompt_text(), "injected via bracketed paste");
}

#[test]
fn handle_input_event_paste_returns_screen_action_none() {
    let mut screen = mock_screen();
    let event = crossterm::event::Event::Paste("line1\nline2\nline3\n".to_string());

    let action = screen.handle_input(&event, InputMode::Normal);

    assert_eq!(action, ScreenAction::None);
}

#[test]
fn paste_text_in_image_list_focus_pushes_to_image_paths() {
    let mut screen = screen_in_image_list_focus();
    assert!(
        screen
            .focus_ring
            .is_focused(PromptInputScreen::IMAGE_LIST_PANE)
    );

    screen.paste_text("/tmp/screenshot.png");

    assert_eq!(screen.image_paths, vec!["/tmp/screenshot.png".to_string()]);
    assert_eq!(screen.prompt_text(), "");
}

#[test]
fn paste_text_strips_ansi_escape_codes() {
    let mut screen = mock_screen();
    let payload = "\x1b[32mgreen\x1b[0m and \x1b[1;31mred\x1b[0m text";

    screen.paste_text(payload);

    assert_eq!(screen.prompt_text(), "[32mgreen[0m and [1;31mred[0m text");
}

#[test]
fn paste_text_strips_c0_control_bytes_except_newline_and_tab() {
    let mut screen = mock_screen();
    let payload = "keep\nnewline\tand\ttab but\x00drop\x07bell\x1bescape\x7fdelete";

    screen.paste_text(payload);

    assert_eq!(
        screen.prompt_text(),
        "keep\nnewline\tand\ttab butdropbellescapedelete"
    );
}

#[test]
fn paste_text_preserves_printable_ascii_and_unicode() {
    let mut screen = mock_screen();
    let payload = "ASCII + emoji 🦀 + accents café";

    screen.paste_text(payload);

    assert_eq!(screen.prompt_text(), "ASCII + emoji 🦀 + accents café");
}

#[test]
fn paste_text_image_list_sanitizes_path() {
    let mut screen = screen_in_image_list_focus();
    let payload = "/tmp/\x1bevil\x00path.png";

    screen.paste_text(payload);

    assert_eq!(screen.image_paths, vec!["/tmp/evilpath.png".to_string()]);
}
