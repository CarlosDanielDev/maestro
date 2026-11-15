//! Backend + editing logic for the canonical `Input` widget (#963).
//!
//! Wraps `tui_textarea::TextArea` with the two behaviours every maestro
//! input site needs on top of the raw editor: a single-line mode that never
//! holds a newline, and an optional max-length clamp. Rendering lives in
//! `super::render`; this module is a pure editing engine, unit-testable
//! without a terminal.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Style;
use tui_textarea::{CursorMove, TextArea};

/// Builder for an [`Input`]. All fields are optional.
#[derive(Debug, Clone, Default)]
pub struct InputConfig {
    placeholder: String,
    max_len: Option<usize>,
    initial: String,
}

impl InputConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hint text shown when the input is empty.
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    /// Cap the total character count. Inserts past the cap are dropped.
    pub fn max_len(mut self, n: usize) -> Self {
        self.max_len = Some(n);
        self
    }

    /// Seed the input with starting text (normalised the same as `set_text`).
    pub fn initial(mut self, s: impl Into<String>) -> Self {
        self.initial = s.into();
        self
    }
}

/// One canonical text input. Single-line or multi-line, backed by a
/// `TextArea` so cursor movement, word-jump, and deletion come for free.
pub struct Input {
    area: TextArea<'static>,
    single_line: bool,
    placeholder: String,
    max_len: Option<usize>,
}

impl Input {
    /// A field that never holds a newline: `Enter` is swallowed and pasted
    /// newlines collapse to a single space.
    pub fn single_line(cfg: InputConfig) -> Self {
        Self::build(cfg, true)
    }

    /// A field that preserves `\n` as a logical newline.
    pub fn multi_line(cfg: InputConfig) -> Self {
        Self::build(cfg, false)
    }

    fn build(cfg: InputConfig, single_line: bool) -> Self {
        let mut this = Self {
            area: TextArea::default(),
            single_line,
            placeholder: cfg.placeholder,
            max_len: cfg.max_len,
        };
        this.area.set_cursor_line_style(Style::default());
        if !cfg.initial.is_empty() {
            this.set_text(&cfg.initial);
        }
        this
    }

    pub fn is_single_line(&self) -> bool {
        self.single_line
    }

    pub fn placeholder(&self) -> &str {
        &self.placeholder
    }

    /// Full content, logical lines joined with `\n`.
    pub fn text(&self) -> String {
        self.area.lines().join("\n")
    }

    /// Logical lines (a single-line input always has exactly one).
    pub fn lines(&self) -> &[String] {
        self.area.lines()
    }

    /// Cursor as a logical `(row, col)`, col a character index — matches
    /// `tui_textarea::TextArea::cursor`.
    pub fn cursor(&self) -> (usize, usize) {
        self.area.cursor()
    }

    /// True when the input holds no text.
    pub fn is_empty(&self) -> bool {
        self.area.lines().iter().all(|l| l.is_empty())
    }

    /// Total character count (newlines included for multi-line).
    fn len_chars(&self) -> usize {
        let lines = self.area.lines();
        let content: usize = lines.iter().map(|l| l.chars().count()).sum();
        // Newlines between logical lines also count toward the budget.
        content + lines.len().saturating_sub(1)
    }

    /// Replace the whole content, moving the cursor to the end. Single-line
    /// collapses newlines; the max-length clamp is applied.
    pub fn set_text(&mut self, s: &str) {
        let normalised = if self.single_line {
            collapse_newlines_to_space(s)
        } else {
            s.to_string()
        };
        let clamped = self.clamp_to_max(&normalised);
        let lines: Vec<String> = if clamped.is_empty() {
            vec![String::new()]
        } else {
            clamped.lines().map(String::from).collect()
        };
        let mut area = TextArea::new(lines);
        area.set_cursor_line_style(Style::default());
        let last_row = area.lines().len().saturating_sub(1) as u16;
        let last_col = area.lines().last().map(|l| l.chars().count()).unwrap_or(0) as u16;
        area.move_cursor(CursorMove::Jump(last_row, last_col));
        self.area = area;
    }

    /// Forward an event to the backend. Returns `true` when the content or
    /// cursor changed. Single-line mode swallows `Enter`; the max-length cap
    /// drops overflowing inserts; single-line paste collapses newlines.
    pub fn input(&mut self, event: Event) -> bool {
        match &event {
            Event::Paste(text) => {
                let normalised = if self.single_line {
                    collapse_newlines_to_space(text)
                } else {
                    text.clone()
                };
                let to_insert = self.clamp_insert(&normalised);
                if to_insert.is_empty() {
                    return false;
                }
                self.area.insert_str(&to_insert);
                true
            }
            Event::Key(key) => {
                // Single-line: Enter must never insert a newline.
                if self.single_line && is_bare_enter(key) {
                    return false;
                }
                // Max-length: block a plain character insert at the cap.
                if let Some(max) = self.max_len
                    && is_plain_char(key)
                    && self.len_chars() >= max
                {
                    return false;
                }
                self.area.input(event)
            }
            _ => self.area.input(event),
        }
    }

    /// Truncate `s` so the full content stays within `max_len`.
    fn clamp_to_max(&self, s: &str) -> String {
        match self.max_len {
            Some(max) => s.chars().take(max).collect(),
            None => s.to_string(),
        }
    }

    /// Truncate an insertion to the remaining length budget.
    fn clamp_insert(&self, s: &str) -> String {
        match self.max_len {
            Some(max) => {
                let remaining = max.saturating_sub(self.len_chars());
                s.chars().take(remaining).collect()
            }
            None => s.to_string(),
        }
    }
}

fn is_bare_enter(key: &KeyEvent) -> bool {
    key.code == KeyCode::Enter && key.modifiers.is_empty()
}

/// A plain character insert — no Ctrl/Alt (Shift for capitals is fine).
fn is_plain_char(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char(_))
        && !key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
}

/// Collapse every newline variant (`\r\n`, `\n`, `\r`, U+2028, U+2029) to a
/// single space, so a multi-line paste flattens into one line.
pub(crate) fn collapse_newlines_to_space(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                out.push(' ');
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            '\n' | '\u{2028}' | '\u{2029}' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(c: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    fn key_code(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn ctrl(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::CONTROL))
    }

    fn type_str(input: &mut Input, s: &str) {
        for c in s.chars() {
            input.input(key(c));
        }
    }

    #[test]
    fn single_line_and_multi_line_construct() {
        assert!(Input::single_line(InputConfig::new()).is_single_line());
        assert!(!Input::multi_line(InputConfig::new()).is_single_line());
    }

    #[test]
    fn config_initial_and_placeholder() {
        let input = Input::single_line(InputConfig::new().placeholder("name").initial("hi"));
        assert_eq!(input.placeholder(), "name");
        assert_eq!(input.text(), "hi");
    }

    #[test]
    fn typing_appends_and_cursor_tracks() {
        let mut input = Input::single_line(InputConfig::new());
        type_str(&mut input, "abc");
        assert_eq!(input.text(), "abc");
        assert_eq!(input.cursor(), (0, 3));
    }

    #[test]
    fn single_line_swallows_enter() {
        let mut input = Input::single_line(InputConfig::new());
        type_str(&mut input, "ab");
        let changed = input.input(key_code(KeyCode::Enter));
        assert!(!changed);
        assert_eq!(input.lines().len(), 1);
        assert_eq!(input.text(), "ab");
    }

    #[test]
    fn multi_line_enter_inserts_newline() {
        let mut input = Input::multi_line(InputConfig::new());
        type_str(&mut input, "a");
        input.input(key_code(KeyCode::Enter));
        type_str(&mut input, "b");
        assert_eq!(input.lines().len(), 2);
        assert_eq!(input.text(), "a\nb");
    }

    #[test]
    fn single_line_paste_collapses_newlines() {
        let mut input = Input::single_line(InputConfig::new());
        input.input(Event::Paste("one\ntwo\r\nthree".to_string()));
        assert_eq!(input.text(), "one two three");
        assert_eq!(input.lines().len(), 1);
    }

    #[test]
    fn multi_line_paste_keeps_newlines() {
        let mut input = Input::multi_line(InputConfig::new());
        input.input(Event::Paste("one\ntwo".to_string()));
        assert_eq!(input.text(), "one\ntwo");
        assert_eq!(input.lines().len(), 2);
    }

    #[test]
    fn set_text_single_line_collapses() {
        let mut input = Input::single_line(InputConfig::new());
        input.set_text("x\ny");
        assert_eq!(input.text(), "x y");
    }

    #[test]
    fn max_len_blocks_typing_past_cap() {
        let mut input = Input::single_line(InputConfig::new().max_len(3));
        type_str(&mut input, "abcdef");
        assert_eq!(input.text(), "abc");
    }

    #[test]
    fn max_len_clamps_paste() {
        let mut input = Input::single_line(InputConfig::new().max_len(4));
        type_str(&mut input, "ab");
        input.input(Event::Paste("cdefgh".to_string()));
        assert_eq!(input.text(), "abcd");
    }

    #[test]
    fn max_len_clamps_initial_text() {
        let input = Input::single_line(InputConfig::new().max_len(2).initial("abcd"));
        assert_eq!(input.text(), "ab");
    }

    #[test]
    fn home_end_and_word_jump_navigation() {
        let mut input = Input::single_line(InputConfig::new());
        type_str(&mut input, "hello world");
        input.input(key_code(KeyCode::Home));
        assert_eq!(input.cursor(), (0, 0));
        input.input(key_code(KeyCode::End));
        assert_eq!(input.cursor(), (0, 11));
        // Ctrl+Left is a word jump in tui-textarea's default keymap.
        input.input(ctrl(KeyCode::Left));
        assert_eq!(input.cursor(), (0, 6));
    }

    #[test]
    fn backspace_deletes_at_cursor() {
        let mut input = Input::single_line(InputConfig::new());
        type_str(&mut input, "abc");
        input.input(key_code(KeyCode::Backspace));
        assert_eq!(input.text(), "ab");
    }

    #[test]
    fn is_empty_reflects_content() {
        let mut input = Input::multi_line(InputConfig::new());
        assert!(input.is_empty());
        type_str(&mut input, "x");
        assert!(!input.is_empty());
    }
}
