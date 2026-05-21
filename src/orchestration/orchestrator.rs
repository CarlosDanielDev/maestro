//! L2 per-issue orchestrator prompt assembly (issue #757).
//!
//! Assembles the system prompt for an L2-spawned role from:
//!   1. issue/team/primitive/tools header (synthesized — NEVER includes
//!      `issue.body`; the origin gate from #707 forbids leak).
//!   2. canonical fragments `core/premises.md`, `core/tdd-cycle.md`,
//!      `core/dependency-graph.md`, loaded through [`FragmentSource`].
//!   3. the role binding's `prompt_addendum` (first non-empty by canonical
//!      role order — see [`pick_addendum`]).
//!
//! Empty sections collapse, mirroring `dispatch::compose_prompt` for L1.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::types::{Primitive, TeamRole};
use crate::provider::types::Issue;

/// Source of canonical core fragments for the L2 prompt composer.
///
/// Production reads `core/<name>.md` from `.maestro/templates/`; tests inject
/// a static map. `None` means "fragment absent — skip silently".
pub trait FragmentSource {
    fn load(&self, name: &str) -> Option<String>;
}

/// Filesystem-backed fragment source rooted at a configurable directory.
///
/// Production wires it at `.maestro/templates/core/`. The root is private; the
/// only constructor is [`FsFragmentSource::new`], so callers cannot retarget
/// the source after creation.
pub struct FsFragmentSource {
    root: PathBuf,
}

impl FsFragmentSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl FragmentSource for FsFragmentSource {
    fn load(&self, name: &str) -> Option<String> {
        let path = self.root.join(format!("{name}.md"));
        std::fs::read_to_string(path).ok()
    }
}

/// Assemble the L2 system prompt. Pure function — same inputs yield same
/// output. `issue.body` is read for no field other than the issue number.
pub fn build_system_prompt(
    team: &ResolvedTeam,
    issue: &Issue,
    fragments: &dyn FragmentSource,
) -> String {
    let primitive = team.primitive.label();
    let tools = match team.primitive {
        Primitive::Pipeline => "Task, GhPrCreate, ReportFailure",
        _ => "Task, ReportFailure",
    };

    let header = format!(
        "You are Maestro L2 for issue #{}.\n\
Primitive: {primitive}.\n\
Team: {}.\n\
Allowed tools: {tools}.\n\
Forbidden: Read, Edit, Write, Bash, Grep.\n\
Do not inspect files or the issue body. Delegate with Task(role, instructions) only.\n\
Pass only concise structured summaries between roles. ReportFailure on blocked or invalid results.",
        issue.number, team.name
    );

    let mut sections: Vec<String> = vec![header];
    if let Some(body) = fragments.load("premises") {
        sections.push(body);
    }
    if let Some(body) = fragments.load("tdd-cycle") {
        sections.push(body);
    }
    if let Some(body) = fragments.load("dependency-graph") {
        sections.push(body);
    }
    if let Some(body) = pick_addendum(team) {
        sections.push(body);
    }

    sections
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Returns the first non-empty `prompt_addendum` walking team bindings in
/// canonical role order (Orchestrator first, then Implementer, Reviewer, Docs,
/// Devops, Triager, Researcher). Deterministic — required for snapshots.
fn pick_addendum(team: &ResolvedTeam) -> Option<String> {
    const ORDER: &[TeamRole] = &[
        TeamRole::Orchestrator,
        TeamRole::Implementer,
        TeamRole::Reviewer,
        TeamRole::Docs,
        TeamRole::Devops,
        TeamRole::Triager,
        TeamRole::Researcher,
    ];
    ORDER.iter().find_map(|r| {
        team.bindings
            .get(r)
            .and_then(|b| b.prompt_addendum.clone())
            .filter(|s| !s.is_empty())
    })
}

/// Convenience constructor for production L2 spawn: points the composer at
/// the workspace's canonical `.maestro/templates/core/` directory.
pub fn canonical_fragment_root() -> &'static Path {
    Path::new(".maestro/templates/core")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::team::{ResolvedTeam, RoleBinding, SourceTier};
    use crate::orchestration::types::TeamRole;
    use std::collections::HashMap;

    pub(super) struct StaticFragmentSource(
        pub(super) std::collections::HashMap<&'static str, &'static str>,
    );

    impl FragmentSource for StaticFragmentSource {
        fn load(&self, name: &str) -> Option<String> {
            self.0.get(name).map(|s| s.to_string())
        }
    }

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

    fn empty_fragments() -> StaticFragmentSource {
        StaticFragmentSource(std::collections::HashMap::new())
    }

    #[test]
    fn pipeline_prompt_snapshot() {
        let prompt = build_system_prompt(&team(Primitive::Pipeline), &issue(), &empty_fragments());
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
        let prompt =
            build_system_prompt(&team(Primitive::SinglePass), &issue(), &empty_fragments());
        assert!(prompt.contains("single-pass"));
        assert!(prompt.contains("#662"));
        assert!(prompt.contains("Allowed tools: Task, ReportFailure."));
        assert!(!prompt.contains("GhPrCreate"));
        assert!(prompt.contains("Forbidden: Read, Edit, Write, Bash, Grep."));
        assert!(!prompt.contains("SECRET ISSUE BODY"));
    }

    #[test]
    fn fragments_appended_in_canonical_order() {
        let src = StaticFragmentSource(std::collections::HashMap::from([
            ("premises", "# P\n"),
            ("tdd-cycle", "# T\n"),
            ("dependency-graph", "# D\n"),
        ]));
        let prompt = build_system_prompt(&team(Primitive::Pipeline), &issue(), &src);
        let p_idx = prompt.find("# P").expect("premises present");
        let t_idx = prompt.find("# T").expect("tdd present");
        let d_idx = prompt.find("# D").expect("dep-graph present");
        assert!(p_idx < t_idx);
        assert!(t_idx < d_idx);
    }

    #[test]
    fn empty_addendum_string_collapses_like_none() {
        let mut team_empty = team(Primitive::SinglePass);
        if let Some(b) = team_empty.bindings.get_mut(&TeamRole::Reviewer) {
            b.prompt_addendum = Some(String::new());
        }
        let with_empty = build_system_prompt(&team_empty, &issue(), &empty_fragments());
        let with_none =
            build_system_prompt(&team(Primitive::SinglePass), &issue(), &empty_fragments());
        assert_eq!(with_empty, with_none);
    }

    #[test]
    fn origin_gate_no_issue_body_leak_with_fragments_present() {
        let src = StaticFragmentSource(std::collections::HashMap::from([
            ("premises", "fragment premises body"),
            ("tdd-cycle", "fragment tdd body"),
            ("dependency-graph", "fragment dep body"),
        ]));
        let prompt = build_system_prompt(&team(Primitive::Pipeline), &issue(), &src);
        assert!(
            !prompt.contains("SECRET ISSUE BODY"),
            "issue.body must NEVER appear in L2 prompt (#707 origin gate)"
        );
    }
}
