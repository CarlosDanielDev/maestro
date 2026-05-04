use crate::tui::navigation::focus::{FocusId, FocusRing};
use ratatui::style::Style;
use std::path::PathBuf;
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

/// Production clipboard using arboard.
///
/// Wraps clipboard access in `catch_unwind` because `arboard::Clipboard::new()`
/// can panic on WSL environments without a display server (X11/Wayland).
pub struct SystemClipboard;

impl ClipboardProvider for SystemClipboard {
    fn read(&self) -> ClipboardContent {
        if !crate::tui::clipboard::backend_available() {
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
    pub(crate) fn refresh_detected_refs(&mut self) {
        let text = self.editor_text();
        self.detected_issue_numbers = crate::tui::issue_refs::extract_issue_numbers(&text);
        // Auto-hide toggle if fewer than 2 distinct refs
        if self.detected_issue_numbers.len() < 2 {
            self.unified_pr = false;
        }
    }

    pub(crate) fn is_prompt_editor_focused(&self) -> bool {
        self.focus_ring.is_focused(Self::PROMPT_EDITOR_PANE)
    }

    pub(crate) fn is_image_list_focused(&self) -> bool {
        self.focus_ring.is_focused(Self::IMAGE_LIST_PANE)
    }

    pub(crate) fn paste_from_clipboard(&mut self) {
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
}
