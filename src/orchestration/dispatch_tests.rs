use super::*;
use crate::orchestration::contracts::{Finding, FindingSeverity, ReviewVerdict, SubagentResult};

#[test]
fn compose_prompt_all_sections_exact_match() {
    let out = compose_prompt("SYSTEM", Some("ADDENDUM"), "INSTRUCTIONS");
    assert_eq!(out, "SYSTEM\n\nADDENDUM\n\nINSTRUCTIONS");
}

#[test]
fn compose_prompt_no_addendum_empty_mode_returns_instructions_only() {
    let out = compose_prompt("", None, "DO IT");
    assert_eq!(out, "DO IT");
}

#[test]
fn compose_prompt_mode_and_instructions_no_addendum_single_blank_line() {
    let out = compose_prompt("MODE", None, "INSTRUCTIONS");
    assert_eq!(out, "MODE\n\nINSTRUCTIONS");
}

#[test]
fn compose_prompt_preserves_internal_whitespace_in_sections() {
    let out = compose_prompt("  leading space", Some("  pad  "), "end");
    assert!(out.contains("  leading space"));
    assert!(out.contains("  pad  "));
    assert_eq!(out, "  leading space\n\n  pad  \n\nend");
}

#[test]
fn compose_prompt_all_empty_returns_empty_string() {
    let out = compose_prompt("", None, "");
    assert_eq!(out, "");
}

#[test]
fn parse_review_findings_round_trip() {
    let original = SubagentResult::ReviewFindings {
        verdict: ReviewVerdict::Approved,
        findings: vec![Finding {
            file: Some("src/foo.rs".into()),
            line: Some(10),
            severity: FindingSeverity::Warn,
            note: "check bounds".into(),
        }],
    };
    let raw = serde_json::to_string(&original).expect("serialize");
    let parsed = parse_result(TeamRole::Reviewer, &raw).expect("parse");
    match parsed {
        SubagentResult::ReviewFindings { verdict, findings } => {
            assert_eq!(verdict, ReviewVerdict::Approved);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].note, "check bounds");
        }
        other => panic!("expected ReviewFindings, got {other:?}"),
    }
}

#[test]
fn parse_plain_text_for_implementer_returns_shape_mismatch() {
    let result = parse_result(TeamRole::Implementer, "hello world");
    match result {
        Err(SubagentError::ResultShapeMismatch { role, got, .. }) => {
            assert_eq!(role, TeamRole::Implementer);
            assert!(got.to_lowercase().contains("non-json"), "got={got}");
        }
        other => panic!("expected ResultShapeMismatch, got {other:?}"),
    }
}

#[test]
fn parse_wrong_kind_for_role() {
    let raw = serde_json::to_string(&SubagentResult::ReviewFindings {
        verdict: ReviewVerdict::Approved,
        findings: vec![],
    })
    .expect("serialize");
    let result = parse_result(TeamRole::Implementer, &raw);
    assert!(matches!(
        result,
        Err(SubagentError::ResultShapeMismatch {
            role: TeamRole::Implementer,
            ..
        })
    ));
}

#[test]
fn parse_generic_fallback_for_reviewer() {
    let raw = r#"{"foo":"bar"}"#;
    let result = parse_result(TeamRole::Reviewer, raw).expect("parse");
    match result {
        SubagentResult::Generic { json } => {
            assert_eq!(json["foo"], "bar");
        }
        other => panic!("expected Generic, got {other:?}"),
    }
}

#[test]
fn parse_no_generic_for_implementer() {
    let result = parse_result(TeamRole::Implementer, r#"{"foo":"bar"}"#);
    assert!(matches!(
        result,
        Err(SubagentError::ResultShapeMismatch {
            role: TeamRole::Implementer,
            ..
        })
    ));
}

#[test]
fn parse_verdict_for_triager() {
    let raw = serde_json::to_string(&SubagentResult::Verdict {
        decision: "promote".into(),
        rationale: "good idea".into(),
        new_issues: vec![],
    })
    .expect("serialize");
    let parsed = parse_result(TeamRole::Triager, &raw).expect("parse");
    match parsed {
        SubagentResult::Verdict { decision, .. } => assert_eq!(decision, "promote"),
        other => panic!("expected Verdict, got {other:?}"),
    }
}
