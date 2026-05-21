//! Integration tests for the L2 composer (issue #757).
//!
//! Exercises the public composer surface with a filesystem-backed
//! `FsFragmentSource` rooted at `tests/fixtures/templates/core/`. The fixture
//! root is small + stable so snapshots stay readable and decoupled from any
//! future edits to the real `.maestro/templates/core/*.md` files.

use std::collections::HashMap;
use std::path::PathBuf;

use maestro::orchestration::types::Primitive;
use maestro::orchestration::{
    FragmentSource, FsFragmentSource, ResolvedTeam, RoleBinding, SourceTier, TeamRole,
    build_system_prompt,
};
use maestro::provider::types::Issue;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/templates/core")
}

fn issue() -> Issue {
    Issue {
        number: 757,
        title: "L2 composer integration".into(),
        body: "SECRET ISSUE BODY (must never appear in prompt)".into(),
        labels: vec![],
        state: "open".into(),
        html_url: "https://example.test/issues/757".into(),
        milestone: None,
        assignees: vec![],
    }
}

fn team(primitive: Primitive, addendum: Option<&str>) -> ResolvedTeam {
    ResolvedTeam {
        name: "default-coder".into(),
        primitive,
        min_agents: vec!["claude".into()],
        bindings: HashMap::from([(
            TeamRole::Implementer,
            RoleBinding {
                agent: "claude".into(),
                mode: None,
                model_override: None,
                prompt_addendum: addendum.map(str::to_string),
                fallback_agent: None,
            },
        )]),
        source_tier: SourceTier::BuiltIn,
    }
}

#[test]
fn pipeline_with_addendum_snapshot() {
    let src = FsFragmentSource::new(fixture_root());
    let prompt = build_system_prompt(
        &team(Primitive::Pipeline, Some("Be terse.")),
        &issue(),
        &src,
    );
    insta::assert_snapshot!("pipeline_with_addendum", prompt);
}

#[test]
fn single_pass_no_addendum_snapshot() {
    let src = FsFragmentSource::new(fixture_root());
    let prompt = build_system_prompt(&team(Primitive::SinglePass, None), &issue(), &src);
    insta::assert_snapshot!("single_pass_no_addendum", prompt);
}

#[test]
fn origin_gate_no_issue_body_leak() {
    let src = FsFragmentSource::new(fixture_root());
    let prompt = build_system_prompt(
        &team(Primitive::Pipeline, Some("Be terse.")),
        &issue(),
        &src,
    );
    assert!(
        !prompt.contains("SECRET ISSUE BODY"),
        "L2 prompt must not embed issue body (origin gate from #707)"
    );
}

#[test]
fn missing_fragments_do_not_panic_and_yield_header_only() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = FsFragmentSource::new(tmp.path());
    let prompt = build_system_prompt(&team(Primitive::SinglePass, None), &issue(), &src);
    assert!(prompt.starts_with("You are Maestro L2 for issue #"));
    assert!(!prompt.contains("Premises (test fixture)"));
}

#[test]
fn fragment_source_trait_object_works_via_dyn_dispatch() {
    struct Empty;
    impl FragmentSource for Empty {
        fn load(&self, _: &str) -> Option<String> {
            None
        }
    }
    let prompt = build_system_prompt(&team(Primitive::SinglePass, None), &issue(), &Empty);
    assert!(prompt.contains("Forbidden:"));
}
