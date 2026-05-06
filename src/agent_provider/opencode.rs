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
use crate::session::types::TokenUsage;

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
            name: "opencode-json".to_string(),
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
            Some("step_start") => Vec::new(),
            Some("text") => parse_text_event(&value),
            Some("tool_use") => parse_tool_use_event(&value),
            Some("step_finish") => parse_step_finish_event(&value),
            Some("error") => vec![StreamEvent::Error {
                message: opencode_error_message(&value),
            }],
            Some(_) | None => vec![StreamEvent::Unknown {
                raw: line.to_string(),
            }],
        }
    }

    pub fn stdout_bytes(&self) -> &[u8] {
        &self.stdout_bytes
    }
}

fn parse_text_event(value: &Value) -> Vec<StreamEvent> {
    value
        .pointer("/part/text")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(|text| {
            vec![StreamEvent::AssistantMessage {
                text: text.to_string(),
            }]
        })
        .unwrap_or_else(|| {
            vec![StreamEvent::Unknown {
                raw: value.to_string(),
            }]
        })
}

fn parse_tool_use_event(value: &Value) -> Vec<StreamEvent> {
    let Some(part) = value.get("part") else {
        return vec![StreamEvent::Unknown {
            raw: value.to_string(),
        }];
    };

    let tool = part
        .get("tool")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let state = part.get("state");
    let input = state.and_then(|state| state.get("input"));
    let file_path = input
        .and_then(extract_opencode_file_path)
        .or_else(|| state.and_then(extract_opencode_metadata_file_path));
    let command_preview = input.and_then(extract_opencode_command_preview);
    let is_error = state
        .and_then(|state| state.get("status"))
        .and_then(Value::as_str)
        .is_some_and(|status| status != "completed");

    vec![
        StreamEvent::ToolUse {
            tool: tool.clone(),
            file_path,
            command_preview,
            subagent_name: None,
        },
        StreamEvent::ToolResult { tool, is_error },
    ]
}

fn parse_step_finish_event(value: &Value) -> Vec<StreamEvent> {
    let Some(part) = value.get("part") else {
        return vec![StreamEvent::Unknown {
            raw: value.to_string(),
        }];
    };

    let mut events = Vec::new();
    if let Some(tokens) = part.get("tokens") {
        events.push(StreamEvent::TokenUpdate {
            usage: parse_opencode_tokens(tokens),
        });
    }

    match part.get("reason").and_then(Value::as_str) {
        Some("stop") => events.push(StreamEvent::Completed {
            cost_usd: part.get("cost").and_then(Value::as_f64).unwrap_or(0.0),
        }),
        Some("tool-calls") => {}
        Some(reason) => events.push(StreamEvent::Unknown {
            raw: format!("opencode step_finish reason:{reason}"),
        }),
        None => events.push(StreamEvent::Unknown {
            raw: value.to_string(),
        }),
    }

    events
}

fn parse_opencode_tokens(tokens: &Value) -> TokenUsage {
    TokenUsage {
        input_tokens: tokens.get("input").and_then(Value::as_u64).unwrap_or(0),
        output_tokens: tokens.get("output").and_then(Value::as_u64).unwrap_or(0),
        cache_read_tokens: tokens
            .pointer("/cache/read")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cache_creation_tokens: tokens
            .pointer("/cache/write")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    }
}

fn extract_opencode_file_path(input: &Value) -> Option<String> {
    input
        .get("filePath")
        .or_else(|| input.get("file_path"))
        .or_else(|| input.get("path"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            input
                .get("patchText")
                .and_then(Value::as_str)
                .and_then(extract_patch_file_path)
        })
}

fn extract_opencode_metadata_file_path(state: &Value) -> Option<String> {
    state
        .pointer("/metadata/files")
        .and_then(Value::as_array)
        .and_then(|files| files.first())
        .and_then(|file| {
            file.get("relativePath")
                .or_else(|| file.get("filePath"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

fn extract_opencode_command_preview(input: &Value) -> Option<String> {
    input
        .get("command")
        .or_else(|| input.get("cmd"))
        .or_else(|| input.get("patchText"))
        .and_then(Value::as_str)
        .map(short_preview)
}

fn extract_patch_file_path(patch: &str) -> Option<String> {
    patch.lines().find_map(|line| {
        line.strip_prefix("*** Add File: ")
            .or_else(|| line.strip_prefix("*** Update File: "))
            .or_else(|| line.strip_prefix("*** Delete File: "))
            .map(str::to_string)
    })
}

fn short_preview(value: &str) -> String {
    if value.len() > 60 {
        let boundary = char_boundary(value, 60);
        format!("{}...", &value[..boundary])
    } else {
        value.to_string()
    }
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

fn opencode_error_message(value: &Value) -> String {
    value
        .pointer("/error/data/message")
        .or_else(|| value.pointer("/error/message"))
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("opencode run failed")
        .to_string()
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
mod tests;
