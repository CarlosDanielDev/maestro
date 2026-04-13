//! Soft-wrap utilities for the prompt composition editor.
//!
//! All functions are pure transforms on strings and widths — no ratatui frame
//! or terminal state needed, making them trivially unit-testable.

use unicode_width::UnicodeWidthChar;

/// Result of wrapping logical lines for display.
pub struct WrapResult {
    /// Visual lines after wrapping (each fits within `viewport_width` columns).
    pub lines: Vec<String>,
    /// Visual `(row, col)` of the cursor after wrapping.
    pub cursor: (u16, u16),
    /// Total number of visual rows (used in tests for validation).
    #[cfg_attr(not(test), allow(dead_code))]
    pub total_rows: usize,
}

/// Wrap logical editor lines into visual display lines that fit within
/// `viewport_width` display columns, and map the logical cursor position
/// to its visual (row, col) equivalent.
///
/// - Each logical line is wrapped independently at character boundaries.
/// - Empty logical lines produce exactly one visual row (preserving manual newlines).
/// - `viewport_width` of 0 is clamped to 1 to prevent infinite loops.
/// - Cursor column is a **character index** (matching `tui-textarea::TextArea::cursor()`).
pub fn wrap_lines(
    logical_lines: &[impl AsRef<str>],
    cursor: (usize, usize),
    viewport_width: u16,
) -> WrapResult {
    let vw = (viewport_width as usize).max(1);
    let (cursor_row, cursor_col) = cursor;

    let mut visual_lines: Vec<String> = Vec::new();
    let mut visual_row: usize = 0;
    let mut visual_cursor: (u16, u16) = (0, 0);

    for (line_idx, line) in logical_lines.iter().enumerate() {
        let line = line.as_ref();

        if line.is_empty() {
            visual_lines.push(String::new());
            if line_idx == cursor_row {
                visual_cursor = (visual_row as u16, 0);
            }
            visual_row += 1;
            continue;
        }

        let mut current_line = String::new();
        let mut current_width: usize = 0;
        let mut char_idx: usize = 0;

        for ch in line.chars() {
            let ch_width = UnicodeWidthChar::width(ch)
                .unwrap_or(0)
                .max(if ch.is_control() { 0 } else { 1 });

            // Need to wrap?
            if current_width + ch_width > vw && current_width > 0 {
                visual_lines.push(current_line.clone());
                current_line.clear();
                current_width = 0;
                visual_row += 1;
            }

            // Check cursor BEFORE advancing
            if line_idx == cursor_row && char_idx == cursor_col {
                visual_cursor = (visual_row as u16, current_width as u16);
            }

            current_line.push(ch);
            current_width += ch_width;
            char_idx += 1;
        }

        // Emit remaining content
        visual_lines.push(current_line);

        // Cursor at end of line (past last char)
        if line_idx == cursor_row && cursor_col >= char_idx {
            visual_cursor = (visual_row as u16, current_width as u16);
        }

        visual_row += 1;
    }

    // Handle empty input
    if visual_lines.is_empty() {
        visual_lines.push(String::new());
        visual_row = 1;
    }

    WrapResult {
        total_rows: visual_row,
        lines: visual_lines,
        cursor: visual_cursor,
    }
}

/// Given a cursor at `visual_row` and a viewport of `visible_height` rows,
/// return the scroll offset (first visible row index) that keeps the cursor
/// visible.
pub fn scroll_offset_for_cursor(visual_row: usize, visible_height: usize) -> usize {
    let h = visible_height.max(1);
    if visual_row < h {
        0
    } else {
        visual_row + 1 - h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: wrap a single string (one logical line) and return visual lines.
    fn wrap_single(text: &str, width: u16) -> Vec<String> {
        let lines = if text.contains('\n') {
            text.split('\n').map(String::from).collect::<Vec<_>>()
        } else {
            vec![text.to_string()]
        };
        wrap_lines(&lines, (0, 0), width).lines
    }

    // Helper: wrap multiple logical lines and return visual lines.
    fn wrap_multi(lines: &[&str], width: u16) -> Vec<String> {
        let owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        wrap_lines(&owned, (0, 0), width).lines
    }

    // === Group 1: Basic wrapping ===

    #[test]
    fn short_text_fits_in_one_line() {
        assert_eq!(wrap_single("hello", 40), vec!["hello"]);
    }

    #[test]
    fn text_exactly_at_width_does_not_split() {
        assert_eq!(wrap_single("ab", 2), vec!["ab"]);
    }

    #[test]
    fn text_over_width_wraps() {
        let result = wrap_single("abcdef", 4);
        assert_eq!(result, vec!["abcd", "ef"]);
    }

    #[test]
    fn long_text_wraps_multiple_times() {
        let result = wrap_single("abcdefghijkl", 4);
        assert_eq!(result, vec!["abcd", "efgh", "ijkl"]);
    }

    #[test]
    fn empty_string_returns_one_empty_line() {
        assert_eq!(wrap_single("", 40), vec![""]);
    }

    #[test]
    fn width_one_does_not_panic() {
        let result = wrap_single("hi", 1);
        assert_eq!(result, vec!["h", "i"]);
    }

    #[test]
    fn width_zero_clamped_to_one() {
        let result = wrap_single("hi", 0);
        assert!(!result.is_empty());
    }

    // === Group 2: Manual newlines preserved ===

    #[test]
    fn explicit_newline_produces_separate_visual_lines() {
        let lines = vec!["hello".to_string(), "world".to_string()];
        let result = wrap_lines(&lines, (0, 0), 40);
        assert_eq!(result.lines, vec!["hello", "world"]);
    }

    #[test]
    fn empty_line_between_content_preserved() {
        let lines = vec!["a".to_string(), "".to_string(), "b".to_string()];
        let result = wrap_lines(&lines, (0, 0), 40);
        assert_eq!(result.lines, vec!["a", "", "b"]);
    }

    #[test]
    fn manual_newline_then_soft_wrap_combined() {
        let lines = vec!["short".to_string(), "abcdefgh".to_string()];
        let result = wrap_lines(&lines, (0, 0), 5);
        assert_eq!(result.lines, vec!["short", "abcde", "fgh"]);
    }

    // === Group 3: Cursor position mapping ===

    #[test]
    fn cursor_in_unwrapped_line() {
        let lines = vec!["hello".to_string()];
        let result = wrap_lines(&lines, (0, 3), 40);
        assert_eq!(result.cursor, (0, 3));
    }

    #[test]
    fn cursor_at_end_of_unwrapped_line() {
        let lines = vec!["hello".to_string()];
        let result = wrap_lines(&lines, (0, 5), 40);
        assert_eq!(result.cursor, (0, 5));
    }

    #[test]
    fn cursor_in_first_wrapped_segment() {
        let lines = vec!["abcdefgh".to_string()];
        let result = wrap_lines(&lines, (0, 2), 4);
        // "abcd" | "efgh", cursor at char 2 -> visual (0, 2)
        assert_eq!(result.cursor, (0, 2));
    }

    #[test]
    fn cursor_in_second_wrapped_segment() {
        let lines = vec!["abcdefgh".to_string()];
        let result = wrap_lines(&lines, (0, 5), 4);
        // "abcd" | "efgh", char 5 = 'f' -> visual (1, 1)
        assert_eq!(result.cursor, (1, 1));
    }

    #[test]
    fn cursor_at_wrap_boundary() {
        let lines = vec!["abcdefgh".to_string()];
        let result = wrap_lines(&lines, (0, 4), 4);
        // "abcd" | "efgh", char 4 = 'e' -> visual (1, 0)
        assert_eq!(result.cursor, (1, 0));
    }

    #[test]
    fn cursor_on_second_logical_line() {
        let lines = vec!["abc".to_string(), "defgh".to_string()];
        let result = wrap_lines(&lines, (1, 2), 40);
        // Line 0: "abc" (1 visual row), Line 1: "defgh" (1 visual row)
        // Cursor at (1, 2) -> visual (1, 2)
        assert_eq!(result.cursor, (1, 2));
    }

    #[test]
    fn cursor_on_second_logical_line_with_wrap_on_first() {
        let lines = vec!["abcdefgh".to_string(), "xyz".to_string()];
        let result = wrap_lines(&lines, (1, 1), 4);
        // Line 0: "abcd" | "efgh" (2 visual rows)
        // Line 1: "xyz" (1 visual row, starts at visual row 2)
        // Cursor at logical (1, 1) -> visual (2, 1)
        assert_eq!(result.cursor, (2, 1));
    }

    #[test]
    fn cursor_on_empty_line() {
        let lines = vec!["abc".to_string(), "".to_string(), "def".to_string()];
        let result = wrap_lines(&lines, (1, 0), 40);
        assert_eq!(result.cursor, (1, 0));
    }

    // === Group 4: Total rows ===

    #[test]
    fn total_rows_single_line_no_wrap() {
        let lines = vec!["hello".to_string()];
        let result = wrap_lines(&lines, (0, 0), 40);
        assert_eq!(result.total_rows, 1);
    }

    #[test]
    fn total_rows_with_wrap() {
        let lines = vec!["abcdefgh".to_string()];
        let result = wrap_lines(&lines, (0, 0), 4);
        assert_eq!(result.total_rows, 2);
    }

    #[test]
    fn total_rows_multi_line_with_wrap() {
        let lines = vec!["abcdefgh".to_string(), "xyz".to_string()];
        let result = wrap_lines(&lines, (0, 0), 4);
        // Line 0 wraps to 2 visual rows, Line 1 is 1 visual row
        assert_eq!(result.total_rows, 3);
    }

    // === Group 5: Scroll offset ===

    #[test]
    fn scroll_zero_when_cursor_fits() {
        assert_eq!(scroll_offset_for_cursor(3, 10), 0);
    }

    #[test]
    fn scroll_advances_when_cursor_below_viewport() {
        assert_eq!(scroll_offset_for_cursor(12, 10), 3);
    }

    #[test]
    fn scroll_does_not_go_negative() {
        assert_eq!(scroll_offset_for_cursor(0, 10), 0);
    }

    #[test]
    fn scroll_cursor_at_last_visible_row() {
        assert_eq!(scroll_offset_for_cursor(9, 10), 0);
    }

    #[test]
    fn scroll_cursor_one_past_visible() {
        assert_eq!(scroll_offset_for_cursor(10, 10), 1);
    }
}
