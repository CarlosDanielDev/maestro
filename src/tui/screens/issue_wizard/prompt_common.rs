//! Shared prompt-body formatting for the Issue Wizard's AI-assisted steps
//! (#296 review companion + #450 improve companion). Keeps the payload
//! serialization in one place so adding a DOR field only touches one site.

use super::IssueCreationPayload;

/// Render the 8 DOR text sections of `payload` as `## Header\n<body>` blocks
/// in the canonical order. Bug-only fields (`Current Behavior`,
/// `Steps to Reproduce`) are omitted when their trimmed body is empty.
///
/// Does NOT include scalars like `issue_type`, `title`, `blocked_by`,
/// `milestone`, or `image_paths` — callers prepend/append those as needed.
/// This keeps the improve flow's trusted-seat contract (it must never ask
/// the AI to rewrite `blocked_by` or `milestone`) independent from the
/// review flow's informational inclusion of those scalars.
pub(super) fn format_payload_for_prompt(p: &IssueCreationPayload) -> String {
    let mut s = String::new();
    push_section(&mut s, "Overview", &p.overview);
    push_section(&mut s, "Expected Behavior", &p.expected_behavior);
    if !p.current_behavior.trim().is_empty() {
        push_section(&mut s, "Current Behavior", &p.current_behavior);
    }
    if !p.steps_to_reproduce.trim().is_empty() {
        push_section(&mut s, "Steps to Reproduce", &p.steps_to_reproduce);
    }
    push_section(&mut s, "Acceptance Criteria", &p.acceptance_criteria);
    push_section(&mut s, "Files to Modify", &p.files_to_modify);
    push_section(&mut s, "Test Hints", &p.test_hints);
    s
}

fn push_section(out: &mut String, title: &str, body: &str) {
    out.push('\n');
    out.push_str("## ");
    out.push_str(title);
    out.push('\n');
    out.push_str(body.trim());
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::super::IssueType;
    use super::*;

    fn sample_payload_full() -> IssueCreationPayload {
        IssueCreationPayload {
            issue_type: IssueType::Feature,
            title: "Add gauge widget".into(),
            overview: "Render progress as a horizontal gauge.".into(),
            expected_behavior: "Gauge fills proportionally.".into(),
            current_behavior: String::new(),
            steps_to_reproduce: String::new(),
            acceptance_criteria: "- Renders 0..=100%\n- Handles overflow".into(),
            files_to_modify: "src/widgets/gauge.rs".into(),
            test_hints: "Test boundary values.".into(),
            blocked_by: vec![10],
            milestone: Some(42),
            image_paths: vec![],
        }
    }

    #[test]
    fn format_payload_includes_all_eight_text_fields_when_filled() {
        let mut p = sample_payload_full();
        p.current_behavior = "It crashes.".into();
        p.steps_to_reproduce = "1. open".into();
        let out = format_payload_for_prompt(&p);
        assert!(out.contains("## Overview"));
        assert!(out.contains("## Expected Behavior"));
        assert!(out.contains("## Current Behavior"));
        assert!(out.contains("## Steps to Reproduce"));
        assert!(out.contains("## Acceptance Criteria"));
        assert!(out.contains("## Files to Modify"));
        assert!(out.contains("## Test Hints"));
    }

    #[test]
    fn format_payload_omits_bug_fields_when_empty() {
        let p = sample_payload_full();
        let out = format_payload_for_prompt(&p);
        assert!(!out.contains("## Current Behavior"));
        assert!(!out.contains("## Steps to Reproduce"));
    }

    #[test]
    fn format_payload_omits_scalar_seats() {
        let p = sample_payload_full();
        let out = format_payload_for_prompt(&p);
        assert!(!out.contains("Title:"));
        assert!(!out.contains("Type:"));
        assert!(!out.contains("Blocked By"));
        assert!(!out.contains("milestone"));
    }
}
