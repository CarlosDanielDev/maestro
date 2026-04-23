//! AI prompt building and response parsing for the Milestone Wizard.
//! Keeps the string handling out of the screen module so it can be unit
//! tested without spawning `claude --print`.

use super::types::{AiGeneratedPlan, AiProposedIssue, MilestonePlanPayload};

/// Build the structured prompt sent to `claude --print` for the
/// `AiStructuring` step.
pub fn build_planning_prompt(payload: &MilestonePlanPayload) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "You are helping plan a software milestone. Output ONLY a JSON object with this shape:\n",
    );
    prompt.push_str(
        "{\"title\":\"…\",\"description\":\"…\",\"issues\":[{\"title\":\"…\",\"overview\":\"…\",\"blocked_by\":[<indices>]}]}\n\n",
    );
    prompt.push_str("## Goals\n");
    prompt.push_str(payload.goals.trim());
    prompt.push_str("\n\n## Non-Goals\n");
    if payload.non_goals.trim().is_empty() {
        prompt.push_str("(none specified)");
    } else {
        prompt.push_str(payload.non_goals.trim());
    }
    if !payload.doc_references.is_empty() {
        prompt.push_str("\n\n## References\n");
        for r in &payload.doc_references {
            prompt.push_str("- ");
            prompt.push_str(r);
            prompt.push('\n');
        }
    }
    if !payload.image_paths.is_empty() {
        prompt.push_str("\n\n## Attachments\n");
        for p in &payload.image_paths {
            prompt.push_str("- [Attached image: ");
            prompt.push_str(p);
            prompt.push_str("]\n");
        }
    }
    prompt.push_str("\n\nRespond with the JSON object only, no commentary, no markdown fences.");
    prompt
}

/// Parse the AI's JSON response into an `AiGeneratedPlan`. Tolerates a
/// trailing markdown fence the model may emit despite instructions.
pub fn parse_planning_response(raw: &str) -> Result<AiGeneratedPlan, String> {
    let trimmed = strip_fences(raw.trim());
    let value: serde_json::Value =
        serde_json::from_str(trimmed).map_err(|e| format!("invalid JSON: {e}"))?;

    let title = value
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or("missing `title`")?
        .to_string();
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or("missing `description`")?
        .to_string();

    let mut issues: Vec<AiProposedIssue> = Vec::new();
    if let Some(arr) = value.get("issues").and_then(|v| v.as_array()) {
        for it in arr {
            let title = it
                .get("title")
                .and_then(|v| v.as_str())
                .ok_or("issue missing `title`")?
                .to_string();
            let overview = it
                .get("overview")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let blocked_by: Vec<usize> = it
                .get("blocked_by")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|n| n.as_u64().map(|n| n as usize))
                        .collect()
                })
                .unwrap_or_default();
            issues.push(AiProposedIssue {
                title,
                overview,
                blocked_by,
                accepted: true,
            });
        }
    }
    Ok(AiGeneratedPlan {
        milestone_title: title,
        milestone_description: description,
        issues,
    })
}

fn strip_fences(s: &str) -> &str {
    let mut t = s.trim();
    if let Some(stripped) = t.strip_prefix("```json") {
        t = stripped;
    } else if let Some(stripped) = t.strip_prefix("```") {
        t = stripped;
    }
    if let Some(stripped) = t.strip_suffix("```") {
        t = stripped;
    }
    t.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_includes_goals_section() {
        let p = MilestonePlanPayload {
            goals: "build a thing".into(),
            ..Default::default()
        };
        let out = build_planning_prompt(&p);
        assert!(out.contains("## Goals"));
        assert!(out.contains("build a thing"));
    }

    #[test]
    fn build_prompt_marks_empty_non_goals() {
        let p = MilestonePlanPayload {
            goals: "x".into(),
            non_goals: String::new(),
            ..Default::default()
        };
        assert!(build_planning_prompt(&p).contains("(none specified)"));
    }

    #[test]
    fn build_prompt_includes_references_section() {
        let p = MilestonePlanPayload {
            goals: "x".into(),
            doc_references: vec!["docs/PRD.md".into(), "https://example.com".into()],
            ..Default::default()
        };
        let out = build_planning_prompt(&p);
        assert!(out.contains("## References"));
        assert!(out.contains("docs/PRD.md"));
        assert!(out.contains("https://example.com"));
    }

    #[test]
    fn parse_response_extracts_title_and_description() {
        let raw = r#"{"title":"v0.20.0","description":"desc","issues":[]}"#;
        let plan = parse_planning_response(raw).unwrap();
        assert_eq!(plan.milestone_title, "v0.20.0");
        assert_eq!(plan.milestone_description, "desc");
        assert!(plan.issues.is_empty());
    }

    #[test]
    fn parse_response_tolerates_markdown_fence() {
        let raw = "```json\n{\"title\":\"t\",\"description\":\"d\",\"issues\":[]}\n```";
        let plan = parse_planning_response(raw).unwrap();
        assert_eq!(plan.milestone_title, "t");
    }

    #[test]
    fn parse_response_extracts_issues_with_dependencies() {
        let raw = r#"{
            "title":"t","description":"d",
            "issues":[
                {"title":"a","overview":"x","blocked_by":[]},
                {"title":"b","overview":"y","blocked_by":[0]}
            ]
        }"#;
        let plan = parse_planning_response(raw).unwrap();
        assert_eq!(plan.issues.len(), 2);
        assert!(plan.issues[0].blocked_by.is_empty());
        assert_eq!(plan.issues[1].blocked_by, vec![0usize]);
        assert!(plan.issues[0].accepted);
    }

    #[test]
    fn parse_response_returns_error_on_invalid_json() {
        let raw = "not json";
        assert!(parse_planning_response(raw).is_err());
    }
}
