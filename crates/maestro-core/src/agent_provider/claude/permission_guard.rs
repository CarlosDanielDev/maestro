//! #1395: a headless Claude run (`--print`, stream-json) has nobody to ask
//! for a permission: maestro is not an SDK host, passes no
//! `--permission-prompt-tool`, and gives the child a null stdin. With no
//! `--permission-mode` flag and no allowed tools, every tool that needs a
//! permission fails, so the run is refused before it spawns.

use crate::agent_provider::types::{AgentError, AgentOutputFormat, AgentRequest};

/// Why a headless run was refused. The TUI shows it in the activity log,
/// which raises an error toast.
pub const HEADLESS_NO_PERMISSION: &str = "Headless Claude cannot ask for permissions: \
     set sessions.permission_mode (acceptEdits/bypassPermissions) or allowed_tools in maestro.toml";

/// Refuse a headless stream-json run that has no permission mode flag
/// and no allowed tools. One-shot text turns are not affected.
pub(super) fn check(request: &AgentRequest) -> Result<(), AgentError> {
    let mode = request.permission_mode.as_deref().map_or("", str::trim);
    let no_mode_flag = mode.is_empty() || mode == "default";
    if matches!(request.output_format, AgentOutputFormat::StreamJson)
        && no_mode_flag
        && request.allowed_tools.is_empty()
    {
        return Err(AgentError::Config(HEADLESS_NO_PERMISSION.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::agent_provider::{
        AgentProvider, ClaudeProvider, ClaudeTransport, InteractiveDriver,
    };

    /// Never exists, so a run that passes the guard fails with `Spawn`
    /// and no real session starts.
    const SENTINEL: &str = "/nonexistent-sentinel-binary-maestro-1395";

    fn request(mode: Option<&str>, tools: &[&str]) -> AgentRequest {
        let mut request = AgentRequest::stream_json("hello".into(), "claude-sonnet".into());
        request.permission_mode = mode.map(str::to_string);
        request.allowed_tools = tools.iter().map(|t| t.to_string()).collect();
        request
    }

    async fn run(transport: ClaudeTransport, request: AgentRequest) -> Result<(), AgentError> {
        let provider = ClaudeProvider::with_transport(SENTINEL, transport);
        let (tx, _rx) = mpsc::unbounded_channel();
        provider
            .run(request, tx, CancellationToken::new())
            .await
            .map(|_| ())
    }

    fn is_refusal(result: &Result<(), AgentError>) -> bool {
        matches!(result, Err(AgentError::Config(msg)) if msg == HEADLESS_NO_PERMISSION)
    }

    #[tokio::test]
    async fn headless_default_without_tools_is_refused() {
        for mode in [None, Some("default"), Some(""), Some(" default ")] {
            let result = run(ClaudeTransport::Headless, request(mode, &[])).await;
            assert!(is_refusal(&result), "mode {mode:?}: got {result:?}");
        }
    }

    #[tokio::test]
    async fn headless_with_allowed_tools_spawns() {
        let result = run(ClaudeTransport::Headless, request(None, &["Edit"])).await;
        assert!(
            matches!(result, Err(AgentError::Spawn { .. })),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn headless_with_a_permission_mode_spawns() {
        for mode in ["bypassPermissions", "acceptEdits"] {
            let result = run(ClaudeTransport::Headless, request(Some(mode), &[])).await;
            assert!(
                matches!(result, Err(AgentError::Spawn { .. })),
                "{mode}: {result:?}"
            );
        }
    }

    #[tokio::test]
    async fn interactive_default_without_tools_spawns() {
        let transport = ClaudeTransport::Interactive(InteractiveDriver::PortablePty);
        let result = run(transport, request(None, &[])).await;
        assert!(
            matches!(result, Err(AgentError::Spawn { .. })),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn headless_text_turn_is_not_refused() {
        let request = AgentRequest::text("one shot", "claude-sonnet", None);
        let result = run(ClaudeTransport::Headless, request).await;
        assert!(
            matches!(result, Err(AgentError::Spawn { .. })),
            "{result:?}"
        );
    }
}
