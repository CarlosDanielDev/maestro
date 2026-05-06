pub mod claude;
pub mod codex;
pub mod minimax;
pub mod ollama;
pub mod openai_compat;
pub mod opencode;
pub mod qwen;
mod qwen_parser;
#[cfg(test)]
mod qwen_tests;
pub mod types;
#[cfg(test)]
mod types_tests;

#[allow(unused_imports)]
pub use claude::ClaudeProvider;
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
