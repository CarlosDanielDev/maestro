//! Soft cross-entry validator for `[teams.<id>.role_overrides.<role>]`
//! sub-tables. Split out of `team.rs` (#908) to keep both files under
//! the 400-LOC guardrail.
//!
//! Mirrors the `validate_extends` pattern (#803) but returns warnings
//! instead of `Result<()>` — Save proceeds regardless.

use std::collections::{BTreeSet, HashMap};

use super::team::TeamConfig;

/// Per-field discriminant for a soft `role_overrides` warning. Maps to
/// the TOML key that the warning references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoleOverrideField {
    Agent,
    Mode,
    FallbackAgent,
}

impl RoleOverrideField {
    pub const fn schema_key(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Mode => "mode",
            Self::FallbackAgent => "fallback_agent",
        }
    }
}

/// Soft warning emitted by [`validate_role_overrides`] when a
/// `role_overrides.<role>.<field>` value does not resolve in the
/// configured `[agents.<id>]` / `[modes.<id>]` id sets.
///
/// The Settings save banner surfaces the structured path; Save still
/// proceeds (mirrors the `teams.<id>.extends` validator pattern).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleOverrideWarning {
    pub team_id: String,
    pub role_id: String,
    pub field: RoleOverrideField,
    pub value: String,
}

impl RoleOverrideWarning {
    pub fn structured_path(&self) -> String {
        format!(
            "teams.{}.role_overrides.{}.{}",
            self.team_id,
            self.role_id,
            self.field.schema_key(),
        )
    }

    pub fn message(&self) -> String {
        match self.field {
            RoleOverrideField::Agent | RoleOverrideField::FallbackAgent => {
                format!("unknown agent `{}`", self.value)
            }
            RoleOverrideField::Mode => format!("unknown mode `{}`", self.value),
        }
    }
}

/// Soft cross-entry validator for `[teams.<id>.role_overrides.<role>]`
/// sub-tables. Returns one warning per `agent`, `mode`, or `fallback_agent`
/// value that does not resolve in the supplied id sets.
///
/// Empty / whitespace-only strings are treated as "inherit from
/// bindings" and emit no warning. Save is allowed to proceed regardless
/// of warning count — the banner surfaces them for the user.
///
/// Output order is deterministic (sorted by team_id, then role_id, then
/// field) so the banner reads the same on every Save with the same
/// configuration.
pub fn validate_role_overrides(
    teams: &HashMap<String, TeamConfig>,
    known_agents: &BTreeSet<String>,
    known_modes: &BTreeSet<String>,
) -> Vec<RoleOverrideWarning> {
    let mut warnings = Vec::new();
    let mut team_entries: Vec<(&String, &TeamConfig)> = teams.iter().collect();
    team_entries.sort_by_key(|(k, _)| k.as_str());
    for (team_id, team) in team_entries {
        let mut role_entries: Vec<(&String, &super::team::RoleOverride)> =
            team.role_overrides.iter().collect();
        role_entries.sort_by_key(|(k, _)| k.as_str());
        for (role_id, ov) in role_entries {
            if let Some(v) = check_value(ov.agent.as_deref(), known_agents) {
                warnings.push(RoleOverrideWarning {
                    team_id: team_id.clone(),
                    role_id: role_id.clone(),
                    field: RoleOverrideField::Agent,
                    value: v,
                });
            }
            if let Some(v) = check_value(ov.mode.as_deref(), known_modes) {
                warnings.push(RoleOverrideWarning {
                    team_id: team_id.clone(),
                    role_id: role_id.clone(),
                    field: RoleOverrideField::Mode,
                    value: v,
                });
            }
            if let Some(v) = check_value(ov.fallback_agent.as_deref(), known_agents) {
                warnings.push(RoleOverrideWarning {
                    team_id: team_id.clone(),
                    role_id: role_id.clone(),
                    field: RoleOverrideField::FallbackAgent,
                    value: v,
                });
            }
        }
    }
    warnings
}

/// Return the trimmed value if it is non-empty AND not present in `known`;
/// otherwise `None` (which means inherit or known and emits no warning).
fn check_value(value: Option<&str>, known: &BTreeSet<String>) -> Option<String> {
    let raw = value?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if known.contains(trimmed) {
        return None;
    }
    Some(trimmed.to_string())
}

#[cfg(test)]
mod tests {
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
}
