//! `agents.<id>.transport` field tests (#750) — split out of `agents.rs` to
//! keep it under the 400-line guardrail.

use crate::config::{AgentConfig, AgentKind};

fn claude_with_transport(transport: Option<&str>) -> AgentConfig {
    let mut agent = AgentConfig::builtin_claude("opus", "default", Vec::new());
    agent.transport = transport.map(str::to_string);
    agent
}

#[test]
fn toml_round_trip_parses_transport() {
    let agent: AgentConfig = toml::from_str(
        r#"
            kind = "claude"
            command = "claude"
            transport = "interactive"
            "#,
    )
    .expect("parse");
    assert_eq!(agent.transport.as_deref(), Some("interactive"));
}

#[test]
fn toml_without_transport_defaults_to_none() {
    let agent: AgentConfig = toml::from_str(
        r#"
            kind = "claude"
            command = "claude"
            "#,
    )
    .expect("parse");
    assert_eq!(agent.transport, None);
}

#[test]
fn validate_accepts_headless_and_interactive() {
    for value in ["headless", "interactive"] {
        claude_with_transport(Some(value))
            .validate("claude")
            .expect(value);
    }
    // Empty string round-trips from TUI text inputs as "unset".
    claude_with_transport(Some(""))
        .validate("claude")
        .expect("empty");
    claude_with_transport(None)
        .validate("claude")
        .expect("none");
}

#[test]
fn validate_rejects_unknown_transport_listing_valid_values() {
    let err = claude_with_transport(Some("bogus"))
        .validate("claude")
        .expect_err("bogus must fail");
    let msg = err.to_string();
    assert!(msg.contains("headless, interactive"), "{msg}");
}

#[test]
fn validate_rejects_transport_on_non_claude_agents() {
    let mut agent = claude_with_transport(Some("interactive"));
    agent.kind = AgentKind::Qwen;
    let err = agent.validate("qwen").expect_err("must fail");
    assert!(err.to_string().contains("only valid for claude"), "{err}");
}
