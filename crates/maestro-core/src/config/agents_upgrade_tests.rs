//! Tests for `agents_upgrade.rs`. Extracted via `#[path]` to keep
//! `agents_upgrade.rs` under the 400-line file-size guardrail.

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
    assert!(toml::from_str::<toml::Value>(&plan.normalized_toml).is_ok());
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
    assert!(toml::from_str::<toml::Value>(&plan.normalized_toml).is_ok());
}

// --- Byte-identity tests (issue #718) -----------------------------------

fn assert_original_lines_untouched(before: &str, after: &str) {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    for original_line in &before_lines {
        assert!(
            after_lines.contains(original_line),
            "original line was mutated or removed.\n  missing: {original_line:?}\n--- before ---\n{before}\n--- after ---\n{after}"
        );
    }
}

#[test]
fn partial_agents_preserves_comments_and_blanks() {
    let existing = concat!(
        "# maestro config\n",
        "\n",
        "[sessions]\n",
        "default_model = \"opus\"\n",
        "\n",
        "[agents]\n",
        "\n",
        "[agents.claude]\n",
        "kind = \"claude\"\n",
    );

    let plan = plan_agent_config_upgrade(existing).unwrap();
    assert_eq!(plan.version, AgentConfigVersion::PartialExplicitAgents);
    assert!(plan.needs_update);

    let normalized = &plan.normalized_toml;
    assert_original_lines_untouched(existing, normalized);
    assert!(normalized.contains("# maestro config"));

    let blank_count_before = existing.lines().filter(|l| l.is_empty()).count();
    let blank_count_after = normalized.lines().filter(|l| l.is_empty()).count();
    assert!(
        blank_count_after >= blank_count_before,
        "blank lines must be preserved (before={blank_count_before} after={blank_count_after}):\n{normalized}"
    );
    assert!(normalized.contains("default = \"claude\""));
}

#[test]
fn partial_agents_preserves_unmodeled_sections() {
    let existing = concat!(
        "[sessions]\n",
        "default_model = \"opus\"\n",
        "\n",
        "[agents]\n",
        "\n",
        "[agents.claude]\n",
        "kind = \"claude\"\n",
        "\n",
        "[my_custom]\n",
        "foo = \"bar\"\n",
        "baz = 42\n",
    );

    let plan = plan_agent_config_upgrade(existing).unwrap();
    assert_eq!(plan.version, AgentConfigVersion::PartialExplicitAgents);

    let normalized = &plan.normalized_toml;
    assert!(normalized.contains("[my_custom]"));
    assert!(normalized.contains("foo = \"bar\""));
    assert!(normalized.contains("baz = 42"));
    assert_original_lines_untouched(existing, normalized);
}

#[test]
fn partial_agents_byte_identity_when_only_default_missing() {
    let existing = concat!(
        "[sessions]\n",
        "default_model = \"opus\"\n",
        "\n",
        "[agents]\n",
        "\n",
        "[agents.claude]\n",
        "kind = \"claude\"\n",
        "enabled = true\n",
        "command = \"claude\"\n",
        "model = \"opus\"\n",
        "permission_mode = \"bypassPermissions\"\n",
        "allowed_tools = []\n",
    );

    let plan = plan_agent_config_upgrade(existing).unwrap();
    assert_eq!(plan.version, AgentConfigVersion::PartialExplicitAgents);
    assert!(plan.needs_update);
    assert_eq!(
        plan.keys_added,
        vec!["agents.default".to_string()],
        "only agents.default must be added: {:?}",
        plan.keys_added
    );

    let normalized = &plan.normalized_toml;
    assert_original_lines_untouched(existing, normalized);
    assert!(normalized.contains("default = \"claude\""));
}

#[test]
fn partial_agents_drops_normalized_banner() {
    let existing = concat!(
        "[sessions]\n",
        "default_model = \"opus\"\n",
        "\n",
        "[agents]\n",
        "\n",
        "[agents.claude]\n",
        "kind = \"claude\"\n",
    );

    let plan = plan_agent_config_upgrade(existing).unwrap();
    assert_eq!(plan.version, AgentConfigVersion::PartialExplicitAgents);
    assert!(
        !plan.normalized_toml.contains("Normalized by Maestro"),
        "banner must be absent — it lied about preservation:\n{}",
        plan.normalized_toml
    );
}
