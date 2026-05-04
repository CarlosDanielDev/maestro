use super::*;

#[test]
fn full_config_layout_defaults_when_section_absent() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
    )
    .unwrap();
    let cfg = Config::load(f.path()).expect("load failed");
    assert_eq!(cfg.tui.layout.mode, LayoutMode::Vertical);
    assert_eq!(cfg.tui.layout.density, Density::Default);
}

// --- Issue #70: Config round-trip (save/load) tests ---

#[test]
fn config_save_round_trip_minimal() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
    )
    .unwrap();
    let original = Config::load(f.path()).expect("load failed");
    let out = tempfile::NamedTempFile::new().unwrap();
    original.save(out.path()).expect("save failed");
    let reloaded = Config::load(out.path()).expect("reload failed");
    assert_eq!(original, reloaded);
}

#[test]
fn config_save_round_trip_full() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
base_branch = "develop"
[sessions]
max_concurrent = 5
stall_timeout_secs = 600
default_model = "sonnet"
default_mode = "plan"
permission_mode = "default"
allowed_tools = ["Read", "Write"]
max_retries = 3
retry_cooldown_secs = 30
hollow_max_retries = 2
max_prompt_history = 50
[sessions.context_overflow]
overflow_threshold_pct = 85
auto_fork = false
commit_prompt_pct = 60
max_fork_depth = 3
[sessions.conflict]
enabled = false
policy = "kill"
[budget]
per_session_usd = 10.0
total_usd = 100.0
alert_threshold_pct = 90
[github]
issue_filter_labels = ["ready", "approved"]
auto_pr = false
cache_ttl_secs = 600
auto_merge = true
merge_method = "rebase"
[notifications]
desktop = false
slack = true
slack_webhook_url = "https://hooks.slack.com/test"
slack_rate_limit_per_min = 5
[gates]
enabled = false
test_command = "make test"
ci_poll_interval_secs = 60
ci_max_wait_secs = 3600
[gates.ci_auto_fix]
enabled = false
max_retries = 5
[concurrency]
heavy_task_labels = ["gpu", "large"]
heavy_task_limit = 1
[monitoring]
work_tick_interval_secs = 30
[flags]
ci_auto_fix = true
review_council = false
"#
    )
    .unwrap();
    let original = Config::load(f.path()).expect("load failed");
    let out = tempfile::NamedTempFile::new().unwrap();
    original.save(out.path()).expect("save failed");
    let reloaded = Config::load(out.path()).expect("reload failed");
    assert_eq!(original, reloaded);
}

#[test]
fn config_save_writes_all_sections() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
    )
    .unwrap();
    let original = Config::load(f.path()).unwrap();
    let out = tempfile::NamedTempFile::new().unwrap();
    original.save(out.path()).unwrap();
    let content = std::fs::read_to_string(out.path()).unwrap();
    assert!(content.contains("[project]"));
    assert!(content.contains("[sessions]"));
    assert!(content.contains("[budget]"));
    assert!(content.contains("max_concurrent"));
    assert!(content.contains("stall_timeout_secs"));
}

#[test]
fn config_save_round_trip_with_completion_gates() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
[sessions.completion_gates]
enabled = true
[[sessions.completion_gates.commands]]
name = "fmt"
run = "cargo fmt --check"
required = true
[[sessions.completion_gates.commands]]
name = "clippy"
run = "cargo clippy"
required = false
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
    )
    .unwrap();
    let original = Config::load(f.path()).expect("load failed");
    let out = tempfile::NamedTempFile::new().unwrap();
    original.save(out.path()).expect("save failed");
    let reloaded = Config::load(out.path()).expect("reload failed");
    assert_eq!(original, reloaded);
    assert_eq!(reloaded.sessions.completion_gates.commands.len(), 2);
}

#[test]
fn config_partial_eq_detects_difference() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(
        f,
        r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#
    )
    .unwrap();
    let mut cfg1 = Config::load(f.path()).unwrap();
    let cfg2 = Config::load(f.path()).unwrap();
    assert_eq!(cfg1, cfg2);
    cfg1.sessions.max_concurrent = 999;
    assert_ne!(cfg1, cfg2);
}
