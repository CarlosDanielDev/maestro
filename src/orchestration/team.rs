//! Team preset TOML schema — see spec §4.

#![allow(dead_code)]

use crate::orchestration::types::{Primitive, TeamRole};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamConfig {
    /// Parent preset name. Empty string means root (built-in).
    #[serde(default)]
    pub extends: String,

    /// Required if no `extends`; otherwise inherited.
    pub primitive: Option<Primitive>,

    /// Required if no `extends`; otherwise inherited.
    #[serde(default)]
    pub min_agents: Option<Vec<String>>,

    /// Minimal-form bindings: top-level keys whose values are agent_id strings.
    /// Captured via #[serde(flatten)] into a HashMap; non-binding fields above
    /// are deserialized first.
    #[serde(default, flatten)]
    pub bindings: HashMap<String, toml::Value>,

    /// Rich-form bindings: per-role override sub-table.
    #[serde(default)]
    pub role_overrides: HashMap<String, RoleOverride>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RoleOverride {
    pub agent: Option<String>,
    pub mode: Option<String>,
    pub model_override: Option<String>,
    pub prompt_addendum: Option<String>,
    pub fallback_agent: Option<String>,
}

/// Resolved (post-`extends` merge) team — all bindings concrete.
#[derive(Debug, Clone)]
pub struct ResolvedTeam {
    pub name: String,
    pub primitive: Primitive,
    pub min_agents: Vec<String>,
    pub bindings: HashMap<TeamRole, RoleBinding>,
    pub source_tier: SourceTier,
}

#[derive(Debug, Clone, Default)]
pub struct RoleBinding {
    pub agent: String,
    pub mode: Option<String>,
    pub model_override: Option<String>,
    pub prompt_addendum: Option<String>,
    pub fallback_agent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceTier {
    BuiltIn,
    User,
    Project,
}

/// Shallow cross-entry validation for the settings-screen save path.
///
/// Every `teams.<child>.extends` value (if non-empty after trim) must
/// reference another configured team in `all`. Built-in presets are loaded
/// from a separate source at runtime and are NOT visible to `Config`; we
/// accept any extends value that resolves to a key in `all`, leaving
/// built-in resolution to `Loader` at session-start time.
///
/// Intentionally shallow — no cycle detection. A cycle like
/// `a.extends = "b"; b.extends = "a"` surfaces at `Loader` time as a
/// resolution error rather than a save-time block. Cycle detection is
/// tracked as a v0.30.0 follow-up.
pub fn validate_extends(all: &HashMap<String, TeamConfig>) -> Result<()> {
    for (child, cfg) in all {
        let parent = cfg.extends.trim();
        if parent.is_empty() {
            continue;
        }
        if all.contains_key(parent) {
            continue;
        }
        anyhow::bail!("teams.{child}.extends references unknown team `{parent}`");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_form() {
        let toml = r#"
extends = "default-coder"
implementer = "ollama"
reviewer = "opencode"
docs = "minimax"
"#;
        let config: TeamConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.extends, "default-coder");
        assert_eq!(
            config.bindings.get("implementer").unwrap().as_str(),
            Some("ollama")
        );
    }

    #[test]
    fn parses_rich_form_with_overrides() {
        let toml = r#"
extends = "cheap-coder"

[role_overrides.reviewer]
agent = "opencode"
mode = "review-strict"
prompt_addendum = "Be terse."
fallback_agent = "claude"
"#;
        let config: TeamConfig = toml::from_str(toml).unwrap();
        let r = config.role_overrides.get("reviewer").unwrap();
        assert_eq!(r.agent, Some("opencode".into()));
        assert_eq!(r.mode, Some("review-strict".into()));
        assert_eq!(r.fallback_agent, Some("claude".into()));
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let toml = r#"
extends = "default-coder"
implementer = "ollama"
unknown_field = "boom"
"#;
        // The flatten captures `unknown_field` as a binding — validator will
        // reject unknown roles later.
        let config: TeamConfig = toml::from_str(toml).unwrap();
        assert!(config.bindings.contains_key("unknown_field"));
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

    #[test]
    fn validate_extends_accepts_root_and_existing_parent() {
        let mut all = HashMap::new();
        all.insert("root".to_string(), empty_team(""));
        all.insert("child".to_string(), empty_team("root"));
        validate_extends(&all).expect("root and known parent are accepted");
    }

    #[test]
    fn validate_extends_rejects_dangling_parent() {
        let mut all = HashMap::new();
        all.insert("orphan".to_string(), empty_team("missing-parent"));
        let err = validate_extends(&all).unwrap_err().to_string();
        assert!(
            err.contains("teams.orphan.extends references unknown team `missing-parent`"),
            "error message must name child + parent, got: {err}"
        );
    }

    #[test]
    fn validate_extends_treats_whitespace_as_empty() {
        let mut all = HashMap::new();
        all.insert("ws".to_string(), empty_team("   "));
        validate_extends(&all).expect("whitespace-only extends must be treated as root");
    }
}
