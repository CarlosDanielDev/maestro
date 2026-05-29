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

    /// Human-readable message for the Settings inline-warning slot.
    /// Lists the configured ids the user could pick from so they do
    /// not have to leave Settings to grep `[agents.*]` / `[modes.*]`
    /// (#912). The list is capped at six entries; overflow renders as
    /// `(+N more)`. An empty `known` set produces a `no agents
    /// configured` / `no modes configured` tail instead of an empty
    /// `— valid: `.
    pub fn message(&self, known: &BTreeSet<String>) -> String {
        let (head, kind) = match self.field {
            RoleOverrideField::Agent | RoleOverrideField::FallbackAgent => {
                (format!("unknown agent `{}`", self.value), "agents")
            }
            RoleOverrideField::Mode => (format!("unknown mode `{}`", self.value), "modes"),
        };
        format!("{head} — {}", format_known_ids_tail(known, kind))
    }
}

/// Format the trailing `valid: …` / `no … configured` clause shared
/// by all `RoleOverrideField` variants. `kind` is the plural noun
/// (`agents` / `modes`) used in the empty-set fallback.
fn format_known_ids_tail(known: &BTreeSet<String>, kind: &str) -> String {
    const CAP: usize = 6;
    if known.is_empty() {
        return format!("no {kind} configured");
    }
    let total = known.len();
    let head: Vec<&str> = known.iter().take(CAP).map(String::as_str).collect();
    let mut tail = format!("valid: {}", head.join(", "));
    if total > CAP {
        tail.push_str(&format!(" (+{} more)", total - CAP));
    }
    tail
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
#[path = "team_role_overrides_tests.rs"]
mod tests;
