//! Tests for `src/config/trust.rs` (workspace trust gate, #1045).
//!
//! Loaded via `#[cfg(test)] #[path = "trust_tests.rs"] mod tests;` so the
//! parent module stays under the 400-LOC file-size cap (see
//! `docs/RUST-GUARDRAILS.md` §7). Mirrors the `schema_tests.rs` convention.

use super::trust_consent::refuse_untrusted;
use super::*;
use crate::config::Config;
use std::path::PathBuf;

fn config_from(toml_str: &str) -> Config {
    toml::from_str(toml_str).unwrap()
}

fn minimal() -> Config {
    config_from(
        r#"
            [project]
            repo = "owner/repo"
            base_branch = "main"
            [sessions]
            [budget]
            per_session_usd = 5.0
            total_usd = 50.0
            alert_threshold_pct = 80
            [notifications]
            "#,
    )
}

fn executable() -> Config {
    config_from(
        r#"
            [project]
            repo = "owner/repo"
            base_branch = "main"
            [sessions]
            [budget]
            per_session_usd = 5.0
            total_usd = 50.0
            alert_threshold_pct = 80
            [notifications]
            [gates]
            test_command = "pytest -q"
            [agents]
            default = "claude"
            [agents.claude]
            kind = "claude"
            command = "./scripts/helper"
            [agents.claude.env]
            TOKEN = "x"
            [agents.helper]
            kind = "codex"
            extra_args = ["--full-auto"]
            [[plugins]]
            name = "bell"
            on = "session_completed"
            run = "echo done"
            [[sessions.completion_gates.commands]]
            name = "fmt"
            run = "cargo fmt --check"
            [review]
            command = "custom-review {pr_number}"
            [[review.reviewers]]
            name = "sec"
            command = "sec-scan {pr_number}"
            "#,
    )
}

fn temp_store(tag: &str) -> (tempfile::TempDir, TrustStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = TrustStore::at(dir.path().join(format!("trusted-{tag}.toml")));
    (dir, store)
}

fn write_config(dir: &tempfile::TempDir, body: &str) -> PathBuf {
    let path = dir.path().join("maestro.toml");
    std::fs::write(&path, body).unwrap();
    path
}

const EXECUTABLE_TOML: &str = r#"
[project]
repo = "owner/repo"
base_branch = "main"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[notifications]
[gates]
test_command = "pytest -q"
[agents]
default = "claude"
[agents.claude]
kind = "claude"
command = "./scripts/helper"
[[plugins]]
name = "bell"
on = "session_completed"
run = "echo done"
"#;

#[test]
fn content_hash_is_stable_hex_sha256_length() {
    let a = content_hash("hello");
    assert_eq!(a, content_hash("hello"));
    assert_ne!(a, content_hash("hellO"));
    assert_eq!(a.len(), 64);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn inventory_collects_every_executable_family_verbatim() {
    let config = executable();
    let findings = inventory(&config);
    let get = |k: &str| {
        findings
            .iter()
            .find(|f| f.key == k)
            .map(|f| f.value.clone())
            .unwrap_or_else(|| panic!("missing {k}"))
    };
    assert_eq!(get("agents.claude.command"), "./scripts/helper");
    assert_eq!(get("agents.claude.env"), "TOKEN=x");
    assert_eq!(get("agents.helper.extra_args"), "--full-auto");
    assert_eq!(get("plugins.bell.run"), "echo done");
    assert_eq!(get("sessions.completion_gates.fmt"), "cargo fmt --check");
    assert_eq!(get("gates.test_command"), "pytest -q");
    assert_eq!(get("review.command"), "custom-review {pr_number}");
    assert_eq!(get("review.reviewers.sec.command"), "sec-scan {pr_number}");
}

#[test]
fn inventory_empty_and_has_executable_keys_false_for_minimal_config() {
    let config = minimal();
    assert!(inventory(&config).is_empty());
    assert!(!has_executable_keys(&config));
    assert!(has_executable_keys(&executable()));
}

#[test]
fn has_config_supplied_commands_ignores_builtin_defaults() {
    let builtin_only = config_from(
        r#"
            [project]
            repo = "owner/repo"
            base_branch = "main"
            [sessions]
            [budget]
            per_session_usd = 5.0
            total_usd = 50.0
            alert_threshold_pct = 80
            [notifications]
            [agents]
            default = "claude"
            [agents.claude]
            kind = "claude"
            command = "claude"
            "#,
    );
    assert!(!has_config_supplied_commands(&builtin_only));
    assert!(has_config_supplied_commands(&executable()));
    // Built-in names must not appear in the consent prompt either.
    assert!(
        inventory(&builtin_only)
            .iter()
            .all(|f| !f.key.ends_with(".command")),
        "builtin-name commands must not be listed as executable findings"
    );
    assert!(is_builtin_agent_binary("claude"));
    assert!(!is_builtin_agent_binary("./scripts/helper"));
}

#[test]
fn trust_store_record_check_revoke_roundtrip() {
    let (_dir, store) = temp_store("roundtrip");
    let path = Path::new("/tmp/does-not-exist-1045/maestro.toml");
    assert!(!store.is_trusted(path, "hash-a"));
    store.record(path, "hash-a").unwrap();
    assert!(store.is_trusted(path, "hash-a"));
    assert!(
        !store.is_trusted(path, "hash-b"),
        "different hash must not match"
    );
    assert!(store.revoke(path).unwrap());
    assert!(!store.is_trusted(path, "hash-a"));
    assert!(!store.revoke(path).unwrap(), "second revoke is a no-op");
}

#[test]
fn trust_store_record_replaces_previous_hash_for_same_path() {
    let (_dir, store) = temp_store("replace");
    let path = Path::new("/tmp/does-not-exist-1045/maestro.toml");
    store.record(path, "old").unwrap();
    store.record(path, "new").unwrap();
    assert!(!store.is_trusted(path, "old"));
    assert!(store.is_trusted(path, "new"));
}

#[test]
fn evaluate_untrusted_before_record_trusted_after_reprompts_on_change() {
    let (cfg_dir, store) = temp_store("evaluate");
    let config = config_from(EXECUTABLE_TOML);
    let path = write_config(&cfg_dir, EXECUTABLE_TOML);

    assert_eq!(
        evaluate(&path, &config, &store),
        TrustDecision::Untrusted,
        "executable config without a record is untrusted"
    );

    store.record(&path, &content_hash(EXECUTABLE_TOML)).unwrap();
    assert_eq!(evaluate(&path, &config, &store), TrustDecision::Trusted);

    let mutated = format!("{EXECUTABLE_TOML}\n# tampered\n");
    std::fs::write(&path, &mutated).unwrap();
    let mutated_config = config_from(&mutated);
    assert_eq!(
        evaluate(&path, &mutated_config, &store),
        TrustDecision::Untrusted,
        "config file change must invalidate the trust record"
    );
}

#[test]
fn evaluate_trusted_when_no_executable_keys() {
    let (_dir, store) = temp_store("no-keys");
    let config = minimal();
    let path = Path::new("/tmp/does-not-exist-1045/minimal.toml");
    assert_eq!(evaluate(path, &config, &store), TrustDecision::Trusted);
}

#[test]
fn refuse_untrusted_errors_and_trusted_passes() {
    let (cfg_dir, store) = temp_store("refuse");
    let config = config_from(EXECUTABLE_TOML);
    let path = write_config(&cfg_dir, EXECUTABLE_TOML);

    let err = refuse_untrusted(&path, &config, &store).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("not trusted"), "message: {msg}");
    assert!(
        msg.contains("./scripts/helper"),
        "must list values verbatim: {msg}"
    );

    store.record(&path, &content_hash(EXECUTABLE_TOML)).unwrap();
    assert!(refuse_untrusted(&path, &config, &store).is_ok());
}
