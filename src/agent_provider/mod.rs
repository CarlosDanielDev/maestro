pub mod claude;
pub mod claude_pricing;
pub mod codex;
pub mod minimax;
pub mod ollama;
pub mod openai_compat;
pub mod opencode;
pub mod parser;
pub mod quota;
pub mod qwen;
mod qwen_parser;
#[cfg(test)]
mod qwen_tests;
#[cfg(test)]
pub(crate) mod test_fakes;
pub mod types;
#[cfg(test)]
mod types_tests;

/// Apply `[agents.<id>].env` to a command, skipping keys rejected by
/// [`crate::util::validate_agent_env_key`] (EXEC-4 / TRUST-2, #1029).
/// Config validation is the first line; this is the spawn-site backstop,
/// mirroring the plugin runner's re-validation.
pub(crate) fn apply_agent_env(
    cmd: &mut tokio::process::Command,
    env: &std::collections::BTreeMap<String, String>,
) {
    for (key, value) in env {
        if let Err(e) = crate::util::validate_agent_env_key(key) {
            tracing::warn!("Skipping unsafe env var {key}: {e}");
            continue;
        }
        cmd.env(key, value);
    }
}

#[allow(unused_imports)]
pub use claude::{ClaudeProvider, ClaudeTransport, InteractiveDriver};
#[allow(unused_imports)]
pub use codex::CodexProvider;
#[allow(unused_imports)]
pub use minimax::MinimaxProvider;
#[allow(unused_imports)]
pub use ollama::OllamaProvider;
#[allow(unused_imports)]
pub use opencode::OpenCodeProvider;
#[allow(unused_imports)]
pub use qwen::QwenProvider;
#[allow(unused_imports)]
pub use types::{
    AgentError, AgentHealthCheck, AgentOutputFormat, AgentProvider, AgentProviderDefinition,
    AgentProviderEvent, AgentProviderFactory, AgentProviderId, AgentProviderKind,
    AgentProvidersConfig, AgentRequest, AgentRunResult, AgentRunStarted, AgentTextOutput,
    ParserBinding,
};
