//! Issue reference detection and highlighting for prompt text.
//!
//! Detects `#NNN` patterns (e.g., `#42`, `#123`) in text, avoiding false
//! positives like `C#` or `F#`. Provides span builders for ratatui highlighting.

use std::sync::LazyLock;

use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};
use regex::Regex;

/// A detected issue reference in text, with byte-offset span info.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueRef {
    pub number: u64,
    pub start: usize,
    pub end: usize,
}

/// Regex: `#` followed by a non-zero digit then 0-8 more digits.
/// Rejects `#0`, `#007`, and caps at 999_999_999.
static ISSUE_REF_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#([1-9]\d{0,8})").unwrap());

/// Extract all issue references from text, filtering false positives.
///
/// A `#NNN` is rejected if preceded by an alphanumeric or underscore character
/// (catches `C#`, `F#123`, `var_#5`).
pub fn extract_issue_refs(text: &str) -> Vec<IssueRef> {
    ISSUE_REF_RE
        .find_iter(text)
        .filter_map(|m| {
            let start = m.start();
            if start > 0 {
                let prev = text.as_bytes()[start - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    return None;
                }
            }
            let digits = &text[start + 1..m.end()];
            let number = digits.parse::<u64>().ok()?;
            Some(IssueRef {
                number,
                start,
                end: m.end(),
            })
        })
        .collect()
}

/// Extract unique issue numbers (deduplicated, order preserved).
pub fn extract_issue_numbers(text: &str) -> Vec<u64> {
    let mut seen = std::collections::HashSet::new();
    extract_issue_refs(text)
        .into_iter()
        .filter_map(|r| {
            if seen.insert(r.number) {
                Some(r.number)
            } else {
                None
            }
        })
        .collect()
}

/// Build a `Vec<Span>` from text, highlighting issue references with accent color + bold.
pub fn highlight_issue_refs<'a>(
    text: &'a str,
    accent_color: Color,
    normal_color: Color,
) -> Vec<Span<'a>> {
    let refs = extract_issue_refs(text);
    if refs.is_empty() {
        return vec![Span::styled(text, Style::default().fg(normal_color))];
    }

    let mut spans = Vec::new();
    let mut last_end = 0;

    for issue_ref in &refs {
        if issue_ref.start > last_end {
            spans.push(Span::styled(
                &text[last_end..issue_ref.start],
                Style::default().fg(normal_color),
            ));
        }
        spans.push(Span::styled(
            &text[issue_ref.start..issue_ref.end],
            Style::default()
                .fg(accent_color)
                .add_modifier(Modifier::BOLD),
        ));
        last_end = issue_ref.end;
    }

    if last_end < text.len() {
        spans.push(Span::styled(
            &text[last_end..],
            Style::default().fg(normal_color),
        ));
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    // --- extract_issue_refs ---

    #[test]
    fn basic_extraction() {
        let refs = extract_issue_refs("Fix #42 and #100");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].number, 42);
        assert_eq!(refs[1].number, 100);
    }

    #[test]
    fn false_positive_c_sharp() {
        let refs = extract_issue_refs("C# language");
        assert!(refs.is_empty());
    }

    #[test]
    fn false_positive_f_sharp_with_number() {
        let refs = extract_issue_refs("F#123");
        assert!(refs.is_empty());
    }

    #[test]
    fn underscore_prefix_rejected() {
        let refs = extract_issue_refs("var_#5");
        assert!(refs.is_empty());
    }

    #[test]
    fn hash_zero_rejected() {
        let refs = extract_issue_refs("#0 is not valid");
        assert!(refs.is_empty());
    }

    #[test]
    fn leading_zero_rejected() {
        let refs = extract_issue_refs("#007 is spy");
        // #007 matches as #0 (rejected) — the regex requires [1-9] first digit
        assert!(refs.is_empty());
    }

    #[test]
    fn hash_at_start_of_text() {
        let refs = extract_issue_refs("#1 is first");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].number, 1);
    }

    #[test]
    fn large_number_within_limit() {
        let refs = extract_issue_refs("#999999999");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].number, 999_999_999);
    }

    #[test]
    fn ten_digit_number_not_matched() {
        // 10 digits: regex only allows up to 9
        let refs = extract_issue_refs("#9999999999");
        // The regex matches #999999999 (first 9 digits), the trailing 9 is left over
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].number, 999_999_999);
    }

    #[test]
    fn empty_string() {
        let refs = extract_issue_refs("");
        assert!(refs.is_empty());
    }

    #[test]
    fn no_hash() {
        let refs = extract_issue_refs("no issue here");
        assert!(refs.is_empty());
    }

    #[test]
    fn mixed_content() {
        let refs = extract_issue_refs("See #10, not C#, then #20");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].number, 10);
        assert_eq!(refs[1].number, 20);
    }

    #[test]
    fn span_offsets_are_correct() {
        let text = "fix #42 now";
        let refs = extract_issue_refs(text);
        assert_eq!(refs[0].start, 4);
        assert_eq!(refs[0].end, 7);
        assert_eq!(&text[refs[0].start..refs[0].end], "#42");
    }

    // --- extract_issue_numbers ---

    #[test]
    fn deduplication() {
        let nums = extract_issue_numbers("#5 and #5");
        assert_eq!(nums, vec![5]);
    }

    #[test]
    fn multiple_unique() {
        let nums = extract_issue_numbers("#10 #20 #30");
        assert_eq!(nums, vec![10, 20, 30]);
    }

    #[test]
    fn dedup_preserves_order() {
        let nums = extract_issue_numbers("#3 #1 #3 #2");
        assert_eq!(nums, vec![3, 1, 2]);
    }

    // --- highlight_issue_refs ---

    #[test]
    fn highlight_no_refs_returns_single_span() {
        let spans = highlight_issue_refs("hello world", Color::Cyan, Color::White);
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn highlight_single_ref() {
        let spans = highlight_issue_refs("fix #42 now", Color::Cyan, Color::White);
        assert_eq!(spans.len(), 3); // "fix " + "#42" + " now"
        assert_eq!(spans[1].content.as_ref(), "#42");
    }

    #[test]
    fn highlight_ref_has_bold_modifier() {
        let spans = highlight_issue_refs("#1", Color::Cyan, Color::White);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn highlight_multiple_refs() {
        let spans = highlight_issue_refs("#1 and #2", Color::Cyan, Color::White);
        // "#1" + " and " + "#2"
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "#1");
        assert_eq!(spans[1].content.as_ref(), " and ");
        assert_eq!(spans[2].content.as_ref(), "#2");
    }

    #[test]
    fn hash_in_parentheses() {
        let refs = extract_issue_refs("(#42)");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].number, 42);
    }

    #[test]
    fn hash_after_newline() {
        let refs = extract_issue_refs("line1\n#42");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].number, 42);
    }
}
