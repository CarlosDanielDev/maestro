use serde_json::Value;

use super::pricing;
use crate::session::types::{StreamEvent, TokenUsage};

#[derive(Debug, Default)]
pub struct OpenCodeJsonParser {
    stdout_bytes: Vec<u8>,
    model: String,
}

impl OpenCodeJsonParser {
    /// Build a parser that knows its session model so `step_finish` frames
    /// missing or reporting `cost: 0` fall back to the per-model pricing
    /// table. The factory path keeps `default()` (empty model → fallback
    /// returns 0, matching the previous behavior).
    pub fn with_model(model: impl Into<String>) -> Self {
        Self {
            stdout_bytes: Vec::new(),
            model: model.into(),
        }
    }

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
            Some("step_finish") => parse_step_finish_event(&value, &self.model),
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

fn parse_step_finish_event(value: &Value, model: &str) -> Vec<StreamEvent> {
    let Some(part) = value.get("part") else {
        return vec![StreamEvent::Unknown {
            raw: value.to_string(),
        }];
    };

    let mut events = Vec::new();
    let token_usage = part
        .get("tokens")
        .map(|tokens| parse_opencode_tokens(tokens, &mut events));
    if let Some(usage) = token_usage.as_ref() {
        events.push(StreamEvent::TokenUpdate {
            usage: usage.clone(),
        });
    }

    // Use telemetry-reported cost when finite and positive; otherwise fall
    // back to the per-model pricing table. OpenCode reports `cost: 0` for
    // many providers even when tokens were consumed, so the fallback fires
    // any time the telemetry would otherwise hide the real spend.
    let telemetry_cost = part
        .get("cost")
        .and_then(Value::as_f64)
        .filter(|c| c.is_finite() && *c > 0.0);
    let computed_cost = telemetry_cost.unwrap_or_else(|| {
        token_usage
            .as_ref()
            .map(|usage| pricing::compute_cost(model, usage))
            .unwrap_or(0.0)
    });

    if computed_cost.is_finite() && computed_cost > 0.0 {
        events.push(StreamEvent::CostUpdate {
            cost_usd: computed_cost,
        });
    }

    match part.get("reason").and_then(Value::as_str) {
        Some("stop") => events.push(StreamEvent::Completed {
            cost_usd: computed_cost,
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

fn parse_opencode_tokens(tokens: &Value, warnings: &mut Vec<StreamEvent>) -> TokenUsage {
    TokenUsage {
        input_tokens: sanitize_field(
            "input",
            tokens.get("input").and_then(Value::as_u64).unwrap_or(0),
            warnings,
        ),
        output_tokens: sanitize_field(
            "output",
            tokens.get("output").and_then(Value::as_u64).unwrap_or(0),
            warnings,
        ),
        cache_read_tokens: sanitize_field(
            "cache.read",
            tokens
                .pointer("/cache/read")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            warnings,
        ),
        cache_creation_tokens: sanitize_field(
            "cache.write",
            tokens
                .pointer("/cache/write")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            warnings,
        ),
    }
}

/// Clamp a single OpenCode-reported token field; record a `Warning` when the
/// raw value was above [`crate::session::types::TOKEN_COUNT_CAP`] (#846).
fn sanitize_field(field: &str, raw: u64, warnings: &mut Vec<StreamEvent>) -> u64 {
    use crate::session::types::{TOKEN_COUNT_CAP, sanitize_token_count};
    let capped = sanitize_token_count(raw);
    if capped < raw {
        tracing::warn!(
            provider = "opencode",
            field,
            raw,
            cap = TOKEN_COUNT_CAP,
            "token count above cap; clamping"
        );
        warnings.push(StreamEvent::Warning {
            code: "token_count_clamped".to_string(),
            message: format!("opencode: {field}={raw} clamped to {TOKEN_COUNT_CAP}"),
        });
    }
    capped
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
