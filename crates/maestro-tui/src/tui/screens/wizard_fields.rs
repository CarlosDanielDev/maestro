//! Shared text-field editing primitives for the Issue and Milestone
//! wizards.
//!
//! Historically each wizard maintained a `String` buffer per field and
//! hand-rolled character append / backspace / newline / paste handling.
//! That duplicated the work `tui_textarea` already does — cursor
//! movement, selection, undo, word-wise deletion — and every missing
//! nicety showed up as a bug report (issue #447). This module wraps
//! `TextArea` with two concessions the wizards need on top of the raw
//! editor:
//!
//! 1. A `single_line` switch for fields like `Title` that must never
//!    contain a newline. The sanitizer collapses `\n` / `\r` / `\r\n`
//!    into a single space on insert.
//! 2. An `insert_sanitized` entry point that strips control characters
//!    (ANSI escape codes, DEL, C0/C1 range) before delegating to
//!    `TextArea::insert_str`. Matches the sanitizer the prompt-input
//!    screen applies to bracketed-paste payloads.

use ratatui::style::{Modifier, Style};
use tui_textarea::{CursorMove, TextArea};

/// One focusable text field inside a wizard step. Wraps a
/// `tui_textarea::TextArea` with the wizard-specific single-line vs
/// multi-line distinction and paste sanitizer.
pub struct TextAreaField {
    area: TextArea<'static>,
    single_line: bool,
}

impl TextAreaField {
    /// Single-line field: newlines are collapsed to spaces on any kind
    /// of insert (typing, paste, `set_text`). Use for GitHub titles and
    /// the milestone doc-reference buffer line.
    pub fn single_line() -> Self {
        let mut area = TextArea::default();
        area.set_cursor_line_style(Style::default());
        Self {
            area,
            single_line: true,
        }
    }

    /// Multi-line field: `\n` is preserved as a logical newline in the
    /// textarea. Use for Overview, Acceptance Criteria, etc.
    pub fn multi_line() -> Self {
        let mut area = TextArea::default();
        area.set_cursor_line_style(Style::default());
        Self {
            area,
            single_line: false,
        }
    }

    pub fn is_single_line(&self) -> bool {
        self.single_line
    }

    pub fn area(&self) -> &TextArea<'static> {
        &self.area
    }

    pub fn area_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.area
    }

    /// Current content, joining multi-line text with `\n`.
    pub fn text(&self) -> String {
        self.area.lines().join("\n")
    }

    /// Replace the full content. Single-line fields collapse any `\n`
    /// / `\r` / `\r\n` sequences to a single space. Cursor lands at
    /// end of content (same behaviour as `PromptInputScreen::set_editor_text`).
    pub fn set_text(&mut self, s: &str) {
        let normalised = if self.single_line {
            collapse_newlines_to_space(s)
        } else {
            s.to_string()
        };
        let lines: Vec<String> = if normalised.is_empty() {
            vec![String::new()]
        } else {
            normalised.lines().map(String::from).collect()
        };
        // Preserve existing cursor-line styling. `TextArea::new` resets
        // internal state; reapply the style we set in the constructor.
        let mut area = TextArea::new(lines);
        area.set_cursor_line_style(Style::default());
        let last_row = area.lines().len().saturating_sub(1) as u16;
        let last_col = area.lines().last().map(|l| l.len()).unwrap_or(0) as u16;
        area.move_cursor(CursorMove::Jump(last_row, last_col));
        self.area = area;
    }

    /// Insert pasted or programmatically-supplied text at the current
    /// cursor. C0/C1 control chars, DEL, bidi overrides, Unicode line
    /// separators, and BOM are stripped; `\n` and `\t` are preserved.
    /// On single-line fields, every newline variant collapses to a
    /// single space before insertion.
    pub fn insert_sanitized(&mut self, text: &str) {
        // For single-line fields, collapse newlines BEFORE the
        // control-char filter so a bare `\r` (which is itself a
        // control char) still becomes a space rather than being dropped.
        let normalised = if self.single_line {
            collapse_newlines_to_space(text)
        } else {
            text.to_string()
        };
        let filtered: String = normalised
            .chars()
            .filter(|&c| c == '\n' || c == '\t' || (!c.is_control() && !is_hostile_format(c)))
            .collect();
        if filtered.is_empty() {
            return;
        }
        self.area.insert_str(&filtered);
    }
}

/// Unicode code points that survive `char::is_control` (category `Cc`)
/// but are still hostile to terminal / markdown / LLM-prompt surfaces:
///   - `U+202A..U+202E`, `U+2066..U+2069` — bidi overrides and
///     isolates. The "Trojan Source" family (CVE-2021-42574).
///   - `U+2028`, `U+2029` — Unicode line / paragraph separators.
///     Would smuggle a logical newline into a "single-line" Title.
///   - `U+FEFF` — zero-width no-break space / BOM. Commonly used to
///     break homoglyph detection.
fn is_hostile_format(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{FEFF}'
    )
}

/// Normalise every newline variant (`\r\n`, `\n`, `\r`, `U+2028`,
/// `U+2029`) to a single space. Used by single-line fields so a
/// multi-line clipboard paste flattens into one line.
fn collapse_newlines_to_space(s: &str) -> String {
    // Single pass with lookahead-by-peek: `\r\n` collapses to ONE
    // space (consumes both), bare `\r`/`\n`/LS/PS to a space.
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

/// A collection of text fields with a focus cursor. One `WizardFields`
/// is bound to a single wizard step; focus cycles within the step.
pub struct WizardFields {
    fields: Vec<TextAreaField>,
    focus: usize,
}

impl WizardFields {
    pub fn new(fields: Vec<TextAreaField>) -> Self {
        Self { fields, focus: 0 }
    }

    pub fn empty() -> Self {
        Self {
            fields: Vec::new(),
            focus: 0,
        }
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    pub fn focus_next(&mut self) {
        if !self.fields.is_empty() {
            self.focus = (self.focus + 1) % self.fields.len();
        }
    }

    pub fn focus_prev(&mut self) {
        if !self.fields.is_empty() {
            self.focus = (self.focus + self.fields.len() - 1) % self.fields.len();
        }
    }

    pub fn get(&self, idx: usize) -> Option<&TextAreaField> {
        self.fields.get(idx)
    }

    pub fn get_mut(&mut self, idx: usize) -> Option<&mut TextAreaField> {
        self.fields.get_mut(idx)
    }

    pub fn focused_mut(&mut self) -> Option<&mut TextAreaField> {
        self.fields.get_mut(self.focus)
    }

    /// Paint the focused field's cursor as reverse-video and hide the
    /// cursor on every other field. Called from the wizard's `draw`
    /// before rendering each textarea's widget.
    pub fn refresh_focus_styles(&mut self) {
        let focused = self.focus;
        for (i, field) in self.fields.iter_mut().enumerate() {
            if i == focused {
                field
                    .area
                    .set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
            } else {
                field
                    .area
                    .set_cursor_style(Style::default().add_modifier(Modifier::HIDDEN));
            }
        }
    }
}

#[cfg(test)]
#[path = "wizard_fields_tests.rs"]
mod tests;
