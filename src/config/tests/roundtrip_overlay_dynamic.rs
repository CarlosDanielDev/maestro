//! Round-trip tests for `Config::save_into_str` covering dynamic-section
//! edits — add, remove, and (feature-gated) reorder — introduced by issue
//! #790. See `tests/fixtures/dynamic_config/` for the golden fixtures.

use super::roundtrip_overlay::{load_fixture, temp_file_with};
use super::*;
use crate::config::agents::{AgentConfig, AgentKind};
use crate::config::sessions::CompletionGateEntry;
use std::collections::BTreeMap;

const FIXTURE_DIR: &str = "tests/fixtures/dynamic_config";

fn qwen_fast_agent() -> AgentConfig {
    AgentConfig {
        kind: AgentKind::Qwen,
        enabled: true,
        command: Some("qwen".to_string()),
        base_url: None,
        model: Some("qwen-fast".to_string()),
        env: BTreeMap::new(),
        extra_args: Vec::new(),
        permission_mode: None,
        allowed_tools: Vec::new(),
        sandbox: None,
        json: None,
        ephemeral: None,
        profile: None,
        config_overrides: BTreeMap::new(),
        cli_flags: BTreeMap::new(),
        request_timeout_secs: None,
        api_key_env: None,
        num_ctx: None,
    }
}

#[test]
fn add_agents_entry_appends_block_and_preserves_neighbor_comments() {
    let (before, mut cfg) = load_fixture(FIXTURE_DIR, "agents_add.toml.before");
    cfg.agents
        .entries
        .insert("qwen-fast".to_string(), qwen_fast_agent());

    let after = cfg
        .save_into_str(&before)
        .expect("save_into_str must succeed");

    assert!(
        after.contains("[agents.qwen-fast]"),
        "new agent block must be emitted as a header:\n{after}"
    );
    assert!(
        after.contains("# === Agents ===")
            && after.contains("# Primary Claude agent — do not remove")
            && after.contains("# Marker comment that must survive an add operation"),
        "all neighbor comments must survive the add:\n{after}"
    );
    let claude_idx = after
        .find("[agents.claude]")
        .expect("claude header present");
    let qwen_idx = after
        .find("[agents.qwen-fast]")
        .expect("qwen header present");
    assert!(
        claude_idx < qwen_idx,
        "added agent must follow the existing one, not replace it"
    );
}

#[test]
fn remove_agents_entry_drops_block_and_collapses_blank_lines() {
    let (before, mut cfg) = load_fixture(FIXTURE_DIR, "agents_remove.toml.before");
    cfg.agents.entries.remove("qwen-fast");

    let after = cfg
        .save_into_str(&before)
        .expect("save_into_str must succeed");

    assert!(
        !after.contains("[agents.qwen-fast]"),
        "removed agent header must be gone:\n{after}"
    );
    assert!(
        !after.contains("kind = \"qwen\""),
        "removed agent body must be gone:\n{after}"
    );
    assert!(
        after.contains("# Primary Claude agent")
            && after.contains("# Sentinel section that must remain comment-anchored after removal"),
        "neighbor comments must survive removal:\n{after}"
    );
    assert!(
        !after.contains("\n\n\n\n"),
        "blank lines must not pile up after removal:\n{after}"
    );
}

#[test]
fn remove_then_add_idempotent_does_not_accumulate_blank_lines() {
    let (before, mut cfg) = load_fixture(FIXTURE_DIR, "agents_remove.toml.before");
    cfg.agents.entries.remove("qwen-fast");
    let pass1 = cfg.save_into_str(&before).expect("first save");

    let tmp2 = temp_file_with(&pass1);
    let cfg2 = Config::load(tmp2.path()).expect("pass1 must parse");
    let pass2 = cfg2.save_into_str(&pass1).expect("second save");

    assert_eq!(
        pass1.matches("\n\n\n").count(),
        pass2.matches("\n\n\n").count(),
        "blank-line density must not drift on resave:\nfirst:\n{pass1}\nsecond:\n{pass2}"
    );
}

#[test]
fn array_of_tables_pure_append_preserves_existing_comments() {
    let (before, mut cfg) = load_fixture(FIXTURE_DIR, "completion_gates_reorder.toml.before");
    cfg.sessions
        .completion_gates
        .commands
        .push(CompletionGateEntry {
            name: "doc".to_string(),
            run: "cargo doc".to_string(),
            required: false,
        });

    let after = cfg
        .save_into_str(&before)
        .expect("save_into_str must succeed");

    assert!(
        after.contains("name = \"doc\""),
        "appended gate must be present"
    );
    for marker in [
        "# Gate 1 — runs first",
        "# Gate 2 — runs second",
        "# Gate 3 — runs last",
    ] {
        assert!(
            after.contains(marker),
            "existing comment {marker:?} must survive append:\n{after}"
        );
    }
}

#[test]
fn array_of_tables_pure_remove_keeps_remaining_comments_anchored() {
    let (before, mut cfg) = load_fixture(FIXTURE_DIR, "completion_gates_reorder.toml.before");
    cfg.sessions.completion_gates.commands.pop();

    let after = cfg
        .save_into_str(&before)
        .expect("save_into_str must succeed");

    assert!(
        !after.contains("name = \"test\""),
        "popped gate's body must be gone:\n{after}"
    );
    for marker in ["# Gate 1 — runs first", "# Gate 2 — runs second"] {
        assert!(
            after.contains(marker),
            "kept gate comment must survive:\n{after}"
        );
    }
}

#[cfg(feature = "dynamic-config-reorder")]
#[test]
fn array_of_tables_reorder_swaps_elements_carrying_comments() {
    let (before, mut cfg) = load_fixture(FIXTURE_DIR, "completion_gates_reorder.toml.before");
    cfg.sessions.completion_gates.commands.swap(0, 1);

    let after = cfg
        .save_into_str(&before)
        .expect("save_into_str must succeed");

    let clippy_idx = after
        .find("name = \"clippy\"")
        .expect("clippy element present");
    let fmt_idx = after.find("name = \"fmt\"").expect("fmt element present");
    assert!(
        clippy_idx < fmt_idx,
        "clippy must now precede fmt:\n{after}"
    );

    let gate1_idx = after.find("# Gate 1 — runs first").expect("gate 1 comment");
    let gate2_idx = after
        .find("# Gate 2 — runs second")
        .expect("gate 2 comment");
    assert!(
        gate2_idx < gate1_idx,
        "comments must follow their elements through the swap:\n{after}"
    );
    let gate3_idx = after.find("# Gate 3 — runs last").expect("gate 3 comment");
    assert!(
        gate3_idx > fmt_idx,
        "third element comment must remain anchored:\n{after}"
    );
}

#[cfg(not(feature = "dynamic-config-reorder"))]
#[test]
fn array_of_tables_reorder_without_feature_falls_back_to_wholesale_rewrite() {
    let (before, mut cfg) = load_fixture(FIXTURE_DIR, "completion_gates_reorder.toml.before");
    cfg.sessions.completion_gates.commands.swap(0, 1);

    let after = cfg
        .save_into_str(&before)
        .expect("save_into_str must succeed");

    let clippy_idx = after
        .find("name = \"clippy\"")
        .expect("clippy element present");
    let fmt_idx = after.find("name = \"fmt\"").expect("fmt element present");
    assert!(
        clippy_idx < fmt_idx,
        "fallback rewrite must still reflect the new order:\n{after}"
    );
    assert!(
        !after.contains("# Gate 1 — runs first") && !after.contains("# Gate 2 — runs second"),
        "wholesale-rewrite path is documented to drop per-element comments on \
         moved rows; the feature-on path keeps them. Without this distinction \
         the test would not exercise the fallback:\n{after}"
    );
}

#[test]
fn comments_preserved_mixed_scenario_keeps_every_input_comment() {
    let (before, mut cfg) = load_fixture(FIXTURE_DIR, "comments_preserved.toml");
    cfg.agents
        .entries
        .insert("qwen-fast".to_string(), qwen_fast_agent());
    cfg.budget.alert_threshold_pct = 90;
    cfg.sessions
        .completion_gates
        .commands
        .push(CompletionGateEntry {
            name: "doc".to_string(),
            run: "cargo doc".to_string(),
            required: false,
        });

    let after = cfg
        .save_into_str(&before)
        .expect("save_into_str must succeed");

    for comment in before.lines().filter(|l| l.trim_start().starts_with('#')) {
        assert!(
            after.contains(comment),
            "comment {comment:?} from input must appear in output:\n{after}"
        );
    }

    let tmp2 = temp_file_with(&after);
    let cfg2 = Config::load(tmp2.path()).expect("after must parse");
    let after2 = cfg2.save_into_str(&after).expect("re-save");
    assert_eq!(
        after, after2,
        "second save with no mutations must be byte-identical (idempotent)"
    );
}
