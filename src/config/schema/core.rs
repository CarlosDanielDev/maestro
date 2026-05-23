//! Field arrays for the early tables: project, sessions and nested
//! sub-sections, budget, github, notifications.

use super::dynamic::COMPLETION_GATE_ENTRY_FIELDS;
use super::{DefaultValue, FieldKind, FieldSchema, validate_non_empty, validate_url_or_empty};

const PERMISSION_MODES: &[&str] = &[
    "default",
    "acceptEdits",
    "bypassPermissions",
    "dontAsk",
    "plan",
    "auto",
];
const HOLLOW_POLICIES: &[&str] = &["always", "intent-aware", "never"];
const CONFLICT_POLICIES: &[&str] = &["warn", "pause", "kill"];
const MERGE_METHODS: &[&str] = &["merge", "squash", "rebase"];

pub(super) const PROJECT_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "repo",
        label: "Repository",
        help: "GitHub `owner/repo` or Azure DevOps project — used by `gh`/`az` CLI calls",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: Some(validate_non_empty),
        presentation: None,
    },
    FieldSchema {
        key: "base_branch",
        label: "Base Branch",
        help: "Default branch worktrees fork from",
        default: DefaultValue::Str("main"),
        kind: FieldKind::String,
        validator: Some(validate_non_empty),
        presentation: None,
    },
    FieldSchema {
        key: "language",
        label: "Primary Language",
        help: "Primary detected stack id (e.g. `rust`, `node`, `python`, `go`) — set by `maestro init`",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "languages",
        label: "Detected Languages",
        help: "All detected stack ids when the project is polyglot",
        default: DefaultValue::StrList(&[]),
        kind: FieldKind::StringList,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "build_command",
        label: "Build Command",
        help: "Stack-appropriate build command (e.g. `cargo build`, `npm run build`)",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "test_command",
        label: "Test Command",
        help: "Stack-appropriate test command (e.g. `cargo test`, `npm test`)",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "run_command",
        label: "Run Command",
        help: "Stack-appropriate run command (e.g. `cargo run`, `npm start`)",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
];

pub(super) const SESSIONS_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "max_concurrent",
        label: "Max Concurrent Sessions",
        help: "Maximum simultaneous Claude sessions",
        default: DefaultValue::Int(3),
        kind: FieldKind::Int {
            min: 1,
            max: 20,
            step: 1,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "stall_timeout_secs",
        label: "Stall Timeout (sec)",
        help: "Mark a session stalled after no output for this many seconds",
        default: DefaultValue::Int(300),
        kind: FieldKind::Int {
            min: 30,
            max: 3600,
            step: 30,
        },
        validator: None,
        presentation: None,
    },
    // `default_model` and `permission_mode` removed from the Sessions
    // schema in favor of per-provider configuration. Both fields stay on
    // SessionsConfig (serde defaults preserve old TOML round-trip), but
    // the renderer surfaces them via `[agents.<id>].model` and
    // `[agents.<id>].permission_mode` now. `default_mode` stays — it's
    // a maestro-level mode (orchestrator vs vibe), not a provider concept.
    FieldSchema {
        key: "default_mode",
        label: "Default Mode",
        help: "Session mode used when none is set explicitly",
        default: DefaultValue::Str("orchestrator"),
        kind: FieldKind::String,
        validator: Some(validate_non_empty),
        presentation: None,
    },
    FieldSchema {
        key: "allowed_tools",
        label: "Allowed Tools",
        help: "Tool whitelist passed to Claude — empty means all tools",
        default: DefaultValue::StrList(&[]),
        kind: FieldKind::StringList,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "max_retries",
        label: "Max Retries",
        help: "Retries for failed or stalled sessions",
        default: DefaultValue::Int(2),
        kind: FieldKind::Int {
            min: 0,
            max: 10,
            step: 1,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "retry_cooldown_secs",
        label: "Retry Cooldown (sec)",
        help: "Cooldown between retries",
        default: DefaultValue::Int(60),
        kind: FieldKind::Int {
            min: 0,
            max: 600,
            step: 10,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "max_prompt_history",
        label: "Max Prompt History",
        help: "Maximum prompt-history entries retained per session",
        default: DefaultValue::Int(100),
        kind: FieldKind::Int {
            min: 0,
            max: 10_000,
            step: 10,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "session_history_cap",
        label: "Session History Cap",
        help: "How many completed sessions to persist across restarts (0 disables history — top-bar cost resets on relaunch)",
        default: DefaultValue::Int(10),
        kind: FieldKind::Int {
            min: 0,
            max: 1000,
            step: 1,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "guardrail_prompt",
        label: "Guardrail Prompt",
        help: "Custom guardrail injected into the system prompt — empty falls back to language-based default",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "hollow_retry",
        label: "Hollow Retry",
        help: "Retry policy and per-intent limits for hollow completions",
        default: DefaultValue::Nested,
        kind: FieldKind::NestedTable(HOLLOW_RETRY_FIELDS),
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "context_overflow",
        label: "Context Overflow",
        help: "Behaviour when a session approaches the context limit",
        default: DefaultValue::Nested,
        kind: FieldKind::NestedTable(CONTEXT_OVERFLOW_FIELDS),
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "conflict",
        label: "Conflict Detection",
        help: "Detect and handle concurrent edits across sessions",
        default: DefaultValue::Nested,
        kind: FieldKind::NestedTable(CONFLICT_FIELDS),
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "completion_gates",
        label: "Completion Gates",
        help: "Commands run after session completion before PR creation",
        default: DefaultValue::Nested,
        kind: FieldKind::NestedTable(COMPLETION_GATES_FIELDS),
        validator: None,
        presentation: None,
    },
];

pub(super) const COMPLETION_GATES_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "enabled",
        label: "Completion Gates Enabled",
        help: "Master switch for completion-gate commands",
        default: DefaultValue::Bool(true),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "commands",
        label: "Commands",
        help: "Ordered list of gate commands — declaration order is execution order",
        default: DefaultValue::Empty,
        kind: FieldKind::VecOfStruct {
            entry_fields: COMPLETION_GATE_ENTRY_FIELDS,
        },
        validator: None,
        presentation: Some(super::Presentation::Rows),
    },
];

pub(super) const HOLLOW_RETRY_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "policy",
        label: "Hollow Retry Policy",
        help: "When to retry hollow (empty-output) completions",
        default: DefaultValue::Str("intent-aware"),
        kind: FieldKind::Enum(HOLLOW_POLICIES),
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "work_max_retries",
        label: "Work Session Retries",
        help: "Retries for hollow completions in work sessions",
        default: DefaultValue::Int(2),
        kind: FieldKind::Int {
            min: 0,
            max: 10,
            step: 1,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "consultation_max_retries",
        label: "Consultation Retries",
        help: "Retries for hollow completions in consultation sessions",
        default: DefaultValue::Int(0),
        kind: FieldKind::Int {
            min: 0,
            max: 10,
            step: 1,
        },
        validator: None,
        presentation: None,
    },
];

pub(super) const CONTEXT_OVERFLOW_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "overflow_threshold_pct",
        label: "Overflow Threshold (%)",
        help: "Context-usage percentage at which auto-fork triggers",
        default: DefaultValue::Int(70),
        kind: FieldKind::Int {
            min: 10,
            max: 100,
            step: 5,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "auto_fork",
        label: "Auto Fork",
        help: "Automatically fork on overflow instead of stalling",
        default: DefaultValue::Bool(true),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "commit_prompt_pct",
        label: "Commit Prompt (%)",
        help: "Context percentage at which to prompt a periodic commit",
        default: DefaultValue::Int(50),
        kind: FieldKind::Int {
            min: 10,
            max: 100,
            step: 5,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "max_fork_depth",
        label: "Max Fork Depth",
        help: "Cap fork chains to prevent runaway forking",
        default: DefaultValue::Int(5),
        kind: FieldKind::Int {
            min: 1,
            max: 20,
            step: 1,
        },
        validator: None,
        presentation: None,
    },
];

pub(super) const CONFLICT_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "enabled",
        label: "Conflict Detection",
        help: "Detect real-time file conflicts between sessions",
        default: DefaultValue::Bool(true),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "policy",
        label: "Conflict Policy",
        help: "What to do when a conflict is detected",
        default: DefaultValue::Str("warn"),
        kind: FieldKind::Enum(CONFLICT_POLICIES),
        validator: None,
        presentation: None,
    },
];

pub(super) const BUDGET_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "per_session_usd",
        label: "Per-Session Budget (USD)",
        help: "Hard cap per session before alerts and termination",
        default: DefaultValue::Float(5.0),
        kind: FieldKind::Float {
            min: 0.1,
            max: 100.0,
            step: 0.5,
            display_scale: 10,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "total_usd",
        label: "Total Budget (USD)",
        help: "Aggregate budget across all sessions",
        default: DefaultValue::Float(50.0),
        kind: FieldKind::Float {
            min: 0.1,
            max: 1000.0,
            step: 5.0,
            display_scale: 10,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "alert_threshold_pct",
        label: "Alert Threshold (%)",
        help: "Percentage of budget at which to surface a warning",
        default: DefaultValue::Int(80),
        kind: FieldKind::Int {
            min: 10,
            max: 100,
            step: 5,
        },
        validator: None,
        presentation: None,
    },
];

pub(super) const GITHUB_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "issue_filter_labels",
        label: "Issue Filter Labels",
        help: "Only fetch issues with at least one of these labels",
        default: DefaultValue::StrList(&["maestro:ready"]),
        kind: FieldKind::StringList,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "auto_pr",
        label: "Auto Create PR",
        help: "Open a PR automatically when a session finishes cleanly",
        default: DefaultValue::Bool(true),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "cache_ttl_secs",
        label: "Cache TTL (sec)",
        help: "How long to cache issue data before refetching",
        default: DefaultValue::Int(300),
        kind: FieldKind::Int {
            min: 30,
            max: 3600,
            step: 30,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "auto_merge",
        label: "Auto Merge",
        help: "Merge PRs automatically once all required checks pass",
        default: DefaultValue::Bool(false),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "merge_method",
        label: "Merge Method",
        help: "Strategy used when merging PRs",
        default: DefaultValue::Str("squash"),
        kind: FieldKind::Enum(MERGE_METHODS),
        validator: None,
        presentation: None,
    },
];

pub(super) const NOTIFICATIONS_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "desktop",
        label: "Desktop Notifications",
        help: "Show native desktop notifications on session events",
        default: DefaultValue::Bool(true),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "slack",
        label: "Slack Notifications",
        help: "Send notifications to Slack via webhook",
        default: DefaultValue::Bool(false),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "slack_webhook_url",
        label: "Slack Webhook URL",
        help: "Incoming-webhook URL — leave empty to disable Slack",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: Some(validate_url_or_empty),
        presentation: None,
    },
    FieldSchema {
        key: "slack_rate_limit_per_min",
        label: "Slack Rate Limit (per min)",
        help: "Cap on Slack messages per minute",
        default: DefaultValue::Int(10),
        kind: FieldKind::Int {
            min: 1,
            max: 60,
            step: 1,
        },
        validator: None,
        presentation: None,
    },
];
