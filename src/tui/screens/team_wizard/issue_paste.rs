//! Pure helper for parsing pasted clipboard content into an issue number
//! buffer for the Launch IssuePicker step (#875).

/// Extract a numeric issue token from clipboard content. Accepts:
///   "42", "#42", "gh-42", "GH-42", "  42  ",
///   "https://github.com/owner/repo/issues/42",
///   "https://github.com/owner/repo/issues/42\nfeat: ...".
/// Returns None for empty / non-numeric input. Truncates to 10 digits.
pub(super) fn parse_pasted_issue_token(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let stripped = trimmed
        .strip_prefix("gh-")
        .or_else(|| trimmed.strip_prefix("GH-"))
        .or_else(|| trimmed.strip_prefix("#"))
        .unwrap_or(trimmed);
    let candidate = stripped.rsplit('/').next().unwrap_or(stripped);
    let digits: String = candidate
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .take(10)
        .collect();
    if digits.is_empty() {
        None
    } else {
        Some(digits)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_pasted_issue_token;

    #[test]
    fn accepts_plain_digits() {
        assert_eq!(parse_pasted_issue_token("42"), Some("42".to_string()));
    }

    #[test]
    fn accepts_hash_prefix() {
        assert_eq!(parse_pasted_issue_token("#42"), Some("42".to_string()));
    }

    #[test]
    fn accepts_gh_prefix_lowercase() {
        assert_eq!(parse_pasted_issue_token("gh-42"), Some("42".to_string()));
    }

    #[test]
    fn accepts_gh_prefix_uppercase() {
        assert_eq!(parse_pasted_issue_token("GH-42"), Some("42".to_string()));
    }

    #[test]
    fn accepts_github_url() {
        assert_eq!(
            parse_pasted_issue_token("https://github.com/owner/repo/issues/42"),
            Some("42".to_string())
        );
    }

    #[test]
    fn rejects_non_numeric() {
        assert_eq!(parse_pasted_issue_token("abc"), None);
    }

    #[test]
    fn rejects_empty_string() {
        assert_eq!(parse_pasted_issue_token(""), None);
    }

    #[test]
    fn truncates_to_ten_digits() {
        assert_eq!(
            parse_pasted_issue_token("12345678901234"),
            Some("1234567890".to_string())
        );
    }

    #[test]
    fn handles_multiline_url() {
        assert_eq!(
            parse_pasted_issue_token("https://github.com/owner/repo/issues/42\nfeat: something"),
            Some("42".to_string())
        );
    }

    #[test]
    fn trims_leading_and_trailing_whitespace() {
        assert_eq!(parse_pasted_issue_token("  42  "), Some("42".to_string()));
    }
}
