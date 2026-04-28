//! DOR (Definition of Ready) checker for one issue (#500).

use crate::milestone_health::types::{BlockedBySection, DorResult, IssueType, MissingField};
use crate::provider::github::types::GhIssue;

const FEATURE_SECTIONS: &[&str] = &[
    "Overview",
    "Expected Behavior",
    "Acceptance Criteria",
    "Files to Modify",
    "Test Hints",
    "Blocked By",
    "Definition of Done",
];

const BUG_SECTIONS: &[&str] = &[
    "Overview",
    "Current Behavior",
    "Expected Behavior",
    "Steps to Reproduce",
    "Acceptance Criteria",
    "Blocked By",
    "Definition of Done",
];

pub fn required_sections(t: IssueType) -> &'static [&'static str] {
    match t {
        IssueType::Feature => FEATURE_SECTIONS,
        IssueType::Bug => BUG_SECTIONS,
    }
}

pub fn detect_issue_type(issue: &GhIssue) -> IssueType {
    if issue.labels.iter().any(|l| l == "type:bug") {
        return IssueType::Bug;
    }
    if issue.labels.iter().any(|l| l == "type:feature") {
        return IssueType::Feature;
    }
    if has_section(&issue.body, "Steps to Reproduce") {
        return IssueType::Bug;
    }
    IssueType::Feature
}

pub fn check_issue(issue: &GhIssue) -> DorResult {
    let issue_type = detect_issue_type(issue);
    let mut missing: Vec<MissingField> = Vec::new();

    for &section in required_sections(issue_type) {
        if !has_section(&issue.body, section) {
            missing.push(MissingField::Section(section));
        }
    }

    if has_section(&issue.body, "Acceptance Criteria") && !acceptance_criteria_strong(&issue.body) {
        missing.push(MissingField::WeakAcceptanceCriteria);
    }

    if has_section(&issue.body, "Blocked By")
        && let Some(BlockedBySection::Weak) = parse_blocked_by_section(&issue.body)
    {
        missing.push(MissingField::WeakBlockedBy);
    }

    DorResult {
        issue_number: issue.number,
        issue_type,
        missing,
    }
}

pub fn check_issues(issues: &[GhIssue]) -> Vec<DorResult> {
    issues.iter().map(check_issue).collect()
}

/// Return `Some(BlockedBySection::*)` when the body has a `## Blocked By`
/// heading; `None` if absent.
pub fn parse_blocked_by_section(body: &str) -> Option<BlockedBySection> {
    let content = section_body(body, "Blocked By")?;
    let mut numbers: Vec<u64> = Vec::new();
    let mut saw_none = false;
    let mut saw_item = false;

    for raw in content.lines() {
        let line = raw.trim_start();
        if let Some(rest) = line.strip_prefix("- ") {
            saw_item = true;
            let stripped = rest.trim();
            if stripped.eq_ignore_ascii_case("none") {
                saw_none = true;
                continue;
            }
            // Match `#NNN ...`
            if let Some(rest) = stripped.strip_prefix('#') {
                let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = digits.parse::<u64>() {
                    numbers.push(n);
                    continue;
                }
            }
        }
    }

    if !numbers.is_empty() {
        return Some(BlockedBySection::Issues(numbers));
    }
    if saw_none {
        return Some(BlockedBySection::None);
    }
    // Heading present, but neither `- None` nor `- #N` items.
    let _ = saw_item;
    Some(BlockedBySection::Weak)
}

fn has_section(body: &str, heading: &str) -> bool {
    body.lines()
        .any(|line| line.trim_end() == format!("## {}", heading))
}

/// Return the body of `## <heading>` up to the next `## ` heading or EOF.
/// Returns `None` if the heading is absent.
fn section_body<'a>(body: &'a str, heading: &str) -> Option<&'a str> {
    let target = format!("## {}", heading);
    let mut start: Option<usize> = None;
    let mut end: Option<usize> = None;
    let mut cursor: usize = 0;
    let mut after_heading = false;

    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n');
        if start.is_none() && trimmed.trim_end() == target {
            after_heading = true;
            // Body starts after this line.
            start = Some(cursor + line.len());
        } else if after_heading && trimmed.starts_with("## ") {
            end = Some(cursor);
            break;
        }
        cursor += line.len();
    }

    let s = start?;
    let e = end.unwrap_or(body.len());
    Some(&body[s..e])
}

/// Returns true when the `## Acceptance Criteria` section has at least one
/// `- [ ]` checkbox item.
fn acceptance_criteria_strong(body: &str) -> bool {
    let Some(content) = section_body(body, "Acceptance Criteria") else {
        return false;
    };
    content
        .lines()
        .any(|l| l.trim_start().starts_with("- [ ]") || l.trim_start().starts_with("- [x]"))
}

#[cfg(test)]
mod tests;
