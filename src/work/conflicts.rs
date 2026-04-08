use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An issue paired with its expected file modifications.
///
/// `files_to_modify: None` means the issue does not specify which files it
/// will touch (unknown scope). `Some(vec![])` means the issue explicitly
/// declares it touches no files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueWithFiles {
    pub issue_number: u64,
    pub files_to_modify: Option<Vec<String>>,
}

/// A single file path claimed by two or more issues.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileConflict {
    pub file_path: String,
    pub issue_numbers: Vec<u64>,
}

/// Result of pre-launch conflict prediction.
///
/// `is_safe` is `true` only when there are zero file conflicts AND zero
/// unknown-scope issues.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictReport {
    pub conflicts: Vec<FileConflict>,
    pub unknown_scope_issues: Vec<u64>,
    pub is_safe: bool,
}

/// Analyze a set of issues for overlapping `files_to_modify` entries.
///
/// This is a pure function — no I/O, no side effects. It builds a map of
/// `file_path -> Vec<issue_number>` and flags any file claimed by 2+ issues.
pub fn predict_conflicts(issues: &[IssueWithFiles]) -> ConflictReport {
    let mut file_map: HashMap<&str, Vec<u64>> = HashMap::new();
    let mut unknown_scope_issues: Vec<u64> = Vec::new();

    for issue in issues {
        match &issue.files_to_modify {
            None => {
                unknown_scope_issues.push(issue.issue_number);
            }
            Some(files) => {
                for file in files {
                    file_map
                        .entry(file.as_str())
                        .or_default()
                        .push(issue.issue_number);
                }
            }
        }
    }

    let conflicts: Vec<FileConflict> = file_map
        .into_iter()
        .filter(|(_, issues)| issues.len() >= 2)
        .map(|(path, issue_numbers)| FileConflict {
            file_path: path.to_string(),
            issue_numbers,
        })
        .collect();

    let is_safe = conflicts.is_empty() && unknown_scope_issues.is_empty();

    ConflictReport {
        conflicts,
        unknown_scope_issues,
        is_safe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(number: u64, files: Option<Vec<&str>>) -> IssueWithFiles {
        IssueWithFiles {
            issue_number: number,
            files_to_modify: files.map(|v| v.into_iter().map(String::from).collect()),
        }
    }

    fn conflict_for<'a>(report: &'a ConflictReport, path: &str) -> Option<&'a FileConflict> {
        report.conflicts.iter().find(|c| c.file_path == path)
    }

    // -- Core conflict detection --

    #[test]
    fn two_issues_sharing_a_file_produces_conflict() {
        let issues = vec![
            issue(10, Some(vec!["src/tui/app.rs"])),
            issue(20, Some(vec!["src/tui/app.rs"])),
        ];
        let report = predict_conflicts(&issues);

        assert_eq!(report.conflicts.len(), 1, "expected exactly one conflict");
        let c = &report.conflicts[0];
        assert_eq!(c.file_path, "src/tui/app.rs");
        let mut nums = c.issue_numbers.clone();
        nums.sort_unstable();
        assert_eq!(nums, vec![10, 20]);
        assert!(!report.is_safe);
    }

    #[test]
    fn three_issues_with_no_file_overlap_is_safe() {
        let issues = vec![
            issue(1, Some(vec!["src/a.rs"])),
            issue(2, Some(vec!["src/b.rs"])),
            issue(3, Some(vec!["src/c.rs"])),
        ];
        let report = predict_conflicts(&issues);

        assert!(report.conflicts.is_empty(), "expected no conflicts");
        assert!(report.unknown_scope_issues.is_empty());
        assert!(report.is_safe);
    }

    // -- Unknown scope (files_to_modify: None) --

    #[test]
    fn issue_with_none_files_adds_to_unknown_scope() {
        let issues = vec![issue(5, Some(vec!["src/a.rs"])), issue(6, None)];
        let report = predict_conflicts(&issues);

        assert!(report.unknown_scope_issues.contains(&6));
        assert!(!report.is_safe);
    }

    #[test]
    fn is_safe_false_when_unknown_scope_only_no_file_conflicts() {
        let issues = vec![issue(1, Some(vec!["src/foo.rs"])), issue(2, None)];
        let report = predict_conflicts(&issues);

        assert!(
            report.conflicts.is_empty(),
            "no direct file conflicts expected"
        );
        assert!(report.unknown_scope_issues.contains(&2));
        assert!(!report.is_safe);
    }

    #[test]
    fn all_issues_with_none_files_all_go_to_unknown_scope() {
        let issues = vec![issue(1, None), issue(2, None), issue(3, None)];
        let report = predict_conflicts(&issues);

        let mut scope = report.unknown_scope_issues.clone();
        scope.sort_unstable();
        assert_eq!(scope, vec![1, 2, 3]);
        assert!(report.conflicts.is_empty());
        assert!(!report.is_safe);
    }

    // -- Empty files list (Some(vec![])) --

    #[test]
    fn issue_with_empty_files_list_produces_no_conflict() {
        let issues = vec![issue(7, Some(vec![])), issue(8, Some(vec!["src/b.rs"]))];
        let report = predict_conflicts(&issues);

        assert!(
            report.conflicts.is_empty(),
            "empty file list must not conflict"
        );
        assert!(report.unknown_scope_issues.is_empty());
        assert!(report.is_safe);
    }

    // -- Partial overlap --

    #[test]
    fn conflict_detected_only_between_overlapping_issues() {
        let issues = vec![
            issue(1, Some(vec!["src/x.rs"])),
            issue(2, Some(vec!["src/y.rs"])),
            issue(3, Some(vec!["src/x.rs", "src/z.rs"])),
        ];
        let report = predict_conflicts(&issues);

        let x_conflict = conflict_for(&report, "src/x.rs").expect("src/x.rs must be in conflicts");
        let mut x_nums = x_conflict.issue_numbers.clone();
        x_nums.sort_unstable();
        assert_eq!(x_nums, vec![1, 3], "only issues 1 and 3 share src/x.rs");

        assert!(
            conflict_for(&report, "src/y.rs").is_none(),
            "src/y.rs is unique to issue 2"
        );
        assert!(
            conflict_for(&report, "src/z.rs").is_none(),
            "src/z.rs is unique to issue 3"
        );

        assert!(!report.is_safe);
    }

    // -- Edge cases --

    #[test]
    fn empty_input_returns_safe_report_with_no_conflicts() {
        let report = predict_conflicts(&[]);

        assert!(report.conflicts.is_empty());
        assert!(report.unknown_scope_issues.is_empty());
        assert!(report.is_safe);
    }

    #[test]
    fn single_issue_returns_safe_report() {
        let issues = vec![issue(99, Some(vec!["src/main.rs"]))];
        let report = predict_conflicts(&issues);

        assert!(report.conflicts.is_empty());
        assert!(report.unknown_scope_issues.is_empty());
        assert!(report.is_safe);
    }

    // -- Multi-file / multi-issue stress --

    #[test]
    fn multiple_files_with_multiple_conflicts_across_multiple_issues() {
        let issues = vec![
            issue(1, Some(vec!["file_a"])),
            issue(2, Some(vec!["file_a", "file_b"])),
            issue(3, Some(vec!["file_b"])),
            issue(4, Some(vec!["file_b", "file_c"])),
        ];
        let report = predict_conflicts(&issues);

        assert_eq!(report.conflicts.len(), 2, "expected two conflicting files");

        let a = conflict_for(&report, "file_a").expect("file_a must be a conflict");
        let mut a_nums = a.issue_numbers.clone();
        a_nums.sort_unstable();
        assert_eq!(a_nums, vec![1, 2]);

        let b = conflict_for(&report, "file_b").expect("file_b must be a conflict");
        let mut b_nums = b.issue_numbers.clone();
        b_nums.sort_unstable();
        assert_eq!(b_nums, vec![2, 3, 4]);

        assert!(
            conflict_for(&report, "file_c").is_none(),
            "file_c is touched by only one issue"
        );

        assert!(report.unknown_scope_issues.is_empty());
        assert!(!report.is_safe);
    }
}
