//! String formatting and truncation helpers.

/// Find the largest byte offset <= max_bytes that is a valid char boundary.
pub fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> usize {
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
