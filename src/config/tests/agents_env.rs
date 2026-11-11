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
    let denylisted = [
        "PATH",
        "LD_LIBRARY_PATH",
        "DYLD_INSERT_LIBRARIES",
        "NODE_OPTIONS",
        "PYTHONPATH",
        "PERL5LIB",
        "BASH_ENV",
        "ENV",
        "IFS",
        "GIT_SSH_COMMAND",
        "GIT_CONFIG",
        // Extended for #1107 (post-merge security review, CWE-426 / CWE-454):
        // loader/interpreter env vars that still reached agent children.
        "PERL5OPT",
        "PERLLIB",
        "PYTHONHOME",
        "PYTHONSTARTUP",
        "RUBYOPT",
        "RUBYLIB",
        "JAVA_TOOL_OPTIONS",
        "JDK_JAVA_OPTIONS",
        "CLASSPATH",
        "CPATH",
        "NODE_PATH",
        "NODE_EXTRA_CA_CERTS",
        "SHELLOPTS",
        "BASHOPTS",
        "ZDOTDIR",
        "GEM_HOME",
        "GEM_PATH",
        "BUNDLE_GEMFILE",
        "HOME",
        "SHELL",
        // Parity additions (judge review on #1107): CA-trust redirection
        // (same effect as NODE_EXTRA_CA_CERTS) and toolchain-home
        // redirection (same effect as GEM_HOME).
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "NPM_CONFIG_PREFIX",
        "PERL5DB",
    ];
    for key in denylisted {
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
