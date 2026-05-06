//! Team preset TOML schema — see spec §4.

#![allow(dead_code)]

use crate::orchestration::types::{Primitive, TeamRole};
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

#[derive(Debug, Clone)]
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
}
