use super::*;

// --- Group 11: Clipboard paste (Ctrl+V) ---

#[test]
fn prompt_input_ctrl_v_with_image_adds_path_to_image_list() {
    let mut screen = PromptInputScreen::with_clipboard(MockClipboard::with_image(
        "/tmp/maestro-clips/clip-abc.png",
    ));
    let action = screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
    assert_eq!(
        screen.image_paths,
        vec!["/tmp/maestro-clips/clip-abc.png".to_string()]
    );
    assert!(screen.status_message.unwrap().contains("Pasted image"));
}

#[test]
fn prompt_input_ctrl_v_text_in_editor_inserts_into_prompt() {
    let mut screen = PromptInputScreen::with_clipboard(MockClipboard::with_text("hello world"));
    assert!(
        screen
            .focus_ring
            .is_focused(PromptInputScreen::PROMPT_EDITOR_PANE)
    );
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "hello world");
    assert!(screen.image_paths.is_empty());
    assert!(screen.status_message.unwrap().contains("Pasted text"));
}

#[test]
fn prompt_input_ctrl_v_text_in_editor_appends_to_existing_prompt() {
    let mut screen = PromptInputScreen::with_clipboard(MockClipboard::with_text(" world"));
    screen.set_editor_text("hello");
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "hello world");
}

#[test]
fn prompt_input_ctrl_v_text_in_image_list_adds_as_path() {
    let mut screen =
        PromptInputScreen::with_clipboard(MockClipboard::with_text("/home/user/screenshot.png"));
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    assert!(
        screen
            .focus_ring
            .is_focused(PromptInputScreen::IMAGE_LIST_PANE)
    );
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(
        screen.image_paths,
        vec!["/home/user/screenshot.png".to_string()]
    );
    assert!(
        screen
            .status_message
            .as_deref()
            .unwrap()
            .contains("Pasted path")
    );
    assert_eq!(screen.prompt_text(), "");
}

#[test]
fn prompt_input_ctrl_v_with_empty_clipboard_shows_message() {
    let mut screen = PromptInputScreen::with_clipboard(MockClipboard::empty());
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert!(screen.image_paths.is_empty());
    assert_eq!(screen.status_message.unwrap(), "Clipboard is empty");
}

#[test]
fn prompt_input_ctrl_v_image_from_editor_focus_adds_to_attachments() {
    let mut screen = PromptInputScreen::with_clipboard(MockClipboard::with_image("/tmp/shot.png"));
    assert!(
        screen
            .focus_ring
            .is_focused(PromptInputScreen::PROMPT_EDITOR_PANE)
    );
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(screen.image_paths, vec!["/tmp/shot.png".to_string()]);
    assert_eq!(screen.prompt_text(), "");
}

#[test]
fn prompt_input_ctrl_v_text_in_image_list_appends_to_existing() {
    let mut screen = PromptInputScreen::with_clipboard(MockClipboard::with_text("/tmp/new.png"));
    screen.image_paths = vec!["/tmp/existing.png".to_string()];
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(
        screen.image_paths,
        vec!["/tmp/existing.png".to_string(), "/tmp/new.png".to_string()]
    );
}

#[test]
fn prompt_input_ctrl_v_text_in_editor_resets_history_cursor() {
    let mut screen = PromptInputScreen::with_clipboard(MockClipboard::with_text("pasted"));
    screen.history_cursor = Some(2);
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert!(screen.history_cursor.is_none());
}

// --- Group 12: Clipboard unavailability (WSL / headless — issue #235) ---

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn unavailable_screen() -> PromptInputScreen {
    PromptInputScreen::with_clipboard(MockClipboard::unavailable())
}

struct CountingUnavailableClipboard {
    read_count: Arc<AtomicUsize>,
}

impl ClipboardProvider for CountingUnavailableClipboard {
    fn read(&self) -> ClipboardContent {
        self.read_count.fetch_add(1, Ordering::SeqCst);
        ClipboardContent::Unavailable
    }
}

#[test]
fn clipboard_unavailable_returns_no_image_paths() {
    let mut screen = unavailable_screen();
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert!(screen.image_paths.is_empty());
}

#[test]
fn clipboard_unavailable_sets_status_message() {
    let mut screen = unavailable_screen();
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert!(
        screen
            .status_message
            .as_deref()
            .unwrap()
            .contains("not available")
    );
}

#[test]
fn clipboard_unavailable_action_is_none() {
    let mut screen = unavailable_screen();
    let action = screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(action, ScreenAction::None);
}

#[test]
fn clipboard_unavailable_multiple_ctrl_v_no_crash() {
    let mut screen = unavailable_screen();
    for _ in 0..5 {
        screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    }
    assert!(screen.image_paths.is_empty());
    assert!(screen.status_message.is_some());
}

#[test]
fn clipboard_unavailable_status_message_is_stable_across_presses() {
    let mut screen = unavailable_screen();
    for _ in 0..3 {
        screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
        assert!(
            screen
                .status_message
                .as_deref()
                .unwrap()
                .contains("not available")
        );
    }
}

#[test]
fn clipboard_unavailable_read_count_mock_documents_call_pattern() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut screen = PromptInputScreen::with_clipboard(Box::new(CountingUnavailableClipboard {
        read_count: Arc::clone(&counter),
    }));
    for _ in 0..3 {
        screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    }
    assert_eq!(counter.load(Ordering::SeqCst), 3);
    assert!(screen.image_paths.is_empty());
}

#[test]
fn clipboard_unavailable_does_not_affect_prompt_text() {
    let mut screen = unavailable_screen();
    screen.set_editor_text("my prompt");
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "my prompt");
}

#[test]
fn clipboard_normal_text_in_editor_still_works_after_unavailable_variant_added() {
    let mut screen = PromptInputScreen::with_clipboard(MockClipboard::with_text("pasted text"));
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(screen.prompt_text(), "pasted text");
    assert!(screen.image_paths.is_empty());
    assert!(
        screen
            .status_message
            .as_deref()
            .unwrap()
            .contains("Pasted text")
    );
}

#[test]
fn clipboard_normal_text_in_image_list_still_works_after_unavailable_variant_added() {
    let mut screen = PromptInputScreen::with_clipboard(MockClipboard::with_text("/tmp/file.png"));
    screen.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(screen.image_paths, vec!["/tmp/file.png".to_string()]);
    assert!(
        screen
            .status_message
            .as_deref()
            .unwrap()
            .contains("Pasted path")
    );
}

#[test]
fn clipboard_normal_image_still_works_after_unavailable_variant_added() {
    let mut screen = PromptInputScreen::with_clipboard(MockClipboard::with_image(
        "/tmp/maestro-clips/clip-xyz.png",
    ));
    screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
    assert_eq!(
        screen.image_paths,
        vec!["/tmp/maestro-clips/clip-xyz.png".to_string()]
    );
    assert!(
        screen
            .status_message
            .as_deref()
            .unwrap()
            .contains("Pasted image")
    );
}
