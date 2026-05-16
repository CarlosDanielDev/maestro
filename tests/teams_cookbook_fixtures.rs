//! Integration test: every TOML snippet in the teams usage guide and recipes
//! cookbook resolves without error and satisfies the per-fixture assertion
//! list from issue #675.
//!
//! Fixture directory: tests/fixtures/teams_cookbook/

use std::path::PathBuf;

use maestro::orchestration::loader::Loader;
use maestro::orchestration::team::ResolvedTeam;
use maestro::orchestration::types::{Primitive, TeamRole};

const COOKBOOK_STEMS: &[&str] = &[
    "cheap-coder",
    "strict-reviewer",
    "repo-policy-coder",
    "fanout-reviewers",
    "inbox-triager",
    "ship-it",
];

const BUILTIN_COUNT: usize = 5;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/teams_cookbook")
}

fn load_cookbook() -> std::collections::HashMap<String, ResolvedTeam> {
    let dir = fixture_dir();
    assert!(
        dir.exists(),
        "fixture dir missing: {} — create tests/fixtures/teams_cookbook/ \
         and add the six cookbook TOML files before running this test.",
        dir.display()
    );
    let loader = Loader::new(Some(dir), None);
    loader
        .resolve()
        .unwrap_or_else(|e| panic!("Loader::resolve() failed for cookbook fixtures: {e:#}"))
}

fn agent_for(team: &ResolvedTeam, role: TeamRole) -> &str {
    team.bindings
        .get(&role)
        .unwrap_or_else(|| panic!("{role:?} binding missing"))
        .agent
        .as_str()
}

fn addendum_for(team: &ResolvedTeam, role: TeamRole) -> &str {
    team.bindings
        .get(&role)
        .unwrap_or_else(|| panic!("{role:?} binding missing"))
        .prompt_addendum
        .as_deref()
        .unwrap_or_else(|| panic!("{role:?} must have a prompt_addendum"))
}

// validate_preset_name is pub(crate) so we mirror its rules here for the
// name-compliance subtest. Source: src/orchestration/loader.rs:254-279.
// Keep in sync with that function — production enforcement still happens
// there at runtime; this mirror only proves the documented fixture names
// would survive it.
fn inline_validate_preset_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("preset name must not be empty".into());
    }
    if name.len() > 64 {
        return Err(format!(
            "preset name {name:?} is {} chars, max 64",
            name.len()
        ));
    }
    if name.starts_with('.') {
        return Err(format!("preset name {name:?} cannot start with '.'"));
    }
    if name.starts_with('-') {
        return Err(format!("preset name {name:?} cannot start with '-'"));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') || name.contains("..") {
        return Err(format!(
            "preset name {name:?} contains illegal path characters"
        ));
    }
    Ok(())
}

#[test]
fn cheap_coder_pipeline_with_opencode_implementer() {
    let resolved = load_cookbook();
    let team = resolved
        .get("cheap-coder")
        .expect("cheap-coder fixture missing");

    assert_eq!(team.primitive, Primitive::Pipeline);
    assert_eq!(agent_for(team, TeamRole::Implementer), "opencode");
    assert_eq!(
        agent_for(team, TeamRole::Reviewer),
        "claude",
        "cheap-coder reviewer must inherit claude from default-coder"
    );
    assert_eq!(
        agent_for(team, TeamRole::Docs),
        "claude",
        "cheap-coder docs must inherit claude from default-coder"
    );
}

#[test]
fn strict_reviewer_pipeline_with_reviewer_prompt_addendum() {
    let resolved = load_cookbook();
    let team = resolved
        .get("strict-reviewer")
        .expect("strict-reviewer fixture missing");

    assert_eq!(team.primitive, Primitive::Pipeline);
    assert!(!addendum_for(team, TeamRole::Reviewer).is_empty());
}

#[test]
fn repo_policy_coder_pipeline_with_docs_prompt_addendum() {
    let resolved = load_cookbook();
    let team = resolved
        .get("repo-policy-coder")
        .expect("repo-policy-coder fixture missing");

    assert_eq!(team.primitive, Primitive::Pipeline);
    assert!(!addendum_for(team, TeamRole::Docs).is_empty());
}

#[test]
fn fanout_reviewers_fan_out_primitive_with_reviewer_bound() {
    let resolved = load_cookbook();
    let team = resolved
        .get("fanout-reviewers")
        .expect("fanout-reviewers fixture missing");

    assert_eq!(team.primitive, Primitive::FanOut);
    for required in Primitive::FanOut.required_roles() {
        assert!(
            team.bindings.contains_key(required),
            "fanout-reviewers missing required role {required:?} for FanOut"
        );
    }
}

#[test]
fn inbox_triager_verdict_only_with_triager_prompt_addendum() {
    let resolved = load_cookbook();
    let team = resolved
        .get("inbox-triager")
        .expect("inbox-triager fixture missing");

    assert_eq!(team.primitive, Primitive::VerdictOnly);
    assert!(!addendum_for(team, TeamRole::Triager).is_empty());
}

#[test]
fn ship_it_pipeline_with_opencode_implementer() {
    let resolved = load_cookbook();
    let team = resolved.get("ship-it").expect("ship-it fixture missing");

    assert_eq!(team.primitive, Primitive::Pipeline);
    assert_eq!(agent_for(team, TeamRole::Implementer), "opencode");
}

#[test]
fn all_fixture_stems_pass_name_validation() {
    for name in COOKBOOK_STEMS {
        inline_validate_preset_name(name)
            .unwrap_or_else(|e| panic!("fixture stem {name:?} failed name validation: {e}"));
    }
}

#[test]
fn all_six_fixtures_resolve() {
    let resolved = load_cookbook();
    for name in COOKBOOK_STEMS {
        assert!(
            resolved.contains_key(*name),
            "fixture {name:?} missing from resolved map — \
             check that tests/fixtures/teams_cookbook/{name}.toml exists and parses"
        );
    }
    assert_eq!(
        resolved.len(),
        COOKBOOK_STEMS.len() + BUILTIN_COUNT,
        "expected {} resolved teams ({} cookbook + {} built-ins), got {}",
        COOKBOOK_STEMS.len() + BUILTIN_COUNT,
        COOKBOOK_STEMS.len(),
        BUILTIN_COUNT,
        resolved.len()
    );
}
