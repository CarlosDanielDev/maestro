//! L1 — subagent dispatch.
//!
//! Translates a `Task(role)` call from L2 into a provider session, captures
//! stream events, and parses the emitted payload into a typed
//! [`SubagentResult`]. See spec §5 (L1 dispatch flow) and §4 (role contracts).
//!
//! ## Design notes
//!
//! - **Provider lookup:** v1 uses `factory.default_provider()` for every
//!   dispatched call. Multi-agent routing (resolve `RoleBinding.agent` to a
//!   distinct provider) is a follow-up — see TODO at the lookup site.
//! - **`ManagedSession` is not used.** L1 dispatch is one-shot ("spawn →
//!   drain → return"); `ManagedSession`'s pause/resume/worktree lifecycle is
//!   TUI-coupled and out of scope for L1. This honors spec §5's intent (a
//!   real provider session) without coupling to TUI plumbing.
//! - **Channel choice:** `mpsc::unbounded_channel` matches the existing
//!   `AgentProvider::run` signature. The receiver drains synchronously inside
//!   `drive_provider`, so unbounded does not materially risk memory growth
//!   for a single dispatched call.

#![allow(dead_code)]

use crate::agent_provider::types::{
    AgentProvider, AgentProviderEvent, AgentProviderFactory, AgentRequest,
};
use crate::config::Config;
use crate::modes::resolve_mode;
use crate::orchestration::contracts::{SubagentError, SubagentResult};
use crate::orchestration::team::{ResolvedTeam, RoleBinding};
use crate::orchestration::types::TeamRole;
use crate::session::types::StreamEvent;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Hard cap on accumulated assistant text per dispatch — guards against a
/// runaway provider OOMing the L1 caller. Typed `SubagentResult` payloads
/// never approach this; legitimate `Generic` JSON shouldn't either.
const MAX_ASSISTANT_TEXT_BYTES: usize = 1024 * 1024;

/// Carrier for everything `dispatch_subagent` needs that isn't the role or
/// instructions for *this* call. Built once per L2 issue run; reused across
/// every Task() call from that L2 session.
pub struct DispatchContext {
    pub team: ResolvedTeam,
    /// Optional Maestro config — used to resolve `[modes.<name>]` overrides.
    /// `None` is valid: built-in modes (`orchestrator`, `vibe`, `review`)
    /// still resolve.
    // Reason: Arc avoids deep-cloning Config across multiple DispatchContexts
    // sharing one issue run. Config is read-only after load, so no Mutex.
    pub config: Option<Arc<Config>>,
    /// Default model when a `RoleBinding.model_override` is `None`.
    pub default_model: String,
    /// Provider factory. Production code passes the config-derived factory;
    /// tests pass a fake-provider-backed factory through
    /// [`Self::with_provider_factory`].
    pub factory: AgentProviderFactory,
}

impl DispatchContext {
    pub fn new(
        team: ResolvedTeam,
        config: Option<Arc<Config>>,
        default_model: impl Into<String>,
    ) -> Self {
        Self {
            team,
            config,
            default_model: default_model.into(),
            factory: AgentProviderFactory::default(),
        }
    }

    /// Builder hook — used both in production (to inject a config-derived
    /// factory) and in tests (to inject a fake provider via
    /// `AgentProviderFactory::with_default_provider`).
    pub fn with_provider_factory(mut self, factory: AgentProviderFactory) -> Self {
        self.factory = factory;
        self
    }
}

/// Compose the prompt sent to the provider:
/// `mode_system_prompt + prompt_addendum + instructions`.
///
/// Empty sections collapse — no leading/trailing blank lines, no double
/// blank lines between adjacent non-empty sections. Internal whitespace
/// inside each section is preserved verbatim (so embedded code blocks or
/// indented lists survive).
pub fn compose_prompt(
    mode_system_prompt: &str,
    prompt_addendum: Option<&str>,
    instructions: &str,
) -> String {
    let mut sections: Vec<&str> = Vec::with_capacity(3);
    if !mode_system_prompt.is_empty() {
        sections.push(mode_system_prompt);
    }
    if let Some(a) = prompt_addendum
        && !a.is_empty()
    {
        sections.push(a);
    }
    if !instructions.is_empty() {
        sections.push(instructions);
    }
    sections.join("\n\n")
}

/// Validate the provider's raw output against `role.allowed_results()`.
///
/// Algorithm:
/// 1. Parse `raw` as `serde_json::Value`. Failure → `ResultShapeMismatch`
///    (the raw text is not JSON at all — that's a role-contract violation,
///    not a transport problem).
/// 2. Deserialize the `Value` into a typed `SubagentResult`. Success →
///    check the variant's kind against `role.allowed_results()`. If the
///    kind isn't allowed, return `ResultShapeMismatch`.
/// 3. Typed deserialization failure → if `role.allowed_results()` includes
///    `"generic"`, wrap the parsed `Value` as `SubagentResult::Generic`.
///    Otherwise return `ResultShapeMismatch`.
pub fn parse_result(role: TeamRole, raw: &str) -> Result<SubagentResult, SubagentError> {
    let trimmed = raw.trim();
    let allowed = role.allowed_results();
    let allows_generic = allowed.contains(&"generic");
    let expected = format!("one of {allowed:?}");

    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => {
            return Err(SubagentError::ResultShapeMismatch {
                role,
                expected,
                got: "non-JSON text".into(),
            });
        }
    };

    match serde_json::from_value::<SubagentResult>(value.clone()) {
        Ok(parsed) => {
            let kind = parsed.kind();
            if allowed.contains(&kind) {
                Ok(parsed)
            } else {
                Err(SubagentError::ResultShapeMismatch {
                    role,
                    expected,
                    got: kind.into(),
                })
            }
        }
        Err(_) if allows_generic => Ok(SubagentResult::Generic { json: value }),
        Err(_) => Err(SubagentError::ResultShapeMismatch {
            role,
            expected,
            got: "unrecognized JSON shape".into(),
        }),
    }
}

/// L1 entrypoint: dispatch one subagent invocation.
pub async fn dispatch_subagent(
    ctx: &DispatchContext,
    role: TeamRole,
    instructions: &str,
) -> Result<SubagentResult, SubagentError> {
    let binding = binding_for(&ctx.team, role)?;
    let (mode_prompt, allowed_tools, permission_mode) =
        resolve_role_mode(binding, ctx.config.as_deref(), role)?;

    let full_prompt = compose_prompt(
        &mode_prompt,
        binding.prompt_addendum.as_deref(),
        instructions,
    );
    let request = build_request(
        binding,
        &ctx.default_model,
        full_prompt,
        allowed_tools,
        permission_mode,
    );

    // TODO(L1-multi-provider): resolve `binding.agent` against a
    // multi-provider factory. v1 uses the default provider for every call.
    let provider = ctx.factory.default_provider();

    let raw = drive_provider(provider, request).await?;
    parse_result(role, &raw)
}

fn resolve_role_mode(
    binding: &RoleBinding,
    config: Option<&Config>,
    role: TeamRole,
) -> Result<(String, Vec<String>, Option<String>), SubagentError> {
    let Some(name) = binding.mode.as_deref() else {
        return Ok((String::new(), Vec::new(), None));
    };
    let Some(mode) = resolve_mode(name, config) else {
        return Err(SubagentError::Other(format!(
            "mode `{name}` not found for role {role:?}"
        )));
    };
    Ok((mode.system_prompt, mode.allowed_tools, mode.permission_mode))
}

fn binding_for(team: &ResolvedTeam, role: TeamRole) -> Result<&RoleBinding, SubagentError> {
    team.bindings.get(&role).ok_or_else(|| {
        SubagentError::Other(format!(
            "no binding for role {role:?} in team `{}`",
            team.name
        ))
    })
}

fn build_request(
    binding: &RoleBinding,
    default_model: &str,
    full_prompt: String,
    mode_allowed_tools: Vec<String>,
    mode_permission: Option<String>,
) -> AgentRequest {
    let model = binding
        .model_override
        .clone()
        .unwrap_or_else(|| default_model.to_string());
    let mut request = AgentRequest::text(full_prompt, model, None);
    request.permission_mode = mode_permission;
    request.allowed_tools = mode_allowed_tools;
    request
}

async fn drive_provider(
    provider: Arc<dyn AgentProvider>,
    request: AgentRequest,
) -> Result<String, SubagentError> {
    let (events_tx, mut events_rx) = mpsc::unbounded_channel::<AgentProviderEvent>();
    let cancel = CancellationToken::new();
    let cancel_for_run = cancel.clone();

    let run_handle =
        tokio::spawn(async move { provider.run(request, events_tx, cancel_for_run).await });

    let mut assistant_text = String::new();
    let mut saw_assistant = false;
    let mut stream_error: Option<String> = None;

    while let Some(event) = events_rx.recv().await {
        match event {
            AgentProviderEvent::Stream(StreamEvent::AssistantMessage { text }) => {
                assistant_text.push_str(&text);
                saw_assistant = true;
                if assistant_text.len() > MAX_ASSISTANT_TEXT_BYTES {
                    stream_error = Some(format!(
                        "assistant output exceeded {MAX_ASSISTANT_TEXT_BYTES} bytes"
                    ));
                    cancel.cancel();
                }
            }
            AgentProviderEvent::Stream(StreamEvent::Error { message }) => {
                stream_error = Some(message);
                cancel.cancel();
            }
            _ => {}
        }
    }

    let join_result = run_handle.await;

    if let Some(message) = stream_error {
        return Err(SubagentError::Provider(message));
    }

    match join_result {
        Err(join_err) => Err(SubagentError::Other(format!(
            "provider task panicked: {join_err}"
        ))),
        Ok(Err(agent_err)) => Err(agent_err.into()),
        Ok(Ok(_)) if !saw_assistant => Err(SubagentError::Malformed(
            "provider closed stream without AssistantMessage".into(),
        )),
        Ok(Ok(_)) => Ok(assistant_text),
    }
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
