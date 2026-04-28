//! Tests for the DOR (Definition of Ready) checker (#500).

use super::*;
use crate::provider::github::types::GhIssue;

fn raw_issue(number: u64, labels: &[&str], body: &str) -> GhIssue {
    GhIssue {
        number,
        title: format!("Issue #{}", number),
        body: body.to_string(),
        labels: labels.iter().map(|s| s.to_string()).collect(),
        state: "open".to_string(),
        html_url: format!("https://github.com/owner/repo/issues/{}", number),
        milestone: Some(1),
        assignees: vec![],
    }
}

fn body_with(sections: &[&str]) -> String {
    let mut out = String::new();
    for &name in sections {
        out.push_str(&format!("## {}\n\n", name));
        match name {
            "Acceptance Criteria" => out.push_str("- [ ] item one\n\n"),
            "Blocked By" => out.push_str("- None\n\n"),
            _ => out.push_str("placeholder text\n\n"),
        }
    }
    out
}

pub(crate) fn make_feature_issue(number: u64, missing: &[&str]) -> GhIssue {
    let kept: Vec<&str> = FEATURE_SECTIONS
        .iter()
        .copied()
        .filter(|s| !missing.contains(s))
        .collect();
    raw_issue(number, &["type:feature"], &body_with(&kept))
}

pub(crate) fn make_bug_issue(number: u64, missing: &[&str]) -> GhIssue {
    let kept: Vec<&str> = BUG_SECTIONS
        .iter()
        .copied()
        .filter(|s| !missing.contains(s))
        .collect();
    raw_issue(number, &["type:bug"], &body_with(&kept))
}

#[test]
fn detect_issue_type_bug_label_wins() {
    let issue = raw_issue(1, &["type:bug"], "");
    assert_eq!(detect_issue_type(&issue), IssueType::Bug);
}

#[test]
fn detect_issue_type_feature_label_wins() {
    let issue = raw_issue(1, &["type:feature"], "");
    assert_eq!(detect_issue_type(&issue), IssueType::Feature);
}

#[test]
fn detect_issue_type_bug_label_beats_feature_label() {
    let issue = raw_issue(1, &["type:bug", "type:feature"], "");
    assert_eq!(detect_issue_type(&issue), IssueType::Bug);
}

#[test]
fn detect_issue_type_body_fallback_steps_to_reproduce_implies_bug() {
    let issue = raw_issue(1, &[], "## Steps to Reproduce\n\n1. do thing\n");
    assert_eq!(detect_issue_type(&issue), IssueType::Bug);
}

#[test]
fn detect_issue_type_default_feature_when_no_signals() {
    let issue = raw_issue(1, &["enhancement"], "## Overview\n\nhi\n");
    assert_eq!(detect_issue_type(&issue), IssueType::Feature);
}

#[test]
fn required_sections_feature_returns_seven_sections() {
    assert_eq!(
        required_sections(IssueType::Feature),
        &[
            "Overview",
            "Expected Behavior",
            "Acceptance Criteria",
            "Files to Modify",
            "Test Hints",
            "Blocked By",
            "Definition of Done",
        ]
    );
}

#[test]
fn required_sections_bug_returns_seven_sections() {
    assert_eq!(
        required_sections(IssueType::Bug),
        &[
            "Overview",
            "Current Behavior",
            "Expected Behavior",
            "Steps to Reproduce",
            "Acceptance Criteria",
            "Blocked By",
            "Definition of Done",
        ]
    );
}

#[test]
fn parse_blocked_by_section_none_when_text_is_none() {
    let body = "## Blocked By\n\n- None\n";
    assert_eq!(parse_blocked_by_section(body), Some(BlockedBySection::None));
}

#[test]
fn parse_blocked_by_section_issues_single() {
    let body = "## Blocked By\n\n- #42 some title\n";
    assert_eq!(
        parse_blocked_by_section(body),
        Some(BlockedBySection::Issues(vec![42]))
    );
}

#[test]
fn parse_blocked_by_section_issues_multiple() {
    let body = "## Blocked By\n\n- #10 a\n- #20 b\n- #30 c\n";
    assert_eq!(
        parse_blocked_by_section(body),
        Some(BlockedBySection::Issues(vec![10, 20, 30]))
    );
}

#[test]
fn parse_blocked_by_section_absent_returns_none() {
    let body = "## Overview\n\nhi\n";
    assert_eq!(parse_blocked_by_section(body), None);
}

#[test]
fn parse_blocked_by_section_weak_when_no_items() {
    let body = "## Blocked By\n\n(pending)\n";
    assert_eq!(parse_blocked_by_section(body), Some(BlockedBySection::Weak));
}

#[test]
fn check_issue_feature_all_sections_passes() {
    let issue = make_feature_issue(1, &[]);
    let result = check_issue(&issue);
    assert!(result.passed());
    assert!(result.missing.is_empty());
}

#[test]
fn check_issue_feature_missing_blocked_by_fails_with_single_field() {
    let issue = make_feature_issue(2, &["Blocked By"]);
    let result = check_issue(&issue);
    assert!(!result.passed());
    assert_eq!(result.missing.len(), 1);
    assert_eq!(result.missing[0], MissingField::Section("Blocked By"));
}

#[test]
fn check_issue_feature_missing_multiple_sections_reports_all() {
    let issue = make_feature_issue(3, &["Acceptance Criteria", "Test Hints", "Files to Modify"]);
    let result = check_issue(&issue);
    assert!(!result.passed());
    assert!(
        result
            .missing
            .contains(&MissingField::Section("Acceptance Criteria"))
    );
    assert!(
        result
            .missing
            .contains(&MissingField::Section("Test Hints"))
    );
    assert!(
        result
            .missing
            .contains(&MissingField::Section("Files to Modify"))
    );
}

#[test]
fn check_issue_bug_all_sections_passes() {
    let issue = make_bug_issue(4, &[]);
    let result = check_issue(&issue);
    assert!(result.passed(), "missing = {:?}", result.missing);
}

#[test]
fn check_issue_bug_missing_steps_to_reproduce_fails() {
    let issue = make_bug_issue(5, &["Steps to Reproduce"]);
    let result = check_issue(&issue);
    assert!(
        result
            .missing
            .contains(&MissingField::Section("Steps to Reproduce"))
    );
}

#[test]
fn check_issue_weak_acceptance_criteria_reported() {
    let body = "## Overview\n\nx\n## Expected Behavior\n\nx\n## Acceptance Criteria\n\nfree prose only\n## Files to Modify\n\nx\n## Test Hints\n\nx\n## Blocked By\n\n- None\n## Definition of Done\n\nx\n";
    let issue = raw_issue(6, &["type:feature"], body);
    let result = check_issue(&issue);
    assert!(
        result
            .missing
            .contains(&MissingField::WeakAcceptanceCriteria)
    );
}

#[test]
fn check_issue_weak_blocked_by_reported() {
    let body = "## Overview\n\nx\n## Expected Behavior\n\nx\n## Acceptance Criteria\n\n- [ ] one\n## Files to Modify\n\nx\n## Test Hints\n\nx\n## Blocked By\n\n(pending)\n## Definition of Done\n\nx\n";
    let issue = raw_issue(7, &["type:feature"], body);
    let result = check_issue(&issue);
    assert!(result.missing.contains(&MissingField::WeakBlockedBy));
}

#[test]
fn check_issues_batch_returns_one_result_per_issue() {
    let issues = vec![
        make_feature_issue(1, &[]),
        make_feature_issue(2, &["Blocked By"]),
        make_bug_issue(3, &["Steps to Reproduce"]),
    ];
    let results = check_issues(&issues);
    assert_eq!(results.len(), 3);
    assert!(results[0].passed());
    assert!(!results[1].passed());
    assert!(!results[2].passed());
}
