//! Unit tests for the diff reviewer (#918) — split from `diff_review.rs`
//! to keep it under the 400-line guardrail.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crossterm::event::{KeyCode, KeyModifiers};

const SAMPLE: &str = "\
diff --git a/src/a.rs b/src/a.rs
index 111..222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!(\"hi\");
 }
@@ -10,2 +11,1 @@
-    old();
 fn tail() {}
diff --git a/src/b.rs b/src/b.rs
index 333..444 100644
--- a/src/b.rs
+++ b/src/b.rs
@@ -1,1 +1,1 @@
-fn b_old() {}
+fn b_new() {}
";

#[test]
fn parse_splits_files_and_counts_lines() {
    let files = parse_unified_diff(SAMPLE);
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, "src/a.rs");
    assert_eq!(files[0].adds, 1);
    assert_eq!(files[0].dels, 1);
    assert_eq!(files[1].path, "src/b.rs");
    assert_eq!(files[1].adds, 1);
    assert_eq!(files[1].dels, 1);
}

#[test]
fn parse_empty_diff_yields_no_files() {
    assert!(parse_unified_diff("").is_empty());
}

#[test]
fn hunk_jump_moves_between_markers() {
    let mut review = DiffReview::new(SAMPLE);
    review.viewport = 5;
    assert_eq!(review.scroll, 0);
    review.handle_key(KeyCode::Char(']'), KeyModifiers::NONE);
    let first_hunk = review.scroll;
    assert!(
        review.current_lines()[first_hunk].kind == DiffLineKind::Hunk,
        "lands on a hunk marker"
    );
    review.handle_key(KeyCode::Char(']'), KeyModifiers::NONE);
    assert!(review.scroll > first_hunk, "advances to the second hunk");
    review.handle_key(KeyCode::Char('['), KeyModifiers::NONE);
    assert_eq!(review.scroll, first_hunk, "jumps back");
}

#[test]
fn tab_cycles_files_and_resets_scroll() {
    let mut review = DiffReview::new(SAMPLE);
    review.scroll = 3;
    review.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(review.selected, 1);
    assert_eq!(review.scroll, 0);
    review.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(review.selected, 0, "wraps");
    review.handle_key(KeyCode::BackTab, KeyModifiers::NONE);
    assert_eq!(review.selected, 1, "BackTab cycles back");
}

#[test]
fn search_jumps_to_match_and_n_advances() {
    let mut review = DiffReview::new(SAMPLE);
    review.viewport = 3;
    review.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    for c in "fn".chars() {
        review.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
    }
    review.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    let first = review.scroll;
    assert!(review.current_lines()[first].text.contains("fn"));
    review.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
    assert!(review.scroll > first, "n advances to the next match");
    review.handle_key(KeyCode::Char('N'), KeyModifiers::NONE);
    assert_eq!(review.scroll, first, "N goes back");
}

#[test]
fn close_and_shell_outcomes() {
    let mut review = DiffReview::new(SAMPLE);
    assert_eq!(
        review.handle_key(KeyCode::Char('q'), KeyModifiers::NONE),
        DiffReviewOutcome::Close
    );
    assert_eq!(
        review.handle_key(KeyCode::Esc, KeyModifiers::NONE),
        DiffReviewOutcome::Close
    );
    assert_eq!(
        review.handle_key(KeyCode::Char('o'), KeyModifiers::NONE),
        DiffReviewOutcome::OpenShell
    );
}

#[test]
fn motions_are_read_only_surface() {
    // No key mutates the parsed diff — the only state that moves is
    // scroll/selection/search.
    let mut review = DiffReview::new(SAMPLE);
    let before: Vec<usize> = review.files.iter().map(|f| f.lines.len()).collect();
    for code in [
        KeyCode::Char('j'),
        KeyCode::Char('k'),
        KeyCode::Char('g'),
        KeyCode::Char('G'),
        KeyCode::Char(']'),
        KeyCode::Char('['),
        KeyCode::Tab,
    ] {
        review.handle_key(code, KeyModifiers::NONE);
    }
    let after: Vec<usize> = review.files.iter().map(|f| f.lines.len()).collect();
    assert_eq!(before, after);
}
