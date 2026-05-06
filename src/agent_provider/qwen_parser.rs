use std::collections::HashMap;

use serde_json::Value;

use crate::session::types::StreamEvent;

#[derive(Debug, Default)]
pub struct QwenStreamParser {
    tool_blocks: HashMap<u64, QwenToolBlock>,
    tool_names_by_id: HashMap<String, String>,
    text_delta_seen_in_message: bool,
}

#[derive(Debug, Default)]
struct QwenToolBlock {
    id: Option<String>,
    name: String,
    partial_json: String,
}

impl QwenStreamParser {
    pub fn parse_line(&mut self, line: &str) -> Vec<StreamEvent> {
        let line = line.trim();
        if line.is_empty() {
            return vec![StreamEvent::Unknown { raw: String::new() }];
        }

        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return vec![StreamEvent::Unknown {
                raw: line.to_string(),
            }];
        };

        match v.get("type").and_then(|t| t.as_str()) {
            Some("stream_event") => self.parse_stream_event(&v),
            Some("assistant") => self.parse_assistant(&v),
            Some("user") => self.parse_user(&v),
            Some("result") => self.parse_result(&v),
            Some("error") => vec![StreamEvent::Error {
                message: v
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
            }],
            Some("system") => vec![StreamEvent::Unknown { raw: v.to_string() }],
            _ => vec![StreamEvent::Unknown {
                raw: line.to_string(),
            }],
        }
    }

    fn parse_stream_event(&mut self, v: &Value) -> Vec<StreamEvent> {
        let Some(event) = v.get("event") else {
            return vec![StreamEvent::Unknown { raw: v.to_string() }];
        };
        match event.get("type").and_then(|t| t.as_str()) {
            Some("message_start") => {
                self.text_delta_seen_in_message = false;
                Vec::new()
            }
            Some("message_stop") => {
                self.text_delta_seen_in_message = false;
                Vec::new()
            }
            Some("content_block_start") => {
                self.track_content_block_start(event);
                Vec::new()
            }
            Some("content_block_delta") => self.parse_content_block_delta(event),
            Some("content_block_stop") => self.parse_content_block_stop(event),
            _ => vec![StreamEvent::Unknown { raw: v.to_string() }],
        }
    }

    fn track_content_block_start(&mut self, event: &Value) {
        let Some(index) = event.get("index").and_then(|i| i.as_u64()) else {
            return;
        };
        let Some(block) = event.get("content_block") else {
            return;
        };
        if block.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
            return;
        }

        let id = block
            .get("id")
            .and_then(|id| id.as_str())
            .map(str::to_string);
        let name = block
            .get("name")
            .and_then(|name| name.as_str())
            .unwrap_or("unknown")
            .to_string();
        self.tool_blocks.insert(
            index,
            QwenToolBlock {
                id,
                name,
                partial_json: String::new(),
            },
        );
    }

    fn parse_content_block_delta(&mut self, event: &Value) -> Vec<StreamEvent> {
        let Some(delta) = event.get("delta") else {
            return Vec::new();
        };
        match delta.get("type").and_then(|t| t.as_str()) {
            Some("text_delta") => {
                let text = delta
                    .get("text")
                    .and_then(|text| text.as_str())
                    .unwrap_or("")
                    .to_string();
                self.text_delta_seen_in_message = true;
                if text.starts_with("[API Error:") {
                    vec![StreamEvent::Error { message: text }]
                } else {
                    vec![StreamEvent::AssistantMessage { text }]
                }
            }
            Some("input_json_delta") => {
                if let Some(index) = event.get("index").and_then(|i| i.as_u64())
                    && let Some(block) = self.tool_blocks.get_mut(&index)
                    && let Some(partial) = delta.get("partial_json").and_then(|p| p.as_str())
                {
                    block.partial_json.push_str(partial);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn parse_content_block_stop(&mut self, event: &Value) -> Vec<StreamEvent> {
        let Some(index) = event.get("index").and_then(|i| i.as_u64()) else {
            return Vec::new();
        };
        let Some(block) = self.tool_blocks.remove(&index) else {
            return Vec::new();
        };

        let input = serde_json::from_str::<Value>(&block.partial_json).ok();
        let file_path = input.as_ref().and_then(extract_file_path);
        let command_preview = input.as_ref().and_then(extract_command_preview);
        if let Some(id) = block.id.as_ref() {
            self.tool_names_by_id.insert(id.clone(), block.name.clone());
        }
        vec![StreamEvent::ToolUse {
            tool: block.name,
            file_path,
            command_preview,
            subagent_name: None,
        }]
    }

    fn parse_assistant(&mut self, v: &Value) -> Vec<StreamEvent> {
        let Some(content) = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            return vec![StreamEvent::Unknown { raw: v.to_string() }];
        };

        let mut events = Vec::new();
        for block in content {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    let text = block
                        .get("text")
                        .and_then(|text| text.as_str())
                        .unwrap_or("")
                        .to_string();
                    if text.starts_with("[API Error:") {
                        events.push(StreamEvent::Error { message: text });
                    } else if !self.text_delta_seen_in_message && !text.is_empty() {
                        events.push(StreamEvent::AssistantMessage { text });
                    }
                }
                Some("tool_use") if !self.tool_was_streamed(block) => {
                    let tool = block
                        .get("name")
                        .and_then(|name| name.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let input = block.get("input");
                    if let Some(id) = block.get("id").and_then(|id| id.as_str()) {
                        self.tool_names_by_id.insert(id.to_string(), tool.clone());
                    }
                    events.push(StreamEvent::ToolUse {
                        tool,
                        file_path: input.and_then(extract_file_path),
                        command_preview: input.and_then(extract_command_preview),
                        subagent_name: None,
                    });
                }
                _ => {}
            }
        }
        events
    }

    fn tool_was_streamed(&self, block: &Value) -> bool {
        block
            .get("id")
            .and_then(|id| id.as_str())
            .is_some_and(|id| self.tool_names_by_id.contains_key(id))
    }

    fn parse_user(&self, v: &Value) -> Vec<StreamEvent> {
        let Some(content) = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            return vec![StreamEvent::Unknown { raw: v.to_string() }];
        };

        content
            .iter()
            .filter(|block| block.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
            .map(|block| {
                let tool = block
                    .get("tool_use_id")
                    .and_then(|id| id.as_str())
                    .and_then(|id| self.tool_names_by_id.get(id))
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                let is_error = block
                    .get("is_error")
                    .and_then(|is_error| is_error.as_bool())
                    .unwrap_or(false);
                StreamEvent::ToolResult { tool, is_error }
            })
            .collect()
    }

    fn parse_result(&self, v: &Value) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        if v.get("is_error").and_then(|is_error| is_error.as_bool()) == Some(true) {
            let message = v
                .get("result")
                .and_then(|result| result.as_str())
                .or_else(|| v.get("error").and_then(|error| error.as_str()))
                .unwrap_or("qwen run failed")
                .to_string();
            events.push(StreamEvent::Error { message });
        }
        events.push(StreamEvent::Completed { cost_usd: 0.0 });
        events
    }
}

fn extract_file_path(input: &Value) -> Option<String> {
    input
        .get("file_path")
        .or_else(|| input.get("path"))
        .and_then(|path| path.as_str())
        .map(str::to_string)
}

fn extract_command_preview(input: &Value) -> Option<String> {
    input
        .get("command")
        .and_then(|command| command.as_str())
        .map(|command| {
            if command.len() > 60 {
                let boundary = char_boundary(command, 60);
                format!("{}...", &command[..boundary])
            } else {
                command.to_string()
            }
        })
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
