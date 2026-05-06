use serde_json::Value;

use crate::session::types::{StreamEvent, TokenUsage};

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
