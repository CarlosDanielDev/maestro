use std::collections::BTreeMap;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub(crate) mod parser;

use super::types::{
    AgentError, AgentHealthCheck, AgentOutputFormat, AgentProvider, AgentProviderEvent,
    AgentProviderId, AgentProviderKind, AgentRequest, AgentRunResult, AgentRunStarted,
    ParserBinding,
};
use crate::session::types::StreamEvent;
use parser::CodexStreamParser;

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
        if codex_yolo_enabled(request.permission_mode.as_deref()) {
            args.push("--yolo".to_string());
        }

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
                let request = AgentRequest::stream_json(
                    "Maestro doctor preflight. Respond with OK only.".to_string(),
                    String::new(),
                );
                let mut preflight = Command::new(&self.binary);
                preflight.args(self.build_stream_args(&request));
                preflight.envs(&self.env);
                preflight
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true);

                match tokio::time::timeout(Duration::from_secs(10), preflight.output()).await {
                    Ok(Ok(preflight_out)) if preflight_out.status.success() => {}
                    Ok(Ok(preflight_out)) => {
                        let stderr = String::from_utf8_lossy(&preflight_out.stderr)
                            .trim()
                            .to_string();
                        return Ok(AgentHealthCheck {
                            provider_id: AgentProviderId::new(self.id()),
                            available: false,
                            version: Some(version),
                            message: if stderr.is_empty() {
                                "codex exec --json preflight failed".to_string()
                            } else {
                                stderr
                            },
                        });
                    }
                    Ok(Err(source)) => {
                        return Err(AgentError::Spawn {
                            provider_id: self.id().to_string(),
                            source,
                        });
                    }
                    Err(_) => {
                        return Ok(AgentHealthCheck {
                            provider_id: AgentProviderId::new(self.id()),
                            available: false,
                            version: Some(version),
                            message: "codex exec --json preflight timed out after 10s".to_string(),
                        });
                    }
                }

                Ok(AgentHealthCheck {
                    provider_id: AgentProviderId::new(self.id()),
                    available: true,
                    version: Some(version.clone()),
                    message: format!("{version}; exec --json preflight passed"),
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

fn codex_yolo_enabled(mode: Option<&str>) -> bool {
    matches!(
        mode.map(str::trim).filter(|mode| !mode.is_empty()),
        Some("bypassPermissions" | "yolo")
    )
}

#[cfg(test)]
mod tests;
