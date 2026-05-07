//! Integration tests for L1 dispatch.
//!
//! Exercises `dispatch_subagent` end-to-end against a `FakeProvider`
//! injected via the production `AgentProviderFactory::with_default_provider`
//! path — no `cfg(test)` leakage into `src/orchestration/dispatch.rs`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent_provider::types::{
    AgentError, AgentHealthCheck, AgentProvider, AgentProviderEvent, AgentProviderFactory,
    AgentProviderId, AgentProviderKind, AgentRequest, AgentRunResult, AgentRunStarted,
    ParserBinding,
};
use crate::orchestration::contracts::{ReviewVerdict, SubagentError, SubagentResult};
use crate::orchestration::dispatch::{DispatchContext, dispatch_subagent};
use crate::orchestration::team::{ResolvedTeam, RoleBinding, SourceTier};
use crate::orchestration::types::{Primitive, TeamRole};
use crate::session::types::StreamEvent;

enum FakeBehavior {
    Text(String),
    SplitText { combined: String, mid: usize },
    Error(String),
    Silent,
}

struct FakeProvider {
    behavior: FakeBehavior,
}

impl FakeProvider {
    fn with_text(s: impl Into<String>) -> Self {
        Self {
            behavior: FakeBehavior::Text(s.into()),
        }
    }

    fn with_error(msg: impl Into<String>) -> Self {
        Self {
            behavior: FakeBehavior::Error(msg.into()),
        }
    }

    fn silent() -> Self {
        Self {
            behavior: FakeBehavior::Silent,
        }
    }

    fn split_text(combined: impl Into<String>, mid: usize) -> Self {
        Self {
            behavior: FakeBehavior::SplitText {
                combined: combined.into(),
                mid,
            },
        }
    }
}

#[async_trait]
impl AgentProvider for FakeProvider {
    fn id(&self) -> &str {
        "fake"
    }

    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Subprocess
    }

    fn parser_binding(&self) -> ParserBinding {
        ParserBinding::claude_stream_json()
    }

    async fn health_check(&self) -> Result<AgentHealthCheck, AgentError> {
        Ok(AgentHealthCheck {
            provider_id: AgentProviderId::new("fake"),
            available: true,
            version: None,
            message: "fake provider always healthy".into(),
        })
    }

    async fn run(
        &self,
        _request: AgentRequest,
        events: mpsc::UnboundedSender<AgentProviderEvent>,
        _cancel: CancellationToken,
    ) -> Result<AgentRunResult, AgentError> {
        let _ = events.send(AgentProviderEvent::Started(AgentRunStarted {
            process_id: None,
        }));
        match &self.behavior {
            FakeBehavior::Text(text) => {
                let _ = events.send(AgentProviderEvent::Stream(StreamEvent::AssistantMessage {
                    text: text.clone(),
                }));
                let _ = events.send(AgentProviderEvent::Stream(StreamEvent::Completed {
                    cost_usd: 0.0,
                }));
            }
            FakeBehavior::SplitText { combined, mid } => {
                let (a, b) = combined.split_at(*mid);
                let _ = events.send(AgentProviderEvent::Stream(StreamEvent::AssistantMessage {
                    text: a.to_string(),
                }));
                let _ = events.send(AgentProviderEvent::Stream(StreamEvent::AssistantMessage {
                    text: b.to_string(),
                }));
                let _ = events.send(AgentProviderEvent::Stream(StreamEvent::Completed {
                    cost_usd: 0.0,
                }));
            }
            FakeBehavior::Error(msg) => {
                let _ = events.send(AgentProviderEvent::Stream(StreamEvent::Error {
                    message: msg.clone(),
                }));
            }
            FakeBehavior::Silent => {}
        }
        Ok(AgentRunResult { exit_code: Some(0) })
    }
}

fn binding(agent: &str) -> RoleBinding {
    RoleBinding {
        agent: agent.into(),
        ..Default::default()
    }
}

fn make_team(primitive: Primitive, bindings: Vec<(TeamRole, &str)>) -> ResolvedTeam {
    let mut map: HashMap<TeamRole, RoleBinding> = HashMap::new();
    for (role, agent) in bindings {
        map.insert(role, binding(agent));
    }
    ResolvedTeam {
        name: "test-team".into(),
        primitive,
        min_agents: vec!["claude".into()],
        bindings: map,
        source_tier: SourceTier::BuiltIn,
    }
}

fn make_ctx(team: ResolvedTeam, fake: FakeProvider) -> DispatchContext {
    let factory = AgentProviderFactory::with_default_provider(Arc::new(fake));
    DispatchContext::new(team, None, "claude-sonnet-4-5").with_provider_factory(factory)
}

#[tokio::test]
async fn dispatch_reviewer_happy_path_returns_review_findings() {
    let raw = serde_json::to_string(&SubagentResult::ReviewFindings {
        verdict: ReviewVerdict::Approved,
        findings: vec![],
    })
    .expect("serialize");
    let team = make_team(Primitive::Pipeline, vec![(TeamRole::Reviewer, "claude")]);
    let ctx = make_ctx(team, FakeProvider::with_text(&raw));

    let result = dispatch_subagent(&ctx, TeamRole::Reviewer, "review the diff").await;
    match result {
        Ok(SubagentResult::ReviewFindings { verdict, .. }) => {
            assert_eq!(verdict, ReviewVerdict::Approved);
        }
        other => panic!("expected ReviewFindings, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_returns_provider_error_on_stream_error() {
    let team = make_team(Primitive::Pipeline, vec![(TeamRole::Reviewer, "claude")]);
    let ctx = make_ctx(team, FakeProvider::with_error("boom"));

    let result = dispatch_subagent(&ctx, TeamRole::Reviewer, "review").await;
    match result {
        Err(SubagentError::Provider(msg)) => {
            assert!(msg.contains("boom"), "expected `boom` in {msg}");
        }
        other => panic!("expected Provider error, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_missing_role_binding_returns_other_error() {
    let team = make_team(Primitive::Pipeline, vec![(TeamRole::Reviewer, "claude")]);
    let ctx = make_ctx(team, FakeProvider::with_text("{}"));

    let result = dispatch_subagent(&ctx, TeamRole::Implementer, "implement").await;
    match result {
        Err(SubagentError::Other(msg)) => {
            assert!(
                msg.to_lowercase().contains("implementer"),
                "expected `implementer` in {msg}"
            );
        }
        other => panic!("expected Other error, got {other:?}"),
    }
}

#[tokio::test]
async fn dispatch_implementer_with_plain_text_returns_shape_mismatch() {
    let team = make_team(Primitive::Pipeline, vec![(TeamRole::Implementer, "claude")]);
    let ctx = make_ctx(team, FakeProvider::with_text("hello world"));

    let result = dispatch_subagent(&ctx, TeamRole::Implementer, "implement").await;
    assert!(
        matches!(
            result,
            Err(SubagentError::ResultShapeMismatch {
                role: TeamRole::Implementer,
                ..
            })
        ),
        "expected ResultShapeMismatch, got {result:?}"
    );
}

#[tokio::test]
async fn dispatch_handles_empty_stream_as_malformed() {
    let team = make_team(Primitive::Pipeline, vec![(TeamRole::Reviewer, "claude")]);
    let ctx = make_ctx(team, FakeProvider::silent());

    let result = dispatch_subagent(&ctx, TeamRole::Reviewer, "review").await;
    assert!(
        matches!(result, Err(SubagentError::Malformed(_))),
        "expected Malformed, got {result:?}"
    );
}

#[tokio::test]
async fn dispatch_concatenates_multiple_assistant_messages() {
    let full_json = serde_json::to_string(&SubagentResult::ReviewFindings {
        verdict: ReviewVerdict::Approved,
        findings: vec![],
    })
    .expect("serialize");
    let mid = full_json.len() / 2;
    let team = make_team(Primitive::Pipeline, vec![(TeamRole::Reviewer, "claude")]);
    let ctx = make_ctx(team, FakeProvider::split_text(full_json, mid));

    let result = dispatch_subagent(&ctx, TeamRole::Reviewer, "review").await;
    assert!(
        matches!(
            result,
            Ok(SubagentResult::ReviewFindings {
                verdict: ReviewVerdict::Approved,
                ..
            })
        ),
        "expected ReviewFindings, got {result:?}"
    );
}
