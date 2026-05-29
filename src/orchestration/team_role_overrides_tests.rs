use super::*;

use crate::orchestration::team::RoleOverride;
use crate::orchestration::types::Primitive;

fn team_with_role(extends: &str, role: &str, ov: RoleOverride) -> TeamConfig {
    let mut role_overrides = HashMap::new();
    role_overrides.insert(role.to_string(), ov);
    TeamConfig {
        extends: extends.to_string(),
        primitive: Some(Primitive::Pipeline),
        min_agents: None,
        bindings: HashMap::new(),
        role_overrides,
    }
}

fn empty_team(extends: &str) -> TeamConfig {
    TeamConfig {
        extends: extends.to_string(),
        primitive: Some(Primitive::Pipeline),
        min_agents: None,
        bindings: HashMap::new(),
        role_overrides: HashMap::new(),
    }
}

fn agents() -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    s.insert("claude".to_string());
    s.insert("opencode".to_string());
    s
}

fn modes() -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    s.insert("review-strict".to_string());
    s
}

#[test]
fn returns_empty_when_no_overrides() {
    let mut teams = HashMap::new();
    teams.insert("t1".to_string(), empty_team(""));
    let w = validate_role_overrides(&teams, &agents(), &modes());
    assert!(w.is_empty(), "no overrides means no warnings, got {w:?}");
}

#[test]
fn empty_when_all_refs_resolve() {
    let mut teams = HashMap::new();
    teams.insert(
        "t1".to_string(),
        team_with_role(
            "",
            "reviewer",
            RoleOverride {
                agent: Some("opencode".to_string()),
                mode: Some("review-strict".to_string()),
                fallback_agent: Some("claude".to_string()),
                ..Default::default()
            },
        ),
    );
    let w = validate_role_overrides(&teams, &agents(), &modes());
    assert!(w.is_empty(), "all refs resolve, got {w:?}");
}

#[test]
fn flags_unknown_agent() {
    let mut teams = HashMap::new();
    teams.insert(
        "t1".to_string(),
        team_with_role(
            "",
            "reviewer",
            RoleOverride {
                agent: Some("nonexistent".to_string()),
                ..Default::default()
            },
        ),
    );
    let w = validate_role_overrides(&teams, &agents(), &modes());
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].team_id, "t1");
    assert_eq!(w[0].role_id, "reviewer");
    assert_eq!(w[0].field, RoleOverrideField::Agent);
    assert_eq!(w[0].value, "nonexistent");
    assert_eq!(
        w[0].structured_path(),
        "teams.t1.role_overrides.reviewer.agent",
    );
}

#[test]
fn flags_unknown_mode() {
    let mut teams = HashMap::new();
    teams.insert(
        "t1".to_string(),
        team_with_role(
            "",
            "reviewer",
            RoleOverride {
                mode: Some("ghost-mode".to_string()),
                ..Default::default()
            },
        ),
    );
    let w = validate_role_overrides(&teams, &agents(), &modes());
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].field, RoleOverrideField::Mode);
    assert_eq!(
        w[0].structured_path(),
        "teams.t1.role_overrides.reviewer.mode",
    );
}

#[test]
fn flags_unknown_fallback_agent() {
    let mut teams = HashMap::new();
    teams.insert(
        "t1".to_string(),
        team_with_role(
            "",
            "reviewer",
            RoleOverride {
                fallback_agent: Some("missing".to_string()),
                ..Default::default()
            },
        ),
    );
    let w = validate_role_overrides(&teams, &agents(), &modes());
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].field, RoleOverrideField::FallbackAgent);
    assert_eq!(
        w[0].structured_path(),
        "teams.t1.role_overrides.reviewer.fallback_agent",
    );
}

#[test]
fn ignores_empty_string_as_inherit() {
    let mut teams = HashMap::new();
    teams.insert(
        "t1".to_string(),
        team_with_role(
            "",
            "reviewer",
            RoleOverride {
                agent: Some(String::new()),
                mode: Some("   ".to_string()),
                fallback_agent: Some(String::new()),
                ..Default::default()
            },
        ),
    );
    let w = validate_role_overrides(&teams, &agents(), &modes());
    assert!(
        w.is_empty(),
        "empty / whitespace values mean inherit and emit no warning, got {w:?}"
    );
}

// --- #912 message enrichment ---

fn agents_set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

#[test]
fn message_appends_sorted_valid_agent_ids() {
    let w = RoleOverrideWarning {
        team_id: "t".to_string(),
        role_id: "r".to_string(),
        field: RoleOverrideField::Agent,
        value: "ghost".to_string(),
    };
    let known = agents_set(&["claude", "opencode"]);
    let msg = w.message(&known);
    assert_eq!(msg, "unknown agent `ghost` — valid: claude, opencode");
}

#[test]
fn message_uses_mode_label_for_mode_field() {
    let w = RoleOverrideWarning {
        team_id: "t".to_string(),
        role_id: "r".to_string(),
        field: RoleOverrideField::Mode,
        value: "ghost".to_string(),
    };
    let known = agents_set(&["review-strict"]);
    let msg = w.message(&known);
    assert_eq!(msg, "unknown mode `ghost` — valid: review-strict");
}

#[test]
fn message_uses_agent_label_for_fallback_agent_field() {
    let w = RoleOverrideWarning {
        team_id: "t".to_string(),
        role_id: "r".to_string(),
        field: RoleOverrideField::FallbackAgent,
        value: "ghost".to_string(),
    };
    let known = agents_set(&["claude"]);
    let msg = w.message(&known);
    assert_eq!(msg, "unknown agent `ghost` — valid: claude");
}

#[test]
fn message_caps_valid_list_at_six_with_overflow_count() {
    let w = RoleOverrideWarning {
        team_id: "t".to_string(),
        role_id: "r".to_string(),
        field: RoleOverrideField::Agent,
        value: "ghost".to_string(),
    };
    // BTreeSet iterates in sort order; 7 entries → cap at 6 + "(+1 more)"
    let known = agents_set(&["a1", "a2", "a3", "a4", "a5", "a6", "a7"]);
    let msg = w.message(&known);
    assert_eq!(
        msg,
        "unknown agent `ghost` — valid: a1, a2, a3, a4, a5, a6 (+1 more)",
    );
}

#[test]
fn message_caps_at_six_exact_no_overflow() {
    let w = RoleOverrideWarning {
        team_id: "t".to_string(),
        role_id: "r".to_string(),
        field: RoleOverrideField::Agent,
        value: "ghost".to_string(),
    };
    let known = agents_set(&["a1", "a2", "a3", "a4", "a5", "a6"]);
    let msg = w.message(&known);
    assert_eq!(msg, "unknown agent `ghost` — valid: a1, a2, a3, a4, a5, a6");
}

#[test]
fn message_empty_known_set_says_no_agents_configured() {
    let w = RoleOverrideWarning {
        team_id: "t".to_string(),
        role_id: "r".to_string(),
        field: RoleOverrideField::Agent,
        value: "ghost".to_string(),
    };
    let known = BTreeSet::new();
    let msg = w.message(&known);
    assert_eq!(msg, "unknown agent `ghost` — no agents configured");
}

#[test]
fn message_empty_known_set_says_no_modes_configured() {
    let w = RoleOverrideWarning {
        team_id: "t".to_string(),
        role_id: "r".to_string(),
        field: RoleOverrideField::Mode,
        value: "ghost".to_string(),
    };
    let known = BTreeSet::new();
    let msg = w.message(&known);
    assert_eq!(msg, "unknown mode `ghost` — no modes configured");
}

#[test]
fn collects_multiple_warnings_in_deterministic_order() {
    let mut teams = HashMap::new();
    teams.insert(
        "alpha".to_string(),
        team_with_role(
            "",
            "reviewer",
            RoleOverride {
                agent: Some("ghost".to_string()),
                mode: Some("phantom".to_string()),
                ..Default::default()
            },
        ),
    );
    teams.insert(
        "beta".to_string(),
        team_with_role(
            "",
            "docs",
            RoleOverride {
                fallback_agent: Some("missing".to_string()),
                ..Default::default()
            },
        ),
    );
    let w = validate_role_overrides(&teams, &agents(), &modes());
    assert_eq!(w.len(), 3);
    assert_eq!(w[0].team_id, "alpha");
    assert_eq!(w[0].field, RoleOverrideField::Agent);
    assert_eq!(w[1].team_id, "alpha");
    assert_eq!(w[1].field, RoleOverrideField::Mode);
    assert_eq!(w[2].team_id, "beta");
    assert_eq!(w[2].field, RoleOverrideField::FallbackAgent);
}
