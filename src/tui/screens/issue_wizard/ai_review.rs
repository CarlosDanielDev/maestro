//! Prompt building for the Issue Wizard's AI review companion step (#296).
//! Pure string helpers, no I/O — keeps the prompt logic unit-testable.

use super::IssueCreationPayload;
use super::prompt_common::format_payload_for_prompt;

/// Build the structured prompt sent to `claude --print` for the AI
/// review step. The model is asked to critique completeness, testability,
/// missing edge cases, and suggest non-goals.
pub fn build_review_prompt(payload: &IssueCreationPayload) -> String {
    let mut s = String::new();
    s.push_str("You are reviewing a draft GitHub issue for completeness and quality.\n");
    s.push_str("Output a concise, bulleted critique covering:\n");
    s.push_str("  1. Missing or unclear acceptance criteria\n");
    s.push_str("  2. Edge cases the author may have overlooked\n");
    s.push_str("  3. Scope concerns and suggested non-goals\n");
    s.push_str("  4. Testability gaps\n\n");
    s.push_str("--- DRAFT ISSUE ---\n");
    s.push_str(&format!("Type: {:?}\n", payload.issue_type));
    s.push_str(&format!("Title: {}\n", payload.title));
    s.push_str(&format_payload_for_prompt(payload));
    if !payload.blocked_by.is_empty() {
        let refs: Vec<String> = payload
            .blocked_by
            .iter()
            .map(|n| format!("#{}", n))
            .collect();
        s.push('\n');
        s.push_str("## Blocked By\n");
        s.push_str(&refs.join(", "));
        s.push('\n');
    }
    s.push_str("\n--- END DRAFT ---\n");
    s
}

#[cfg(test)]
mod tests {
    use super::super::IssueType;
    use super::*;

    fn sample_payload() -> IssueCreationPayload {
        IssueCreationPayload {
            issue_type: IssueType::Feature,
            title: "Add gauge widget".into(),
            overview: "Render progress as a horizontal gauge.".into(),
            expected_behavior: "Gauge fills proportionally to value/max.".into(),
            acceptance_criteria: "- Renders 0..=100%\n- Handles overflow".into(),
            files_to_modify: "src/widgets/gauge.rs".into(),
            test_hints: "Test boundary values 0 and 100.".into(),
            blocked_by: vec![10, 11],
            ..Default::default()
        }
    }

    #[test]
    fn prompt_includes_all_dor_sections() {
        let p = sample_payload();
        let prompt = build_review_prompt(&p);
        assert!(prompt.contains("Add gauge widget"));
        assert!(prompt.contains("## Overview"));
        assert!(prompt.contains("## Expected Behavior"));
        assert!(prompt.contains("## Acceptance Criteria"));
        assert!(prompt.contains("## Files to Modify"));
        assert!(prompt.contains("## Test Hints"));
        assert!(prompt.contains("## Blocked By"));
        assert!(prompt.contains("#10"));
    }

    #[test]
    fn prompt_omits_bug_only_sections_for_feature() {
        let p = sample_payload();
        let prompt = build_review_prompt(&p);
        assert!(!prompt.contains("## Current Behavior"));
        assert!(!prompt.contains("## Steps to Reproduce"));
    }

    #[test]
    fn prompt_includes_bug_only_sections_when_filled() {
        let mut p = sample_payload();
        p.issue_type = IssueType::Bug;
        p.current_behavior = "It crashes.".into();
        p.steps_to_reproduce = "1. open\n2. crash".into();
        let prompt = build_review_prompt(&p);
        assert!(prompt.contains("## Current Behavior"));
        assert!(prompt.contains("## Steps to Reproduce"));
    }

    #[test]
    fn prompt_omits_blocked_by_section_when_empty() {
        let mut p = sample_payload();
        p.blocked_by.clear();
        let prompt = build_review_prompt(&p);
        assert!(!prompt.contains("## Blocked By"));
    }
}
