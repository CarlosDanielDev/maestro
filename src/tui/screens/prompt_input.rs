use super::wrap::{scroll_offset_for_cursor, wrap_lines};
use super::{PromptSessionConfig, Screen, ScreenAction, draw_keybinds_bar};
use crate::tui::icons::{self, IconId};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::focus::{FocusId, FocusRing};
use crate::tui::navigation::keymap::{KeyBinding, KeyBindingGroup, KeymapProvider};
use crate::tui::theme::Theme;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::path::PathBuf;
use std::sync::OnceLock;
use tui_textarea::TextArea;

/// Result of reading the system clipboard.
pub enum ClipboardContent {
    /// Clipboard contained image data, saved to a temp file at this path.
    Image(PathBuf),
    /// Clipboard contained text (possibly a file path).
    Text(String),
    /// Clipboard was empty or unreadable.
    Empty,
    /// Clipboard backend failed to initialise (e.g. no display on WSL).
    Unavailable,
}

/// Trait for clipboard access, enabling test mocking.
pub trait ClipboardProvider: Send {
    fn read(&self) -> ClipboardContent;
}

/// Caches whether the clipboard backend is available. Once a failure is
/// recorded, subsequent paste attempts skip `arboard::Clipboard::new()`.
static CLIPBOARD_AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Production clipboard using arboard.
///
/// Wraps clipboard access in `catch_unwind` because `arboard::Clipboard::new()`
/// can panic on WSL environments without a display server (X11/Wayland).
pub struct SystemClipboard;

impl ClipboardProvider for SystemClipboard {
    fn read(&self) -> ClipboardContent {
        let available = CLIPBOARD_AVAILABLE.get_or_init(|| {
            std::panic::catch_unwind(arboard::Clipboard::new)
                .ok()
                .and_then(|r| r.ok())
                .is_some()
        });

        if !available {
            return ClipboardContent::Unavailable;
        }

        let mut clipboard = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(_) => return ClipboardContent::Unavailable,
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
    pub(crate) editor: TextArea<'static>,
    pub(crate) image_paths: Vec<String>,
    pub(crate) focus_ring: FocusRing,
    pub(crate) image_path_input: String,
    pub(crate) editing_image_path: bool,
    pub(crate) selected_image: usize,
    pub(crate) clipboard: Box<dyn ClipboardProvider>,
    /// Transient status message shown after clipboard paste.
    pub(crate) status_message: Option<String>,
    /// Snapshot of prompt history strings, most recent last.
    pub(crate) history: Vec<String>,
    /// Current position in history. None = user is typing a new prompt.
    pub(crate) history_cursor: Option<usize>,
    /// Stashed user input when browsing history.
    pub(crate) draft_prompt: String,
    /// Whether unified PR mode is enabled (user toggled Ctrl+U).
    pub(crate) unified_pr: bool,
    /// Detected issue numbers from the current editor text (cached).
    pub(crate) detected_issue_numbers: Vec<u64>,
}

impl PromptInputScreen {
    /// Get the current editor text as a single string.
    pub fn editor_text(&self) -> String {
        self.editor.lines().join("\n")
    }

    /// Backward-compatible accessor for tests and external code.
    #[cfg(test)]
    pub fn prompt_text(&self) -> String {
        self.editor_text()
    }

    /// Returns a display string like "2/3" when browsing history, or None otherwise.
    pub(crate) fn history_indicator(&self) -> Option<String> {
        let cursor = self.history_cursor?;
        let total = self.history.len();
        let position = cursor + 1;
        Some(format!("{}/{}", position, total))
    }

    /// Replace editor content with new text, cursor at end.
    pub fn set_editor_text(&mut self, text: &str) {
        let lines: Vec<String> = if text.is_empty() {
            vec![String::new()]
        } else {
            text.lines().map(String::from).collect()
        };
        self.editor = TextArea::new(lines);
        self.editor.set_cursor_line_style(Style::default());
        // Move cursor to end of text
        let last_row = self.editor.lines().len().saturating_sub(1);
        let last_col = self.editor.lines().last().map(|l| l.len()).unwrap_or(0);
        self.editor.move_cursor(tui_textarea::CursorMove::Jump(
            last_row as u16,
            last_col as u16,
        ));
    }
}

impl PromptInputScreen {
    pub fn new() -> Self {
        Self::with_clipboard(Box::new(SystemClipboard))
    }

    pub const PROMPT_EDITOR_PANE: FocusId = FocusId("prompt:editor");
    pub const IMAGE_LIST_PANE: FocusId = FocusId("prompt:images");

    pub fn with_clipboard(clipboard: Box<dyn ClipboardProvider>) -> Self {
        let mut editor = TextArea::default();
        editor.set_cursor_line_style(Style::default());
        editor.set_placeholder_text("Type your prompt here...");
        Self {
            editor,
            image_paths: Vec::new(),
            focus_ring: FocusRing::new(vec![Self::PROMPT_EDITOR_PANE, Self::IMAGE_LIST_PANE]),
            image_path_input: String::new(),
            editing_image_path: false,
            selected_image: 0,
            clipboard,
            status_message: None,
            history: Vec::new(),
            history_cursor: None,
            draft_prompt: String::new(),
            unified_pr: false,
            detected_issue_numbers: Vec::new(),
        }
    }

    /// Inject prompt history from the App.
    pub fn set_history(&mut self, prompts: Vec<String>) {
        self.history = prompts;
        self.history_cursor = None;
        self.draft_prompt.clear();
    }

    /// Refresh detected issue references from the current editor text.
    fn refresh_detected_refs(&mut self) {
        let text = self.editor_text();
        self.detected_issue_numbers = crate::tui::issue_refs::extract_issue_numbers(&text);
        // Auto-hide toggle if fewer than 2 distinct refs
        if self.detected_issue_numbers.len() < 2 {
            self.unified_pr = false;
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
                self.paste_text(&text);
            }
            ClipboardContent::Empty => {
                self.status_message = Some("Clipboard is empty".to_string());
            }
            ClipboardContent::Unavailable => {
                self.status_message = Some("Clipboard not available on this platform".to_string());
            }
        }
    }

    /// Insert a pasted payload into the active text target.
    ///
    /// Insertion is atomic: embedded newlines stay as newline characters
    /// and never become `KeyCode::Enter` submit events. Shared by the
    /// `Ctrl+V` clipboard path and the bracketed-paste event arm.
    ///
    /// Unicode control chars (C0 + C1 + DEL) are stripped before insert —
    /// only `\n` and `\t` are preserved — so ANSI colour codes in a pasted
    /// terminal dump don't survive into the textarea or the prompt payload
    /// shipped to the model.
    pub fn paste_text(&mut self, text: &str) {
        let sanitized = Self::sanitize_paste(text);
        if sanitized.is_empty() {
            return;
        }
        if self.is_prompt_editor_focused() {
            self.editor.insert_str(&sanitized);
            self.status_message = Some(Self::STATUS_PASTED_TEXT.to_string());
            self.history_cursor = None;
            self.refresh_detected_refs();
        } else if self.is_image_list_focused() {
            self.status_message = Some(format!("Pasted path: {}", sanitized));
            self.image_paths.push(sanitized);
        }
    }

    const STATUS_PASTED_TEXT: &'static str = "Pasted text into prompt";

    fn sanitize_paste(text: &str) -> String {
        text.chars()
            .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
            .collect()
    }

    fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let show_toggle = self.detected_issue_numbers.len() >= 2;
        let toggle_height = if show_toggle { 1 } else { 0 };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(6),                // prompt editor
                Constraint::Length(toggle_height), // unified PR toggle (conditional)
                Constraint::Length(8),             // image list
                Constraint::Length(1),             // keybinds bar
            ])
            .split(area);

        // Prompt editor — custom wrapped rendering
        let editor_title = match self.history_indicator() {
            Some(ref indicator) => format!("Compose Prompt [history {}]", indicator),
            None => "Compose Prompt".to_string(),
        };
        let editor_block = theme.styled_block(&editor_title, self.is_prompt_editor_focused());
        let inner = editor_block.inner(chunks[0]);

        // Read logical lines and cursor from the editing backend
        let logical_lines: Vec<String> = self.editor.lines().to_vec();
        let (cursor_row, cursor_col) = self.editor.cursor();

        // Compute wrapped lines and visual cursor position
        let wrap_result = wrap_lines(&logical_lines, (cursor_row, cursor_col), inner.width);
        let scroll = scroll_offset_for_cursor(wrap_result.cursor.0 as usize, inner.height as usize);

        // Build styled visual lines with issue reference highlighting
        let visual_lines: Vec<Line> = wrap_result
            .lines
            .iter()
            .map(|s| {
                Line::from(crate::tui::issue_refs::highlight_issue_refs(
                    s.as_str(),
                    theme.accent_identifier,
                    theme.text_primary,
                ))
            })
            .collect();

        // Render placeholder when empty
        let paragraph = if logical_lines.len() == 1 && logical_lines[0].is_empty() {
            Paragraph::new(vec![Line::from(Span::styled(
                "Type your prompt here...",
                Style::default().fg(theme.text_secondary),
            ))])
        } else {
            Paragraph::new(visual_lines)
        };

        f.render_widget(
            paragraph.block(editor_block).scroll((scroll as u16, 0)),
            chunks[0],
        );

        // Place cursor manually
        if self.is_prompt_editor_focused() {
            let cursor_visual_row = wrap_result.cursor.0 as usize;
            let cursor_visual_col = wrap_result.cursor.1;
            if cursor_visual_row >= scroll && (cursor_visual_row - scroll) < inner.height as usize {
                f.set_cursor_position(Position::new(
                    inner.x + cursor_visual_col,
                    inner.y + (cursor_visual_row - scroll) as u16,
                ));
            }
        }

        // Image list
        let image_title = format!("Attachments ({})", self.image_paths.len());
        let image_block = theme.styled_block(&image_title, self.is_image_list_focused());

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

        // Unified PR toggle (conditional)
        if show_toggle {
            crate::tui::widgets::unified_pr_toggle::draw_unified_pr_toggle(
                f,
                chunks[1],
                self.unified_pr,
                theme,
            );
        }

        let image_list = Paragraph::new(lines).block(image_block);
        f.render_widget(image_list, chunks[2]);

        // Status message or keybinds bar
        let history_keys = format!(
            "{}/{}",
            icons::get(IconId::ArrowUp),
            icons::get(IconId::ArrowDown)
        );
        if let Some(ref msg) = self.status_message {
            let status = Paragraph::new(Line::from(Span::styled(
                format!(" {} ", msg),
                Style::default().fg(theme.accent_warning),
            )));
            f.render_widget(status, chunks[3]);
        } else if show_toggle {
            draw_keybinds_bar(
                f,
                chunks[3],
                &[
                    ("Enter", "Submit"),
                    ("Ctrl+U", "Unified PR"),
                    ("Ctrl+J", "New line"),
                    (&history_keys, "History"),
                    ("Ctrl+V", "Paste"),
                    ("Tab", "Switch"),
                    ("Esc", "Cancel"),
                ],
                theme,
            );
        } else {
            draw_keybinds_bar(
                f,
                chunks[3],
                &[
                    ("Enter", "Submit"),
                    ("Ctrl+J", "New line"),
                    (&history_keys, "History"),
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
                        key: "Ctrl+j",
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
                        key: "Up/Down",
                        description: "Browse prompt history",
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

    fn alt_key(code: KeyCode) -> crossterm::event::Event {
        key_event_with_modifiers(code, KeyModifiers::ALT)
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
        let mut screen = PromptInputScreen::with_clipboard(MockClipboard::with_text(
            "/home/user/screenshot.png",
        ));
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
        let mut screen =
            PromptInputScreen::with_clipboard(MockClipboard::with_image("/tmp/shot.png"));
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
        let mut screen =
            PromptInputScreen::with_clipboard(MockClipboard::with_text("/tmp/new.png"));
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
        let mut screen =
            PromptInputScreen::with_clipboard(Box::new(CountingUnavailableClipboard {
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
        let mut screen =
            PromptInputScreen::with_clipboard(MockClipboard::with_text("/tmp/file.png"));
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
        let mut screen =
            PromptInputScreen::with_clipboard(MockClipboard::with_text("/tmp/img.png"));
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
}
