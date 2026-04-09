pub fn cmd_init() -> anyhow::Result<()> {
    let path = std::path::PathBuf::from("maestro.toml");
    if path.exists() {
        println!("maestro.toml already exists.");
        return Ok(());
    }

    let content = r#"[project]
repo = ""
base_branch = "main"

[sessions]
max_concurrent = 3
stall_timeout_secs = 300
default_model = "opus"
default_mode = "orchestrator"
permission_mode = "bypassPermissions"  # Options: default, acceptEdits, bypassPermissions, dontAsk, plan, auto
allowed_tools = []                      # Empty = all tools. Example: ["Bash", "Read", "Write", "Edit"]

[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80

[github]
issue_filter_labels = ["maestro:ready"]
auto_pr = true
auto_merge = false                      # Set to true to auto-merge PRs after CI + review pass
merge_method = "squash"                 # Options: merge, squash, rebase
cache_ttl_secs = 300

[gates]
enabled = true
test_command = "cargo test"
ci_poll_interval_secs = 30
ci_max_wait_secs = 1800

[notifications]
desktop = true
slack = false
# slack_webhook_url = "https://hooks.slack.com/services/T.../B.../xxx"
# slack_rate_limit_per_min = 10

[review]
enabled = false
command = "gh pr review {pr_number} --comment --body 'Automated review by Maestro'"
# reviewers = [
#   { name = "claude", command = "claude --print 'review PR #{pr_number}'", required = true },
#   { name = "codex", command = "codex review {pr_number}", required = false },
# ]

[concurrency]
heavy_task_labels = []                  # Labels that mark a task as resource-intensive
heavy_task_limit = 2                    # Max concurrent heavy tasks

[monitoring]
work_tick_interval_secs = 10

# Plugin hooks — shell commands triggered on lifecycle events
# [[plugins]]
# name = "notify-team"
# on = "session_completed"             # Hook points: session_started, session_completed, tests_passed,
#                                      #   tests_failed, budget_threshold, file_conflict, pr_created
# run = "curl -X POST https://slack.webhook/..."
# timeout_secs = 30                    # Optional per-plugin timeout

# Feature flag overrides
[flags]
# continuous_mode = true   # default: true
# auto_fork = true         # default: true
# ci_auto_fix = false      # default: false
# review_council = false   # experimental — disabled by default
# model_routing = false    # experimental — disabled by default
# context_overflow = false # default: false

# Custom modes — define system prompt and allowed tools
# [modes.review]
# system_prompt = "You are a code reviewer. Review the PR and leave comments."
# allowed_tools = ["Read", "Grep", "Glob"]
# permission_mode = "plan"
"#;

    std::fs::write(&path, content)?;
    println!("Created maestro.toml");
    Ok(())
}
