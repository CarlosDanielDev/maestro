//! Shared fake `AgentProvider` implementations for tests.
//!
//! Two stub providers used across unit tests (in `src/session/pool.rs`) and
//! integration tests (in `src/integration_tests/templates_runtime.rs`) to
//! exercise the HTTP-provider rendered-template injection path introduced
//! in issue #707. Both implement `AgentProvider::run` as a no-op so the
//! pool can promote sessions against them without spawning processes.

#![cfg(test)]
#![allow(dead_code)]

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent_provider::AgentProvider;
use crate::agent_provider::types::{
    AgentError, AgentHealthCheck, AgentOutputFormat, AgentProviderEvent, AgentProviderId,
    AgentProviderKind, AgentRequest, AgentRunResult, ParserBinding,
};
use crate::templates::TemplateProviderRules;
use crate::templates::provider_rules::{claude_rules, http_generic_rules};

/// HTTP-generic provider stub. `template_rules().target_dir()` returns
/// `None`, so the runtime-injection gate fires for sessions configured
/// against this provider.
pub(crate) struct FakeHttpProvider;

#[async_trait]
impl AgentProvider for FakeHttpProvider {
    fn id(&self) -> &str {
        "qwen"
    }
    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Http
    }
    fn parser_binding(&self) -> ParserBinding {
        ParserBinding {
            name: "fake-http".to_string(),
            output_format: AgentOutputFormat::StreamJson,
        }
    }
    async fn health_check(&self) -> Result<AgentHealthCheck, AgentError> {
        Ok(AgentHealthCheck {
            provider_id: AgentProviderId::new(self.id()),
            available: true,
            version: None,
            message: "ok".to_string(),
        })
    }
    async fn run(
        &self,
        _request: AgentRequest,
        _events: mpsc::UnboundedSender<AgentProviderEvent>,
        _cancel: CancellationToken,
    ) -> Result<AgentRunResult, AgentError> {
        Ok(AgentRunResult {
            exit_code: None,
            session_id: None,
        })
    }
    fn template_rules(&self) -> &'static dyn TemplateProviderRules {
        http_generic_rules()
    }
}

/// Claude-like provider stub. `template_rules().target_dir()` returns
/// `Some(...)`, so the runtime-injection gate is skipped — Claude
/// discovers rendered templates on disk.
pub(crate) struct FakeClaudeProvider;

#[async_trait]
impl AgentProvider for FakeClaudeProvider {
    fn id(&self) -> &str {
        "claude"
    }
    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Subprocess
    }
    fn parser_binding(&self) -> ParserBinding {
        ParserBinding {
            name: "fake-claude".to_string(),
            output_format: AgentOutputFormat::StreamJson,
        }
    }
    async fn health_check(&self) -> Result<AgentHealthCheck, AgentError> {
        Ok(AgentHealthCheck {
            provider_id: AgentProviderId::new(self.id()),
            available: true,
            version: None,
            message: "ok".to_string(),
        })
    }
    async fn run(
        &self,
        _request: AgentRequest,
        _events: mpsc::UnboundedSender<AgentProviderEvent>,
        _cancel: CancellationToken,
    ) -> Result<AgentRunResult, AgentError> {
        Ok(AgentRunResult {
            exit_code: None,
            session_id: None,
        })
    }
    fn template_rules(&self) -> &'static dyn TemplateProviderRules {
        claude_rules()
    }
}

// ---------------------------------------------------------------------------
// Scripted provider for interaction-turn tests (#751)
// ---------------------------------------------------------------------------

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::agent_provider::types::AgentRunStarted;
use crate::session::types::StreamEvent;

/// How one scripted `run` call ends.
pub(crate) enum ScriptedEnd {
    /// `Ok(AgentRunResult { exit_code, session_id })`
    Ok {
        exit_code: Option<i32>,
        session_id: Option<&'static str>,
    },
    /// `Err(AgentError::Spawn { .. })`
    SpawnFail,
    /// `Err(AgentError::FailedStatus { status, stderr, .. })`
    FailedStatus {
        status: &'static str,
        stderr: &'static str,
    },
    /// Emit events, then park until the token cancels, then
    /// `Err(AgentError::Cancelled)`.
    WaitForCancel,
}

pub(crate) struct ScriptedTurn {
    pub events: Vec<StreamEvent>,
    pub end: ScriptedEnd,
}

/// Mock `AgentProvider` that pops one [`ScriptedTurn`] per `run` call and
/// records every [`AgentRequest`] so tests can assert `resume_session_id`
/// across turns (RUST-GUARDRAILS §7).
pub(crate) struct ScriptedProvider {
    turns: Mutex<VecDeque<ScriptedTurn>>,
    pub requests: Mutex<Vec<AgentRequest>>,
}

impl ScriptedProvider {
    pub(crate) fn new(turns: Vec<ScriptedTurn>) -> Self {
        Self {
            turns: Mutex::new(turns.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl AgentProvider for ScriptedProvider {
    fn id(&self) -> &str {
        "scripted"
    }
    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Subprocess
    }
    fn parser_binding(&self) -> ParserBinding {
        ParserBinding::claude_stream_json()
    }
    async fn health_check(&self) -> Result<AgentHealthCheck, AgentError> {
        Ok(AgentHealthCheck {
            provider_id: AgentProviderId::new(self.id()),
            available: true,
            version: None,
            message: "ok".to_string(),
        })
    }
    async fn run(
        &self,
        request: AgentRequest,
        events: mpsc::UnboundedSender<AgentProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<AgentRunResult, AgentError> {
        self.requests.lock().unwrap().push(request);
        let turn = self
            .turns
            .lock()
            .unwrap()
            .pop_front()
            .expect("ScriptedProvider: more run calls than scripted turns");

        if matches!(turn.end, ScriptedEnd::SpawnFail) {
            return Err(AgentError::Spawn {
                provider_id: self.id().to_string(),
                source: std::io::Error::other("scripted spawn failure"),
            });
        }

        let _ = events.send(AgentProviderEvent::Started(AgentRunStarted {
            process_id: Some(7),
        }));
        for event in turn.events {
            let _ = events.send(AgentProviderEvent::Stream(event));
        }

        match turn.end {
            ScriptedEnd::Ok {
                exit_code,
                session_id,
            } => Ok(AgentRunResult {
                exit_code,
                session_id: session_id.map(str::to_string),
            }),
            ScriptedEnd::FailedStatus { status, stderr } => Err(AgentError::FailedStatus {
                provider_id: self.id().to_string(),
                status: status.to_string(),
                stderr: stderr.to_string(),
            }),
            ScriptedEnd::WaitForCancel => {
                cancel.cancelled().await;
                Err(AgentError::Cancelled {
                    provider_id: self.id().to_string(),
                })
            }
            ScriptedEnd::SpawnFail => unreachable!("handled above"),
        }
    }
    fn template_rules(&self) -> &'static dyn TemplateProviderRules {
        claude_rules()
    }
}
