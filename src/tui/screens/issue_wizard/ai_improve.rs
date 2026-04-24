//! Prompt building and response parsing for the Issue Wizard's AI
//! *improve* companion step (#450). The improve flow asks the model to
//! rewrite the draft using its own critique as guidance, then shows the
//! user a before/after diff for atomic accept/discard.
//!
//! Trusted-seat contract: `issue_type`, `blocked_by`, `milestone`, and
//! `image_paths` are NEVER asked from the model. The parser re-stamps
//! those fields from the caller-supplied `original` payload regardless
//! of what the JSON emits. This keeps the AI strictly inside its lane
//! (the 8 text fields) and prevents hallucinated dependencies or typed
//! drift from leaking into the wizard state.

use super::IssueCreationPayload;
use super::prompt_common::format_payload_for_prompt;
use serde::Deserialize;

/// AI-authored subset of `IssueCreationPayload`: the 8 text fields the
/// model is allowed to rewrite. Everything else (`issue_type`,
/// `blocked_by`, `milestone`, `image_paths`) is a trusted seat that the
/// parser re-stamps from the caller's `original` — the model cannot
/// reach those fields even if it tries.
#[derive(Debug, Deserialize)]
struct ImproveResponse {
    title: String,
    overview: String,
    expected_behavior: String,
    current_behavior: String,
    steps_to_reproduce: String,
    acceptance_criteria: String,
    files_to_modify: String,
    test_hints: String,
}

/// Build the structured prompt sent to `claude --print` for the AI
/// improve step. Embeds the current draft plus the previous critique,
/// asks for a JSON object with exactly 8 string fields.
pub fn build_improve_prompt(payload: &IssueCreationPayload, critique: &str) -> String {
    let mut s = String::new();
    s.push_str(
        "You are rewriting a draft GitHub issue using your own prior critique as guidance.\n",
    );
    s.push_str(
        "Apply the critique. Improve clarity, completeness, testability, and acceptance criteria.\n",
    );
    s.push_str(
        "Preserve the user's intent — do not change the scope or the chosen issue type.\n\n",
    );
    s.push_str(
        "Output ONLY a JSON object with this exact shape, no markdown fences, no commentary:\n",
    );
    s.push_str("{\n");
    s.push_str("  \"title\": \"…\",\n");
    s.push_str("  \"overview\": \"…\",\n");
    s.push_str("  \"expected_behavior\": \"…\",\n");
    s.push_str("  \"current_behavior\": \"…\",\n");
    s.push_str("  \"steps_to_reproduce\": \"…\",\n");
    s.push_str("  \"acceptance_criteria\": \"…\",\n");
    s.push_str("  \"files_to_modify\": \"…\",\n");
    s.push_str("  \"test_hints\": \"…\"\n");
    s.push_str("}\n");
    s.push_str(
        "All eight keys are required. Use empty strings for fields that don't apply (e.g. bug-only fields on a feature issue).\n\n",
    );
    s.push_str("--- PRIOR CRITIQUE ---\n");
    s.push_str(critique.trim());
    s.push_str("\n--- END CRITIQUE ---\n");
    s.push_str("\n--- CURRENT DRAFT ---\n");
    s.push_str(&format!("Title: {}\n", payload.title));
    s.push_str(&format_payload_for_prompt(payload));
    s.push_str("\n--- END DRAFT ---\n");
    s
}

/// Parse the AI's JSON response into an improved `IssueCreationPayload`.
/// Trusted seats (`issue_type`, `blocked_by`, `milestone`, `image_paths`)
/// are re-stamped from `original` regardless of what the response contains.
/// Tolerates markdown fences, surrounding text, and other artefacts via
/// `adapt::prompts::parse_json_response`.
pub fn parse_improve_response(
    original: &IssueCreationPayload,
    raw: &str,
) -> Result<IssueCreationPayload, String> {
    let r: ImproveResponse = crate::adapt::prompts::parse_json_response(raw)
        .map_err(|e| format!("invalid JSON: {e}"))?;
    Ok(IssueCreationPayload {
        title: r.title,
        overview: r.overview,
        expected_behavior: r.expected_behavior,
        current_behavior: r.current_behavior,
        steps_to_reproduce: r.steps_to_reproduce,
        acceptance_criteria: r.acceptance_criteria,
        files_to_modify: r.files_to_modify,
        test_hints: r.test_hints,
        issue_type: original.issue_type,
        blocked_by: original.blocked_by.clone(),
        milestone: original.milestone,
        image_paths: original.image_paths.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::IssueType;
    use super::*;

    fn sample_payload_full() -> IssueCreationPayload {
        IssueCreationPayload {
            issue_type: IssueType::Feature,
            title: "Add gauge widget".into(),
            overview: "Render a horizontal gauge.".into(),
            expected_behavior: "Fills 0–100%.".into(),
            current_behavior: String::new(),
            steps_to_reproduce: String::new(),
            acceptance_criteria: "- Renders correctly\n- Handles overflow".into(),
            files_to_modify: "src/widgets/gauge.rs".into(),
            test_hints: "Test boundary values.".into(),
            blocked_by: vec![10],
            milestone: Some(42),
            image_paths: vec!["/tmp/a.png".into()],
        }
    }

    // ── build_improve_prompt ───────────────────────────────────────────

    #[test]
    fn build_improve_prompt_contains_critique_verbatim() {
        let p = sample_payload_full();
        let critique = "The AC is weak and misses error paths.";
        let out = build_improve_prompt(&p, critique);
        assert!(out.contains(critique));
    }

    #[test]
    fn build_improve_prompt_contains_all_eight_field_headers() {
        let mut p = sample_payload_full();
        p.current_behavior = "Crashes.".into();
        p.steps_to_reproduce = "1. Open".into();
        let out = build_improve_prompt(&p, "critique");
        assert!(out.contains("Title:"));
        assert!(out.contains("## Overview"));
        assert!(out.contains("## Expected Behavior"));
        assert!(out.contains("## Acceptance Criteria"));
        assert!(out.contains("## Files to Modify"));
        assert!(out.contains("## Test Hints"));
    }

    #[test]
    fn build_improve_prompt_omits_trusted_seats() {
        let p = sample_payload_full();
        let out = build_improve_prompt(&p, "c");
        assert!(!out.contains("\"blocked_by\""));
        assert!(!out.contains("\"milestone\""));
        assert!(!out.contains("\"image_paths\""));
        assert!(!out.contains("## Blocked By"));
    }

    #[test]
    fn build_improve_prompt_instructs_json_only_output() {
        let p = sample_payload_full();
        let out = build_improve_prompt(&p, "c");
        assert!(out.contains("JSON"));
        assert!(out.contains("no markdown"));
    }

    #[test]
    fn build_improve_prompt_omits_bug_headers_when_empty() {
        let p = sample_payload_full();
        let out = build_improve_prompt(&p, "c");
        assert!(!out.contains("## Current Behavior"));
        assert!(!out.contains("## Steps to Reproduce"));
    }

    #[test]
    fn build_improve_prompt_includes_bug_headers_when_filled() {
        let mut p = sample_payload_full();
        p.issue_type = IssueType::Bug;
        p.current_behavior = "Crashes.".into();
        p.steps_to_reproduce = "1. Open".into();
        let out = build_improve_prompt(&p, "c");
        assert!(out.contains("## Current Behavior"));
        assert!(out.contains("## Steps to Reproduce"));
    }

    // ── parse_improve_response ─────────────────────────────────────────

    fn valid_json() -> &'static str {
        r#"{
          "title": "New title",
          "overview": "new overview",
          "expected_behavior": "new expected",
          "current_behavior": "",
          "steps_to_reproduce": "",
          "acceptance_criteria": "- new ac",
          "files_to_modify": "new files",
          "test_hints": "new hints"
        }"#
    }

    #[test]
    fn parse_improve_response_accepts_bare_json() {
        let p = sample_payload_full();
        let got = parse_improve_response(&p, valid_json()).unwrap();
        assert_eq!(got.title, "New title");
        assert_eq!(got.overview, "new overview");
        assert_eq!(got.acceptance_criteria, "- new ac");
    }

    #[test]
    fn parse_improve_response_accepts_fenced_json() {
        let p = sample_payload_full();
        let raw = format!("```json\n{}\n```", valid_json());
        let got = parse_improve_response(&p, &raw).unwrap();
        assert_eq!(got.title, "New title");
    }

    #[test]
    fn parse_improve_response_restamps_trusted_seats_from_original() {
        let original = sample_payload_full(); // blocked_by=[10], milestone=Some(42), issue_type=Feature, image_paths=["/tmp/a.png"]
        // Give the model a JSON that tries to overwrite every trusted seat.
        let raw = r#"{
          "title": "t",
          "overview": "o",
          "expected_behavior": "e",
          "current_behavior": "",
          "steps_to_reproduce": "",
          "acceptance_criteria": "ac",
          "files_to_modify": "f",
          "test_hints": "th",
          "blocked_by": [999],
          "milestone": 1,
          "issue_type": "Bug",
          "image_paths": ["/evil.png"]
        }"#;
        let got = parse_improve_response(&original, raw).unwrap();
        assert_eq!(got.blocked_by, vec![10]);
        assert_eq!(got.milestone, Some(42));
        assert_eq!(got.issue_type, IssueType::Feature);
        assert_eq!(got.image_paths, vec!["/tmp/a.png".to_string()]);
    }

    #[test]
    fn parse_improve_response_returns_err_on_missing_required_key() {
        let p = sample_payload_full();
        let raw = r#"{"overview":"o","expected_behavior":"e","current_behavior":"","steps_to_reproduce":"","acceptance_criteria":"ac","files_to_modify":"f","test_hints":"th"}"#;
        assert!(parse_improve_response(&p, raw).is_err());
    }

    #[test]
    fn parse_improve_response_returns_err_on_wrong_type() {
        let p = sample_payload_full();
        let raw = r#"{"title":123,"overview":"o","expected_behavior":"e","current_behavior":"","steps_to_reproduce":"","acceptance_criteria":"ac","files_to_modify":"f","test_hints":"th"}"#;
        assert!(parse_improve_response(&p, raw).is_err());
    }

    #[test]
    fn parse_improve_response_returns_err_on_invalid_json() {
        let p = sample_payload_full();
        assert!(parse_improve_response(&p, "not json at all").is_err());
    }

    #[test]
    fn parse_improve_response_accepts_empty_strings_for_optional_fields() {
        let p = sample_payload_full();
        let got = parse_improve_response(&p, valid_json()).unwrap();
        assert_eq!(got.current_behavior, "");
        assert_eq!(got.steps_to_reproduce, "");
    }
}
