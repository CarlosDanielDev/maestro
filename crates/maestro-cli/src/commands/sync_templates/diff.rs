//! Minimal hand-rolled line diff for `--check` drift reporting.
//!
//! Mirrors the format used by `tests/templates_render.rs::assert_byte_identical`:
//! one line per change, `L<n> -` for expected and `L<n> +` for actual.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

pub fn line_diff(expected: &str, actual: &str) -> String {
    if expected == actual {
        return String::new();
    }
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let max_len = expected_lines.len().max(actual_lines.len());
    let mut out = String::new();
    for i in 0..max_len {
        let e = expected_lines.get(i).copied();
        let a = actual_lines.get(i).copied();
        match (e, a) {
            (Some(ev), Some(av)) if ev != av => {
                out.push_str(&format!("L{} - {ev}\n", i + 1));
                out.push_str(&format!("L{} + {av}\n", i + 1));
            }
            (None, Some(av)) => out.push_str(&format!("L{} + {av}\n", i + 1)),
            (Some(ev), None) => out.push_str(&format!("L{} - {ev}\n", i + 1)),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_diff_returns_empty_string_for_identical_inputs() {
        let diff = line_diff("line one\nline two\n", "line one\nline two\n");
        assert_eq!(diff, "");
    }

    #[test]
    fn line_diff_marks_changed_line_with_minus_and_plus() {
        let diff = line_diff("expected\n", "actual\n");
        assert!(diff.contains("L1 - expected"), "got: {diff}");
        assert!(diff.contains("L1 + actual"), "got: {diff}");
    }

    #[test]
    fn line_diff_marks_appended_lines_with_plus_only() {
        let diff = line_diff("line one\n", "line one\nline two\n");
        assert!(!diff.contains("L1"), "line 1 is identical, must not appear");
        assert!(diff.contains("L2 + line two"), "got: {diff}");
        assert!(!diff.contains("L2 -"), "no minus for an added line");
    }

    #[test]
    fn line_diff_marks_removed_lines_with_minus_only() {
        let diff = line_diff("line one\nline two\n", "line one\n");
        assert!(!diff.contains("L1"), "line 1 is identical");
        assert!(diff.contains("L2 - line two"), "got: {diff}");
        assert!(!diff.contains("L2 +"), "no plus for a removed line");
    }

    #[test]
    fn line_diff_uses_one_based_line_numbers() {
        let diff = line_diff("changed\n", "also changed\n");
        assert!(
            diff.contains("L1 "),
            "first line must be L1, not L0: {diff}"
        );
        assert!(!diff.contains("L0 "), "zero-based must not appear: {diff}");
    }
}
