//! Parses numbered "next iteration path" suggestions out of free-form
//! adapt session output so the TUI can present them as selectable actions.
//!
//! The expected format is a header (e.g. `## Suggested next iteration paths`)
//! followed by a numbered list:
//! ```text
//! ## Suggested next iteration paths
//!
//! 1. Burn down M0 — start with high-severity tests
//! 2. Fill the empty feature docs — sos.mdx, respiracao.mdx
//! 3. Observability gap — no Sentry/error reporting
//! ```
//!
//! The parser is tolerant:
//! - Any of a few well-known headers qualifies as "start of list"
//! - Numbered items may use `1.` or `1)` separators
//! - Whitespace and blank lines inside an item are preserved but collapsed

/// Headers that indicate the start of a suggestion list.
const HEADER_MARKERS: &[&str] = &[
    "suggested next iteration paths",
    "suggested next steps",
    "next iteration paths",
    "next steps",
    "recommended next actions",
    "what to do next",
];

/// Parse numbered suggestions from adapt session output.
///
/// Returns the trimmed body of each numbered item under the first matching
/// header found. Returns an empty vec when no suggestions section is present.
pub fn parse_suggestions(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let header_start = HEADER_MARKERS.iter().find_map(|m| lower.find(m));
    let Some(start) = header_start else {
        return parse_numbered_items(text);
    };

    // Skip past the header line.
    let after_header = match text[start..].find('\n') {
        Some(nl) => &text[start + nl + 1..],
        None => return Vec::new(),
    };

    parse_numbered_items(after_header)
}

/// Extract numbered list items ("1. foo", "1) foo") from the start of `text`,
/// stopping at the first non-item, non-blank line.
fn parse_numbered_items(text: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut current: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            // Blank line — if we have a current item being built, finish it.
            // If we're between items or before any, keep scanning.
            if let Some(item) = current.take() {
                items.push(item);
            }
            continue;
        }

        if let Some(body) = strip_number_prefix(trimmed) {
            if let Some(prev) = current.take() {
                items.push(prev);
            }
            current = Some(body.to_string());
        } else if let Some(cur) = current.as_mut() {
            // Continuation of the previous numbered item (wrap).
            cur.push(' ');
            cur.push_str(trimmed);
        } else {
            // We haven't started the list yet — ignore preamble lines.
            continue;
        }
    }

    if let Some(item) = current {
        items.push(item);
    }

    // Trim trailing whitespace from each item body.
    items.into_iter().map(|s| s.trim().to_string()).collect()
}

fn strip_number_prefix(s: &str) -> Option<&str> {
    let mut chars = s.char_indices();
    let mut digit_end: Option<usize> = None;
    for (i, c) in chars.by_ref() {
        if c.is_ascii_digit() {
            digit_end = Some(i + c.len_utf8());
        } else {
            break;
        }
    }
    let end = digit_end?;
    let rest = &s[end..];
    // Need a ". " or ") " after the digits.
    if let Some(body) = rest.strip_prefix(". ") {
        Some(body.trim())
    } else if let Some(body) = rest.strip_prefix(") ") {
        Some(body.trim())
    } else {
        None
    }
}

/// Build the prompt for the follow-up session after the user selected a
/// direction from the adapt output.
pub fn build_follow_up_prompt(direction: &str) -> String {
    format!(
        "Based on the previous adapt analysis, execute this direction:\n\n\
         {}\n\n\
         Plan the work into GitHub issues (or milestones when appropriate) \
         and either implement the first step immediately or queue the issues \
         for the /implement flow.",
        direction.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_from_issue_391() {
        let text = "\
Some preamble text that should be ignored.

## Suggested next iteration paths

1. Burn down M0 — start with high-severity tests (#37/#38/#39).
2. Fill the empty feature docs — sos.mdx, respiracao.mdx.
3. Observability gap — no Sentry/error reporting.

Tell me which direction to go and I'll plan it.
";
        let out = parse_suggestions(text);
        assert_eq!(out.len(), 3);
        assert!(out[0].starts_with("Burn down M0"));
        assert!(out[1].starts_with("Fill the empty feature docs"));
        assert!(out[2].starts_with("Observability gap"));
    }

    #[test]
    fn parses_next_steps_header_variant() {
        let text = "## Next steps\n\n1. First action\n2. Second action\n";
        let out = parse_suggestions(text);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn parses_parens_style_numbering() {
        let text = "## Next steps\n\n1) First action\n2) Second action\n";
        let out = parse_suggestions(text);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], "First action");
    }

    #[test]
    fn collapses_wrapped_items_into_single_string() {
        let text = "\
## Suggested next steps

1. Burn down M0 by
   tackling the tests
   one milestone at a time
2. Something else
";
        let out = parse_suggestions(text);
        assert_eq!(out.len(), 2);
        assert!(out[0].contains("Burn down M0"));
        assert!(out[0].contains("tackling the tests"));
        assert!(out[0].contains("one milestone"));
    }

    #[test]
    fn returns_empty_when_no_suggestions_section() {
        let text = "Just some prose without any numbered list of directions.";
        let out = parse_suggestions(text);
        assert!(out.is_empty());
    }

    #[test]
    fn parses_standalone_numbered_list_without_header() {
        // No well-known header, but the text IS just a numbered list —
        // treat it as suggestions.
        let text = "1. Do the thing\n2. Do the other thing\n";
        let out = parse_suggestions(text);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(parse_suggestions("").is_empty());
    }

    #[test]
    fn header_without_items_returns_empty() {
        let text = "## Suggested next iteration paths\n\nNothing to suggest.\n";
        let out = parse_suggestions(text);
        assert!(out.is_empty(), "prose after header is not a numbered item");
    }

    #[test]
    fn parses_case_insensitive_header() {
        let text = "## SUGGESTED NEXT ITERATION PATHS\n\n1. Action one\n";
        let out = parse_suggestions(text);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn build_follow_up_prompt_includes_direction() {
        let prompt = build_follow_up_prompt("Burn down M0");
        assert!(prompt.contains("Burn down M0"));
        assert!(prompt.contains("adapt analysis"));
        assert!(prompt.contains("GitHub issues"));
    }

    #[test]
    fn build_follow_up_prompt_trims_direction() {
        let prompt = build_follow_up_prompt("   Burn down M0   \n");
        assert!(prompt.contains("Burn down M0\n"));
        assert!(!prompt.contains("   Burn down"));
    }

    #[test]
    fn parses_multi_line_wrapping_survives_blank_lines_between_items() {
        let text = "\
## Suggested next steps

1. First

2. Second
";
        let out = parse_suggestions(text);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], "First");
        assert_eq!(out[1], "Second");
    }

    #[test]
    fn items_with_special_chars_preserved() {
        let text = "## Next steps\n\n1. Fix #42 — edge case in parser.rs\n";
        let out = parse_suggestions(text);
        assert_eq!(out[0], "Fix #42 — edge case in parser.rs");
    }
}
