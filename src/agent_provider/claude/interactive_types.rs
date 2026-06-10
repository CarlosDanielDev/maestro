//! Shared types + pure helpers for the interactive (PTY) transport (#749) —
//! split from `interactive.rs` to honor the 400-line guardrail. The turn
//! loop lives in `interactive.rs`; the portable-pty backend in
//! `pty_backend.rs`.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::agent_provider::types::{AgentError, AgentRequest};

const DEFAULT_SPAWN_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_READINESS_TIMEOUT: Duration = Duration::from_secs(30);

/// Env vars always removed from the interactive child (grep-able review
/// surface). The prefix sweep below catches future `ANTHROPIC_*` additions.
pub(super) const SCRUBBED_VARS: &[&str] = &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];
/// Any var with this prefix is scrubbed, beyond the fixed list.
pub(super) const SCRUBBED_PREFIX: &str = "ANTHROPIC_";

/// What to spawn. Built by `run_session_turn`, consumed by a backend.
pub(super) struct SpawnSpec {
    pub binary: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Var *names* to remove from the child env.
    pub env_remove: Vec<String>,
    /// Extra vars to set (tests use this to redirect `HOME` at a stub).
    pub env_set: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
}

/// A spawned interactive child. Implementations must be cheap to call from
/// async context (writes are small and buffered).
pub(super) trait InteractiveChild: Send {
    fn write_text(&mut self, text: &str) -> Result<(), AgentError>;
    fn try_wait(&mut self) -> Result<Option<i32>, AgentError>;
    fn kill(&mut self) -> Result<(), AgentError>;
    fn process_id(&self) -> Option<u32>;
}

/// Spawns interactive children. `PortablePtyBackend` is the production impl;
/// tests inject mocks (RUST-GUARDRAILS.md §7).
pub(super) trait InteractiveBackend: Send + Sync {
    fn spawn(&self, spec: &SpawnSpec) -> Result<Box<dyn InteractiveChild>, AgentError>;
}

/// Per-run parameters that don't belong on [`AgentRequest`].
pub(super) struct RunSpec {
    pub binary: String,
    pub provider_id: String,
    /// Home directory of the *child* (transcript root). Production passes
    /// `$HOME`; tests pass a tempdir and mirror it via `extra_env`.
    pub home_dir: PathBuf,
    /// Pre-generated session UUID (pins the transcript path).
    pub session_id: String,
    pub extra_env: Vec<(String, String)>,
    pub spawn_timeout: Duration,
    pub readiness_timeout: Duration,
}

impl RunSpec {
    pub(super) fn new(
        binary: impl Into<String>,
        provider_id: impl Into<String>,
        home_dir: PathBuf,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            binary: binary.into(),
            provider_id: provider_id.into(),
            home_dir,
            session_id: session_id.into(),
            extra_env: Vec::new(),
            spawn_timeout: DEFAULT_SPAWN_TIMEOUT,
            readiness_timeout: DEFAULT_READINESS_TIMEOUT,
        }
    }
}

/// Claude Code's project-dir munge: `/` and `.` both become `-`.
pub(super) fn munge_project_dir(cwd: &Path) -> String {
    cwd.to_string_lossy().replace(['/', '.'], "-")
}

/// Transcript JSONL path for a session rooted at `home`.
pub(super) fn transcript_path(home: &Path, cwd: &Path, session_id: &str) -> PathBuf {
    home.join(".claude")
        .join("projects")
        .join(munge_project_dir(cwd))
        .join(format!("{session_id}.jsonl"))
}

/// Resolve the scrub list from an iterator of env var names: the fixed
/// [`SCRUBBED_VARS`] plus every name matching [`SCRUBBED_PREFIX`].
pub(super) fn scrub_env_names(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = SCRUBBED_VARS.iter().map(|s| s.to_string()).collect();
    for name in names {
        if name.starts_with(SCRUBBED_PREFIX) && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// Common args after the identity pair (`--session-id`/`--resume`), which
/// [`run_session_turn`]'s spawn path owns.
pub(super) fn build_args(request: &AgentRequest) -> Vec<String> {
    let mut args = vec!["--model".to_string(), request.model.clone()];
    if let Some(mode) = request.permission_mode.as_deref()
        && !mode.is_empty()
        && mode != "default"
    {
        args.push("--permission-mode".to_string());
        args.push(mode.to_string());
    }
    args
}

/// Wrap the prompt in bracketed-paste markers so embedded newlines do not
/// submit early, then submit with `\r` (spike #747: `\r` submits, `\n`
/// inserts a newline in the composer).
pub(super) fn turn_payload(prompt: &str) -> String {
    format!("\x1b[200~{prompt}\x1b[201~\r")
}
