//! Entry-field schemas for dynamic-cardinality tables.
//!
//! Spec: `docs/superpowers/specs/2026-05-19-dynamic-config-editing.md` §6.3, §7.
//!
//! v0.29.0 scope ships scalar/list fields for `[agents.<id>]`, `[modes.<id>]`,
//! and `[[sessions.completion_gates.commands]]`. Nested `Map` fields (`env`,
//! `config_overrides`, `cli_flags`, `role_overrides`) are deferred to v0.30.0
//! per §7. `[teams.<id>]` ships in #803 because the `bindings: #[serde(flatten)]`
//! round-trip needs its own sync-time adapter.
//!
//! `AgentKind` and the permission-mode enum live on the strongly-typed
//! `AgentConfig` (see `src/config/agents.rs`); the constant slices below
//! mirror those variants so the TUI dropdowns stay in sync. A drift test in
//! `schema_tests.rs` locks these against the Rust enum.

use super::{DefaultValue, FieldKind, FieldSchema};

/// Allowed `agents.<id>.kind` values. Mirrors `AgentKind` variants.
pub(crate) const AGENT_KINDS: &[&str] =
    &["claude", "codex", "qwen", "opencode", "ollama", "minimax"];

/// Subset of [`AGENT_KINDS`] that runs as a local subprocess (CLI mode).
/// Mirrors `AgentKind::is_subprocess` so the TUI can hide HTTP-only
/// fields (base_url, api_key_env, request_timeout_secs) when kind is
/// one of these.
pub(crate) const SUBPROCESS_KINDS: &[&str] = &["claude", "codex", "qwen", "opencode"];

/// Subset of [`AGENT_KINDS`] that talks over HTTP. Mirrors
/// `AgentKind::is_http`. CLI-only fields (command, permission_mode,
/// allowed_tools, sandbox, extra_args) hide when kind is one of these.
pub(crate) const HTTP_KINDS: &[&str] = &["ollama", "minimax"];

/// Decide whether an `[agents.<id>]` field row should render for a given
/// kind value. Subprocess fields (command, permission_mode, sandbox,
/// allowed_tools, extra_args) hide when kind is an HTTP variant and
/// vice-versa. Unknown keys or unknown kinds default to visible —
/// hiding silently is worse than showing an extra row.
pub(crate) fn agent_field_visible_for_kind(field_key: &str, kind_value: &str) -> bool {
    let is_subprocess = SUBPROCESS_KINDS.contains(&kind_value);
    let is_http = HTTP_KINDS.contains(&kind_value);
    if !is_subprocess && !is_http {
        return true;
    }
    match field_key {
        "command" | "permission_mode" | "sandbox" | "allowed_tools" | "extra_args" => is_subprocess,
        "base_url" | "api_key_env" | "request_timeout_secs" => is_http,
        "num_ctx" => kind_value == "ollama",
        _ => true,
    }
}

/// Allowed `agents.<id>.permission_mode` values. Mirrors
/// `core::PERMISSION_MODES` so the two tabs surface identical choices.
pub(crate) const PERMISSION_MODES: &[&str] = &[
    "default",
    "acceptEdits",
    "bypassPermissions",
    "dontAsk",
    "plan",
    "auto",
];

/// Entry fields for `[agents.<id>]`. 12 scalar/list fields per spec §6.3
/// (11 from v0.29.0 + `num_ctx` for Ollama context-window control, #844).
/// `env`, `config_overrides`, `cli_flags` deferred to v0.30.0 (§7).
pub(crate) const AGENTS_ENTRY_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "kind",
        label: "Kind",
        help: "Agent provider kind",
        default: DefaultValue::Str("claude"),
        kind: FieldKind::Enum(AGENT_KINDS),
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "enabled",
        label: "Enabled",
        help: "Whether this agent is active",
        default: DefaultValue::Bool(true),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "command",
        label: "Command",
        help: "Subprocess command for claude/codex/qwen/opencode agents",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "base_url",
        label: "Base URL",
        help: "HTTP endpoint for ollama/minimax agents",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "model",
        label: "Model",
        help: "Model identifier override",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "num_ctx",
        label: "Context Window (num_ctx)",
        help: "Ollama context window in tokens (#844). Hidden for non-Ollama kinds. 0 = unset (use Ollama default).",
        default: DefaultValue::Int(0),
        kind: FieldKind::Int {
            min: 0,
            max: 1_000_000,
            step: 1024,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "extra_args",
        label: "Extra Args",
        help: "Additional CLI args appended to the subprocess command",
        default: DefaultValue::StrList(&[]),
        kind: FieldKind::StringList,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "permission_mode",
        label: "Permission Mode",
        help: "Per-agent permission-flow override",
        default: DefaultValue::Str("default"),
        kind: FieldKind::Enum(PERMISSION_MODES),
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "allowed_tools",
        label: "Allowed Tools",
        help: "Allowed-tools whitelist — empty means all tools",
        default: DefaultValue::StrList(&[]),
        kind: FieldKind::StringList,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "sandbox",
        label: "Sandbox",
        help: "Sandbox label (provider-specific)",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "request_timeout_secs",
        label: "Request Timeout (sec)",
        help: "Per-request timeout in seconds for HTTP agents",
        default: DefaultValue::Int(120),
        kind: FieldKind::Int {
            min: 0,
            max: 86_400,
            step: 30,
        },
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "api_key_env",
        label: "API Key Env Var",
        help: "Environment variable holding the API key",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
];

/// Entry fields for `[modes.<id>]`. 3 fields per spec §6.3.
pub(crate) const MODES_ENTRY_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "system_prompt",
        label: "System Prompt",
        help: "System-prompt override for this mode",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "allowed_tools",
        label: "Allowed Tools",
        help: "Allowed-tools whitelist — empty means all tools",
        default: DefaultValue::StrList(&[]),
        kind: FieldKind::StringList,
        validator: None,
        presentation: None,
    },
    FieldSchema {
        key: "permission_mode",
        label: "Permission Mode",
        help: "Permission-flow override for this mode",
        default: DefaultValue::Str("default"),
        kind: FieldKind::Enum(PERMISSION_MODES),
        validator: None,
        presentation: None,
    },
];

/// Entry fields for `[[sessions.completion_gates.commands]]`. 3 fields per
/// spec §6.3. Renderer wires this through a `DynamicRowsWidget` appended at
/// the end of the Sessions tab.
pub(crate) const COMPLETION_GATE_ENTRY_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        key: "name",
        label: "Name",
        help: "Activity-log display name (e.g. `fmt`, `clippy`)",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: Some(super::validate_non_empty),
        presentation: None,
    },
    FieldSchema {
        key: "run",
        label: "Run",
        help: "Shell command — exit code 0 means pass",
        default: DefaultValue::Str(""),
        kind: FieldKind::String,
        validator: Some(super::validate_non_empty),
        presentation: None,
    },
    FieldSchema {
        key: "required",
        label: "Required",
        help: "If true, failure blocks PR creation",
        default: DefaultValue::Bool(true),
        kind: FieldKind::Bool,
        validator: None,
        presentation: None,
    },
];
