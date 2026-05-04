use super::*;
use crate::tui::screens::test_helpers::{key_event, key_event_with_modifiers};
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::PathBuf;

/// Mock clipboard that returns a preconfigured response.
struct MockClipboard {
    content: ClipboardContent,
}

impl MockClipboard {
    fn empty() -> Box<Self> {
        Box::new(Self {
            content: ClipboardContent::Empty,
        })
    }

    fn with_text(text: &str) -> Box<Self> {
        Box::new(Self {
            content: ClipboardContent::Text(text.to_string()),
        })
    }

    fn with_image(path: &str) -> Box<Self> {
        Box::new(Self {
            content: ClipboardContent::Image(PathBuf::from(path)),
        })
    }

    fn unavailable() -> Box<Self> {
        Box::new(Self {
            content: ClipboardContent::Unavailable,
        })
    }
}

impl ClipboardProvider for MockClipboard {
    fn read(&self) -> ClipboardContent {
        match &self.content {
            ClipboardContent::Image(p) => ClipboardContent::Image(p.clone()),
            ClipboardContent::Text(t) => ClipboardContent::Text(t.clone()),
            ClipboardContent::Empty => ClipboardContent::Empty,
            ClipboardContent::Unavailable => ClipboardContent::Unavailable,
        }
    }
}

fn ctrl_key(code: KeyCode) -> crossterm::event::Event {
    key_event_with_modifiers(code, KeyModifiers::CONTROL)
}

fn shift_key(code: KeyCode) -> crossterm::event::Event {
    key_event_with_modifiers(code, KeyModifiers::SHIFT)
}

fn mock_screen() -> PromptInputScreen {
    PromptInputScreen::with_clipboard(MockClipboard::empty())
}

fn screen_with_prompt(text: &str) -> PromptInputScreen {
    let mut s = mock_screen();
    s.set_editor_text(text);
    s
}

fn screen_in_image_list_focus() -> PromptInputScreen {
    let mut s = mock_screen();
    s.handle_input(&key_event(KeyCode::Tab), InputMode::Normal);
    s
}

fn screen_with_images(paths: &[&str]) -> PromptInputScreen {
    let mut s = screen_in_image_list_focus();
    s.image_paths = paths.iter().map(|p| p.to_string()).collect();
    s
}

mod clipboard;
mod core;
mod history_unified;
mod paste;
