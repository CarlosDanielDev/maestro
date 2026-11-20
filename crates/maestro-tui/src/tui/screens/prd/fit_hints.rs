//! The PRD hint row at any width (#1399): whole hints or none.

use unicode_width::UnicodeWidthStr;

use crate::tui::screens::landing::draw::ellipsize;

/// Keep the leading hints that fit `width` and drop the rest whole, so no
/// hint is cut mid-word (#1399). Hints are split on two spaces; a lone `·`
/// separator is never left dangling at the end. A first hint wider than the
/// row ends with `…`.
pub(super) fn fit_hints(hint: &str, width: usize) -> String {
    if hint.width() <= width {
        return hint.to_string();
    }
    let mut out = String::new();
    for part in hint.split("  ").filter(|p| !p.is_empty()) {
        let next = if out.is_empty() {
            part.to_string()
        } else {
            format!("{out}  {part}")
        };
        if next.width() > width {
            break;
        }
        out = next;
    }
    let out = out.trim_end_matches([' ', '·']).to_string();
    if out.is_empty() {
        ellipsize(hint, width)
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: &str = "[Tab] pane  [j/k] section  [s] save  [R] reset  [Esc] back";

    #[test]
    fn a_row_that_fits_is_unchanged() {
        assert_eq!(fit_hints(ROW, 80), ROW);
    }

    #[test]
    fn hints_that_do_not_fit_drop_whole() {
        assert_eq!(fit_hints(ROW, 45), "[Tab] pane  [j/k] section  [s] save");
    }

    #[test]
    fn a_dangling_separator_is_dropped() {
        let row = "⚠ Press [d] again to CONFIRM delete  ·  any other key cancels";
        assert_eq!(fit_hints(row, 40), "⚠ Press [d] again to CONFIRM delete");
    }

    #[test]
    fn a_first_hint_wider_than_the_row_ends_with_an_ellipsis() {
        assert_eq!(fit_hints("[Tab] pane", 6), "[Tab]…");
    }
}
