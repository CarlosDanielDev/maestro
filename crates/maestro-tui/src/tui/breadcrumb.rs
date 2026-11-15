//! Breadcrumb-trail elision helper.
//!
//! When the nav-stack trail would overflow the available header width,
//! collapse the middle segments to `…` while keeping the anchor (first)
//! and the deepest two segments (parent + current). This preserves the
//! user's sense of place without truncating the rightmost crumb, which
//! is the one that names the current screen (#867).

/// Elide a breadcrumb label list to fit `max_width` characters.
///
/// The separator (e.g. `" > "`) is counted between every pair of labels.
/// When the joined width exceeds `max_width`, the middle segments are
/// replaced by a single `…` crumb, preserving `first > … > parent > current`.
///
/// Short stacks (≤ 3 labels) are returned unchanged — there is nothing
/// meaningful to elide.
pub fn elide_breadcrumbs(labels: &[&str], max_width: u16, sep: &str) -> Vec<String> {
    if labels.is_empty() {
        return Vec::new();
    }
    let max_w = max_width as usize;
    if joined_width(labels, sep) <= max_w {
        return labels.iter().map(|s| (*s).to_string()).collect();
    }
    if labels.len() <= 3 {
        return labels.iter().map(|s| (*s).to_string()).collect();
    }
    let first = labels[0];
    let last_two = &labels[labels.len() - 2..];
    let mut out = vec![first.to_string(), "…".to_string()];
    for s in last_two {
        out.push((*s).to_string());
    }
    out
}

fn joined_width(labels: &[&str], sep: &str) -> usize {
    let labels_w: usize = labels.iter().map(|s| s.chars().count()).sum();
    let seps_w = sep.chars().count() * labels.len().saturating_sub(1);
    labels_w + seps_w
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEP: &str = " > ";

    #[test]
    fn empty_list_returns_empty() {
        let out = elide_breadcrumbs(&[], 80, SEP);
        assert!(out.is_empty());
    }

    #[test]
    fn single_label_returned_unchanged() {
        let out = elide_breadcrumbs(&["Dashboard"], 80, SEP);
        assert_eq!(out, vec!["Dashboard"]);
    }

    #[test]
    fn fits_returns_all_labels_unchanged() {
        let out = elide_breadcrumbs(&["Welcome", "Dashboard", "Overview"], 80, SEP);
        assert_eq!(out, vec!["Welcome", "Dashboard", "Overview"]);
    }

    #[test]
    fn overflows_with_more_than_three_elides_middle() {
        let out = elide_breadcrumbs(
            &["Welcome", "Dashboard", "Overview", "Summary", "Logs"],
            10,
            SEP,
        );
        assert_eq!(out, vec!["Welcome", "…", "Summary", "Logs"]);
    }

    #[test]
    fn three_label_stack_returned_unchanged_even_when_overflows() {
        let out = elide_breadcrumbs(&["A", "B", "C"], 1, SEP);
        assert_eq!(out, vec!["A", "B", "C"]);
    }

    #[test]
    fn five_label_stack_fits_at_wide_width() {
        let out = elide_breadcrumbs(
            &["Welcome", "Dashboard", "Overview", "Summary", "Logs"],
            80,
            SEP,
        );
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn elision_preserves_first_and_last_two() {
        let out = elide_breadcrumbs(&["A", "B", "C", "D", "E", "F", "G"], 10, SEP);
        assert_eq!(out.first().map(String::as_str), Some("A"));
        assert_eq!(out.last().map(String::as_str), Some("G"));
        assert_eq!(out.get(out.len() - 2).map(String::as_str), Some("F"));
        assert!(out.iter().any(|s| s == "…"));
    }
}
