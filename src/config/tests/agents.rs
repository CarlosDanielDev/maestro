use super::*;

fn load_config(toml: &str) -> anyhow::Result<Config> {
    use std::io::Write;

    let mut file = tempfile::NamedTempFile::new()?;
    write!(file, "{toml}")?;
    Config::load(file.path())
}

#[test]
fn agents_config_defaults_to_builtin_claude_when_section_absent() {
    let cfg = load_config(
        r#"[project]
repo = "owner/repo"
[sessions]
default_model = "sonnet"
permission_mode = "acceptEdits"
allowed_tools = ["Read", "Write"]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#,
    )
    .expect("load failed");

    assert_eq!(cfg.agents.default, "claude");
    assert!(cfg.agents.entries.is_empty());

    let resolved = cfg.resolve_agent(None).expect("default agent resolves");
    assert_eq!(resolved.id, "claude");
    assert_eq!(resolved.config.kind, AgentKind::Claude);
    assert_eq!(resolved.config.command.as_deref(), Some("claude"));
    assert_eq!(resolved.config.model.as_deref(), Some("sonnet"));
    assert_eq!(
        resolved.config.permission_mode.as_deref(),
        Some("acceptEdits")
    );
    assert_eq!(resolved.config.allowed_tools, ["Read", "Write"]);
}

#[test]
fn agents_config_parses_claude_subprocess_agent() {
    let cfg: AgentsConfig = toml::from_str(
        r#"
default = "claude"

[claude]
kind = "claude"
enabled = true
command = "claude"
model = "opus"
permission_mode = "bypassPermissions"
allowed_tools = ["Read"]
"#,
    )
    .expect("parse failed");

    let claude = cfg.entries.get("claude").expect("claude agent");
    assert_eq!(claude.kind, AgentKind::Claude);
    assert!(claude.enabled);
    assert_eq!(claude.command.as_deref(), Some("claude"));
    assert_eq!(claude.model.as_deref(), Some("opus"));
    assert_eq!(claude.permission_mode.as_deref(), Some("bypassPermissions"));
    assert_eq!(claude.allowed_tools, ["Read"]);
}

#[test]
fn agents_config_parses_codex_specific_fields() {
    let cfg: AgentsConfig = toml::from_str(
        r#"
default = "codex"

[codex]
kind = "codex"
command = "codex"
model = "gpt-5.4-codex"
sandbox = "workspace-write"
json = true
ephemeral = false
profile = "work"
extra_args = ["--reasoning-effort", "high"]

[codex.config_overrides]
approval_policy = "never"
"#,
    )
    .expect("parse failed");

    let codex = cfg.entries.get("codex").expect("codex agent");
    assert_eq!(codex.sandbox.as_deref(), Some("workspace-write"));
    assert_eq!(codex.json, Some(true));
    assert_eq!(codex.ephemeral, Some(false));
    assert_eq!(codex.profile.as_deref(), Some("work"));
    assert_eq!(codex.extra_args, ["--reasoning-effort", "high"]);
    assert_eq!(
        codex.config_overrides.get("approval_policy"),
        Some(&toml::Value::String("never".to_string()))
    );
}

#[test]
fn agents_config_parses_http_agent_defaults() {
    let cfg: AgentsConfig = toml::from_str(
        r#"
default = "ollama"

[ollama]
kind = "ollama"
model = "qwen3"

[minimax]
kind = "minimax"
"#,
    )
    .expect("parse failed");

    let ollama = cfg.entries.get("ollama").expect("ollama agent");
    assert_eq!(ollama.base_url.as_deref(), Some("http://localhost:11434"));
    assert_eq!(ollama.request_timeout_secs, Some(120));

    let minimax = cfg.entries.get("minimax").expect("minimax agent");
    assert_eq!(
        minimax.base_url.as_deref(),
        Some("https://api.minimax.io/v1")
    );
    assert_eq!(minimax.model.as_deref(), Some("MiniMax-M2.7"));
    assert_eq!(minimax.request_timeout_secs, Some(120));
    assert_eq!(minimax.api_key_env.as_deref(), Some("MINIMAX_API_KEY"));
}

#[test]
fn agents_config_parses_opencode_provider_model() {
    let cfg: AgentsConfig = toml::from_str(
        r#"
default = "opencode"

[opencode]
kind = "opencode"
model = "openrouter/deepseek/deepseek-coder-v2"
"#,
    )
    .expect("parse failed");

    let opencode = cfg.entries.get("opencode").expect("opencode agent");
    assert_eq!(opencode.command.as_deref(), Some("opencode"));
    assert_eq!(
        opencode.model.as_deref(),
        Some("openrouter/deepseek/deepseek-coder-v2")
    );
}

#[test]
fn config_validate_rejects_unknown_default_agent() {
    let err = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "missing"

[agents.claude]
kind = "claude"
command = "claude"
"#
    ))
    .expect_err("unknown default must fail");

    assert!(err.to_string().contains("agents.default `missing`"));
}

#[test]
fn config_validate_rejects_default_without_agent_table() {
    let err = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "codex"
"#
    ))
    .expect_err("default without matching agent table must fail");

    assert!(err.to_string().contains("agents.default `codex`"));
}

#[test]
fn config_validate_rejects_disabled_default_agent() {
    let err = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "claude"

[agents.claude]
kind = "claude"
enabled = false
command = "claude"
"#
    ))
    .expect_err("disabled default must fail");

    assert!(
        err.to_string()
            .contains("agents.default `claude` is disabled")
    );
}

#[test]
fn config_validate_rejects_subprocess_agent_with_base_url() {
    let err = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "claude"

[agents.claude]
kind = "claude"
command = "claude"
base_url = "http://localhost:11434"
"#
    ))
    .expect_err("subprocess base_url must fail");

    assert!(err.to_string().contains("base_url is only valid for HTTP"));
}

#[test]
fn config_validate_rejects_http_agent_with_command() {
    let err = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "ollama"

[agents.ollama]
kind = "ollama"
command = "ollama"
"#
    ))
    .expect_err("http command must fail");

    assert!(
        err.to_string()
            .contains("command is only valid for subprocess")
    );
}

#[test]
fn api_key_env_stores_only_variable_name() {
    let cfg: AgentsConfig = toml::from_str(
        r#"
default = "minimax"

[minimax]
kind = "minimax"
api_key_env = "MINIMAX_API_KEY"
"#,
    )
    .expect("parse failed");

    let minimax = cfg.entries.get("minimax").expect("minimax agent");
    assert_eq!(minimax.api_key_env.as_deref(), Some("MINIMAX_API_KEY"));
    assert_ne!(
        minimax.api_key_env.as_deref(),
        Some("secret-value-that-must-not-be-read")
    );
}
