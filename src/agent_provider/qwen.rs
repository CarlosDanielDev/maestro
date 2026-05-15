use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::qwen_parser::QwenStreamParser;
use super::types::{
    AgentError, AgentHealthCheck, AgentOutputFormat, AgentProvider, AgentProviderEvent,
    AgentProviderId, AgentProviderKind, AgentRequest, AgentRunResult, AgentRunStarted,
    AgentTextOutput, ParserBinding,
};
use crate::session::types::StreamEvent;

#[derive(Debug, Clone)]
pub struct QwenProvider {
    binary: String,
    extra_args: Vec<String>,
    env: BTreeMap<String, String>,
}

impl QwenProvider {
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
            "--bare".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--include-partial-messages".to_string(),
        ];
        self.push_common_args(&mut args, request);
        args
    }

    pub fn build_text_args(&self, request: &AgentRequest) -> Vec<String> {
        let mut args = vec![
            "--bare".to_string(),
            "--output-format".to_string(),
            "text".to_string(),
        ];
        self.push_common_args(&mut args, request);
        args
    }

    pub async fn run_text(
        &self,
        model: &str,
        prompt: &str,
        cwd: Option<&Path>,
    ) -> Result<AgentTextOutput, AgentError> {
        let request = AgentRequest::text(
            prompt.to_string(),
            model.to_string(),
            cwd.map(PathBuf::from),
        );
        let mut cmd = Command::new(&self.binary);
        cmd.args(self.build_text_args(&request));
        cmd.envs(&self.env);
        if let Some(cwd) = request.cwd.as_ref() {
            cmd.current_dir(cwd);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await.map_err(|source| AgentError::Spawn {
            provider_id: self.id().to_string(),
            source,
        })?;

        Ok(AgentTextOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            status_success: output.status.success(),
        })
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

        if let Some(mode) = qwen_approval_mode(request.permission_mode.as_deref()) {
            args.push("--approval-mode".to_string());
            args.push(mode.to_string());
        }

        args.extend(self.extra_args.iter().cloned());
        args.push("--prompt".to_string());
        args.push(qwen_prompt(request));
    }
}

impl Default for QwenProvider {
    fn default() -> Self {
        Self::new("qwen")
    }
}

#[async_trait::async_trait]
impl AgentProvider for QwenProvider {
    fn id(&self) -> &str {
        "qwen"
    }

    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Subprocess
    }

    fn parser_binding(&self) -> ParserBinding {
        ParserBinding {
            name: "qwen-stream-json".to_string(),
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

    fn template_rules(&self) -> &'static dyn crate::templates::TemplateProviderRules {
        crate::templates::provider_rules::http_generic_rules()
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
            .ok_or_else(|| AgentError::Stream("No stdout from qwen CLI".to_string()))?;
        let stderr = child.stderr.take();

        let _ = events.send(AgentProviderEvent::Started(AgentRunStarted { process_id }));

        let stdout_events = events.clone();
        let stdout_task = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut parser = QwenStreamParser::default();
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

        if !status.success() && !stderr_buf.is_empty() {
            return Err(AgentError::FailedStatus {
                provider_id: self.id().to_string(),
                status: status.to_string(),
                stderr: stderr_buf,
            });
        }

        Ok(AgentRunResult {
            exit_code: status.code(),
        })
    }
}

fn qwen_approval_mode(mode: Option<&str>) -> Option<&str> {
    match mode.map(str::trim).filter(|mode| !mode.is_empty()) {
        Some("bypassPermissions") => Some("yolo"),
        Some("acceptEdits") => Some("auto_edit"),
        Some("default") | None => None,
        Some(mode) => Some(mode),
    }
}

fn qwen_prompt(request: &AgentRequest) -> String {
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
