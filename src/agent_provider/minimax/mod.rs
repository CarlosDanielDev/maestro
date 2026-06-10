#![deny(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

mod client;
pub mod pricing;
pub mod quota;
pub mod types;

pub use client::MinimaxClient;
pub use quota::{MinimaxQuota, QuotaStatus};
pub use types::MinimaxError;

use super::types::{
    AgentError, AgentHealthCheck, AgentOutputFormat, AgentProvider, AgentProviderEvent,
    AgentProviderId, AgentProviderKind, AgentRequest, AgentRunResult, AgentRunStarted,
    ParserBinding,
};
use crate::session::types::StreamEvent;

const DEFAULT_API_KEY_ENV: &str = "MINIMAX_API_KEY";

/// Process-wide bypass for the MiniMax pre-spawn quota gate. Set once at
/// CLI bootstrap when `--force-quota` is passed; read by every subsequent
/// `MinimaxProvider::run`. Avoids threading the flag through session
/// spawn + AgentRequest plumbing.
static FORCE_QUOTA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Flip the process-wide force-quota flag on. Idempotent.
pub fn set_force_quota() {
    FORCE_QUOTA.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Read the process-wide force-quota flag.
fn force_quota_enabled() -> bool {
    FORCE_QUOTA.load(std::sync::atomic::Ordering::Relaxed)
}

#[derive(Debug, Clone)]
pub struct MinimaxProvider {
    id: String,
    model: String,
    http: MinimaxClient,
    quota: Option<Arc<MinimaxQuota>>,
}

impl MinimaxProvider {
    pub fn new(
        id: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
        request_timeout_secs: u64,
        api_key_env: Option<String>,
    ) -> Result<Self, MinimaxError> {
        Self::with_client(
            id,
            model,
            MinimaxClient::new(
                base_url,
                Duration::from_secs(request_timeout_secs),
                api_key_env.unwrap_or_else(|| DEFAULT_API_KEY_ENV.to_string()),
            )?,
        )
    }

    fn with_client(
        id: impl Into<String>,
        model: impl Into<String>,
        http: MinimaxClient,
    ) -> Result<Self, MinimaxError> {
        Ok(Self {
            id: id.into(),
            model: model.into(),
            http,
            quota: None,
        })
    }

    /// Attach a `MinimaxQuota` tracker so the pre-spawn gate fires before
    /// each chat request. Returning `Self` makes this composable with the
    /// existing constructors.
    pub fn with_quota(mut self, quota: Arc<MinimaxQuota>) -> Self {
        self.quota = Some(quota);
        self
    }

    #[cfg(test)]
    fn new_with_api_key_lookup(
        id: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
        request_timeout_secs: u64,
        api_key_env: Option<String>,
        api_key_lookup: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
    ) -> Result<Self, MinimaxError> {
        Self::with_client(
            id,
            model,
            MinimaxClient::with_api_key_lookup(
                base_url,
                Duration::from_secs(request_timeout_secs),
                api_key_env.unwrap_or_else(|| DEFAULT_API_KEY_ENV.to_string()),
                api_key_lookup,
            )?,
        )
    }
}

#[async_trait::async_trait]
impl AgentProvider for MinimaxProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Http
    }

    fn parser_binding(&self) -> ParserBinding {
        ParserBinding {
            name: "openai-compatible-sse".to_string(),
            output_format: AgentOutputFormat::StreamJson,
        }
    }

    async fn health_check(&self) -> Result<AgentHealthCheck, AgentError> {
        self.http
            .models_health()
            .await
            .map_err(MinimaxError::into_agent_error)?;

        Ok(AgentHealthCheck {
            provider_id: AgentProviderId::new(self.id()),
            available: true,
            version: None,
            message: format!(
                "MiniMax models endpoint reachable; model `{}` configured",
                self.model
            ),
        })
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
        let model = if request.model.trim().is_empty() {
            self.model.as_str()
        } else {
            request.model.as_str()
        };

        // User-visible activity hint so the session card surfaces
        // "Connecting to MiniMax server..." instead of just the generic
        // Spawning spinner. The HTTP handshake can take 10s+ on cold
        // routes; this tells the user the wait is intentional.
        let _ = events.send(AgentProviderEvent::Stream(StreamEvent::AssistantMessage {
            text: format!("Connecting to MiniMax ({model})..."),
        }));

        let t_total = std::time::Instant::now();

        // Pre-spawn quota gate (5-hour rolling window). At >= 95% the
        // gate refuses the spawn unless the request carries the
        // `--force-quota` flag. The gate records the request after
        // the policy check and before HTTP work begins so concurrent
        // processes serialize through the file lock.
        let t_quota = std::time::Instant::now();
        if let Some(quota) = self.quota.as_ref() {
            // Honor either per-request `force` (future TUI re-spawn flow)
            // or the CLI's `--force-quota` flag (process-wide static flipped
            // at bootstrap; avoids threading through session spawn paths).
            let forced = request.force || force_quota_enabled();
            let mut bypassed = false;
            let mut forced_pct: Option<u8> = None;
            match quota.check() {
                QuotaStatus::Ok { .. } => {}
                QuotaStatus::Warn { pct } => {
                    tracing::warn!(
                        provider = %self.id,
                        pct,
                        "MiniMax 5h quota approaching limit",
                    );
                }
                QuotaStatus::Refused { pct } if !forced => {
                    return Err(AgentError::Config(format!(
                        "MiniMax 5h quota at {pct}% — refusing spawn. Pass --force-quota to bypass.",
                    )));
                }
                QuotaStatus::Refused { pct } => {
                    tracing::warn!(
                        provider = %self.id,
                        pct,
                        "MiniMax spawn forced over quota refusal threshold",
                    );
                    bypassed = true;
                    // The structured event is emitted AFTER record_forced
                    // succeeds (below) so the displayed `count` always matches
                    // the persisted `forced_count`. The `pct` is captured
                    // here because the post-record read would include the
                    // freshly recorded request.
                    forced_pct = Some(pct);
                }
            }
            let record_result = if bypassed {
                quota.record_forced()
            } else {
                quota.record()
            };
            record_result
                .map_err(|err| AgentError::Config(format!("MiniMax quota persistence: {err}")))?;

            if let Some(pct) = forced_pct {
                let count = quota.forced_count();
                let _ = events.send(AgentProviderEvent::Stream(
                    crate::session::types::StreamEvent::Warning {
                        code: "quota_forced".to_string(),
                        message: format!(
                            "MiniMax 5h quota at {pct}%; spawn forced (count: {count})"
                        ),
                    },
                ));
            }
        }

        let quota_elapsed = t_quota.elapsed();

        let t_http = std::time::Instant::now();
        let mut stream = self
            .http
            .chat_stream(model, &request.prompt)
            .await
            .map_err(MinimaxError::into_agent_error)?;
        let http_handshake_elapsed = t_http.elapsed();
        tracing::info!(
            provider = %self.id,
            model = %model,
            quota_ms = quota_elapsed.as_millis() as u64,
            http_handshake_ms = http_handshake_elapsed.as_millis() as u64,
            total_ms = t_total.elapsed().as_millis() as u64,
            "MiniMax spawn timing breakdown",
        );
        // Surface a slow-handshake warning once the wait exceeds 5s so the
        // user has structured data to share when reporting "minimax is
        // slow" — the activity log + tracing entry both name the elapsed
        // milliseconds and the model.
        if http_handshake_elapsed.as_secs() >= 5 {
            let _ = events.send(AgentProviderEvent::Stream(StreamEvent::Warning {
                code: "minimax_slow_handshake".to_string(),
                message: format!(
                    "MiniMax HTTP handshake took {}ms (model: {model}); server-side latency or cold route",
                    http_handshake_elapsed.as_millis()
                ),
            }));
        }
        let _ = events.send(AgentProviderEvent::Started(AgentRunStarted {
            process_id: None,
        }));

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Err(AgentError::Cancelled {
                        provider_id: self.id().to_string(),
                    });
                }
                next = stream.recv() => {
                    let Some(next) = next else {
                        break;
                    };
                    match next {
                        Ok(event) => {
                            let _ = events.send(AgentProviderEvent::Stream(event));
                        }
                        Err(err) => {
                            return Err(err.into_agent_error());
                        }
                    }
                }
            }
        }

        Ok(AgentRunResult {
            exit_code: None,
            session_id: None,
        })
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
