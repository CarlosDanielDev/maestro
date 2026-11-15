//! Sanitizers for external strings before they hit user-visible sinks
//! (activity log, TUI labels, tracing). Each sanitizer documents its
//! tradeoff so callers can pick the right one for the sink.
//!
//! Original home was `src/tui/app/auto_pr.rs`, hoisted here when #562
//! introduced a second consumer (`completion_git`). `doctor::sanitize`
//! is intentionally a separate flavor (drop vs replace) — see its
//! comment for the reason.

/// Strip ASCII control characters from external error strings before
/// they hit the activity log, replacing them with a space. Defeats
/// terminal-escape / label-spoofing attacks that could ride on
/// `gh` / `git` stderr (LOW-1, #514 security review). Replace-with-
/// space preserves token boundaries — important for human-readable
/// error messages where `git\tpush` should NOT collapse to `gitpush`.
///
/// Unicode format and bidi controls are not `is_control()`, so they are
/// matched explicitly: a hostile path with U+202E could otherwise render
/// reversed in the TUI (#1074).
pub fn sanitize_log(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == ' ' || !(c.is_control() || is_format_control(c)) {
                c
            } else {
                ' '
            }
        })
        .collect()
}

/// Zero-width and directional marks, bidi embeddings/overrides,
/// bidi isolates, and the line/paragraph separators.
fn is_format_control(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{2028}' | '\u{2029}'
    )
}

/// Strip ANSI escape sequences (CSI, SGR, OSC, …) from `input` and any
/// residual C0 control bytes (NUL, BEL, BS, DEL, …) that the underlying
/// parser may pass through. LF (`\n`) is preserved; the underlying
/// `strip-ansi-escapes` (vte) also drops TAB and CR as part of its
/// terminal-state-machine handling. The post-filter is defence in
/// depth: if the parser ever leaves a control byte through, it doesn't
/// reach the OS clipboard (some platforms truncate at NUL).
pub fn strip_ansi(input: &str) -> String {
    strip_ansi_escapes::strip_str(input)
        .chars()
        .filter(|c| {
            let code = *c as u32;
            // Drop C0 (0x00..=0x1F) and DEL (0x7F), except 0x0A LF.
            !matches!(code, 0x00..=0x09 | 0x0B..=0x1F | 0x7F)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_control_chars_with_space() {
        assert_eq!(sanitize_log("a\x1bb"), "a b");
        assert_eq!(sanitize_log("a\nb\tc"), "a b c");
    }

    #[test]
    fn preserves_printable_ascii_and_space() {
        assert_eq!(sanitize_log("hello world"), "hello world");
    }

    #[test]
    fn sanitize_log_strips_bidi_and_format_controls() {
        let format_controls = ('\u{200B}'..='\u{200F}')
            .chain('\u{202A}'..='\u{202E}')
            .chain('\u{2066}'..='\u{2069}')
            .chain(['\u{2028}', '\u{2029}']);
        for c in format_controls {
            let input = format!("safe{c}gnp.txt");
            assert_eq!(
                sanitize_log(&input),
                "safe gnp.txt",
                "U+{:04X} must be replaced",
                c as u32
            );
        }
    }

    #[test]
    fn sanitize_log_keeps_ordinary_unicode() {
        let text = "café São Paulo 東京 🦀 ünïcödé";
        assert_eq!(sanitize_log(text), text);
    }
}
