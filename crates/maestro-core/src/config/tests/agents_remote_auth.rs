//! Remote bearer-credential guard tests (#1030) — split out of
//! `agents.rs` to keep it under the 400-line guardrail.

use super::*;

fn load_config(toml: &str) -> anyhow::Result<Config> {
    use std::io::Write;

    let mut file = tempfile::NamedTempFile::new()?;
    write!(file, "{toml}")?;
    Config::load(file.path())
}

// --- Remote bearer-credential guards (#1030) ---

#[test]
fn config_validate_rejects_http_base_url_for_remote_host() {
    let err = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "ollama"

[agents.ollama]
kind = "ollama"
base_url = "http://collector.example/v1"
"#
    ))
    .expect_err("remote http base_url must fail");

    assert!(err.to_string().contains("cleartext"), "{err}");
}

#[test]
fn config_validate_accepts_http_base_url_on_loopback_hosts() {
    for base_url in [
        "http://localhost:11434",
        "http://127.0.0.1:8080",
        "http://[::1]:11434",
    ] {
        load_config(&format!(
            r#"{MINIMAL_TOML}
[agents]
default = "ollama"

[agents.ollama]
kind = "ollama"
base_url = "{base_url}"
"#
        ))
        .unwrap_or_else(|err| panic!("loopback base_url {base_url} must be accepted: {err}"));
    }
}

#[test]
fn config_validate_rejects_remote_https_host_without_confirmation() {
    // https keeps the credential off the wire, but the host itself is
    // still repo-chosen: without confirmation the bearer token would be
    // handed to whatever server that DNS name resolves to.
    let err = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "minimax"

[agents.minimax]
kind = "minimax"
base_url = "https://relay.example.com/v1"
api_key_env = "MINIMAX_API_KEY"
"#
    ))
    .expect_err("remote https base_url without confirmation must fail");

    assert!(
        err.to_string().contains("MAESTRO_ALLOW_REMOTE_AUTH"),
        "{err}"
    );
}

#[test]
fn config_validate_treats_repo_local_allow_remote_auth_as_inert() {
    // Judge-1105 (blocking): a cloned repo can ship the flag itself, so
    // repo-local `allow_remote_auth = true` alone must NOT confirm a
    // remote host. Confirmation only comes from the operator env switch
    // MAESTRO_ALLOW_REMOTE_AUTH=1.
    let err = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "minimax"

[agents.minimax]
kind = "minimax"
base_url = "https://relay.example.com/v1"
api_key_env = "MINIMAX_API_KEY"
allow_remote_auth = true
"#
    ))
    .expect_err("repo-local allow_remote_auth must be inert");

    assert!(
        err.to_string().contains("MAESTRO_ALLOW_REMOTE_AUTH"),
        "{err}"
    );
}

#[test]
fn config_validate_accepts_provider_default_base_url_without_confirmation() {
    load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "minimax"

[agents.minimax]
kind = "minimax"
"#
    ))
    .expect("provider default base_url must not need confirmation");
}

#[test]
fn config_validate_rejects_api_key_env_outside_provider_allowlist() {
    // The allowlist is absolute: `allow_remote_auth = true` does not lift
    // it, or a cloned repo could exfiltrate any secret env var (SEC-1).
    let err = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "ollama"

[agents.ollama]
kind = "ollama"
base_url = "http://localhost:11434"
api_key_env = "ANTHROPIC_API_KEY"
"#
    ))
    .expect_err("api_key_env outside the provider allowlist must fail");

    assert!(err.to_string().contains("allowlist"), "{err}");
}

#[test]
fn config_validate_accepts_allowlisted_api_key_env_prefixes() {
    load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "ollama"

[agents.ollama]
kind = "ollama"
base_url = "http://localhost:11434"
api_key_env = "OLLAMA_RELAY_TOKEN"
"#
    ))
    .expect("OLLAMA_ prefix must be allowed");

    load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "minimax"

[agents.minimax]
kind = "minimax"
api_key_env = "MAESTRO_MINIMAX_TOKEN"
"#
    ))
    .expect("MAESTRO_ prefix must be allowed");
}

#[test]
fn config_validate_rejects_allow_remote_auth_on_subprocess_agent() {
    let err = load_config(&format!(
        r#"{MINIMAL_TOML}
[agents]
default = "claude"

[agents.claude]
kind = "claude"
command = "claude"
allow_remote_auth = true
"#
    ))
    .expect_err("allow_remote_auth on a subprocess agent must fail");

    assert!(
        err.to_string()
            .contains("allow_remote_auth is only valid for HTTP agents"),
        "{err}"
    );
}
