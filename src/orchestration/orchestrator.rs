//! L2 per-issue orchestrator prompt assembly.

#![allow(dead_code)]

use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::types::Primitive;
use crate::provider::types::Issue;

pub fn build_system_prompt(team: &ResolvedTeam, issue: &Issue) -> String {
    let primitive = primitive_name(team.primitive);
    let tools = if team.primitive == Primitive::Pipeline {
        "Task, GhPrCreate, ReportFailure"
    } else {
        "Task, ReportFailure"
    };

    format!(
        "You are Maestro L2 for issue #{}.\n\
Primitive: {primitive}.\n\
Team: {}.\n\
Allowed tools: {tools}.\n\
Forbidden: Read, Edit, Write, Bash, Grep.\n\
Do not inspect files or the issue body. Delegate with Task(role, instructions) only.\n\
Pass only concise structured summaries between roles. ReportFailure on blocked or invalid results.",
        issue.number, team.name
    )
}

fn primitive_name(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Pipeline => "pipeline",
        Primitive::FanOut => "fan-out",
        Primitive::SinglePass => "single-pass",
        Primitive::VerdictOnly => "verdict-only",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::team::{ResolvedTeam, RoleBinding, SourceTier};
    use crate::orchestration::types::{Primitive, TeamRole};
    use std::collections::HashMap;

    fn issue() -> Issue {
        Issue {
            number: 662,
            title: "L2 orchestrator".into(),
            body: "SECRET ISSUE BODY".into(),
            labels: vec![],
            state: "open".into(),
            html_url: "https://example.test/issues/662".into(),
            milestone: None,
            assignees: vec![],
        }
    }

    fn team(primitive: Primitive) -> ResolvedTeam {
        ResolvedTeam {
            name: "default-coder".into(),
            primitive,
            min_agents: vec!["claude".into()],
            bindings: HashMap::from([(
                TeamRole::Reviewer,
                RoleBinding {
                    agent: "claude".into(),
                    mode: None,
                    model_override: None,
                    prompt_addendum: None,
                    fallback_agent: None,
                },
            )]),
            source_tier: SourceTier::BuiltIn,
        }
    }

    #[test]
    fn pipeline_prompt_snapshot() {
        let prompt = build_system_prompt(&team(Primitive::Pipeline), &issue());
        insta::assert_snapshot!(prompt, @r###"
You are Maestro L2 for issue #662.
Primitive: pipeline.
Team: default-coder.
Allowed tools: Task, GhPrCreate, ReportFailure.
Forbidden: Read, Edit, Write, Bash, Grep.
Do not inspect files or the issue body. Delegate with Task(role, instructions) only.
Pass only concise structured summaries between roles. ReportFailure on blocked or invalid results.
"###);
    }

    #[test]
    fn prompt_includes_expected_bounds() {
        let prompt = build_system_prompt(&team(Primitive::SinglePass), &issue());
        assert!(prompt.contains("single-pass"));
        assert!(prompt.contains("#662"));
        assert!(prompt.contains("Allowed tools: Task, ReportFailure."));
        assert!(!prompt.contains("GhPrCreate"));
        assert!(prompt.contains("Forbidden: Read, Edit, Write, Bash, Grep."));
        assert!(!prompt.contains("SECRET ISSUE BODY"));
    }
}
