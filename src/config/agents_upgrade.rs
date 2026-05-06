use anyhow::{Context, Result};
use toml::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentConfigVersion {
    ImplicitClaude,
    PartialExplicitAgents,
    ExplicitAgents,
}

impl AgentConfigVersion {
    pub fn label(self) -> &'static str {
        match self {
            Self::ImplicitClaude => "legacy implicit-claude",
            Self::PartialExplicitAgents => "partial explicit-agents",
            Self::ExplicitAgents => "explicit agents",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConfigUpgradePlan {
    pub version: AgentConfigVersion,
    pub needs_update: bool,
    pub snippet: String,
    pub normalized_toml: String,
    pub keys_added: Vec<String>,
}

pub fn plan_agent_config_upgrade(existing_toml: &str) -> Result<AgentConfigUpgradePlan> {
    let mut value: Value = toml::from_str(existing_toml).context("parsing maestro.toml")?;
    let Some(root) = value.as_table_mut() else {
        anyhow::bail!("maestro.toml root must be a table");
    };

    let defaults = AgentDefaults::from_root(root);
    let mut keys_added = Vec::new();

    if !root.contains_key("agents") {
        let snippet = render_implicit_claude_snippet(&defaults);
        let normalized_toml = append_snippet(existing_toml, &snippet);
        return Ok(AgentConfigUpgradePlan {
            version: AgentConfigVersion::ImplicitClaude,
            needs_update: true,
            snippet,
            normalized_toml,
            keys_added: vec![
                "agents".to_string(),
                "agents.default".to_string(),
                "agents.claude".to_string(),
                "agents.claude.kind".to_string(),
                "agents.claude.enabled".to_string(),
                "agents.claude.command".to_string(),
                "agents.claude.model".to_string(),
                "agents.claude.permission_mode".to_string(),
                "agents.claude.allowed_tools".to_string(),
            ],
        });
    }

    let mut changed = false;
    let Some(agents) = root.get_mut("agents").and_then(Value::as_table_mut) else {
        anyhow::bail!("agents must be a table");
    };

    if !agents.contains_key("default") {
        agents.insert("default".to_string(), Value::String("claude".to_string()));
        keys_added.push("agents.default".to_string());
        changed = true;
    }

    let default_agent = agents
        .get("default")
        .and_then(Value::as_str)
        .unwrap_or("claude")
        .to_string();

    if default_agent == "claude" || !agents.contains_key(&default_agent) {
        let claude_missing = !agents.contains_key("claude");
        let claude = agents
            .entry("claude".to_string())
            .or_insert_with(|| Value::Table(toml::map::Map::new()));
        let Some(claude_table) = claude.as_table_mut() else {
            anyhow::bail!("agents.claude must be a table");
        };
        changed |= insert_missing(
            claude_table,
            "kind",
            Value::String("claude".to_string()),
            &mut keys_added,
            "agents.claude.kind",
        );
        changed |= insert_missing(
            claude_table,
            "enabled",
            Value::Boolean(true),
            &mut keys_added,
            "agents.claude.enabled",
        );
        changed |= insert_missing(
            claude_table,
            "command",
            Value::String("claude".to_string()),
            &mut keys_added,
            "agents.claude.command",
        );
        if claude_missing {
            changed |= insert_missing(
                claude_table,
                "model",
                Value::String(defaults.model),
                &mut keys_added,
                "agents.claude.model",
            );
            changed |= insert_missing(
                claude_table,
                "permission_mode",
                Value::String(defaults.permission_mode),
                &mut keys_added,
                "agents.claude.permission_mode",
            );
            changed |= insert_missing(
                claude_table,
                "allowed_tools",
                Value::Array(defaults.allowed_tools),
                &mut keys_added,
                "agents.claude.allowed_tools",
            );
        }
    }

    if changed {
        let normalized = toml::to_string_pretty(&value).context("serializing normalized TOML")?;
        return Ok(AgentConfigUpgradePlan {
            version: AgentConfigVersion::PartialExplicitAgents,
            needs_update: true,
            snippet: render_implicit_claude_snippet(&AgentDefaults::from_value(&value)),
            normalized_toml: format!(
                "# Normalized by Maestro agent config upgrade.\n# Existing values were preserved; missing [agents] keys were added.\n{normalized}"
            ),
            keys_added,
        });
    }

    Ok(AgentConfigUpgradePlan {
        version: AgentConfigVersion::ExplicitAgents,
        needs_update: false,
        snippet: String::new(),
        normalized_toml: existing_toml.to_string(),
        keys_added,
    })
}

fn insert_missing(
    table: &mut toml::map::Map<String, Value>,
    key: &str,
    value: Value,
    keys_added: &mut Vec<String>,
    dotted: &str,
) -> bool {
    if table.contains_key(key) {
        return false;
    }
    table.insert(key.to_string(), value);
    keys_added.push(dotted.to_string());
    true
}

#[derive(Debug, Clone)]
struct AgentDefaults {
    model: String,
    permission_mode: String,
    allowed_tools: Vec<Value>,
}

impl AgentDefaults {
    fn from_root(root: &toml::map::Map<String, Value>) -> Self {
        let sessions = root.get("sessions").and_then(Value::as_table);
        Self::from_sessions(sessions)
    }

    fn from_value(value: &Value) -> Self {
        let sessions = value
            .as_table()
            .and_then(|root| root.get("sessions"))
            .and_then(Value::as_table);
        Self::from_sessions(sessions)
    }

    fn from_sessions(sessions: Option<&toml::map::Map<String, Value>>) -> Self {
        let model = sessions
            .and_then(|s| s.get("default_model"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("opus")
            .to_string();
        let permission_mode = sessions
            .and_then(|s| s.get("permission_mode"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("bypassPermissions")
            .to_string();
        let allowed_tools = sessions
            .and_then(|s| s.get("allowed_tools"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        Self {
            model,
            permission_mode,
            allowed_tools,
        }
    }
}

fn append_snippet(existing_toml: &str, snippet: &str) -> String {
    let mut out = existing_toml.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(snippet.trim_end());
    out.push('\n');
    out
}

fn render_implicit_claude_snippet(defaults: &AgentDefaults) -> String {
    format!(
        r#"[agents]
default = "claude"

[agents.claude]
kind = "claude"
enabled = true
command = "claude"
model = {}
permission_mode = {}
allowed_tools = {}
"#,
        toml_string(&defaults.model),
        toml_string(&defaults.permission_mode),
        toml_array(&defaults.allowed_tools)
    )
}

fn toml_string(value: &str) -> String {
    Value::String(value.to_string()).to_string()
}

fn toml_array(values: &[Value]) -> String {
    let items: Vec<String> = values
        .iter()
        .map(|value| match value {
            Value::String(s) => toml_string(s),
            other => other.to_string(),
        })
        .collect();
    format!("[{}]", items.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_insert_for_implicit_claude_config() {
        let existing = r#"[sessions]
default_model = "sonnet"
permission_mode = "acceptEdits"
allowed_tools = ["Read", "Edit"]
"#;

        let plan = plan_agent_config_upgrade(existing).unwrap();

        assert_eq!(plan.version, AgentConfigVersion::ImplicitClaude);
        assert!(plan.needs_update);
        assert!(plan.snippet.contains("[agents.claude]"));
        assert!(plan.snippet.contains("model = \"sonnet\""));
        assert!(plan.snippet.contains("permission_mode = \"acceptEdits\""));
        assert!(
            plan.snippet
                .contains("allowed_tools = [\"Read\", \"Edit\"]")
        );
        assert!(toml::from_str::<Value>(&plan.normalized_toml).is_ok());
    }

    #[test]
    fn plans_noop_for_complete_explicit_agents_config() {
        let existing = r#"[sessions]
default_model = "opus"

[agents]
default = "claude"

[agents.claude]
kind = "claude"
enabled = true
command = "claude"
"#;

        let plan = plan_agent_config_upgrade(existing).unwrap();

        assert_eq!(plan.version, AgentConfigVersion::ExplicitAgents);
        assert!(!plan.needs_update);
        assert!(plan.snippet.is_empty());
    }

    #[test]
    fn normalizes_partial_agents_config() {
        let existing = r#"[sessions]
default_model = "opus"

[agents]

[agents.claude]
kind = "claude"
"#;

        let plan = plan_agent_config_upgrade(existing).unwrap();

        assert_eq!(plan.version, AgentConfigVersion::PartialExplicitAgents);
        assert!(plan.needs_update);
        assert!(plan.normalized_toml.contains("default = \"claude\""));
        assert!(plan.normalized_toml.contains("command = \"claude\""));
        assert!(plan.keys_added.contains(&"agents.default".to_string()));
        assert!(toml::from_str::<Value>(&plan.normalized_toml).is_ok());
    }
}
