use std::sync::Arc;

use futures::future::join_all;

use crate::agent_provider::types::{AgentHealthCheck, AgentProvider, AgentProviderId};
use crate::agent_provider::{
    ClaudeProvider, CodexProvider, MinimaxProvider, OllamaProvider, OpenCodeProvider, QwenProvider,
};
use crate::config::Config;

pub fn cmd_doctor() -> anyhow::Result<()> {
    let config = Config::find_and_load().ok();
    let report = crate::doctor::run_all_checks(config.as_ref());
    crate::doctor::print_report(&report);

    if report.has_failures() {
        std::process::exit(1);
    }
    Ok(())
}

/// Library entrypoint for orchestration pre-flight (spec §8).
///
/// For each `agent_id`, instantiate the matching provider with default
/// binary/url/model and call `health_check()`. No subprocess spawn, no
/// `Config` required — best-effort defaults.
///
/// Unknown ids yield an `available = false` record with an informative
/// message rather than an error — callers iterate the result vector to
/// compute readiness for the team-launch wizard.
///
/// `cmd_doctor()` does not call this — it goes through the richer
/// `crate::doctor::run_all_checks` for git/gh/config/auth reporting.
#[allow(dead_code)]
pub async fn run_health_check(agent_ids: &[String]) -> Vec<AgentHealthCheck> {
    let probes = agent_ids.iter().map(|id| async move {
        match build_default_provider(id) {
            Some(provider) => provider
                .health_check()
                .await
                .unwrap_or_else(|e| AgentHealthCheck {
                    provider_id: AgentProviderId::new(id),
                    available: false,
                    version: None,
                    message: e.to_string(),
                }),
            None => AgentHealthCheck {
                provider_id: AgentProviderId::new(id),
                available: false,
                version: None,
                message: format!("unknown agent id `{id}`"),
            },
        }
    });
    join_all(probes).await
}

#[allow(dead_code)]
fn build_default_provider(id: &str) -> Option<Arc<dyn AgentProvider>> {
    match id {
        "claude" => Some(Arc::new(ClaudeProvider::default())),
        "codex" => Some(Arc::new(CodexProvider::new("codex"))),
        "qwen" => Some(Arc::new(QwenProvider::new("qwen"))),
        "opencode" => Some(Arc::new(OpenCodeProvider::new("opencode"))),
        "ollama" => OllamaProvider::new(
            "ollama".to_string(),
            "http://localhost:11434".to_string(),
            "llama3".to_string(),
            120,
            None,
        )
        .ok()
        .map(|p| Arc::new(p) as Arc<dyn AgentProvider>),
        "minimax" => MinimaxProvider::new(
            "minimax".to_string(),
            "https://api.minimax.io/v1".to_string(),
            "MiniMax-M2.7".to_string(),
            120,
            Some("MINIMAX_API_KEY".to_string()),
        )
        .ok()
        .map(|p| Arc::new(p) as Arc<dyn AgentProvider>),
        _ => None,
    }
}
