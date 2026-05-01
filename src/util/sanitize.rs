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
pub fn sanitize_log(s: &str) -> String {
    s.chars()
        .map(|c| if c == ' ' || !c.is_control() { c } else { ' ' })
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
}
