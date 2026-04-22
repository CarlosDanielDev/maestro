//! String formatting and truncation helpers.

/// Format a token count for display (e.g., "245.0k", "1.2M").
pub fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

/// Find the largest byte offset <= max_bytes that is a valid char boundary.
pub const fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> usize {
    if s.len() <= max_bytes {
        return s.len();
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    end
}

/// Truncate a string at a char boundary and append "..." if it was truncated.
pub fn truncate_with_ellipsis(s: &str, max_bytes: usize) -> String {
    let end = truncate_at_char_boundary(s, max_bytes);
    if end < s.len() {
        format!("{}...", &s[..end])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_tokens_small_numbers() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
    }

    #[test]
    fn format_tokens_thousands() {
        assert_eq!(format_tokens(1000), "1.0k");
        assert_eq!(format_tokens(45000), "45.0k");
    }

    #[test]
    fn format_tokens_millions() {
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(2_500_000), "2.5M");
    }
}
