use super::agents::load_config;
use super::*;

// Tests for `[agents.<id>].env` key validation (#1029, EXEC-4 / TRUST-2).
// Split out of tests/agents.rs so both files stay under the repository's
// 400-line guardrail.

#[test]
fn config_validate_rejects_loader_env_var_and_names_key_and_agent() {
    // EXEC-4 / TRUST-2 (CWE-426 / CWE-454): a repo-local maestro.toml
    // must not be able to inject dynamic-loader variables into every
    // spawned agent child.
    let err = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "claude"

[agents.claude]
kind = "claude"
command = "claude"
env = {{ LD_PRELOAD = "/evil-1029.so" }}
"#
    ))
    .expect_err("LD_PRELOAD in agent env must fail config load");

    assert!(err.to_string().contains("LD_PRELOAD"), "got: {err}");
    assert!(err.to_string().contains("claude"), "got: {err}");
}

#[test]
fn config_validate_rejects_denylisted_env_keys() {
    // Derived from the validation denylist consts (single source of truth):
    // every exact key and one synthetic key per denied prefix must fail
    // config load, and the error must name the key.
    let mut denylisted: Vec<String> = crate::util::validation::AGENT_ENV_DENYLIST_EXACT
        .iter()
        .map(|key| key.to_string())
        .collect();
    for prefix in crate::util::validation::AGENT_ENV_DENYLIST_PREFIXES {
        denylisted.push(format!("{prefix}X"));
    }
    for key in &denylisted {
        let key = key.as_str();
        let toml = format!(
            r#"{MINIMAL_TOML}
[agents]
default = "claude"

[agents.claude]
kind = "claude"
command = "claude"
env = {{ {key} = "/evil-1029" }}
"#
        );
        let err =
            load_config(&toml).expect_err(&format!("{key} in agent env must fail config load"));
        assert!(
            err.to_string().contains(key),
            "error for {key} must name the key, got: {err}"
        );
    }
}

#[test]
fn config_validate_rejects_malformed_env_key_shape() {
    // Shape is ^[A-Z][A-Z0-9_]*$: lowercase or otherwise malformed keys
    // are rejected even when they are not on the denylist.
    let err = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "claude"

[agents.claude]
kind = "claude"
command = "claude"
env = {{ path = "/evil-1029" }}
"#
    ))
    .expect_err("lowercase env key must fail config load");

    assert!(err.to_string().contains("path"), "got: {err}");
}

#[test]
fn config_validate_accepts_benign_agent_env() {
    let cfg = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "claude"

[agents.claude]
kind = "claude"
command = "claude"
env = {{ ANTHROPIC_MODEL = "claude-sonnet-5-5", MY_TOOL_API_KEY = "value" }}
"#
    ))
    .expect("benign agent env must load");
    let claude = cfg
        .agents
        .entries
        .get("claude")
        .expect("claude agent should be present");
    assert_eq!(
        claude.env.get("ANTHROPIC_MODEL").map(String::as_str),
        Some("claude-sonnet-5-5")
    );
    assert_eq!(
        claude.env.get("MY_TOOL_API_KEY").map(String::as_str),
        Some("value")
    );
}
