mod draw;
#[cfg(test)]
mod tests;
mod types;

pub use types::{ClipboardContent, ClipboardProvider, PromptInputScreen, SystemClipboard};

use super::{PromptSessionConfig, Screen, ScreenAction};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{Frame, layout::Rect};

impl KeymapProvider for PromptInputScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![
            key_group(
                "Prompt Editor",
                &[
                    ("Enter", "Submit prompt"),
                    ("Ctrl+j", "New line"),
                    ("Ctrl+v", "Paste from clipboard"),
                    ("Tab", "Toggle focus (editor/images)"),
                    ("Up/Down", "Browse prompt history"),
                    ("Esc", "Back"),
                ],
            ),
            key_group(
                "Image List",
                &[
                    ("a", "Add image path"),
                    ("d", "Delete selected image"),
                    ("j/k", "Navigate images"),
                ],
            ),
        ]
    }
}

fn key_group(title: &'static str, bindings: &[(&'static str, &'static str)]) -> KeyBindingGroup {
    KeyBindingGroup {
        title,
        bindings: bindings
            .iter()
            .map(|(key, description)| KeyBinding { key, description })
            .collect(),
    }
}

impl Screen for PromptInputScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
        if let Event::Paste(text) = event {
            self.paste_text(text);
            return ScreenAction::None;
        }

        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            if *modifiers == KeyModifiers::CONTROL && *code == KeyCode::Char('v') {
                self.paste_from_clipboard();
                return ScreenAction::None;
            }

            if *modifiers == KeyModifiers::CONTROL && *code == KeyCode::Char('u') {
                if self.detected_issue_numbers.len() >= 2 {
                    self.unified_pr = !self.unified_pr;
                }
                return ScreenAction::None;
            }

            if *code == KeyCode::Esc {
                if self.editing_image_path {
                    self.editing_image_path = false;
                    self.image_path_input.clear();
                    return ScreenAction::None;
                }
                return ScreenAction::Pop;
            }

            if *code == KeyCode::Tab {
                self.focus_ring.next();
                return ScreenAction::None;
            }
            if *code == KeyCode::BackTab {
                self.focus_ring.previous();
                return ScreenAction::None;
            }

            // Route input based on focus and editing state
            if self.editing_image_path {
                match code {
                    KeyCode::Enter => {
                        if !self.image_path_input.is_empty() {
                            self.image_paths.push(self.image_path_input.clone());
                        }
                        self.editing_image_path = false;
                        self.image_path_input.clear();
                    }
                    KeyCode::Backspace => {
                        self.image_path_input.pop();
                    }
                    KeyCode::Char(c) => {
                        self.image_path_input.push(*c);
                    }
                    _ => {}
                }
                return ScreenAction::None;
            }

            if self.is_prompt_editor_focused() {
                match (code, modifiers) {
                    // Enter alone → submit prompt
                    (KeyCode::Enter, m) if *m == KeyModifiers::NONE => {
                        let text = self.editor_text();
                        if !text.trim().is_empty() {
                            // Unified PR: launch as unified session if toggled
                            if self.unified_pr && self.detected_issue_numbers.len() >= 2 {
                                let issues: Vec<(u64, String)> = self
                                    .detected_issue_numbers
                                    .iter()
                                    .map(|n| (*n, format!("Issue #{}", n)))
                                    .collect();
                                return ScreenAction::LaunchUnifiedSession(
                                    super::UnifiedSessionConfig {
                                        issues,
                                        custom_prompt: Some(text),
                                    },
                                );
                            }
                            return ScreenAction::LaunchPromptSession(PromptSessionConfig {
                                prompt: text,
                                image_paths: self.image_paths.clone(),
                            });
                        }
                    }
                    // Ctrl+J or Shift+Enter or Alt+Enter → insert newline
                    (KeyCode::Char('j'), m) if m.contains(KeyModifiers::CONTROL) => {
                        self.editor.insert_newline();
                    }
                    (KeyCode::Enter, _) => {
                        self.editor.insert_newline();
                    }
                    // Up → history navigation when cursor is on first line
                    (KeyCode::Up, _) => {
                        let cursor_row = self.editor.cursor().0;
                        if cursor_row == 0 {
                            // On first line — navigate history
                            if !self.history.is_empty() && self.history_cursor.is_none() {
                                self.draft_prompt = self.editor_text();
                                let idx = self.history.len() - 1;
                                self.history_cursor = Some(idx);
                                self.set_editor_text(&self.history[idx].clone());
                            } else if let Some(idx) = self.history_cursor
                                && idx > 0
                            {
                                self.history_cursor = Some(idx - 1);
                                self.set_editor_text(&self.history[idx - 1].clone());
                            }
                        } else {
                            // Multi-line: move cursor up
                            self.editor.input(event.clone());
                        }
                    }
                    // Down → history navigation when cursor is on last line
                    (KeyCode::Down, _) => {
                        let cursor_row = self.editor.cursor().0;
                        let last_row = self.editor.lines().len().saturating_sub(1);
                        if cursor_row == last_row {
                            // On last line — navigate history
                            if let Some(idx) = self.history_cursor {
                                if idx + 1 < self.history.len() {
                                    self.history_cursor = Some(idx + 1);
                                    self.set_editor_text(&self.history[idx + 1].clone());
                                } else {
                                    self.history_cursor = None;
                                    let draft = self.draft_prompt.clone();
                                    self.set_editor_text(&draft);
                                }
                            }
                        } else {
                            // Multi-line: move cursor down
                            self.editor.input(event.clone());
                        }
                    }
                    // All other keys → delegate to TextArea
                    _ => {
                        self.editor.input(event.clone());
                        self.history_cursor = None;
                        self.refresh_detected_refs();
                    }
                }
            } else if self.is_image_list_focused() {
                match code {
                    KeyCode::Char('a') => {
                        self.editing_image_path = true;
                        self.image_path_input.clear();
                    }
                    KeyCode::Char('d') if !self.image_paths.is_empty() => {
                        self.image_paths.remove(self.selected_image);
                        if self.selected_image > 0 && self.selected_image >= self.image_paths.len()
                        {
                            self.selected_image = self.image_paths.len().saturating_sub(1);
                        }
                    }
                    KeyCode::Char('j') | KeyCode::Down
                        if !self.image_paths.is_empty()
                            && self.selected_image < self.image_paths.len() - 1 =>
                    {
                        self.selected_image += 1;
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.selected_image = self.selected_image.saturating_sub(1);
                    }
                    _ => {}
                }
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        self.draw_impl(f, area, theme);
    }

    fn desired_input_mode(&self) -> Option<InputMode> {
        if self.is_prompt_editor_focused() {
            Some(InputMode::Insert)
        } else {
            Some(InputMode::Normal)
        }
    }
}
