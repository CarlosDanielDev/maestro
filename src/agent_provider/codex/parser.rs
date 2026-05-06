use std::collections::HashMap;

use serde_json::Value;

use crate::session::types::{StreamEvent, TokenUsage};

#[derive(Debug, Default)]
pub(crate) struct CodexStreamParser {
    tool_names_by_id: HashMap<String, String>,
}

impl CodexStreamParser {
    pub(crate) fn parse_line(&mut self, line: &str) -> Vec<StreamEvent> {
        let line = line.trim();
        if line.is_empty() {
            return vec![StreamEvent::Unknown { raw: String::new() }];
        }

        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return vec![StreamEvent::Unknown {
                raw: line.to_string(),
            }];
        };

        match v.get("type").and_then(Value::as_str) {
            Some("thread.started") | Some("turn.started") | Some("item.started") => Vec::new(),
            Some("item.completed") => self.parse_item_completed(&v),
            Some("turn.completed") => self.parse_turn_completed(&v),
            Some("turn.failed") => vec![StreamEvent::Error {
                message: extract_error_message(&v).unwrap_or_else(|| "codex turn failed".into()),
            }],
            Some("error") => vec![StreamEvent::Error {
                message: extract_error_message(&v).unwrap_or_else(|| "codex run failed".into()),
            }],
            _ => vec![StreamEvent::Unknown {
                raw: line.to_string(),
            }],
        }
    }

    fn parse_item_completed(&mut self, v: &Value) -> Vec<StreamEvent> {
        let item = v.get("item").unwrap_or(v);
        match item.get("type").and_then(Value::as_str) {
            Some("message") | Some("agent_message") => parse_codex_message(item),
            Some("function_call") | Some("tool_call") => {
                let id = item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let tool = item
                    .get("name")
                    .or_else(|| item.get("tool_name"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                if let Some(id) = id {
                    self.tool_names_by_id.insert(id, tool.clone());
                }
                let input = item
                    .get("arguments")
                    .or_else(|| item.get("input"))
                    .and_then(json_value_from_maybe_string);
                vec![StreamEvent::ToolUse {
                    tool,
                    file_path: input.as_ref().and_then(extract_file_path),
                    command_preview: input.as_ref().and_then(extract_command_preview),
                    subagent_name: None,
                }]
            }
            Some("function_call_output") | Some("tool_result") => {
                let tool = item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .and_then(|id| self.tool_names_by_id.get(id))
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                let is_error = item
                    .get("is_error")
                    .or_else(|| item.get("error"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                vec![StreamEvent::ToolResult { tool, is_error }]
            }
            Some("reasoning") => item
                .get("summary")
                .or_else(|| item.get("text"))
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(|text| {
                    vec![StreamEvent::Thinking {
                        text: text.to_string(),
                    }]
                })
                .unwrap_or_default(),
            _ => vec![StreamEvent::Unknown { raw: v.to_string() }],
        }
    }

    fn parse_turn_completed(&self, v: &Value) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        if let Some(usage) = v.get("usage").or_else(|| v.pointer("/turn/usage")) {
            events.push(StreamEvent::TokenUpdate {
                usage: parse_usage(usage),
            });
        }
        events.push(StreamEvent::Completed { cost_usd: 0.0 });
        events
    }
}

fn parse_codex_message(item: &Value) -> Vec<StreamEvent> {
    if item
        .get("role")
        .and_then(Value::as_str)
        .is_some_and(|role| role != "assistant")
    {
        return Vec::new();
    }
    let text = item
        .get("content")
        .and_then(|content| {
            content.as_array().map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|block| {
                        block
                            .get("text")
                            .or_else(|| block.get("output_text"))
                            .and_then(Value::as_str)
                    })
                    .collect::<Vec<_>>()
                    .join("")
            })
        })
        .or_else(|| item.get("text").and_then(Value::as_str).map(str::to_string))
        .or_else(|| {
            item.get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();

    if text.is_empty() {
        Vec::new()
    } else if text.starts_with("[API Error:") {
        vec![StreamEvent::Error { message: text }]
    } else {
        vec![StreamEvent::AssistantMessage { text }]
    }
}

fn parse_usage(usage: &Value) -> TokenUsage {
    TokenUsage {
        input_tokens: usage_u64(usage, &["input_tokens", "prompt_tokens"]),
        output_tokens: usage_u64(usage, &["output_tokens", "completion_tokens"]),
        cache_read_tokens: usage_u64(
            usage,
            &[
                "cache_read_input_tokens",
                "cached_input_tokens",
                "cache_read_tokens",
            ],
        ),
        cache_creation_tokens: usage_u64(
            usage,
            &["cache_creation_input_tokens", "cache_creation_tokens"],
        ),
    }
}

fn usage_u64(usage: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| usage.get(*key).and_then(Value::as_u64))
        .unwrap_or(0)
}

fn json_value_from_maybe_string(value: &Value) -> Option<Value> {
    match value {
        Value::String(s) => serde_json::from_str(s).ok(),
        other => Some(other.clone()),
    }
}

fn extract_file_path(input: &Value) -> Option<String> {
    input
        .get("file_path")
        .or_else(|| input.get("path"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn extract_command_preview(input: &Value) -> Option<String> {
    input
        .get("command")
        .or_else(|| input.get("cmd"))
        .and_then(Value::as_str)
        .map(|command| {
            if command.len() > 60 {
                let boundary = char_boundary(command, 60);
                format!("{}...", &command[..boundary])
            } else {
                command.to_string()
            }
        })
}

fn extract_error_message(v: &Value) -> Option<String> {
    v.get("message")
        .or_else(|| v.pointer("/error/message"))
        .or_else(|| v.pointer("/error/data/message"))
        .or_else(|| v.pointer("/turn/error/message"))
        .or_else(|| v.pointer("/turn/error/data/message"))
        .or_else(|| v.get("error"))
        .and_then(Value::as_str)
        .map(str::to_string)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_maps_codex_jsonl_to_stream_events() {
        let mut parser = CodexStreamParser::default();
        let events: Vec<StreamEvent> = [
            r#"{"type":"thread.started","thread_id":"t1"}"#,
            r#"{"type":"item.completed","item":{"type":"function_call","call_id":"call_1","name":"shell","arguments":"{\"command\":\"cargo test\",\"path\":\"src/lib.rs\"}"}}"#,
            r#"{"type":"item.completed","item":{"type":"function_call_output","call_id":"call_1","output":"ok"}}"#,
            r#"{"type":"item.completed","item":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done."}]}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5,"cached_input_tokens":2}}"#,
        ]
        .into_iter()
        .flat_map(|line| parser.parse_line(line))
        .collect();

        assert!(events.iter().any(|event| {
            matches!(
                event,
                StreamEvent::ToolUse {
                    tool,
                    file_path: Some(path),
                    command_preview: Some(command),
                    ..
                } if tool == "shell" && path == "src/lib.rs" && command == "cargo test"
            )
        }));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, StreamEvent::ToolResult { tool, is_error: false } if tool == "shell"))
        );
        assert!(events.iter().any(
            |event| matches!(event, StreamEvent::AssistantMessage { text } if text == "Done.")
        ));
        assert!(events.iter().any(|event| {
            matches!(event, StreamEvent::TokenUpdate { usage } if usage.input_tokens == 10 && usage.output_tokens == 5 && usage.cache_read_tokens == 2)
        }));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, StreamEvent::Completed { .. }))
        );
    }

    #[test]
    fn parser_maps_agent_message_and_turn_failed() {
        let mut parser = CodexStreamParser::default();

        let message = parser.parse_line(
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"Hello"}}"#,
        );
        assert!(matches!(
            message.as_slice(),
            [StreamEvent::AssistantMessage { text }] if text == "Hello"
        ));

        let failed =
            parser.parse_line(r#"{"type":"turn.failed","error":{"message":"model failed"}}"#);
        assert!(matches!(
            failed.as_slice(),
            [StreamEvent::Error { message }] if message == "model failed"
        ));
    }
}
