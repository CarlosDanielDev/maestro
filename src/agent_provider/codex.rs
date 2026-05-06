use std::collections::{BTreeMap, HashMap};
use std::process::Stdio;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::types::{
    AgentError, AgentHealthCheck, AgentOutputFormat, AgentProvider, AgentProviderEvent,
    AgentProviderId, AgentProviderKind, AgentRequest, AgentRunResult, AgentRunStarted,
    ParserBinding,
};
use crate::session::types::{StreamEvent, TokenUsage};

#[derive(Debug, Clone)]
pub struct CodexProvider {
    binary: String,
    sandbox: String,
    ephemeral: bool,
    profile: Option<String>,
    config_overrides: BTreeMap<String, toml::Value>,
    extra_args: Vec<String>,
    env: BTreeMap<String, String>,
    json: bool,
}

impl CodexProvider {
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            sandbox: "workspace-write".to_string(),
            ephemeral: false,
            profile: None,
            config_overrides: BTreeMap::new(),
            extra_args: Vec::new(),
            env: BTreeMap::new(),
            json: true,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_config(
        binary: impl Into<String>,
        sandbox: Option<String>,
        ephemeral: Option<bool>,
        profile: Option<String>,
        config_overrides: BTreeMap<String, toml::Value>,
        extra_args: Vec<String>,
        env: BTreeMap<String, String>,
        json: Option<bool>,
    ) -> Self {
        let sandbox = sandbox
            .filter(|sandbox| !sandbox.trim().is_empty())
            .unwrap_or_else(|| "workspace-write".to_string());
        Self {
            binary: binary.into(),
            sandbox,
            ephemeral: ephemeral.unwrap_or(false),
            profile,
            config_overrides,
            extra_args,
            env,
            json: json.unwrap_or(true),
        }
    }

    pub fn build_stream_args(&self, request: &AgentRequest) -> Vec<String> {
        let mut args = vec!["exec".to_string()];
        if self.json {
            args.push("--json".to_string());
        }
        self.push_common_args(&mut args, request);
        args
    }

    pub fn build_text_args(&self, request: &AgentRequest) -> Vec<String> {
        let mut args = vec!["exec".to_string()];
        self.push_common_args(&mut args, request);
        args
    }

    pub fn health_check_blocking(&self) -> AgentHealthCheck {
        match std::process::Command::new(&self.binary)
            .arg("--version")
            .output()
        {
            Ok(out) if out.status.success() => {
                let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
                AgentHealthCheck {
                    provider_id: AgentProviderId::new(self.id()),
                    available: true,
                    version: Some(version.clone()),
                    message: version,
                }
            }
            Ok(out) => AgentHealthCheck {
                provider_id: AgentProviderId::new(self.id()),
                available: false,
                version: None,
                message: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            },
            Err(_) => AgentHealthCheck {
                provider_id: AgentProviderId::new(self.id()),
                available: false,
                version: None,
                message: "not installed".to_string(),
            },
        }
    }

    fn push_common_args(&self, args: &mut Vec<String>, request: &AgentRequest) {
        if !request.model.trim().is_empty() {
            args.push("--model".to_string());
            args.push(request.model.clone());
        }

        args.push("--sandbox".to_string());
        args.push(self.sandbox.clone());

        if let Some(cwd) = request.cwd.as_ref() {
            args.push("--cd".to_string());
            args.push(cwd.display().to_string());
        }

        if self.ephemeral {
            args.push("--ephemeral".to_string());
        }

        if let Some(profile) = self
            .profile
            .as_deref()
            .filter(|profile| !profile.is_empty())
        {
            args.push("--profile".to_string());
            args.push(profile.to_string());
        }

        for (key, value) in &self.config_overrides {
            args.push("--config".to_string());
            args.push(format!("{key}={}", codex_config_value(value)));
        }

        for image in &request.images {
            args.push("--image".to_string());
            args.push(image.display().to_string());
        }

        args.extend(self.extra_args.iter().cloned());
        args.push(codex_prompt(request));
    }
}

impl Default for CodexProvider {
    fn default() -> Self {
        Self::new("codex")
    }
}

#[async_trait::async_trait]
impl AgentProvider for CodexProvider {
    fn id(&self) -> &str {
        "codex"
    }

    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Subprocess
    }

    fn parser_binding(&self) -> ParserBinding {
        ParserBinding {
            name: "codex-json".to_string(),
            output_format: AgentOutputFormat::StreamJson,
        }
    }

    async fn health_check(&self) -> Result<AgentHealthCheck, AgentError> {
        match Command::new(&self.binary).arg("--version").output().await {
            Ok(out) if out.status.success() => {
                let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
                Ok(AgentHealthCheck {
                    provider_id: AgentProviderId::new(self.id()),
                    available: true,
                    version: Some(version.clone()),
                    message: version,
                })
            }
            Ok(out) => Ok(AgentHealthCheck {
                provider_id: AgentProviderId::new(self.id()),
                available: false,
                version: None,
                message: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            }),
            Err(source) => Err(AgentError::Spawn {
                provider_id: self.id().to_string(),
                source,
            }),
        }
    }

    async fn run(
        &self,
        request: AgentRequest,
        events: mpsc::UnboundedSender<AgentProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<AgentRunResult, AgentError> {
        let mut cmd = Command::new(&self.binary);
        match request.output_format {
            AgentOutputFormat::StreamJson => {
                cmd.args(self.build_stream_args(&request));
            }
            AgentOutputFormat::Text => {
                cmd.args(self.build_text_args(&request));
            }
        }
        cmd.envs(&self.env);
        if let Some(cwd) = request.cwd.as_ref() {
            cmd.current_dir(cwd);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|source| AgentError::Spawn {
            provider_id: self.id().to_string(),
            source,
        })?;
        let process_id = child.id();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AgentError::Stream("No stdout from codex CLI".to_string()))?;
        let stderr = child.stderr.take();

        let _ = events.send(AgentProviderEvent::Started(AgentRunStarted { process_id }));

        let stdout_events = events.clone();
        let stdout_task = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut parser = CodexStreamParser::default();
            let mut got_result = false;

            while let Ok(Some(line)) = lines.next_line().await {
                let parsed = parser.parse_line(&line);
                for event in parsed {
                    if matches!(event, StreamEvent::Completed { .. }) {
                        got_result = true;
                    }
                    let _ = stdout_events.send(AgentProviderEvent::Stream(event));
                }
            }

            if !got_result {
                let _ = stdout_events.send(AgentProviderEvent::Stream(StreamEvent::Completed {
                    cost_usd: 0.0,
                }));
            }
        });

        let stderr_task = stderr.map(|stderr| {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                let mut stderr_buf = String::new();

                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        if !stderr_buf.is_empty() {
                            stderr_buf.push('\n');
                        }
                        stderr_buf.push_str(&line);
                    }
                }
                stderr_buf
            })
        });

        let status = tokio::select! {
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                return Err(AgentError::Cancelled {
                    provider_id: self.id().to_string(),
                });
            }
            status = child.wait() => status.map_err(|err| AgentError::Stream(err.to_string()))?,
        };

        let _ = stdout_task.await;
        let stderr_buf = match stderr_task {
            Some(task) => task.await.unwrap_or_default(),
            None => String::new(),
        };

        if !stderr_buf.is_empty() {
            let _ = events.send(AgentProviderEvent::Stream(StreamEvent::Error {
                message: stderr_buf.clone(),
            }));
        }

        if !status.success() {
            return Err(AgentError::FailedStatus {
                provider_id: self.id().to_string(),
                status: status.to_string(),
                stderr: if stderr_buf.is_empty() {
                    "codex exited without stderr".to_string()
                } else {
                    stderr_buf
                },
            });
        }

        Ok(AgentRunResult {
            exit_code: status.code(),
        })
    }
}

#[derive(Debug, Default)]
struct CodexStreamParser {
    tool_names_by_id: HashMap<String, String>,
}

impl CodexStreamParser {
    fn parse_line(&mut self, line: &str) -> Vec<StreamEvent> {
        let line = line.trim();
        if line.is_empty() {
            return vec![StreamEvent::Unknown { raw: String::new() }];
        }

        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return vec![StreamEvent::Unknown {
                raw: line.to_string(),
            }];
        };

        match v.get("type").and_then(Value::as_str) {
            Some("thread.started") | Some("turn.started") | Some("item.started") => Vec::new(),
            Some("item.completed") => self.parse_item_completed(&v),
            Some("turn.completed") => self.parse_turn_completed(&v),
            Some("error") => vec![StreamEvent::Error {
                message: extract_error_message(&v).unwrap_or_else(|| "codex run failed".into()),
            }],
            _ => vec![StreamEvent::Unknown {
                raw: line.to_string(),
            }],
        }
    }

    fn parse_item_completed(&mut self, v: &Value) -> Vec<StreamEvent> {
        let item = v.get("item").unwrap_or(v);
        match item.get("type").and_then(Value::as_str) {
            Some("message") => parse_codex_message(item),
            Some("function_call") | Some("tool_call") => {
                let id = item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let tool = item
                    .get("name")
                    .or_else(|| item.get("tool_name"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                if let Some(id) = id {
                    self.tool_names_by_id.insert(id, tool.clone());
                }
                let input = item
                    .get("arguments")
                    .or_else(|| item.get("input"))
                    .and_then(json_value_from_maybe_string);
                vec![StreamEvent::ToolUse {
                    tool,
                    file_path: input.as_ref().and_then(extract_file_path),
                    command_preview: input.as_ref().and_then(extract_command_preview),
                    subagent_name: None,
                }]
            }
            Some("function_call_output") | Some("tool_result") => {
                let tool = item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .and_then(|id| self.tool_names_by_id.get(id))
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                let is_error = item
                    .get("is_error")
                    .or_else(|| item.get("error"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                vec![StreamEvent::ToolResult { tool, is_error }]
            }
            Some("reasoning") => item
                .get("summary")
                .or_else(|| item.get("text"))
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(|text| {
                    vec![StreamEvent::Thinking {
                        text: text.to_string(),
                    }]
                })
                .unwrap_or_default(),
            _ => vec![StreamEvent::Unknown { raw: v.to_string() }],
        }
    }

    fn parse_turn_completed(&self, v: &Value) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        if let Some(usage) = v.get("usage").or_else(|| v.pointer("/turn/usage")) {
            events.push(StreamEvent::TokenUpdate {
                usage: parse_usage(usage),
            });
        }
        events.push(StreamEvent::Completed { cost_usd: 0.0 });
        events
    }
}

fn parse_codex_message(item: &Value) -> Vec<StreamEvent> {
    if item.get("role").and_then(Value::as_str) != Some("assistant") {
        return Vec::new();
    }
    let text = item
        .get("content")
        .and_then(|content| {
            content.as_array().map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|block| {
                        block
                            .get("text")
                            .or_else(|| block.get("output_text"))
                            .and_then(Value::as_str)
                    })
                    .collect::<Vec<_>>()
                    .join("")
            })
        })
        .or_else(|| item.get("text").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();

    if text.is_empty() {
        Vec::new()
    } else if text.starts_with("[API Error:") {
        vec![StreamEvent::Error { message: text }]
    } else {
        vec![StreamEvent::AssistantMessage { text }]
    }
}

fn parse_usage(usage: &Value) -> TokenUsage {
    TokenUsage {
        input_tokens: usage_u64(usage, &["input_tokens", "prompt_tokens"]),
        output_tokens: usage_u64(usage, &["output_tokens", "completion_tokens"]),
        cache_read_tokens: usage_u64(
            usage,
            &[
                "cache_read_input_tokens",
                "cached_input_tokens",
                "cache_read_tokens",
            ],
        ),
        cache_creation_tokens: usage_u64(
            usage,
            &["cache_creation_input_tokens", "cache_creation_tokens"],
        ),
    }
}

fn usage_u64(usage: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| usage.get(*key).and_then(Value::as_u64))
        .unwrap_or(0)
}

fn json_value_from_maybe_string(value: &Value) -> Option<Value> {
    match value {
        Value::String(s) => serde_json::from_str(s).ok(),
        other => Some(other.clone()),
    }
}

fn extract_file_path(input: &Value) -> Option<String> {
    input
        .get("file_path")
        .or_else(|| input.get("path"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn extract_command_preview(input: &Value) -> Option<String> {
    input
        .get("command")
        .or_else(|| input.get("cmd"))
        .and_then(Value::as_str)
        .map(|command| {
            if command.len() > 60 {
                let boundary = char_boundary(command, 60);
                format!("{}...", &command[..boundary])
            } else {
                command.to_string()
            }
        })
}

fn extract_error_message(v: &Value) -> Option<String> {
    v.get("message")
        .or_else(|| v.pointer("/error/message"))
        .or_else(|| v.get("error"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn char_boundary(s: &str, max_bytes: usize) -> usize {
    if max_bytes >= s.len() {
        return s.len();
    }
    let mut i = max_bytes;
    while !s.is_char_boundary(i) && i > 0 {
        i -= 1;
    }
    i
}

fn codex_prompt(request: &AgentRequest) -> String {
    match request.system_prompt_appendix.as_deref() {
        Some(appendix) if !appendix.trim().is_empty() => {
            format!(
                "Maestro session context:\n{}\n\nUser task:\n{}",
                appendix, request.prompt
            )
        }
        _ => request.prompt.clone(),
    }
}

fn codex_config_value(value: &toml::Value) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tokio::sync::mpsc;

    use super::*;

    fn request() -> AgentRequest {
        let mut request = AgentRequest::stream_json("test prompt".into(), "gpt-5.4-codex".into());
        request.cwd = Some(PathBuf::from("/tmp/worktree"));
        request.images = vec![PathBuf::from("a.png"), PathBuf::from("b.jpg")];
        request.system_prompt_appendix = Some("appendix".to_string());
        request
    }

    #[test]
    fn stream_args_match_codex_exec_contract() {
        let provider = CodexProvider::with_config(
            "codex",
            Some("workspace-write".to_string()),
            Some(true),
            Some("work".to_string()),
            BTreeMap::from([(
                "approval_policy".to_string(),
                toml::Value::String("never".to_string()),
            )]),
            vec!["--reasoning-effort".to_string(), "high".to_string()],
            BTreeMap::new(),
            Some(true),
        );

        let args = provider.build_stream_args(&request());
        assert_eq!(args[0], "exec");
        assert!(args.iter().any(|arg| arg == "--json"));
        assert!(args.windows(2).any(|w| w == ["--model", "gpt-5.4-codex"]));
        assert!(
            args.windows(2)
                .any(|w| w == ["--sandbox", "workspace-write"])
        );
        assert!(args.windows(2).any(|w| w == ["--cd", "/tmp/worktree"]));
        assert!(args.iter().any(|arg| arg == "--ephemeral"));
        assert!(args.windows(2).any(|w| w == ["--profile", "work"]));
        assert!(
            args.windows(2)
                .any(|w| w == ["--config", "approval_policy=\"never\""])
        );
        assert!(args.windows(2).any(|w| w == ["--image", "a.png"]));
        assert!(args.windows(2).any(|w| w == ["--image", "b.jpg"]));
        assert!(args.windows(2).any(|w| w == ["--reasoning-effort", "high"]));
        assert!(!args.iter().any(|arg| arg == "--full-auto"));
        assert_eq!(
            args.last().map(String::as_str),
            Some("Maestro session context:\nappendix\n\nUser task:\ntest prompt")
        );
    }

    #[test]
    fn parser_maps_codex_jsonl_to_stream_events() {
        let mut parser = CodexStreamParser::default();
        let events: Vec<StreamEvent> = [
            r#"{"type":"thread.started","thread_id":"t1"}"#,
            r#"{"type":"item.completed","item":{"type":"function_call","call_id":"call_1","name":"shell","arguments":"{\"command\":\"cargo test\",\"path\":\"src/lib.rs\"}"}}"#,
            r#"{"type":"item.completed","item":{"type":"function_call_output","call_id":"call_1","output":"ok"}}"#,
            r#"{"type":"item.completed","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done."}]}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5,"cached_input_tokens":2}}"#,
        ]
        .into_iter()
        .flat_map(|line| parser.parse_line(line))
        .collect();

        assert!(events.iter().any(|event| {
            matches!(
                event,
                StreamEvent::ToolUse {
                    tool,
                    file_path: Some(path),
                    command_preview: Some(command),
                    ..
                } if tool == "shell" && path == "src/lib.rs" && command == "cargo test"
            )
        }));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, StreamEvent::ToolResult { tool, is_error: false } if tool == "shell"))
        );
        assert!(events.iter().any(
            |event| matches!(event, StreamEvent::AssistantMessage { text } if text == "Done.")
        ));
        assert!(events.iter().any(|event| {
            matches!(event, StreamEvent::TokenUpdate { usage } if usage.input_tokens == 10 && usage.output_tokens == 5 && usage.cache_read_tokens == 2)
        }));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, StreamEvent::Completed { .. }))
        );
    }

    #[tokio::test]
    async fn run_streams_events_from_mock_codex_cli_and_records_process_context() {
        let temp = tempfile::tempdir().expect("tempdir");
        let worktree = tempfile::tempdir().expect("worktree");
        let codex = temp.path().join("codex");
        std::fs::write(
            &codex,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{}\"\npwd > \"{}\"\nprintf '%s\\n' 'codex stderr line' >&2\ncat <<'EOF'\n{}\nEOF\n",
                temp.path().join("argv.txt").display(),
                temp.path().join("cwd.txt").display(),
                codex_fixture_jsonl()
            ),
        )
        .expect("write codex mock");
        make_executable(&codex);

        let provider = CodexProvider::new(codex.to_string_lossy().to_string());
        let mut request = request();
        request.cwd = Some(worktree.path().to_path_buf());
        let (tx, mut rx) = mpsc::unbounded_channel();
        let result = provider
            .run(request, tx, CancellationToken::new())
            .await
            .expect("mock codex run");

        assert_eq!(result.exit_code, Some(0));
        assert!(matches!(
            rx.recv().await,
            Some(AgentProviderEvent::Started(AgentRunStarted {
                process_id: Some(_)
            }))
        ));

        let mut saw_message = false;
        let mut saw_stderr = false;
        let mut saw_completed = false;
        while let Some(event) = rx.recv().await {
            match event {
                AgentProviderEvent::Stream(StreamEvent::AssistantMessage { text })
                    if text == "Done." =>
                {
                    saw_message = true;
                }
                AgentProviderEvent::Stream(StreamEvent::Error { message })
                    if message == "codex stderr line" =>
                {
                    saw_stderr = true;
                }
                AgentProviderEvent::Stream(StreamEvent::Completed { .. }) => {
                    saw_completed = true;
                }
                _ => {}
            }
            if saw_message && saw_stderr && saw_completed {
                break;
            }
        }

        assert!(saw_message);
        assert!(saw_stderr);
        assert!(saw_completed);

        let argv = std::fs::read_to_string(temp.path().join("argv.txt")).expect("argv");
        assert!(argv.contains("exec\n"));
        assert!(argv.contains("--json\n"));
        assert!(argv.contains("--model\ngpt-5.4-codex"));
        assert!(argv.contains("--sandbox\nworkspace-write"));
        assert!(argv.contains("--cd\n"));
        assert!(argv.contains("--image\na.png"));
        assert!(argv.contains("--image\nb.jpg"));
        let recorded_cwd = PathBuf::from(
            std::fs::read_to_string(temp.path().join("cwd.txt"))
                .expect("cwd")
                .trim(),
        )
        .canonicalize()
        .expect("recorded cwd canonicalizes");
        let expected_cwd = worktree
            .path()
            .canonicalize()
            .expect("worktree canonicalizes");
        assert_eq!(recorded_cwd, expected_cwd);
    }

    #[tokio::test]
    async fn run_returns_session_error_on_nonzero_exit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let codex = temp.path().join("codex");
        std::fs::write(
            &codex,
            "#!/bin/sh\nprintf '%s\\n' 'auth failed' >&2\nexit 42\n",
        )
        .expect("write codex mock");
        make_executable(&codex);

        let provider = CodexProvider::new(codex.to_string_lossy().to_string());
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut request = request();
        request.cwd = None;
        let err = provider
            .run(request, tx, CancellationToken::new())
            .await
            .expect_err("nonzero exit should fail");

        assert!(
            err.to_string().contains("auth failed"),
            "unexpected error: {err}"
        );
        assert!(err.to_string().contains("codex exited with status"));
    }

    fn codex_fixture_jsonl() -> &'static str {
        r#"{"type":"thread.started","thread_id":"t1"}
{"type":"item.completed","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done."}]}}
{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":2}}"#
    }

    #[cfg(unix)]
    fn make_executable(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = std::fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).expect("chmod");
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &std::path::Path) {}
}
