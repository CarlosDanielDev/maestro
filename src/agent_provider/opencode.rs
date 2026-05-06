use std::collections::BTreeMap;
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
use crate::session::types::StreamEvent;

const OPENCODE_INSTALL_MESSAGE: &str = "opencode CLI not found; install with `brew install anomalyco/tap/opencode`, \
     `curl -fsSL https://opencode.ai/install | bash`, or `npm install -g opencode-ai`";
const OPENCODE_AUTH_MESSAGE: &str =
    "opencode auth not found; run `opencode /connect` to authenticate with a provider";

#[derive(Debug, Clone)]
pub struct OpenCodeProvider {
    binary: String,
    extra_args: Vec<String>,
    env: BTreeMap<String, String>,
}

impl OpenCodeProvider {
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            extra_args: Vec::new(),
            env: BTreeMap::new(),
        }
    }

    pub fn with_config(
        binary: impl Into<String>,
        extra_args: Vec<String>,
        env: BTreeMap<String, String>,
    ) -> Self {
        Self {
            binary: binary.into(),
            extra_args,
            env,
        }
    }

    pub fn build_stream_args(&self, request: &AgentRequest) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        self.push_common_args(&mut args, request);
        args
    }

    pub fn build_text_args(&self, request: &AgentRequest) -> Vec<String> {
        let mut args = vec!["run".to_string()];
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
                let auth_path = opencode_auth_path();
                if auth_path.exists() {
                    AgentHealthCheck {
                        provider_id: AgentProviderId::new(self.id()),
                        available: true,
                        version: Some(version.clone()),
                        message: version,
                    }
                } else {
                    AgentHealthCheck {
                        provider_id: AgentProviderId::new(self.id()),
                        available: false,
                        version: Some(version),
                        message: OPENCODE_AUTH_MESSAGE.to_string(),
                    }
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
                message: OPENCODE_INSTALL_MESSAGE.to_string(),
            },
        }
    }

    fn push_common_args(&self, args: &mut Vec<String>, request: &AgentRequest) {
        if !request.model.trim().is_empty() {
            args.push("--model".to_string());
            args.push(request.model.clone());
        }

        if let Some(cwd) = request.cwd.as_ref() {
            args.push("--dir".to_string());
            args.push(cwd.display().to_string());
        }

        args.extend(self.extra_args.iter().cloned());
        args.push(opencode_prompt(request));
    }
}

impl Default for OpenCodeProvider {
    fn default() -> Self {
        Self::new("opencode")
    }
}

#[async_trait::async_trait]
impl AgentProvider for OpenCodeProvider {
    fn id(&self) -> &str {
        "opencode"
    }

    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Subprocess
    }

    fn parser_binding(&self) -> ParserBinding {
        ParserBinding {
            name: "opencode-json-stub".to_string(),
            output_format: AgentOutputFormat::StreamJson,
        }
    }

    async fn health_check(&self) -> Result<AgentHealthCheck, AgentError> {
        match Command::new(&self.binary).arg("--version").output().await {
            Ok(out) if out.status.success() => {
                let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let auth_path = opencode_auth_path();
                if auth_path.exists() {
                    Ok(AgentHealthCheck {
                        provider_id: AgentProviderId::new(self.id()),
                        available: true,
                        version: Some(version.clone()),
                        message: version,
                    })
                } else {
                    Ok(AgentHealthCheck {
                        provider_id: AgentProviderId::new(self.id()),
                        available: false,
                        version: Some(version),
                        message: OPENCODE_AUTH_MESSAGE.to_string(),
                    })
                }
            }
            Ok(out) => Ok(AgentHealthCheck {
                provider_id: AgentProviderId::new(self.id()),
                available: false,
                version: None,
                message: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            }),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                Err(AgentError::Config(OPENCODE_INSTALL_MESSAGE.to_string()))
            }
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

        let mut child = cmd.spawn().map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => {
                AgentError::Config(OPENCODE_INSTALL_MESSAGE.to_string())
            }
            _ => AgentError::Spawn {
                provider_id: self.id().to_string(),
                source,
            },
        })?;
        let process_id = child.id();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AgentError::Stream("No stdout from opencode CLI".to_string()))?;
        let stderr = child.stderr.take();

        let _ = events.send(AgentProviderEvent::Started(AgentRunStarted { process_id }));

        let stdout_events = events.clone();
        let stdout_task = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut parser = OpenCodeJsonParser::default();
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
                    "opencode exited without stderr".to_string()
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
pub struct OpenCodeJsonParser {
    stdout_bytes: Vec<u8>,
}

impl OpenCodeJsonParser {
    pub fn parse_line(&mut self, line: &str) -> Vec<StreamEvent> {
        self.stdout_bytes.extend_from_slice(line.as_bytes());
        self.stdout_bytes.push(b'\n');

        let line = line.trim();
        if line.is_empty() {
            return vec![StreamEvent::Unknown { raw: String::new() }];
        }

        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return vec![StreamEvent::Unknown {
                raw: line.to_string(),
            }];
        };

        match value.get("type").and_then(Value::as_str) {
            Some(_) | None => vec![StreamEvent::Unknown {
                raw: line.to_string(),
            }],
        }
    }

    pub fn stdout_bytes(&self) -> &[u8] {
        &self.stdout_bytes
    }
}

fn opencode_prompt(request: &AgentRequest) -> String {
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

fn opencode_auth_path() -> std::path::PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|home| home.join(".local/share"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from(".local/share"))
        .join("opencode/auth.json")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;

    fn request() -> AgentRequest {
        let mut request =
            AgentRequest::stream_json("test prompt".into(), "anthropic/claude-sonnet-4-5".into());
        request.cwd = Some(PathBuf::from("/tmp/worktree"));
        request.system_prompt_appendix = Some("appendix".to_string());
        request
    }

    #[test]
    fn stream_args_match_opencode_run_contract() {
        let provider = OpenCodeProvider::with_config(
            "opencode",
            vec!["--log-level".to_string(), "debug".to_string()],
            BTreeMap::new(),
        );

        let args = provider.build_stream_args(&request());

        assert_eq!(args[0], "run");
        assert!(args.windows(2).any(|w| w == ["--format", "json"]));
        assert!(
            args.windows(2)
                .any(|w| w == ["--model", "anthropic/claude-sonnet-4-5"])
        );
        assert!(args.windows(2).any(|w| w == ["--dir", "/tmp/worktree"]));
        assert!(args.windows(2).any(|w| w == ["--log-level", "debug"]));
        assert_eq!(
            args.last().map(String::as_str),
            Some("Maestro session context:\nappendix\n\nUser task:\ntest prompt")
        );
    }

    #[test]
    fn json_parser_captures_stdout_and_emits_unknown() {
        let mut parser = OpenCodeJsonParser::default();

        let events = parser.parse_line(r#"{"type":"session.started","id":"s1"}"#);

        assert!(matches!(events.as_slice(), [StreamEvent::Unknown { .. }]));
        assert_eq!(
            parser.stdout_bytes(),
            b"{\"type\":\"session.started\",\"id\":\"s1\"}\n"
        );
    }

    #[tokio::test]
    async fn run_streams_events_from_mock_opencode_cli_and_records_process_context() {
        let temp = tempfile::tempdir().expect("tempdir");
        let worktree = tempfile::tempdir().expect("worktree");
        let opencode = temp.path().join("opencode");
        std::fs::write(
            &opencode,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{}\"\npwd > \"{}\"\nprintf '%s\\n' 'opencode stderr line' >&2\ncat <<'EOF'\n{}\nEOF\n",
                temp.path().join("argv.txt").display(),
                temp.path().join("cwd.txt").display(),
                opencode_fixture_jsonl()
            ),
        )
        .expect("write opencode mock");
        make_executable(&opencode);

        let provider = OpenCodeProvider::with_config(
            opencode.to_string_lossy().to_string(),
            vec!["--log-level".to_string(), "debug".to_string()],
            BTreeMap::new(),
        );
        let mut request = request();
        request.cwd = Some(worktree.path().to_path_buf());
        let (tx, mut rx) = mpsc::unbounded_channel();
        let result = provider
            .run(request, tx, CancellationToken::new())
            .await
            .expect("mock opencode run");

        assert_eq!(result.exit_code, Some(0));
        assert!(matches!(
            rx.recv().await,
            Some(AgentProviderEvent::Started(AgentRunStarted {
                process_id: Some(_)
            }))
        ));

        let mut saw_unknown = false;
        let mut saw_stderr = false;
        let mut saw_completed = false;
        while let Some(event) = rx.recv().await {
            match event {
                AgentProviderEvent::Stream(StreamEvent::Unknown { raw })
                    if raw.contains("session.started") =>
                {
                    saw_unknown = true;
                }
                AgentProviderEvent::Stream(StreamEvent::Error { message })
                    if message == "opencode stderr line" =>
                {
                    saw_stderr = true;
                }
                AgentProviderEvent::Stream(StreamEvent::Completed { .. }) => {
                    saw_completed = true;
                }
                _ => {}
            }
            if saw_unknown && saw_stderr && saw_completed {
                break;
            }
        }

        assert!(saw_unknown);
        assert!(saw_stderr);
        assert!(saw_completed);

        let argv = std::fs::read_to_string(temp.path().join("argv.txt")).expect("argv");
        assert!(argv.contains("run\n"));
        assert!(argv.contains("--format\njson"));
        assert!(argv.contains("--model\nanthropic/claude-sonnet-4-5"));
        assert!(argv.contains("--dir\n"));
        assert!(argv.contains("--log-level\ndebug"));
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
        let opencode = temp.path().join("opencode");
        std::fs::write(
            &opencode,
            "#!/bin/sh\nprintf '%s\\n' 'auth failed' >&2\nexit 42\n",
        )
        .expect("write opencode mock");
        make_executable(&opencode);

        let provider = OpenCodeProvider::new(opencode.to_string_lossy().to_string());
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
        assert!(err.to_string().contains("opencode exited with status"));
    }

    #[tokio::test]
    async fn missing_binary_surfaces_install_instructions() {
        let provider = OpenCodeProvider::new("/tmp/maestro-missing-opencode-binary");
        let (tx, _rx) = mpsc::unbounded_channel();

        let err = provider
            .run(request(), tx, CancellationToken::new())
            .await
            .expect_err("missing binary should fail");

        assert!(err.to_string().contains("opencode CLI not found"));
        assert!(
            err.to_string()
                .contains("brew install anomalyco/tap/opencode")
        );
    }

    fn opencode_fixture_jsonl() -> &'static str {
        r#"{"type":"session.started","id":"s1"}
{"type":"message.delta","text":"Done."}"#
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
