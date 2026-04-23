//! Shared paste helpers for the Issue and Milestone wizards.
//!
//! The wizards don't use `tui_textarea` (they maintain bare `String`
//! buffers per field), so this module centralises the sanitization +
//! newline policy that the `PromptInput` screen's `paste_text` bakes
//! into its textarea call.

/// Strip control characters (C0/C1 + DEL + ANSI escapes) from a pasted
/// payload while preserving `\n` and `\t`. Mirrors the sanitizer in
/// `PromptInputScreen::sanitize_paste` so pasted terminal output
/// doesn't smuggle in colour codes or invisible cursor movement.
pub fn sanitize_paste(text: &str) -> String {
    text.chars()
        .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
        .collect()
}

/// Append sanitised pasted text to a target buffer.
///
/// `allow_newlines = false` is used for single-line fields (Title);
/// embedded `\n` and `\r` are replaced with spaces so the user doesn't
/// end up with a multi-line title that fails GitHub's API validation.
pub fn append_paste(target: &mut String, text: &str, allow_newlines: bool) {
    let sanitised = sanitize_paste(text);
    if sanitised.is_empty() {
        return;
    }
    if allow_newlines {
        target.push_str(&sanitised);
    } else {
        for c in sanitised.chars() {
            if c == '\n' || c == '\r' {
                target.push(' ');
            } else {
                target.push(c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_paste_preserves_newlines_and_tabs() {
        let out = sanitize_paste("a\nb\tc");
        assert_eq!(out, "a\nb\tc");
    }

    #[test]
    fn sanitize_paste_strips_ansi_escape_sequences() {
        // CSI colour code + DEL should be stripped; plain chars survive.
        let raw = "\x1b[31mred\x1b[0m\x7fend";
        let out = sanitize_paste(raw);
        assert_eq!(out, "[31mred[0mend");
    }

    #[test]
    fn append_paste_appends_multiline_when_allowed() {
        let mut s = String::from("hi");
        append_paste(&mut s, "\nmore\nlines", true);
        assert_eq!(s, "hi\nmore\nlines");
    }

    #[test]
    fn append_paste_replaces_newlines_with_space_when_single_line() {
        let mut s = String::from("hi");
        append_paste(&mut s, "\nmore\nlines", false);
        assert_eq!(s, "hi more lines");
    }

    #[test]
    fn append_paste_noop_on_pure_control_chars() {
        let mut s = String::from("hi");
        append_paste(&mut s, "\x1b\x1b", true);
        assert_eq!(s, "hi");
    }
}
