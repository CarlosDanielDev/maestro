//! HTTP boundary for the Claude usage endpoint, and lenient response parsing.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::types::{ProviderWindows, QuotaWindow, TransportError};

pub const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// The only seam to the network. Tests inject a fake. Production uses [`ReqwestQuotaHttp`].
#[async_trait::async_trait]
pub trait QuotaHttp: Send + Sync {
    async fn get_usage(&self, access_token: &str) -> Result<HttpResponse, TransportError>;
}

/// The only place in this module that builds a `reqwest` client.
pub struct ReqwestQuotaHttp {
    client: reqwest::Client,
}

impl ReqwestQuotaHttp {
    pub fn new() -> Result<Self, TransportError> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("maestro/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| TransportError(err.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait::async_trait]
impl QuotaHttp for ReqwestQuotaHttp {
    async fn get_usage(&self, access_token: &str) -> Result<HttpResponse, TransportError> {
        let response = self
            .client
            .get(USAGE_URL)
            .bearer_auth(access_token)
            .header("anthropic-beta", OAUTH_BETA)
            .send()
            .await
            .map_err(|err| TransportError(err.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|err| TransportError(err.to_string()))?;
        Ok(HttpResponse { status, body })
    }
}

#[derive(Deserialize)]
struct RawWindow {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

/// Lenient parse. Each window stands alone. A missing, malformed, or undated
/// window is `None`, never 0%. Returns `None` when no window parses.
pub fn parse_usage(body: &str) -> Option<ProviderWindows> {
    let root: serde_json::Value = serde_json::from_str(body).ok()?;
    let windows = ProviderWindows {
        five_hour: window(&root, "five_hour"),
        seven_day: window(&root, "seven_day"),
        seven_day_sonnet: window(&root, "seven_day_sonnet"),
        stale: false,
    };
    let any = windows.five_hour.is_some()
        || windows.seven_day.is_some()
        || windows.seven_day_sonnet.is_some();
    any.then_some(windows)
}

fn window(root: &serde_json::Value, key: &str) -> Option<QuotaWindow> {
    let raw: RawWindow = serde_json::from_value(root.get(key)?.clone()).ok()?;
    let resets_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&raw.resets_at?)
        .ok()?
        .with_timezone(&Utc);
    Some(QuotaWindow {
        utilization_pct: raw.utilization?,
        resets_at,
    })
}
