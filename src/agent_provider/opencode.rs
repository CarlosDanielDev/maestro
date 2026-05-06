use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;

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

mod parser;
pub use parser::OpenCodeJsonParser;

const OPENCODE_INSTALL_MESSAGE: &str = "opencode CLI not found; install with `brew install anomalyco/tap/opencode`, \
     `curl -fsSL https://opencode.ai/install | bash`, or `npm install -g opencode-ai`";
const OPENCODE_AUTH_MESSAGE: &str =
    "opencode auth not found; run `opencode /connect` to authenticate with a provider";

#[derive(Debug, Clone)]
pub struct OpenCodeProvider {
    binary: String,
    extra_args: Vec<String>,
    env: BTreeMap<String, String>,
    auth_path: Option<PathBuf>,
}

impl OpenCodeProvider {
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            extra_args: Vec::new(),
            env: BTreeMap::new(),
            auth_path: None,
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
            auth_path: None,
        }
    }

    #[cfg(test)]
    fn with_auth_path(mut self, auth_path: PathBuf) -> Self {
        self.auth_path = Some(auth_path);
        self
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
                let auth_path = self.auth_path();
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

    fn auth_path(&self) -> PathBuf {
        self.auth_path.clone().unwrap_or_else(opencode_auth_path)
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
                let auth_path = self.auth_path();
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
