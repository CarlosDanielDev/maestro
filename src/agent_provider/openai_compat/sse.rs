#![deny(clippy::unwrap_used)]

use serde_json::Value;

use crate::session::types::{StreamEvent, TokenUsage};

#[derive(Debug, Clone, Default)]
pub struct OpenAiCompatibleSseParser {
    buffer: Vec<u8>,
    completed: bool,
}

impl OpenAiCompatibleSseParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_chunk(&mut self, chunk: &str) -> Result<Vec<StreamEvent>, String> {
        self.push_bytes(chunk.as_bytes())
    }

    pub fn push_bytes(&mut self, chunk: &[u8]) -> Result<Vec<StreamEvent>, String> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some((frame_end, delimiter_len)) = find_frame_end(&self.buffer) {
            let frame = self.buffer[..frame_end].to_vec();
            let drain_to = frame_end + delimiter_len;
            self.buffer.drain(..drain_to);
            events.extend(self.parse_frame(&frame)?);
        }

        Ok(events)
    }

    pub fn finish(&mut self) -> Result<Vec<StreamEvent>, String> {
        if self.buffer.iter().all(u8::is_ascii_whitespace) {
            return Ok(Vec::new());
        }

        let frame = std::mem::take(&mut self.buffer);
        self.parse_frame(&frame)
    }

    fn parse_frame(&mut self, frame: &[u8]) -> Result<Vec<StreamEvent>, String> {
        let Ok(frame) = std::str::from_utf8(frame) else {
            return Ok(vec![StreamEvent::Unknown {
                raw: String::from_utf8_lossy(frame).to_string(),
            }]);
        };

        let data = frame
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .filter_map(data_field_value)
            .collect::<Vec<_>>()
            .join("\n");

        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        if data.trim() == "[DONE]" {
            if self.completed {
                return Ok(Vec::new());
            }
            self.completed = true;
            return Ok(vec![StreamEvent::Completed { cost_usd: 0.0 }]);
        }

        let value: Value = match serde_json::from_str(&data) {
            Ok(value) => value,
            Err(_) => {
                return Ok(vec![StreamEvent::Unknown { raw: data }]);
            }
        };
        Ok(self.parse_json_event(&value))
    }

    fn parse_json_event(&mut self, value: &Value) -> Vec<StreamEvent> {
        let mut events = Vec::new();

        if let Some(error) = value.get("error") {
            events.push(StreamEvent::Error {
                message: error_message(error),
            });
            return events;
        }

        let Some(choice) = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            return vec![StreamEvent::Unknown {
                raw: value.to_string(),
            }];
        };

        if let Some(content) = choice
            .get("delta")
            .and_then(|delta| delta.get("content"))
            .and_then(Value::as_str)
            && !content.is_empty()
        {
            events.push(StreamEvent::AssistantMessage {
                text: content.to_string(),
            });
        }

        // Emit ToolUse only when delta.tool_calls[i].function.name is a
        // non-empty string. Providers that lack tool calling (e.g. MiniMax)
        // sometimes send empty or partial `tool_calls` arrays; emitting
        // synthetic "Using tool_calls" events for those frames pollutes the
        // activity log and confuses the hollow-completion detector (#891).
        if let Some(calls) = choice
            .get("delta")
            .and_then(|delta| delta.get("tool_calls"))
            .and_then(Value::as_array)
        {
            for call in calls {
                if let Some(name) = extract_tool_name(call) {
                    events.push(StreamEvent::ToolUse {
                        tool: name,
                        file_path: None,
                        command_preview: None,
                        subagent_name: None,
                    });
                }
            }
        }

        // Emit TokenUpdate before the finish_reason transitions so handlers
        // see the final token tally before Completed (or before the next tool
        // call). The unexpected-finish-reason branch is the one exception:
        // there we don't surface either, since the frame is being treated as
        // malformed downstream.
        let finish_reason = choice.get("finish_reason").and_then(Value::as_str);
        if !matches!(finish_reason, Some(other) if other != "stop" && other != "tool_calls")
            && let Some(usage) = value
                .get("usage")
                .and_then(|u| parse_openai_usage(u, &mut events))
        {
            events.push(StreamEvent::TokenUpdate { usage });
        }

        match finish_reason {
            Some("stop") if !self.completed => {
                self.completed = true;
                events.push(StreamEvent::Completed { cost_usd: 0.0 });
            }
            Some("stop") => {}
            // The tool_calls finish_reason is informational — the actual
            // ToolUse events were already streamed via deltas above. Emitting
            // another synthetic event here doubles every real tool call and,
            // worse, fires on every MiniMax completion that happens to ship
            // an empty `finish_reason: tool_calls` (#891).
            Some("tool_calls") => {}
            Some(other) => events.push(StreamEvent::Unknown {
                raw: format!("unexpected finish_reason: {other}"),
            }),
            None => {}
        }

        events
    }
}

/// Extract the function name from a single `tool_calls[i]` entry as defined
/// by the OpenAI / OpenAI-compatible streaming schema. Returns `None` when
/// the entry has no `function.name` or the name is empty.
fn extract_tool_name(call: &Value) -> Option<String> {
    let name = call
        .get("function")
        .and_then(|f| f.get("name"))
        .and_then(Value::as_str)?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Build a `TokenUsage` from the OpenAI-compatible `usage` block emitted on
/// the final streaming frame. Returns `None` when the block carries no
/// non-zero token count, so downstream handlers don't see noisy zero events.
///
/// Per-field values above [`crate::session::types::TOKEN_COUNT_CAP`] are
/// clamped to the cap and a `StreamEvent::Warning` is pushed into
/// `warnings` (#846).
fn parse_openai_usage(usage: &Value, warnings: &mut Vec<StreamEvent>) -> Option<TokenUsage> {
    let prompt = sanitize_field(
        "prompt_tokens",
        usage
            .get("prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        warnings,
    );
    let completion = sanitize_field(
        "completion_tokens",
        usage
            .get("completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        warnings,
    );
    let cached = sanitize_field(
        "prompt_tokens_details.cached_tokens",
        usage
            .pointer("/prompt_tokens_details/cached_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        warnings,
    );
    if prompt == 0 && completion == 0 && cached == 0 {
        return None;
    }
    Some(TokenUsage {
        input_tokens: prompt,
        output_tokens: completion,
        cache_read_tokens: cached,
        cache_creation_tokens: 0,
    })
}

/// Clamp a single OpenAI-compatible token field; record a `Warning` when the
/// raw value was above [`crate::session::types::TOKEN_COUNT_CAP`] (#846).
fn sanitize_field(field: &str, raw: u64, warnings: &mut Vec<StreamEvent>) -> u64 {
    use crate::session::types::{TOKEN_COUNT_CAP, sanitize_token_count};
    let capped = sanitize_token_count(raw);
    if capped < raw {
        tracing::warn!(
            provider = "openai-compat",
            field,
            raw,
            cap = TOKEN_COUNT_CAP,
            "token count above cap; clamping"
        );
        warnings.push(StreamEvent::Warning {
            code: "token_count_clamped".to_string(),
            message: format!("openai-compat: {field}={raw} clamped to {TOKEN_COUNT_CAP}"),
        });
    }
    capped
}

fn find_frame_end(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = find_bytes(buffer, b"\n\n").map(|index| (index, 2));
    let crlf = find_bytes(buffer, b"\r\n\r\n").map(|index| (index, 4));
    match (lf, crlf) {
        (Some(lf), Some(crlf)) => Some(if lf.0 < crlf.0 { lf } else { crlf }),
        (Some(lf), None) => Some(lf),
        (None, Some(crlf)) => Some(crlf),
        (None, None) => None,
    }
}

fn find_bytes(buffer: &[u8], needle: &[u8]) -> Option<usize> {
    buffer
        .windows(needle.len())
        .position(|window| window == needle)
}

fn data_field_value(line: &str) -> Option<&str> {
    let value = line.strip_prefix("data:")?;
    Some(value.strip_prefix(' ').unwrap_or(value))
}

fn error_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| error.to_string())
}

#[cfg(test)]
#[path = "sse_tests.rs"]
mod tests;
