//! #897 — per-role L1 provider routing.
//!
//! Exercises `dispatch_subagent`'s provider lookup: each `TeamRole` must
//! route to the provider bound to its `RoleBinding` via the chain
//! `agent` → `fallback_agent` → factory default. Uses a `RecordingProvider`
//! that logs its own id on `run()` so one assertion proves which binary
//! fired for each role.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent_provider::types::{
    AgentError, AgentHealthCheck, AgentProvider, AgentProviderEvent, AgentProviderFactory,
    AgentProviderId, AgentProviderKind, AgentRequest, AgentRunResult, ParserBinding,
};
use crate::orchestration::contracts::{ReviewVerdict, SubagentResult};
use crate::orchestration::dispatch::{DispatchContext, dispatch_subagent};
use crate::orchestration::team::{ResolvedTeam, RoleBinding, SourceTier};
use crate::orchestration::types::{Primitive, TeamRole};
use crate::session::types::StreamEvent;

/// Shared invocation log: each `run()` pushes the provider's id, so one
/// assertion covers the whole dispatch sequence. Mirrors `FakeLauncher`'s
/// `Mutex<Vec<_>>` recorder in `team_runner_tests.rs`.
type ProviderLog = Arc<Mutex<Vec<String>>>;

/// Provider with a configurable id that records its own id on `run()` and
/// emits a canned payload — lets the routing tests assert *which* binary
/// fired for each role.
struct RecordingProvider {
    provider_id: &'static str,
    response: String,
    log: ProviderLog,
}

#[async_trait]
impl AgentProvider for RecordingProvider {
    fn id(&self) -> &str {
        self.provider_id
    }

    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Subprocess
    }

    fn parser_binding(&self) -> ParserBinding {
        ParserBinding::claude_stream_json()
    }

    async fn health_check(&self) -> Result<AgentHealthCheck, AgentError> {
        Ok(AgentHealthCheck {
            provider_id: AgentProviderId::new(self.provider_id),
            available: true,
            version: None,
            message: "ok".into(),
        })
    }

    async fn run(
        &self,
        _request: AgentRequest,
        events: mpsc::UnboundedSender<AgentProviderEvent>,
        _cancel: CancellationToken,
    ) -> Result<AgentRunResult, AgentError> {
        self.log
            .lock()
            .expect("log lock")
            .push(self.provider_id.to_string());
        let _ = events.send(AgentProviderEvent::Stream(StreamEvent::AssistantMessage {
            text: self.response.clone(),
        }));
        let _ = events.send(AgentProviderEvent::Stream(StreamEvent::Completed {
            cost_usd: 0.0,
        }));
        Ok(AgentRunResult {
            exit_code: Some(0),
            session_id: None,
        })
    }
}

fn rec(id: &'static str, response: String, log: &ProviderLog) -> Arc<dyn AgentProvider> {
    Arc::new(RecordingProvider {
        provider_id: id,
        response,
        log: Arc::clone(log),
    })
}

fn review_json() -> String {
    serde_json::to_string(&SubagentResult::ReviewFindings {
        verdict: ReviewVerdict::Approved,
        findings: vec![],
    })
    .expect("serialize")
}

fn code_change_json() -> String {
    serde_json::to_string(&SubagentResult::CodeChange {
        files_touched: vec![],
        summary: "done".into(),
        commit_sha: None,
    })
    .expect("serialize")
}

fn docs_change_json() -> String {
    serde_json::to_string(&SubagentResult::DocsChange {
        files_touched: vec![],
        summary: "documented".into(),
    })
    .expect("serialize")
}

fn full_binding(agent: &str, fallback: Option<&str>) -> RoleBinding {
    RoleBinding {
        agent: agent.into(),
        fallback_agent: fallback.map(Into::into),
        ..Default::default()
    }
}

fn team_with(bindings: Vec<(TeamRole, RoleBinding)>) -> ResolvedTeam {
    let mut map: HashMap<TeamRole, RoleBinding> = HashMap::new();
    for (role, b) in bindings {
        map.insert(role, b);
    }
    ResolvedTeam {
        name: "test-team".into(),
        primitive: Primitive::Pipeline,
        min_agents: vec![],
        bindings: map,
        source_tier: SourceTier::BuiltIn,
    }
}

fn ctx_with_map(
    team: ResolvedTeam,
    default: Arc<dyn AgentProvider>,
    map: HashMap<String, Arc<dyn AgentProvider>>,
) -> DispatchContext {
    let factory = AgentProviderFactory::with_default_provider(default).with_agent_providers(map);
    DispatchContext::new(team, None, "claude-sonnet-4-5").with_provider_factory(factory)
}

#[tokio::test]
async fn dispatch_falls_back_to_fallback_agent_when_agent_empty() {
    let log: ProviderLog = Arc::new(Mutex::new(vec![]));
    let team = team_with(vec![(TeamRole::Reviewer, full_binding("", Some("qwen")))]);
    let mut map: HashMap<String, Arc<dyn AgentProvider>> = HashMap::new();
    map.insert("qwen".into(), rec("qwen", review_json(), &log));
    let ctx = ctx_with_map(team, rec("default", review_json(), &log), map);

    dispatch_subagent(&ctx, TeamRole::Reviewer, "review")
        .await
        .expect("dispatch ok");
    assert_eq!(*log.lock().expect("log lock"), vec!["qwen"]);
}

#[tokio::test]
async fn dispatch_uses_default_when_agent_empty_and_no_fallback() {
    let log: ProviderLog = Arc::new(Mutex::new(vec![]));
    let team = team_with(vec![(TeamRole::Reviewer, full_binding("", None))]);
    let ctx = ctx_with_map(team, rec("default", review_json(), &log), HashMap::new());

    dispatch_subagent(&ctx, TeamRole::Reviewer, "review")
        .await
        .expect("dispatch ok");
    assert_eq!(*log.lock().expect("log lock"), vec!["default"]);
}

#[tokio::test]
async fn dispatch_pipeline_each_role_routes_to_correct_provider() {
    let log: ProviderLog = Arc::new(Mutex::new(vec![]));
    let team = team_with(vec![
        (TeamRole::Implementer, full_binding("codex", None)),
        (TeamRole::Reviewer, full_binding("opencode", None)),
        (TeamRole::Docs, full_binding("qwen", None)),
    ]);
    let mut map: HashMap<String, Arc<dyn AgentProvider>> = HashMap::new();
    map.insert("codex".into(), rec("codex", code_change_json(), &log));
    map.insert("opencode".into(), rec("opencode", review_json(), &log));
    map.insert("qwen".into(), rec("qwen", docs_change_json(), &log));
    // Default records too: a role that wrongly routes here shows in the log.
    let ctx = ctx_with_map(team, rec("default", review_json(), &log), map);

    dispatch_subagent(&ctx, TeamRole::Implementer, "implement")
        .await
        .expect("implementer ok");
    dispatch_subagent(&ctx, TeamRole::Reviewer, "review")
        .await
        .expect("reviewer ok");
    dispatch_subagent(&ctx, TeamRole::Docs, "docs")
        .await
        .expect("docs ok");

    assert_eq!(
        *log.lock().expect("log lock"),
        vec!["codex", "opencode", "qwen"]
    );
}

/// #1000 — production wiring shape. `ProductionSchedulerRunner` builds its
/// factory as `AgentProviderFactory::default().with_agent_providers(map)`
/// (no explicit default provider; the claude default fills the default slot).
/// This proves that exact construction routes each role to its bound
/// provider, distinct from the tests above which inject a named default.
#[tokio::test]
async fn production_factory_shape_routes_all_roles_correctly() {
    let log: ProviderLog = Arc::new(Mutex::new(vec![]));
    let team = team_with(vec![
        (TeamRole::Implementer, full_binding("codex", None)),
        (TeamRole::Reviewer, full_binding("opencode", None)),
        (TeamRole::Docs, full_binding("qwen", None)),
    ]);
    let mut map: HashMap<String, Arc<dyn AgentProvider>> = HashMap::new();
    map.insert("codex".into(), rec("codex", code_change_json(), &log));
    map.insert("opencode".into(), rec("opencode", review_json(), &log));
    map.insert("qwen".into(), rec("qwen", docs_change_json(), &log));

    // Exact factory shape from team_launch.rs after #1000. Every binding has
    // a known id, so the claude default slot is never reached (no binary).
    let factory = AgentProviderFactory::default().with_agent_providers(map);
    let ctx = DispatchContext::new(team, None, "claude-sonnet-4-5").with_provider_factory(factory);

    dispatch_subagent(&ctx, TeamRole::Implementer, "implement")
        .await
        .expect("implementer ok");
    dispatch_subagent(&ctx, TeamRole::Reviewer, "review")
        .await
        .expect("reviewer ok");
    dispatch_subagent(&ctx, TeamRole::Docs, "docs")
        .await
        .expect("docs ok");

    assert_eq!(
        *log.lock().expect("log lock"),
        vec!["codex", "opencode", "qwen"],
        "each role routes to its bound provider, not the default"
    );
}
