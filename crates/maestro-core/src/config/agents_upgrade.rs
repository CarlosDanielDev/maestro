use anyhow::{Context, Result};
use toml_edit::{Array, DocumentMut, Item, Value};

use super::toml_edit_helpers::{EnsureOutcome, ensure_field};

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
    let mut doc: DocumentMut = existing_toml.parse().context("parsing maestro.toml")?;
    let defaults = AgentDefaults::from_doc(&doc);

    if doc.get("agents").is_none() {
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

    let mut keys_added: Vec<String> = Vec::new();
    let mut record = |outcome: EnsureOutcome, path: &str| {
        if outcome == EnsureOutcome::Inserted {
            keys_added.push(path.to_string());
        }
    };

    record(
        ensure_field(&mut doc, "agents.default", Value::from("claude"))?,
        "agents.default",
    );

    let default_agent = read_str(&doc, &["agents", "default"]).unwrap_or_else(|| "claude".into());

    if default_agent == "claude" || !has_table(&doc, &["agents", &default_agent]) {
        let claude_missing = !has_table(&doc, &["agents", "claude"]);

        record(
            ensure_field(&mut doc, "agents.claude.kind", Value::from("claude"))?,
            "agents.claude.kind",
        );
        record(
            ensure_field(&mut doc, "agents.claude.enabled", Value::from(true))?,
            "agents.claude.enabled",
        );
        record(
            ensure_field(&mut doc, "agents.claude.command", Value::from("claude"))?,
            "agents.claude.command",
        );
        if claude_missing {
            record(
                ensure_field(
                    &mut doc,
                    "agents.claude.model",
                    Value::from(&defaults.model),
                )?,
                "agents.claude.model",
            );
            record(
                ensure_field(
                    &mut doc,
                    "agents.claude.permission_mode",
                    Value::from(&defaults.permission_mode),
                )?,
                "agents.claude.permission_mode",
            );
            record(
                ensure_field(
                    &mut doc,
                    "agents.claude.allowed_tools",
                    Value::Array(string_array(&defaults.allowed_tools)),
                )?,
                "agents.claude.allowed_tools",
            );
        }
    }

    if !keys_added.is_empty() {
        return Ok(AgentConfigUpgradePlan {
            version: AgentConfigVersion::PartialExplicitAgents,
            needs_update: true,
            snippet: render_implicit_claude_snippet(&AgentDefaults::from_doc(&doc)),
            normalized_toml: doc.to_string(),
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

#[derive(Debug, Clone)]
struct AgentDefaults {
    model: String,
    permission_mode: String,
    allowed_tools: Vec<String>,
}

impl AgentDefaults {
    fn from_doc(doc: &DocumentMut) -> Self {
        let model = read_str(doc, &["sessions", "default_model"])
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "opus".into());
        let permission_mode = read_str(doc, &["sessions", "permission_mode"])
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "bypassPermissions".into());
        let allowed_tools = read_str_array(doc, &["sessions", "allowed_tools"]);
        Self {
            model,
            permission_mode,
            allowed_tools,
        }
    }
}

fn read_str(doc: &DocumentMut, path: &[&str]) -> Option<String> {
    walk_item(doc, path)
        .and_then(Item::as_value)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn read_str_array(doc: &DocumentMut, path: &[&str]) -> Vec<String> {
    walk_item(doc, path)
        .and_then(Item::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn has_table(doc: &DocumentMut, path: &[&str]) -> bool {
    walk_item(doc, path).map(Item::is_table).unwrap_or(false)
}

fn walk_item<'a>(doc: &'a DocumentMut, path: &[&str]) -> Option<&'a Item> {
    let mut item: &Item = doc.as_item();
    for segment in path {
        let table = item.as_table_like()?;
        item = table.get(segment)?;
    }
    Some(item)
}

fn string_array(values: &[String]) -> Array {
    let mut arr = Array::new();
    for value in values {
        arr.push(value.as_str());
    }
    arr
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
    Value::from(value).to_string()
}

fn toml_array(values: &[String]) -> String {
    let items: Vec<String> = values.iter().map(|s| toml_string(s)).collect();
    format!("[{}]", items.join(", "))
}

#[cfg(test)]
#[path = "agents_upgrade_tests.rs"]
mod tests;
