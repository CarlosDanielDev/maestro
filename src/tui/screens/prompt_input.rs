use super::{PromptSessionConfig, Screen, ScreenAction, draw_keybinds_bar};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::focus::{FocusId, FocusRing};
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::path::PathBuf;

/// Result of reading the system clipboard.
pub enum ClipboardContent {
    /// Clipboard contained image data, saved to a temp file at this path.
    Image(PathBuf),
    /// Clipboard contained text (possibly a file path).
    Text(String),
    /// Clipboard was empty or unreadable.
    Empty,
}

/// Trait for clipboard access, enabling test mocking.
pub trait ClipboardProvider: Send {
    fn read(&self) -> ClipboardContent;
}

/// Production clipboard using arboard.
pub struct SystemClipboard;

impl ClipboardProvider for SystemClipboard {
    fn read(&self) -> ClipboardContent {
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(_) => return ClipboardContent::Empty,
        };

        // Try image first
        if let Ok(image) = clipboard.get_image()
            && let Some(path) = save_clipboard_image(&image)
        {
            return ClipboardContent::Image(path);
        }

        // Fall back to text
        if let Ok(text) = clipboard.get_text()
            && !text.trim().is_empty()
        {
            return ClipboardContent::Text(text.trim().to_string());
        }

        ClipboardContent::Empty
    }
}

/// Save arboard image data to a temp PNG file.
fn save_clipboard_image(img: &arboard::ImageData) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("maestro-clips");
    std::fs::create_dir_all(&dir).ok()?;
    let filename = format!("clip-{}.png", uuid::Uuid::new_v4());
    let path = dir.join(&filename);

    let rgba_buf: image::RgbaImage =
        image::ImageBuffer::from_raw(img.width as u32, img.height as u32, img.bytes.to_vec())?;
    rgba_buf.save(&path).ok()?;

    Some(path)
}

pub struct PromptInputScreen {
    pub(crate) prompt_text: String,
    pub(crate) image_paths: Vec<String>,
    pub(crate) focus_ring: FocusRing,
    pub(crate) image_path_input: String,
    pub(crate) editing_image_path: bool,
    pub(crate) selected_image: usize,
    pub(crate) clipboard: Box<dyn ClipboardProvider>,
    /// Transient status message shown after clipboard paste.
    pub(crate) status_message: Option<String>,
}

impl PromptInputScreen {
    pub fn new() -> Self {
        Self::with_clipboard(Box::new(SystemClipboard))
    }

    pub const PROMPT_EDITOR_PANE: FocusId = FocusId("prompt:editor");
    pub const IMAGE_LIST_PANE: FocusId = FocusId("prompt:images");

    pub fn with_clipboard(clipboard: Box<dyn ClipboardProvider>) -> Self {
        Self {
            prompt_text: String::new(),
            image_paths: Vec::new(),
            focus_ring: FocusRing::new(vec![Self::PROMPT_EDITOR_PANE, Self::IMAGE_LIST_PANE]),
            image_path_input: String::new(),
            editing_image_path: false,
            selected_image: 0,
            clipboard,
            status_message: None,
        }
    }

    fn is_prompt_editor_focused(&self) -> bool {
        self.focus_ring.is_focused(Self::PROMPT_EDITOR_PANE)
    }

    fn is_image_list_focused(&self) -> bool {
        self.focus_ring.is_focused(Self::IMAGE_LIST_PANE)
    }

    fn paste_from_clipboard(&mut self) {
        match self.clipboard.read() {
            ClipboardContent::Image(path) => {
                let path_str = path.to_string_lossy().to_string();
                self.status_message = Some(format!("Pasted image: {}", path_str));
                self.image_paths.push(path_str);
            }
            ClipboardContent::Text(text) => {
                self.status_message = Some(format!("Pasted path: {}", text));
                self.image_paths.push(text);
            }
            ClipboardContent::Empty => {
                self.status_message = Some("Clipboard is empty".to_string());
            }
        }
    }

    fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(6),    // prompt editor
                Constraint::Length(8), // image list
                Constraint::Length(1), // keybinds bar
            ])
            .split(area);

        // Prompt editor
        let editor_border_color = if self.is_prompt_editor_focused() {
            theme.border_active
        } else {
            theme.border_inactive
        };
        let editor_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(editor_border_color))
            .title(Span::styled(
                " Compose Prompt ",
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD),
            ));

        let display_text = if self.prompt_text.is_empty() {
            "Type your prompt here...".to_string()
        } else {
            self.prompt_text.clone()
        };
        let text_style = if self.prompt_text.is_empty() {
            Style::default().fg(theme.text_secondary)
        } else {
            Style::default().fg(theme.text_primary)
        };
        let editor = Paragraph::new(display_text)
            .style(text_style)
            .block(editor_block)
            .wrap(Wrap { trim: false });
        f.render_widget(editor, chunks[0]);

        // Image list
        let image_border_color = if self.is_image_list_focused() {
            theme.border_active
        } else {
            theme.border_inactive
        };
        let image_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(image_border_color))
            .title(Span::styled(
                format!(" Attachments ({}) ", self.image_paths.len()),
                Style::default().fg(theme.accent_info),
            ));

        let mut lines: Vec<Line> = Vec::new();
        if self.image_paths.is_empty() && !self.editing_image_path {
            lines.push(Line::from(Span::styled(
                "  (no images attached)",
                Style::default().fg(theme.text_secondary),
            )));
        }
        for (i, path) in self.image_paths.iter().enumerate() {
            let style = if i == self.selected_image && self.is_image_list_focused() {
                Style::default()
                    .fg(theme.accent_success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            let prefix = if i == self.selected_image && self.is_image_list_focused() {
                " > "
            } else {
                "   "
            };
            lines.push(Line::from(Span::styled(
                format!("{}{}", prefix, path),
                style,
            )));
        }
        if self.editing_image_path {
            lines.push(Line::from(vec![
                Span::styled("  Path: ", Style::default().fg(theme.accent_warning)),
                Span::styled(
                    &self.image_path_input,
                    Style::default().fg(theme.text_primary),
                ),
                Span::styled("_", Style::default().fg(theme.accent_success)),
            ]));
        }
        if self.is_image_list_focused() && !self.editing_image_path {
            lines.push(Line::from(Span::styled(
                "  [a] Add   [d] Remove   [Ctrl+V] Paste",
                Style::default().fg(theme.text_secondary),
            )));
        }

        let image_list = Paragraph::new(lines).block(image_block);
        f.render_widget(image_list, chunks[1]);

        // Status message or keybinds bar
        if let Some(ref msg) = self.status_message {
            let status = Paragraph::new(Line::from(Span::styled(
                format!(" {} ", msg),
                Style::default().fg(theme.accent_warning),
            )));
            f.render_widget(status, chunks[2]);
        } else {
            draw_keybinds_bar(
                f,
                chunks[2],
                &[
                    ("Enter", "Submit"),
                    ("Shift+Enter", "New line"),
                    ("Ctrl+V", "Paste"),
                    ("Tab", "Switch"),
                    ("Esc", "Cancel"),
                ],
                theme,
            );
        }
    }
}

impl KeymapProvider for PromptInputScreen {
    fn keybindings(&self) -> Vec<KeyBindingGroup> {
        vec![
            KeyBindingGroup {
                title: "Prompt Editor",
                bindings: vec![
                    KeyBinding {
                        key: "Enter",
                        description: "Submit prompt",
                    },
                    KeyBinding {
                        key: "Shift+Enter",
                        description: "New line",
                    },
                    KeyBinding {
                        key: "Ctrl+v",
                        description: "Paste from clipboard",
                    },
                    KeyBinding {
                        key: "Tab",
                        description: "Toggle focus (editor/images)",
                    },
                    KeyBinding {
                        key: "Esc",
                        description: "Back",
                    },
                ],
            },
            KeyBindingGroup {
                title: "Image List",
                bindings: vec![
                    KeyBinding {
                        key: "a",
                        description: "Add image path",
                    },
                    KeyBinding {
                        key: "d",
                        description: "Delete selected image",
                    },
                    KeyBinding {
                        key: "j/k",
                        description: "Navigate images",
                    },
                ],
            },
        ]
    }
}

impl Screen for PromptInputScreen {
    fn handle_input(&mut self, event: &Event, _mode: InputMode) -> ScreenAction {
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
                match code {
                    KeyCode::Enter if *modifiers == KeyModifiers::SHIFT => {
                        self.prompt_text.push('\n');
                    }
                    KeyCode::Enter => {
                        if !self.prompt_text.trim().is_empty() {
                            return ScreenAction::LaunchPromptSession(PromptSessionConfig {
                                prompt: self.prompt_text.clone(),
                                image_paths: self.image_paths.clone(),
                            });
                        }
                    }
                    KeyCode::Char(c)
                        if *modifiers == KeyModifiers::NONE
                            || *modifiers == KeyModifiers::SHIFT =>
                    {
                        self.prompt_text.push(*c);
                    }
                    KeyCode::Backspace => {
                        self.prompt_text.pop();
                    }
                    _ => {}
                }
            } else if self.is_image_list_focused() {
                match code {
                    KeyCode::Char('a') => {
                        self.editing_image_path = true;
                        self.image_path_input.clear();
                    }
                    KeyCode::Char('d') => {
                        if !self.image_paths.is_empty() {
                            self.image_paths.remove(self.selected_image);
                            if self.selected_image > 0
                                && self.selected_image >= self.image_paths.len()
                            {
                                self.selected_image = self.image_paths.len().saturating_sub(1);
                            }
                        }
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        if !self.image_paths.is_empty()
                            && self.selected_image < self.image_paths.len() - 1
                        {
                            self.selected_image += 1;
                        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::screens::test_helpers::{key_event, key_event_with_modifiers};
    use crossterm::event::{KeyCode, KeyModifiers};

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
    }

    impl ClipboardProvider for MockClipboard {
        fn read(&self) -> ClipboardContent {
            match &self.content {
                ClipboardContent::Image(p) => ClipboardContent::Image(p.clone()),
                ClipboardContent::Text(t) => ClipboardContent::Text(t.clone()),
                ClipboardContent::Empty => ClipboardContent::Empty,
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
        s.prompt_text = text.to_string();
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

    // --- Group 1: Initial state ---

    #[test]
    fn prompt_input_initial_state_prompt_is_empty() {
        let screen = mock_screen();
        assert_eq!(screen.prompt_text, "");
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
        assert_eq!(screen.prompt_text, "hi!");
    }

    #[test]
    fn prompt_input_shift_enter_inserts_newline() {
        let mut screen = screen_with_prompt("hello");
        let action = screen.handle_input(&shift_key(KeyCode::Enter), InputMode::Normal);
        assert_eq!(screen.prompt_text, "hello\n");
        assert_eq!(action, ScreenAction::None);
    }

    #[test]
    fn prompt_input_backspace_removes_last_character() {
        let mut screen = screen_with_prompt("abc");
        screen.handle_input(&key_event(KeyCode::Backspace), InputMode::Normal);
        assert_eq!(screen.prompt_text, "ab");
    }

    #[test]
    fn prompt_input_backspace_on_empty_prompt_is_noop() {
        let mut screen = mock_screen();
        let action = screen.handle_input(&key_event(KeyCode::Backspace), InputMode::Normal);
        assert_eq!(screen.prompt_text, "");
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
    fn prompt_input_ctrl_s_is_ignored() {
        let mut screen = screen_with_prompt("fix the bug");
        let action = screen.handle_input(&ctrl_key(KeyCode::Char('s')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert_eq!(screen.prompt_text, "fix the bug");
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
        let original_prompt = screen.prompt_text.clone();
        for ch in ['/', 't', 'm', 'p'] {
            screen.handle_input(&key_event(KeyCode::Char(ch)), InputMode::Normal);
        }
        assert_eq!(screen.image_path_input, "/tmp");
        assert_eq!(screen.prompt_text, original_prompt);
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
        screen.prompt_text = "existing".to_string();
        screen.image_paths = vec!["/x.png".to_string()];
        for code in [KeyCode::Char('j'), KeyCode::Char('k'), KeyCode::Char('d')] {
            screen.handle_input(&key_event(code), InputMode::Normal);
        }
        assert_eq!(screen.prompt_text, "existing");
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
    fn prompt_input_ctrl_v_with_text_adds_text_as_path() {
        let mut screen = PromptInputScreen::with_clipboard(MockClipboard::with_text(
            "/home/user/screenshot.png",
        ));
        let action = screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
        assert_eq!(action, ScreenAction::None);
        assert_eq!(
            screen.image_paths,
            vec!["/home/user/screenshot.png".to_string()]
        );
        assert!(screen.status_message.unwrap().contains("Pasted path"));
    }

    #[test]
    fn prompt_input_ctrl_v_with_empty_clipboard_shows_message() {
        let mut screen = PromptInputScreen::with_clipboard(MockClipboard::empty());
        screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
        assert!(screen.image_paths.is_empty());
        assert_eq!(screen.status_message.unwrap(), "Clipboard is empty");
    }

    #[test]
    fn prompt_input_ctrl_v_works_from_prompt_editor_focus() {
        let mut screen =
            PromptInputScreen::with_clipboard(MockClipboard::with_text("/tmp/shot.png"));
        // Default focus is PromptEditor — Ctrl+V should still work
        assert!(
            screen
                .focus_ring
                .is_focused(PromptInputScreen::PROMPT_EDITOR_PANE)
        );
        screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
        assert_eq!(screen.image_paths, vec!["/tmp/shot.png".to_string()]);
    }

    #[test]
    fn prompt_input_ctrl_v_appends_to_existing_images() {
        let mut screen =
            PromptInputScreen::with_clipboard(MockClipboard::with_text("/tmp/new.png"));
        screen.image_paths = vec!["/tmp/existing.png".to_string()];
        screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
        assert_eq!(
            screen.image_paths,
            vec!["/tmp/existing.png".to_string(), "/tmp/new.png".to_string()]
        );
    }

    #[test]
    fn prompt_input_ctrl_v_does_not_affect_prompt_text() {
        let mut screen =
            PromptInputScreen::with_clipboard(MockClipboard::with_text("/tmp/img.png"));
        screen.prompt_text = "my prompt".to_string();
        screen.handle_input(&ctrl_key(KeyCode::Char('v')), InputMode::Normal);
        assert_eq!(screen.prompt_text, "my prompt");
    }
}
