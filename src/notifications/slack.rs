use std::collections::VecDeque;
use std::time::Instant;

use super::types::InterruptLevel;

/// Event types that can trigger Slack notifications.
#[derive(Debug, Clone)]
pub enum SlackEvent {
    SessionCompleted {
        session_id: String,
        issue_number: Option<u64>,
        cost_usd: f64,
    },
    SessionErrored {
        session_id: String,
        issue_number: Option<u64>,
        error: String,
    },
    BudgetAlert {
        usage_pct: u8,
        total_usd: f64,
        spent_usd: f64,
    },
    FileConflict {
        file_path: String,
        sessions: Vec<String>,
    },
}

impl SlackEvent {
    /// Format this event as a Slack message payload (Block Kit).
    pub fn to_payload(&self) -> serde_json::Value {
        let (emoji, title, fields) = match self {
            SlackEvent::SessionCompleted {
                session_id,
                issue_number,
                cost_usd,
            } => {
                let issue_text = issue_number
                    .map(|n| format!("#{n}"))
                    .unwrap_or_else(|| "N/A".into());
                (
                    ":white_check_mark:",
                    "Session Completed",
                    vec![
                        ("Session", short_id(session_id)),
                        ("Issue", issue_text),
                        ("Cost", format!("${cost_usd:.2}")),
                    ],
                )
            }
            SlackEvent::SessionErrored {
                session_id,
                issue_number,
                error,
            } => {
                let issue_text = issue_number
                    .map(|n| format!("#{n}"))
                    .unwrap_or_else(|| "N/A".into());
                (
                    ":x:",
                    "Session Errored",
                    vec![
                        ("Session", short_id(session_id)),
                        ("Issue", issue_text),
                        ("Error", truncate(error, 200)),
                    ],
                )
            }
            SlackEvent::BudgetAlert {
                usage_pct,
                total_usd,
                spent_usd,
            } => (
                ":warning:",
                "Budget Alert",
                vec![
                    ("Usage", format!("{usage_pct}%")),
                    ("Spent", format!("${spent_usd:.2}")),
                    ("Budget", format!("${total_usd:.2}")),
                ],
            ),
            SlackEvent::FileConflict {
                file_path,
                sessions,
            } => {
                let session_list = sessions
                    .iter()
                    .map(|s| short_id(s))
                    .collect::<Vec<_>>()
                    .join(", ");
                (
                    ":rotating_light:",
                    "File Conflict Detected",
                    vec![("File", file_path.clone()), ("Sessions", session_list)],
                )
            }
        };

        let field_blocks: Vec<serde_json::Value> = fields
            .into_iter()
            .map(|(label, value)| {
                serde_json::json!({
                    "type": "mrkdwn",
                    "text": format!("*{label}:* {value}")
                })
            })
            .collect();

        serde_json::json!({
            "blocks": [
                {
                    "type": "header",
                    "text": {
                        "type": "plain_text",
                        "text": format!("{emoji} Maestro: {title}"),
                        "emoji": true
                    }
                },
                {
                    "type": "section",
                    "fields": field_blocks
                }
            ]
        })
    }
}

fn short_id(id: &str) -> String {
    if id.len() > 8 {
        id[..8].to_string()
    } else {
        id.to_string()
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s.to_string()
    }
}

/// Rate-limited Slack webhook client.
pub struct SlackClient {
    webhook_url: String,
    http: reqwest::Client,
    rate_limit_per_min: u32,
    /// Timestamps of recent sends within the rate window.
    send_times: VecDeque<Instant>,
}

impl SlackClient {
    pub fn new(webhook_url: String, rate_limit_per_min: u32) -> Self {
        Self {
            webhook_url,
            http: reqwest::Client::new(),
            rate_limit_per_min,
            send_times: VecDeque::new(),
        }
    }

    /// Send a notification event to Slack. Returns false if rate-limited.
    pub async fn send(&mut self, event: &SlackEvent) -> Result<bool, SlackError> {
        if !self.check_rate_limit() {
            tracing::warn!("Slack notification rate-limited, dropping message");
            return Ok(false);
        }

        let payload = event.to_payload();
        let resp = self
            .http
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| SlackError::Http(e.to_string()))?;

        let status = resp.status();
        if status.is_success() {
            self.record_send();
            Ok(true)
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(SlackError::Api {
                status: status.as_u16(),
                body,
            })
        }
    }

    /// Send a test message to verify the webhook is configured correctly.
    pub async fn send_test(&mut self) -> Result<bool, SlackError> {
        let payload = serde_json::json!({
            "blocks": [
                {
                    "type": "header",
                    "text": {
                        "type": "plain_text",
                        "text": ":test_tube: Maestro: Webhook Test",
                        "emoji": true
                    }
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": "Slack webhook is configured and working correctly."
                    }
                }
            ]
        });

        let resp = self
            .http
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| SlackError::Http(e.to_string()))?;

        let status = resp.status();
        if status.is_success() {
            Ok(true)
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(SlackError::Api {
                status: status.as_u16(),
                body,
            })
        }
    }

    /// Check if we're within rate limits. Prunes old entries.
    fn check_rate_limit(&mut self) -> bool {
        let window = std::time::Duration::from_secs(60);
        let now = Instant::now();

        // Remove entries older than the window
        while let Some(&front) = self.send_times.front() {
            if now.duration_since(front) > window {
                self.send_times.pop_front();
            } else {
                break;
            }
        }

        self.send_times.len() < self.rate_limit_per_min as usize
    }

    fn record_send(&mut self) {
        self.send_times.push_back(Instant::now());
    }

    /// Check rate limit and record a send if within limits.
    /// Returns true if the send is allowed.
    pub fn check_rate_limit_and_record(&mut self) -> bool {
        if self.check_rate_limit() {
            self.record_send();
            true
        } else {
            false
        }
    }

    pub fn webhook_url(&self) -> &str {
        &self.webhook_url
    }
}

/// Maps an `InterruptLevel` to a corresponding `SlackEvent` for generic notifications.
pub fn level_to_slack_event(
    level: InterruptLevel,
    title: &str,
    message: &str,
) -> Option<SlackEvent> {
    // Only send Slack notifications for Warning and above
    match level {
        InterruptLevel::Info => None,
        InterruptLevel::Warning | InterruptLevel::Critical | InterruptLevel::Blocker => {
            // Attempt to parse known notification patterns into structured events.
            // Fall back to a generic error event for anything else.
            if title.contains("Budget") {
                // Budget notifications typically include percentage in the message
                Some(SlackEvent::BudgetAlert {
                    usage_pct: parse_pct(message).unwrap_or(0),
                    total_usd: 0.0,
                    spent_usd: 0.0,
                })
            } else {
                Some(SlackEvent::SessionErrored {
                    session_id: "unknown".into(),
                    issue_number: None,
                    error: format!("[{title}] {message}"),
                })
            }
        }
    }
}

fn parse_pct(s: &str) -> Option<u8> {
    // Look for a number followed by '%'
    let re = regex::Regex::new(r"(\d+)%").ok()?;
    re.captures(s)?.get(1)?.as_str().parse().ok()
}

#[derive(Debug)]
pub enum SlackError {
    Http(String),
    Api { status: u16, body: String },
}

impl std::fmt::Display for SlackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SlackError::Http(e) => write!(f, "Slack HTTP error: {e}"),
            SlackError::Api { status, body } => {
                write!(f, "Slack API error (HTTP {status}): {body}")
            }
        }
    }
}

impl std::error::Error for SlackError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_completed_payload_has_correct_structure() {
        let event = SlackEvent::SessionCompleted {
            session_id: "abcdef12-3456-7890-abcd-ef1234567890".into(),
            issue_number: Some(42),
            cost_usd: 1.23,
        };
        let payload = event.to_payload();
        let blocks = payload["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 2);

        let header = &blocks[0]["text"]["text"];
        assert!(header.as_str().unwrap().contains("Session Completed"));

        let fields = blocks[1]["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 3);
        assert!(fields[0]["text"].as_str().unwrap().contains("abcdef12"));
        assert!(fields[1]["text"].as_str().unwrap().contains("#42"));
        assert!(fields[2]["text"].as_str().unwrap().contains("$1.23"));
    }

    #[test]
    fn session_errored_payload_includes_error() {
        let event = SlackEvent::SessionErrored {
            session_id: "deadbeef-0000-0000-0000-000000000000".into(),
            issue_number: None,
            error: "process crashed".into(),
        };
        let payload = event.to_payload();
        let fields = payload["blocks"][1]["fields"].as_array().unwrap();
        assert!(
            fields[2]["text"]
                .as_str()
                .unwrap()
                .contains("process crashed")
        );
    }

    #[test]
    fn budget_alert_payload_shows_percentages() {
        let event = SlackEvent::BudgetAlert {
            usage_pct: 85,
            total_usd: 50.0,
            spent_usd: 42.50,
        };
        let payload = event.to_payload();
        let fields = payload["blocks"][1]["fields"].as_array().unwrap();
        assert!(fields[0]["text"].as_str().unwrap().contains("85%"));
        assert!(fields[1]["text"].as_str().unwrap().contains("$42.50"));
    }

    #[test]
    fn file_conflict_payload_lists_sessions() {
        let event = SlackEvent::FileConflict {
            file_path: "src/main.rs".into(),
            sessions: vec![
                "aaaaaaaa-1111-2222-3333-444444444444".into(),
                "bbbbbbbb-5555-6666-7777-888888888888".into(),
            ],
        };
        let payload = event.to_payload();
        let fields = payload["blocks"][1]["fields"].as_array().unwrap();
        assert!(fields[0]["text"].as_str().unwrap().contains("src/main.rs"));
        assert!(fields[1]["text"].as_str().unwrap().contains("aaaaaaaa"));
        assert!(fields[1]["text"].as_str().unwrap().contains("bbbbbbbb"));
    }

    #[test]
    fn short_id_truncates_to_8() {
        assert_eq!(short_id("abcdef1234567890"), "abcdef12");
        assert_eq!(short_id("short"), "short");
    }

    #[test]
    fn truncate_adds_ellipsis() {
        assert_eq!(truncate("hello world", 5), "hello...");
        assert_eq!(truncate("hi", 5), "hi");
    }

    #[test]
    fn rate_limiter_allows_within_limit() {
        let mut client = SlackClient::new("https://hooks.slack.com/test".into(), 5);
        for _ in 0..5 {
            assert!(client.check_rate_limit());
            client.record_send();
        }
        // 6th should be blocked
        assert!(!client.check_rate_limit());
    }

    #[test]
    fn level_to_slack_event_filters_info() {
        assert!(level_to_slack_event(InterruptLevel::Info, "test", "msg").is_none());
    }

    #[test]
    fn level_to_slack_event_maps_warning() {
        let event = level_to_slack_event(InterruptLevel::Warning, "Budget", "80% used");
        assert!(event.is_some());
        match event.unwrap() {
            SlackEvent::BudgetAlert { usage_pct, .. } => assert_eq!(usage_pct, 80),
            _ => panic!("expected BudgetAlert"),
        }
    }

    #[test]
    fn level_to_slack_event_maps_critical_non_budget() {
        let event = level_to_slack_event(InterruptLevel::Critical, "Session Failed", "timeout");
        assert!(event.is_some());
        match event.unwrap() {
            SlackEvent::SessionErrored { error, .. } => {
                assert!(error.contains("Session Failed"));
                assert!(error.contains("timeout"));
            }
            _ => panic!("expected SessionErrored"),
        }
    }

    #[test]
    fn parse_pct_extracts_number() {
        assert_eq!(parse_pct("80% used"), Some(80));
        assert_eq!(parse_pct("no percent here"), None);
        assert_eq!(parse_pct("100%"), Some(100));
    }
}
