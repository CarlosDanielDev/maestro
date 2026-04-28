//! Tiny line-pair diff renderer for the milestone-health Patch screen (#500).
//!
//! No external diff dependency. Rough but readable: lines that match in
//! sequence are shown unchanged; deletions are prefixed `-`, insertions
//! `+`. Naive longest-common-subsequence — capped to keep memory and
//! stack bounded for adversarial inputs.

/// Hard cap on input size. The LCS table is O(N*M) memory; 500×500 is
/// 1 MB of usize, generous for milestone descriptions and prevents a
/// crafted-large-description DoS.
const MAX_LINES: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    Same(String),
    Removed(String),
    Added(String),
}

pub fn diff_lines(before: &str, after: &str) -> Vec<DiffLine> {
    let a: Vec<&str> = before.lines().take(MAX_LINES).collect();
    let b: Vec<&str> = after.lines().take(MAX_LINES).collect();
    let lcs = lcs_table(&a, &b);
    backtrack_iter(&lcs, &a, &b)
}

fn lcs_table(a: &[&str], b: &[&str]) -> Vec<Vec<usize>> {
    let mut t = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            t[i][j] = if a[i - 1] == b[j - 1] {
                t[i - 1][j - 1] + 1
            } else {
                t[i - 1][j].max(t[i][j - 1])
            };
        }
    }
    t
}

/// Iterative backtrack — walks (i, j) from (a.len(), b.len()) to (0, 0),
/// pushes events in reverse, then reverses once at the end.
fn backtrack_iter(t: &[Vec<usize>], a: &[&str], b: &[&str]) -> Vec<DiffLine> {
    let mut out: Vec<DiffLine> = Vec::new();
    let (mut i, mut j) = (a.len(), b.len());
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
            out.push(DiffLine::Same(a[i - 1].to_string()));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || t[i][j - 1] >= t[i - 1][j]) {
            out.push(DiffLine::Added(b[j - 1].to_string()));
            j -= 1;
        } else if i > 0 {
            out.push(DiffLine::Removed(a[i - 1].to_string()));
            i -= 1;
        }
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_identical_inputs_yields_only_same_lines() {
        let d = diff_lines("alpha\nbeta\ngamma\n", "alpha\nbeta\ngamma\n");
        assert!(d.iter().all(|l| matches!(l, DiffLine::Same(_))));
        assert_eq!(d.len(), 3);
    }

    #[test]
    fn diff_pure_addition_marks_added() {
        let d = diff_lines("alpha\n", "alpha\nbeta\n");
        assert_eq!(d.len(), 2);
        assert!(matches!(&d[0], DiffLine::Same(s) if s == "alpha"));
        assert!(matches!(&d[1], DiffLine::Added(s) if s == "beta"));
    }

    #[test]
    fn diff_pure_removal_marks_removed() {
        let d = diff_lines("alpha\nbeta\n", "alpha\n");
        assert!(
            d.iter()
                .any(|l| matches!(l, DiffLine::Removed(s) if s == "beta"))
        );
    }

    #[test]
    fn diff_replacement_yields_removed_and_added() {
        let d = diff_lines("alpha\nbeta\n", "alpha\ngamma\n");
        assert!(
            d.iter()
                .any(|l| matches!(l, DiffLine::Removed(s) if s == "beta"))
        );
        assert!(
            d.iter()
                .any(|l| matches!(l, DiffLine::Added(s) if s == "gamma"))
        );
    }
}
